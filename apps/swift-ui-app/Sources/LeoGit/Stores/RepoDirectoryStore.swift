import Foundation

/// Observable state for the multi-repo world: every repository the app knows
/// about, the most-recently-opened order, and the per-repo badge summaries
/// (dirty / behind / ahead) the switcher renders.
///
/// The repo list merges filesystem discovery under the configured scan folders
/// with the shared MRU list (`known_repos`, which both clients call), so a repo
/// that arrived by clone or `leogit <dir>` keeps its row across launches even
/// when it lives outside the scan folders, and a path that no longer exists
/// loses one.
///
/// Badge freshness follows the Tauri client's `repoSyncScheduler` exactly:
/// fetch-less sweeps for rows the switcher is about to show (cheap — two git
/// subprocesses per repo, works offline), plus a tiered background loop that
/// actually fetches — the four most recent repos every 2 min, the next five
/// every 5 min, the next ten every 10 min, always sequentially so at most one
/// background `git fetch` runs at a time. The open repo is excluded from all
/// tiers: its badge is fed for free from the 2 s status poll.
@MainActor
@Observable
final class RepoDirectoryStore {
    // The Tauri scheduler's cadence, kept number-for-number: per-tier repeat
    // intervals, the short initial kicks that fill badges soon after launch,
    // and the 30 s throttles on the visible-row and refocus sweeps.
    private static let tierIntervals: [TimeInterval] = [2 * 60, 5 * 60, 10 * 60]
    private static let tierKicks: [TimeInterval] = [1.5, 4, 8]
    private static let tierRanges: [Range<Int>] = [0..<4, 4..<9, 9..<19]
    private static let sweepThrottle: TimeInterval = 30
    private static let refocusThrottle: TimeInterval = 30

    /// Every known repository path, discovery order (sorted), MRU-only
    /// entries appended.
    private(set) var repos: [String] = []

    /// Repo paths most-recently-opened first — the shared `repos-state.json`
    /// MRU, which also drives the Tauri client.
    private(set) var recentRepos: [String] = []

    /// Latest badge summary per repo path.
    private(set) var syncByPath: [String: RepoSync] = [:]

    /// Tilde-expanded folders discovery walked — the switcher's empty state
    /// names them so "no repositories" is diagnosable.
    private(set) var scanFolders: [String] = []

    /// True while a directory refresh is running, so a list can say "still
    /// looking" instead of "found nothing" during the first walk.
    private(set) var isRefreshing = false

    /// Whether any pass has finished, successfully or not.
    ///
    /// `isRefreshing` alone cannot answer "is an empty list news?": before the
    /// first pass is even *started* it is false, and a list rendering then —
    /// which the Welcome screen does, while launch resolution is still deciding
    /// what to open — would greet every launch with "No repositories found" and
    /// an invitation to go fix the scan paths. Nothing has looked yet, and that
    /// is what the list should say.
    private(set) var hasSearched = false

    /// Why the last walk failed, if it did. Rendered as one inline row above
    /// the list with a Retry — not a phase swap: the rows a previous walk
    /// found are still openable, and replacing them with an error screen
    /// would take away the repositories along with the bad news.
    private(set) var discoveryError: String?

    /// Row order — hydrated from the shared state file on the first walk and
    /// written back by the picker's toggle.
    private(set) var sortMode: SortMode = .recent

    /// The persisted mode is adopted once per launch, not on every walk: the
    /// toggle writes the file in the background, so re-reading it on the next
    /// refresh could put the old value back over a choice the user had just
    /// made. The Tauri store hydrates once for the same reason.
    private var hasHydratedSortMode = false

    /// Shared by everything that fetches in the background: the tier loop
    /// here and the active repo's auto-fetch loop both consult and feed it.
    let breaker = ConnectivityBreaker()

    /// The OS connectivity signal the breaker composes with; owned here so
    /// signal and breaker live in one place. `ContentView` registers the
    /// recovery kick on it.
    let networkObserver = NetworkPathObserver()

    /// The Tauri client's `shouldAttemptBackground()` shape, exactly:
    /// online per the OS path monitor, and the breaker's backoff window
    /// closed. Every background fetch gates on this — while offline the
    /// network goes quiet without burning failures into the breaker first.
    var shouldAttemptBackground: Bool { networkObserver.isOnline && breaker.shouldAttempt }

    /// The most recent walk's result, kept so a refresh in progress can
    /// republish the MRU without dropping rows the last walk found.
    private var discovered: [String] = []

    /// The refresh pass in flight, if any — concurrent callers await it
    /// instead of walking the tree a second time.
    private var refreshPass: Task<Void, Never>?

    private var inFlight: Set<String> = []
    private var lastFullSweep = Date.distantPast
    private var lastRefocusSweep = Date.distantPast

    /// A repository's folder name — the picker's row label wherever no
    /// remote names a better one, and what `RepoIdentifierStore` falls back
    /// to for a repo with no parseable remote.
    nonisolated static func displayName(of path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    /// Re-read config, reload the shared MRU, and re-run discovery. Called
    /// once when the repository screen appears, so the switcher's first open
    /// finds a populated list, and again whenever the popover opens, so repos
    /// created or cloned since the last look show up without a restart.
    ///
    /// Concurrent callers share one pass — the popover opening mid-prime
    /// awaits the running walk rather than starting a second one.
    func refreshDirectory() async {
        if let pass = refreshPass {
            await pass.value
            return
        }
        let pass = Task { await loadDirectory() }
        refreshPass = pass
        await pass.value
        refreshPass = nil
    }

    /// One refresh pass, published in two phases so a slow walk never leaves
    /// the switcher looking empty: the shared MRU lands first (a small JSON
    /// read, and the repos the user actually cycles between), then the
    /// filesystem walk — the slow half on a deep scan tree — replaces the
    /// list when it finishes.
    private func loadDirectory() async {
        isRefreshing = true
        defer {
            isRefreshing = false
            // Set on every exit, failures included: a walk that could not read
            // a scan folder has still looked, and the row it leaves behind is
            // what explains the empty list.
            hasSearched = true
        }

        if let state = try? await GitBridge.reposState() {
            adoptRecents(state.recentRepos ?? [])
            publishRepos()
            if !hasHydratedSortMode {
                hasHydratedSortMode = true
                sortMode = SortMode(persisted: state.repoSortMode) ?? .recent
            }
        }

        let config = try? await GitBridge.appConfig()
        let scanPaths = config?.scanPaths ?? []
        scanFolders = await GitBridge.scanFolders(for: scanPaths)
        do {
            discovered = try await GitBridge.knownRepositories(
                scanPaths: scanPaths,
                depth: config?.scanDepth ?? 3
            )
            discoveryError = nil
            publishRepos()
        } catch {
            // The MRU rows published above stay: a walk that couldn't read
            // one scan folder says nothing about the repositories already on
            // screen, and the row this sets is what offers a retry.
            discoveryError = error.displayMessage
        }
    }

    /// Flip recent ⇄ A-Z and persist the choice for both clients.
    /// Best-effort persistence: the toggle itself must never fail.
    func toggleSortMode() {
        sortMode = sortMode == .recent ? .name : .recent
        hasHydratedSortMode = true
        let mode = sortMode.rawValue
        Task { try? await GitBridge.setRepoSortMode(mode) }
    }

    /// Rebuild the row list.
    ///
    /// `discovered` already holds core's answer — discovery unioned with the
    /// existence-checked MRU — so this only re-adds the locally-known entries
    /// that haven't reached disk yet: `noteOpened` fronts the list before its
    /// write lands, and a refresh racing that write must not make the repo the
    /// user just opened disappear from the switcher.
    private func publishRepos() {
        var merged = discovered
        var seen = Set(discovered)
        for recent in recentRepos where !seen.contains(recent) {
            guard FileManager.default.fileExists(atPath: recent) else { continue }
            merged.append(recent)
            seen.insert(recent)
        }
        repos = merged
    }

    /// Take the persisted MRU, keeping any locally-known entry it doesn't
    /// carry yet: `noteOpened` updates this list before its write lands, and
    /// a refresh racing that write must not make the repo the user just
    /// opened disappear from the switcher.
    private func adoptRecents(_ persisted: [String]) {
        recentRepos = persisted + recentRepos.filter { !persisted.contains($0) }
    }

    /// Record that `path` is now the open repo: front of the local MRU
    /// immediately (so the switcher reorders without waiting on disk), then
    /// persisted to the shared state file — both the MRU and
    /// `last_opened_repo`, which is what either client restores on launch.
    func noteOpened(_ path: String) async {
        recentRepos.removeAll { $0 == path }
        recentRepos.insert(path, at: 0)
        publishRepos()
        if let state = try? await GitBridge.recordRecent(repoPath: path) {
            adoptRecents(state.recentRepos ?? [])
            publishRepos()
        }
        try? await GitBridge.setLastOpened(repoPath: path)
    }

    /// Fold the open repo's freshly polled status into its badge cache — the
    /// same free feed the Tauri client takes from its status poll, and the
    /// reason the active repo needs no tier.
    ///
    /// Written only when it differs. `syncByPath` is observed, and assigning an
    /// equal value still counts as a mutation, so an idle repository was
    /// invalidating every switcher row on every tick — the same waste
    /// `RepoStore`'s equality skip exists to prevent, one store along.
    func noteActiveStatus(_ path: String, _ status: RepoStatus) {
        let summary = RepoSync(
            ahead: status.ahead,
            behind: status.behind,
            hasRemote: status.hasRemote,
            fetched: true,
            dirty: !status.files.isEmpty
        )
        guard syncByPath[path] != summary else { return }
        syncByPath[path] = summary
    }

    /// Fetch-less badge sweep for the rows the switcher is showing: rows with
    /// no cached summary always fill, a full re-sweep at most once per 30 s.
    /// Local-only, so it works offline and costs no network. Obeys
    /// `canRunRepoSweeps` — the other-repos row of the policy table.
    func sweepVisible(activePath: String?, policy: BackgroundSchedulingPolicy) async {
        guard policy.canRunRepoSweeps else { return }
        let full = Date.now.timeIntervalSince(lastFullSweep) >= Self.sweepThrottle
        for path in repos where path != activePath {
            // The caller keys this on the row list, so a walk publishing new
            // rows replaces the pass rather than racing it.
            if Task.isCancelled { return }
            guard full || syncByPath[path] == nil else { continue }
            await sync(path, fetching: false)
        }
        // Charged only by a pass that actually finished. Stamping it on entry
        // let the interim MRU-only publish spend the whole window, after which
        // the pass over the *complete* list would only fill rows that had no
        // summary at all — leaving every other badge stale for another 30 s.
        if full && !Task.isCancelled {
            lastFullSweep = .now
        }
    }

    /// Refocus catch-up: re-fetch the most-recent tier so its badges reflect
    /// remote activity that happened while the app was in the background.
    /// Throttled to once per 30 s, like the Tauri client's `refocusSync`.
    /// Obeys `canRunRepoSweeps` — it runs right after activation, so the
    /// check only ever blocks it while a network operation holds the slot.
    func refocusSweep(activePath: String?, policy: BackgroundSchedulingPolicy) async {
        guard policy.canRunRepoSweeps else { return }
        guard Date.now.timeIntervalSince(lastRefocusSweep) >= Self.refocusThrottle else { return }
        lastRefocusSweep = .now
        await run(tier: 0, activePath: activePath, policy: policy)
    }

    /// The background badge loop, structured to the owning screen's lifetime:
    /// each tier first fires after its short kick, then repeats on its
    /// interval. Cancellation (repo switch, repo closed) ends it. Obeys
    /// `canRunRepoSweeps` — the deferrable fan-out pauses on blur (the GH
    /// Desktop model) and catches up via `refocusSweep`.
    func runScheduler(activePath: String, policy: BackgroundSchedulingPolicy) async {
        // The tiers need the MRU. Normally the screen's prime pass has it
        // already; if that failed or is still running, this coalesces with it
        // rather than loading the list a second way.
        if recentRepos.isEmpty {
            await refreshDirectory()
        }
        var due = Self.tierKicks.map { Date.now.addingTimeInterval($0) }
        while !Task.isCancelled {
            let wait = max((due.min() ?? .now).timeIntervalSince(.now), 1)
            try? await Task.sleep(for: .seconds(wait))
            if Task.isCancelled { return }
            for tier in due.indices where Date.now >= due[tier] {
                due[tier] = Date.now.addingTimeInterval(Self.tierIntervals[tier])
                await run(tier: tier, activePath: activePath, policy: policy)
            }
        }
    }

    /// Sync one tier's members sequentially, with a fetch. Losing
    /// `canRunRepoSweeps` mid-tier (a network operation starting, focus or
    /// visibility lost) abandons the rest of the tier, exactly as the Tauri
    /// scheduler aborts its loop.
    private func run(tier: Int, activePath: String?, policy: BackgroundSchedulingPolicy) async {
        for path in tierMembers(tier, activePath: activePath) {
            guard policy.canRunRepoSweeps else { return }
            await sync(path, fetching: true)
        }
    }

    /// Tier membership, recomputed per run from the MRU with the open repo
    /// excluded: tier 1 = the 4 most recent, tier 2 the next 5, tier 3 the
    /// next 10. Anything older only syncs via the visible-row sweep.
    private func tierMembers(_ tier: Int, activePath: String?) -> [String] {
        let eligible = recentRepos.filter { $0 != activePath }
        let range = Self.tierRanges[tier]
        guard eligible.count > range.lowerBound else { return [] }
        return Array(eligible[range.lowerBound..<min(range.upperBound, eligible.count)])
    }

    /// One repo's badge refresh. Being offline or an open breaker downgrades
    /// a fetching sync to a local one rather than skipping it (the Tauri
    /// client's exact fallback — badges keep tracking local edits), and only
    /// real fetch attempts against a real remote feed the breaker — a repo
    /// with no remote says nothing about connectivity.
    private func sync(_ path: String, fetching: Bool) async {
        guard !inFlight.contains(path) else { return }
        inFlight.insert(path)
        defer { inFlight.remove(path) }

        let fetch = fetching && shouldAttemptBackground
        guard let summary = try? await GitBridge.syncSummary(of: path, fetching: fetch) else {
            return
        }
        syncByPath[path] = summary
        if fetch, summary.hasRemote {
            breaker.record(success: summary.fetched)
        }
    }
}
