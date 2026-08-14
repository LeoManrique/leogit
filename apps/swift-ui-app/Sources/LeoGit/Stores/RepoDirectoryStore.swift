import Foundation

/// Observable state for the multi-repo world: every repository the app knows
/// about, the most-recently-opened order, and the per-repo badge summaries
/// (dirty / behind / ahead) the switcher renders.
///
/// The repo list merges filesystem discovery under the configured scan folders
/// with the shared MRU list, so a repo opened via "Open Other…" keeps its row
/// across launches even when it lives outside the scan folders — a deliberate
/// improvement over the Tauri picker, which forgets such repos on restart.
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

    /// Shared by everything that fetches in the background: the tier loop
    /// here and the active repo's auto-fetch loop both consult and feed it.
    let breaker = ConnectivityBreaker()

    private var inFlight: Set<String> = []
    private var lastFullSweep = Date.distantPast
    private var lastRefocusSweep = Date.distantPast

    /// Row label: the folder's basename, like the Tauri picker's fallback
    /// label (GitHub owner/name lookups are not ported).
    nonisolated static func displayName(of path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    /// Re-read config, run discovery, and reload the shared MRU — the
    /// switcher calls this every time it opens, so repos created or cloned
    /// since the last look appear without a restart.
    func refreshDirectory() async {
        let config = try? await GitBridge.appConfig()
        let scanPaths = config?.scanPaths ?? []
        let depth = config?.scanDepth ?? 3

        let discovered = (try? await GitBridge.discoverRepositories(scanPaths: scanPaths, depth: depth)) ?? []
        scanFolders = await GitBridge.scanFolders(for: scanPaths)
        if let state = try? await GitBridge.reposState() {
            recentRepos = state.recentRepos ?? recentRepos
        }

        var merged = discovered
        var seen = Set(discovered)
        for recent in recentRepos where !seen.contains(recent) {
            // MRU entries can be stale (moved, deleted) or live outside the
            // scan folders; only ones still on disk earn a row.
            guard FileManager.default.fileExists(atPath: recent) else { continue }
            merged.append(recent)
            seen.insert(recent)
        }
        repos = merged
    }

    /// Record that `path` is now the open repo: front of the local MRU
    /// immediately (so the switcher reorders without waiting on disk), then
    /// persisted to the shared state file — both the MRU and
    /// `last_opened_repo`, which is what either client restores on launch.
    func noteOpened(_ path: String) async {
        recentRepos.removeAll { $0 == path }
        recentRepos.insert(path, at: 0)
        if !repos.contains(path) {
            repos.append(path)
        }
        if let state = try? await GitBridge.recordRecent(repoPath: path) {
            recentRepos = state.recentRepos ?? recentRepos
        }
        try? await GitBridge.setLastOpened(repoPath: path)
    }

    /// Fold the open repo's freshly polled status into its badge cache — the
    /// same free feed the Tauri client takes from its 2 s poll, and the
    /// reason the active repo needs no tier.
    func noteActiveStatus(_ path: String, _ status: RepoStatus) {
        syncByPath[path] = RepoSync(
            ahead: status.ahead,
            behind: status.behind,
            hasRemote: status.hasRemote,
            fetched: true,
            dirty: !status.files.isEmpty
        )
    }

    /// Fetch-less badge sweep for the rows the switcher is showing: rows with
    /// no cached summary always fill, a full re-sweep at most once per 30 s.
    /// Local-only, so it works offline and costs no network.
    func sweepVisible(activePath: String?, isPaused: @MainActor () -> Bool) async {
        guard !isPaused() else { return }
        let full = Date.now.timeIntervalSince(lastFullSweep) >= Self.sweepThrottle
        if full {
            lastFullSweep = .now
        }
        for path in repos where path != activePath {
            guard full || syncByPath[path] == nil else { continue }
            await sync(path, fetching: false)
        }
    }

    /// Refocus catch-up: re-fetch the most-recent tier so its badges reflect
    /// remote activity that happened while the app was in the background.
    /// Throttled to once per 30 s, like the Tauri client's `refocusSync`.
    func refocusSweep(activePath: String?, isPaused: @MainActor () -> Bool) async {
        guard !isPaused() else { return }
        guard Date.now.timeIntervalSince(lastRefocusSweep) >= Self.refocusThrottle else { return }
        lastRefocusSweep = .now
        await run(tier: 0, activePath: activePath, isPaused: isPaused)
    }

    /// The background badge loop, structured to the owning screen's lifetime:
    /// each tier first fires after its short kick, then repeats on its
    /// interval. Cancellation (repo switch, repo closed) ends it.
    func runScheduler(activePath: String, isPaused: @MainActor () -> Bool) async {
        // The tiers need the MRU even if the switcher was never opened.
        if recentRepos.isEmpty, let state = try? await GitBridge.reposState() {
            recentRepos = state.recentRepos ?? []
        }
        var due = Self.tierKicks.map { Date.now.addingTimeInterval($0) }
        while !Task.isCancelled {
            let wait = max((due.min() ?? .now).timeIntervalSince(.now), 1)
            try? await Task.sleep(for: .seconds(wait))
            if Task.isCancelled { return }
            for tier in due.indices where Date.now >= due[tier] {
                due[tier] = Date.now.addingTimeInterval(Self.tierIntervals[tier])
                await run(tier: tier, activePath: activePath, isPaused: isPaused)
            }
        }
    }

    /// Sync one tier's members sequentially, with a fetch. A pause (a network
    /// operation starting mid-tier) abandons the rest of the tier, exactly as
    /// the Tauri scheduler aborts its loop.
    private func run(tier: Int, activePath: String?, isPaused: @MainActor () -> Bool) async {
        for path in tierMembers(tier, activePath: activePath) {
            guard !isPaused() else { return }
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

    /// One repo's badge refresh. An open breaker downgrades a fetching sync
    /// to a local one rather than skipping it (the Tauri client's exact
    /// fallback), and only real fetch attempts against a real remote feed the
    /// breaker — a repo with no remote says nothing about connectivity.
    private func sync(_ path: String, fetching: Bool) async {
        guard !inFlight.contains(path) else { return }
        inFlight.insert(path)
        defer { inFlight.remove(path) }

        let fetch = fetching && breaker.shouldAttempt
        guard let summary = try? await GitBridge.syncSummary(of: path, fetching: fetch) else {
            return
        }
        syncByPath[path] = summary
        if fetch, summary.hasRemote {
            breaker.record(success: summary.fetched)
        }
    }
}
