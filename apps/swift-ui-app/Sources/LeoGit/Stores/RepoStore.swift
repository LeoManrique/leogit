import Foundation

/// Identity for the generated record types, so SwiftUI can diff rows.
///
/// A SHA identifies a commit; a path identifies a working-tree entry within one
/// status snapshot. Declared here rather than in the bindings because
/// `ffi/generated/` is rebuilt from Rust on every build and must stay untouched.
extension CommitInfo: Identifiable {
    public var id: String { sha }
}

extension FileEntry: Identifiable {
    public var id: String { path }
}

/// Observable state for the open repository.
///
/// Owns the whole read path: pick a folder → resolve its repo root → load status
/// and history. All mutation happens on the main actor; only the blocking git
/// work hops off it, inside `GitBridge`.
@MainActor
@Observable
final class RepoStore {
    /// How many commits each history page loads; the list appends another
    /// page whenever its last row scrolls into view.
    private static let historyPageSize: Int32 = 100

    /// How deep a refresh re-reads. A reload keeps the depth the user has
    /// scrolled to, but capped — the Tauri client's `MAX_COMMITS` — so a
    /// head move while thousands of rows are loaded doesn't re-fetch them
    /// all; scrolling re-grows the list on demand.
    private static let historyRefreshCap = 500

    private(set) var repoPath: String?
    private(set) var repoName = ""
    private(set) var status: RepoStatus?
    private(set) var commits: [CommitInfo] = []
    private(set) var isLoading = false
    private(set) var coreVersionText = ""

    /// False once a fetch returned fewer commits than requested — the end of
    /// the repository's history.
    private(set) var hasMoreHistory = true
    private var isLoadingMoreHistory = false

    /// Whether a merge is in progress (`MERGE_HEAD` exists) — drives the
    /// subtitle badge and the branch menu's Abort Merge item.
    ///
    /// Read straight off the status rather than asked for separately: every
    /// refresh path needs it, one of them used to forget, and core answers it
    /// from a file check that costs the poll nothing.
    var isMerging: Bool { status?.merging ?? false }

    /// One meaning, exactly: *the working tree may differ from what any
    /// derived view shows — re-derive if you care.* Bumped when the status
    /// changed, on explicit refresh, and on refocus (`forceDiffReload`) —
    /// deliberately not narrower: `git status` cannot tell whether an open
    /// file's *diff content* changed when its row looks identical
    /// (modified → still modified), and any row-comparison heuristic here
    /// would reintroduce the Tauri client's staleness bug (its poll never
    /// reloads the open diff, so terminal edits go stale until reselect).
    /// This store signals possibility; `DiffStore`'s equality skip is where
    /// reality is checked, so a bump for an unchanged file costs one
    /// subprocess and zero repaints.
    private(set) var workingTreeEpoch = 0

    /// Set when the last operation failed; surfaced as a banner and cleared on
    /// the next successful load.
    var errorMessage: String?

    /// Consecutive silent-refresh failures. One is usually a transient lock
    /// mid-write and stays invisible, but a streak means the repository is
    /// genuinely unreadable (deleted, permissions, corrupted), so the poll
    /// surfaces it — the error path the toolbar Refresh button used to be
    /// the only way to reach.
    private var quietFailureStreak = 0
    private static let quietFailureThreshold = 3

    /// Whether the current `errorMessage` came from the poll, so only the
    /// poll's own recovery clears it — an explicit action's failure text is
    /// never silently swept away by a background tick.
    private var errorSurfacedByPoll = false

    /// Whether the banner may offer a ✕.
    ///
    /// The poll's own banner has none: its recovery retires it, and a ✕ would
    /// hide a repository that is still unreadable. Nothing retires the other
    /// kind, so it needs one — the Tauri strip splits on the same line.
    var canDismissError: Bool { errorMessage != nil && !errorSurfacedByPoll }

    var isRepoOpen: Bool { repoPath != nil }

    /// Everyone suspended in `awaitLoadSettled()`. Main-actor-isolated, so
    /// append and resume never race.
    private var loadWaiters: [CheckedContinuation<Void, Never>] = []

    /// How many explicit loads are in flight. `isLoading` is the flag the
    /// progress bar reads and would be enough for it; the *count* is what says
    /// when the waiters below are actually settled. `refresh()` can overlap
    /// `open()` — a branch action's `onWorkingTreeChanged`, a clone handing
    /// its path straight to `open` — and a Bool would let the inner one's exit
    /// release everyone while the outer load still has no status.
    private var loadDepth = 0

    /// Suspend until no explicit load is in flight.
    ///
    /// `open()` publishes `repoPath` *before* it has status, so the
    /// `.task(id: repoPath)` chain it starts can run against a `nil` status.
    /// Anything whose decision depends on the repository's status — the
    /// warm-up fetch's no-remote gate — has to wait for it rather than read
    /// `nil` and guess, or the gate silently stops applying whenever the load
    /// happens to be slower than its caller.
    ///
    /// Not cancellation-aware, deliberately: a cancelled waiter is one whose
    /// `.task(id: repoPath)` was torn down by a repo switch, and a switch
    /// *is* an `open()`, so the next `finishLoad()` resumes it — where its
    /// caller's own `repoPath` check drops it.
    func awaitLoadSettled() async {
        guard loadDepth > 0 else { return }
        await withCheckedContinuation { loadWaiters.append($0) }
    }

    /// Whether any explicit read is in flight — the *lock*, as distinct from
    /// `isLoading`, which is the progress bar.
    ///
    /// The status poll and ⌘R both refuse to run while one is, and they have to
    /// keep refusing for a read that deliberately shows no progress: a poll
    /// that starts before an action's own re-read and resolves after it lands
    /// its pre-action snapshot on top, which on a slow `git status` puts
    /// discarded files back in the list.
    var isBusy: Bool { loadDepth > 0 }

    /// Claim a load. Paired with `finishLoad()` on every exit path.
    ///
    /// `showsProgress: false` claims the lock without the bar, for a read the
    /// user did not ask to watch — see `refreshWorkingTree()`.
    private func beginLoad(showsProgress: Bool = true) {
        loadDepth += 1
        if showsProgress { isLoading = true }
    }

    /// Release the claim, and — once the last one is gone — everyone waiting
    /// on it. Called on every exit from an explicit load, including the
    /// failing ones: a repo that couldn't be read has still settled.
    private func finishLoad() {
        loadDepth -= 1
        guard loadDepth == 0 else { return }
        isLoading = false
        let waiters = loadWaiters
        loadWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }

    /// Open the repository containing `url`, then load its status and history.
    ///
    /// Accepts a subdirectory as well as a repository root, matching the
    /// `leogit <path>` CLI behaviour.
    func open(at url: URL) async {
        beginLoad()
        defer { finishLoad() }

        do {
            let root = try await GitBridge.repoRoot(of: url.path(percentEncoded: false))
            repoPath = root
            repoName = await GitBridge.name(of: root)
            errorMessage = nil
            // A fresh repository starts at page one, whatever depth the
            // previous one had been scrolled to.
            await loadRepoData(root, historyLimit: Self.historyPageSize)
        } catch {
            // Leave any previously open repo intact — a failed open should not
            // blank out what the user was already looking at.
            errorMessage = error.displayMessage
        }
    }

    /// Re-read status and history for the already-open repository.
    func refresh() async {
        guard let repoPath else { return }
        beginLoad()
        defer { finishLoad() }
        await loadRepoData(repoPath, historyLimit: currentHistoryLimit)
    }

    /// Re-read *only* the status, after an action that changed the working tree
    /// but cannot have moved `HEAD` — discarding a file, adding one to
    /// `.gitignore`.
    ///
    /// The full `refresh()` would re-run `git log` at up to 500 commits and
    /// flash the progress bar for an answer that is already on screen: history
    /// is what a working-tree edit does not touch. The Tauri client has always
    /// done a status-only refresh here.
    ///
    /// Not `refreshQuietly()`: this *is* the user's action completing, so a
    /// failure to re-read is theirs to see rather than one tick of a streak,
    /// and a successful read bumps the epoch whether or not the status value
    /// moved — the file the discard rewrote may be the one the diff pane is
    /// showing, and a status row that reads the same before and after says
    /// nothing about its contents.
    ///
    /// **A moved `HEAD` hands over to the full reload.** This action cannot
    /// have moved it, but a commit made in a terminal since the last tick can
    /// have, and `head_sha` is an *edge* two other things watch — the poll's
    /// history refetch and the branch reload beside it. Writing the new status
    /// here would consume that edge without doing either one's work, stranding
    /// the History list and the branch menu until `HEAD` happened to move
    /// again. Rare, and the extra `git status` it costs is what a moved `HEAD`
    /// was going to cost anyway.
    ///
    /// Returns whether `HEAD` had moved, because the *branch* list is the third
    /// thing that edge feeds and it lives in another store — the caller reloads
    /// it, exactly as the poll does.
    @discardableResult
    func refreshWorkingTree() async -> Bool {
        guard let repoPath else { return false }
        beginLoad(showsProgress: false)
        defer { finishLoad() }
        do {
            let newStatus = try await GitBridge.status(of: repoPath)
            guard newStatus.headSha == status?.headSha else {
                await loadRepoData(repoPath, historyLimit: currentHistoryLimit)
                return true
            }
            if newStatus != status { status = newStatus }
            workingTreeEpoch += 1
            errorMessage = nil
        } catch {
            errorMessage = error.displayMessage
        }
        // The banner now reflects an explicit action, so the poll may not
        // clear it — and its streak describes a repository this read has just
        // proved readable (or not) on its own.
        errorSurfacedByPoll = false
        quietFailureStreak = 0
        return false
    }

    /// The background poll's tick — the silent counterpart of `refresh()`,
    /// mirroring the Tauri client's 2 s loop: no `isLoading` (the progress
    /// bar must not flash every 2 s), one-off failures swallowed
    /// (`errorMessage` stays whatever the last real action left — only a
    /// failure *streak* surfaces a banner, and only the poll's recovery
    /// clears it), history refetched only when HEAD actually moved (how
    /// commits made in an outside terminal appear), and `workingTreeEpoch` bumped
    /// only when the status changed — so an idle tick never makes the open
    /// diff reload.
    ///
    /// `forceDiffReload` is the refocus path: files may have been edited on
    /// disk without their status rows changing, so coming back to the app
    /// re-reads the open diff unconditionally, like the Tauri client's
    /// `reloadActiveDiff` on focus.
    func refreshQuietly(forceDiffReload: Bool = false) async {
        guard let repoPath else { return }
        let newStatus: RepoStatus
        do {
            newStatus = try await GitBridge.status(of: repoPath)
        } catch {
            quietFailureStreak += 1
            if quietFailureStreak >= Self.quietFailureThreshold, errorMessage == nil {
                errorMessage = error.displayMessage
                errorSurfacedByPoll = true
            }
            return
        }
        quietFailureStreak = 0
        if errorSurfacedByPoll {
            // The repo is readable again; retire the poll's own banner.
            errorMessage = nil
            errorSurfacedByPoll = false
        }

        let headMoved = newStatus.headSha != status?.headSha
        let statusChanged = newStatus != status
        if statusChanged {
            status = newStatus
        }
        if statusChanged || forceDiffReload {
            workingTreeEpoch += 1
        }
        if headMoved {
            let limit = currentHistoryLimit
            if let newCommits = try? await GitBridge.log(of: repoPath, limit: limit) {
                commits = newCommits
                hasMoreHistory = newCommits.count == Int(limit)
            }
        }
    }

    /// Append the next page of history — the commit list's reaching its last
    /// row calls this. In-flight and end-of-history guarded, so `onAppear`
    /// can fire it freely.
    func loadMoreHistory() async {
        guard let repoPath, hasMoreHistory, !isLoadingMoreHistory else { return }
        isLoadingMoreHistory = true
        defer { isLoadingMoreHistory = false }

        guard
            let page = try? await GitBridge.log(
                of: repoPath,
                limit: Self.historyPageSize,
                skip: Int32(commits.count)
            )
        else { return }
        // The 2 s poll can slide the window under a page in flight; keying
        // out already-known shas keeps every row's identity unique.
        let known = Set(commits.map(\.sha))
        commits += page.filter { !known.contains($0.sha) }
        hasMoreHistory = page.count == Int(Self.historyPageSize)
    }

    /// The depth a reload should keep: what the user has scrolled to, floored
    /// at one page and capped at the refresh limit.
    private var currentHistoryLimit: Int32 {
        Int32(min(max(commits.count, Int(Self.historyPageSize)), Self.historyRefreshCap))
    }

    /// Report which Rust build the UI is linked against.
    func loadCoreVersion() async {
        coreVersionText = await GitBridge.version()
    }

    /// Status and history are independent reads, so run them concurrently and
    /// let each report its own failure.
    private func loadRepoData(_ path: String, historyLimit: Int32) async {
        async let statusResult = GitBridge.status(of: path)
        async let logResult = GitBridge.log(of: path, limit: historyLimit)

        do {
            let (newStatus, newCommits) = try await (statusResult, logResult)
            status = newStatus
            commits = newCommits
            hasMoreHistory = newCommits.count == Int(historyLimit)
            workingTreeEpoch += 1
            errorMessage = nil
        } catch {
            errorMessage = error.displayMessage
        }
        // Whatever the outcome, the banner now reflects an explicit load,
        // not the poll — the poll must not clear another action's failure.
        errorSurfacedByPoll = false
        // The streak describes one repository's readability, and this load may
        // be a different repository: carrying it over would let two failures on
        // the previous repo plus one here raise a banner about this one.
        quietFailureStreak = 0
    }
}
