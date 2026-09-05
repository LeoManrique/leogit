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

    /// How long a repository whose badge probe *failed* is left alone before
    /// being asked again. Deliberately longer than both the 30 s sweep and the
    /// 2 min top tier: what makes `repo_sync_status` fail is a property of the
    /// folder — it stopped being a repository, its permissions changed, the MRU
    /// still names a path that moved — not a hiccup that the next pass will find
    /// resolved, and every re-ask spends a subprocess pair while the badges
    /// behind it in the sequential loop wait.
    private static let probeRetry: Duration = .seconds(5 * 60)

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

    /// The app-wide config owner, for the two settings discovery walks by.
    /// A dependency rather than directory state, hence unobserved.
    @ObservationIgnored private let configStore: AppConfigStore

    init(config: AppConfigStore) {
        configStore = config
    }

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

    /// The one per-repository fetch stamp in the process, owned here beside the
    /// breaker for the same reason — this is where background fetching lives —
    /// and handed to `SyncStore` so the active repository's silent fetch and
    /// the tier sweeps cannot keep two disagreeing answers.
    let fetchCooldown = FetchCooldown()

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

    /// When each repository's badge probe last *failed*, for the ones whose
    /// last one did.
    ///
    /// `syncByPath` on its own is two-valued — a summary, or nothing — and the
    /// sweep reads "nothing" as "never looked up". A repository that cannot
    /// answer therefore had no way to say so, and was re-asked on every pass,
    /// forever, at the head of a sequential loop every other badge waits in.
    /// Together the two dictionaries are three-valued, the way
    /// `RepoIdentifierStore` is deliberately three-valued one file over: absent
    /// from both means "never looked up", a summary means "answered", and a
    /// stamp here means "asked, and it could not answer".
    ///
    /// A failure never blanks a badge — `sync` writes a summary only on success,
    /// so whatever the row last showed stays — and never becomes permanent: this
    /// is a retry cadence, not a deny list.
    ///
    /// `ContinuousClock` for `FetchCooldown`'s reason: it counts through system
    /// sleep and cannot be walked backwards by a wall clock that steps.
    private var lastProbeFailure: [String: ContinuousClock.Instant] = [:]

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

        // Through the shared owner, not a read of its own: the Settings window
        // publishes there before it announces a scan-path change, so the walk
        // this triggers already sees the new folders — and a repo switch stops
        // costing a config file read.
        let scanPaths = configStore.scanPaths
        scanFolders = await GitBridge.scanFolders(for: scanPaths)
        do {
            discovered = try await GitBridge.knownRepositories(
                scanPaths: scanPaths,
                depth: configStore.scanDepth
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
    ///
    /// Both in one write. They were two, and the shared file is read, parsed,
    /// re-serialized and rewritten whole each time, so a repo switch paid for
    /// that twice — with a window in between where the file named one repo as
    /// most-recent and a different one as the one to reopen.
    func noteOpened(_ path: String) async {
        recentRepos.removeAll { $0 == path }
        recentRepos.insert(path, at: 0)
        publishRepos()
        if let state = try? await GitBridge.recordRecent(repoPath: path) {
            adoptRecents(state.recentRepos ?? [])
            publishRepos()
        }
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
        // The open repository just answered a full status, so whatever made a
        // background probe fail is over. Clearing the marker here is what stops
        // the sweep avoiding a repository for another five minutes after the
        // user has already fixed it and opened it.
        lastProbeFailure.removeValue(forKey: path)
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
    ///
    /// "No cached summary" is not the same question as "not looked up yet", and
    /// `sync` is where the two are told apart — a row whose probe failed is
    /// still missing a summary and still selected here, and declined there.
    func sweepVisible(activePath: String?, policy: BackgroundSchedulingPolicy) async {
        let full = Date.now.timeIntervalSince(lastFullSweep) >= Self.sweepThrottle
        for path in repos where path != activePath {
            // The caller keys this on the row list, so a walk publishing new
            // rows replaces the pass rather than racing it.
            if Task.isCancelled { return }
            // Re-asked per row rather than once at entry, like `run(tier:)`
            // beside it: a network operation taking the slot, or the window
            // going away, abandons the rest of a fan-out nobody is looking at
            // instead of finishing it. Bailing here also leaves `lastFullSweep`
            // unstamped, which is the point — an abandoned pass is not a pass,
            // and the next open must be allowed to finish it.
            guard policy.canRunRepoSweeps else { return }
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

    /// One repo's badge refresh. Being offline, an open breaker, or a remote
    /// this repository reached moments ago all downgrade a fetching sync to a
    /// local one rather than skipping it (the Tauri client's exact fallback —
    /// badges keep tracking local edits), and only real fetch attempts against
    /// a real remote feed the breaker — a repo with no remote says nothing
    /// about connectivity, and a fetch never attempted says less still.
    ///
    /// `syncOnSwitch`'s native twin goes through here too, so opening a
    /// repository a minute after leaving it recomputes rather than refetches.
    ///
    /// A repository that failed to answer recently is skipped outright. The
    /// guard lives here rather than in the two callers so every sweep — visible
    /// rows, refocus, each tier — obeys one cadence.
    private func sync(_ path: String, fetching: Bool) async {
        guard !inFlight.contains(path) else { return }
        guard !isProbeSuppressed(path) else { return }
        inFlight.insert(path)
        defer { inFlight.remove(path) }

        var fetch = fetching && shouldAttemptBackground
        if fetch, fetchCooldown.isFresh(path) {
            fetchCooldown.logSkip(path, "recomputing without a fetch")
            fetch = false
        }
        let summary: RepoSync
        do {
            summary = try await GitBridge.syncSummary(of: path, fetching: fetch)
        } catch {
            // Stamped, not swallowed: this is the whole point of the marker, and
            // logging it here rather than on every suppressed retry means one
            // line per five minutes instead of one per sweep.
            lastProbeFailure[path] = .now
            print("[sweep] badge probe failed for \(path): \(error.displayMessage)")
            return
        }
        lastProbeFailure.removeValue(forKey: path)
        syncByPath[path] = summary
        if fetch, summary.hasRemote {
            breaker.record(success: summary.fetched)
        }
        // `fetched` alone is not "the remote replied": core documents it as
        // `true` when no fetch was requested and when there was no remote to
        // reach, because nothing failed. Reading it bare stamped every row of a
        // fetch-less sweep — the one the picker runs the moment it opens — and
        // the repository the user then opened had its own on-open fetch turned
        // away for the next minute. The stamp needs all three: we asked, there
        // was somewhere to ask, and the answer came back.
        if fetch, summary.hasRemote, summary.fetched {
            fetchCooldown.note(path)
        }
    }

    /// Whether `path` failed recently enough that asking again would spend two
    /// subprocesses — and the sequential loop's next slot — to be told the same
    /// thing.
    private func isProbeSuppressed(_ path: String) -> Bool {
        guard let failed = lastProbeFailure[path] else { return false }
        return failed.duration(to: .now) < Self.probeRetry
    }
}
