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

    /// Whether this repository's first `git log` has landed.
    ///
    /// `commits.isEmpty` cannot answer *"has this repository no commits?"* on
    /// its own, because it is equally true of a repository whose log is still
    /// in flight — and the History pane asserts one of those two out loud. So
    /// the empty views wait for this rather than reading the array, and a repo
    /// that does have history never flashes *No commits yet* on the way in.
    ///
    /// Reset by `open()` alone: a `refresh()` re-reads a repository whose log
    /// has already been answered once, and blanking the flag there would put
    /// the flash back on every ⌘R.
    private(set) var historyLoaded = false

    /// Whether a merge is in progress (`MERGE_HEAD` exists) — drives the
    /// branch chip's `· merging` suffix and the branch menu's Abort Merge item.
    ///
    /// Read straight off the status rather than asked for separately: every
    /// refresh path needs it, one of them used to forget, and core answers it
    /// from a file check that costs the poll nothing.
    var isMerging: Bool { status?.merging ?? false }

    /// The dismissable banner line: an explicit read that failed, or something
    /// the app handed to the OS that didn't take. Cleared by its ✕ and by the
    /// next explicit load.
    ///
    /// A separate field from `pollFailure`, and not one slot shared with it:
    /// the two answer different questions, and multiplexing them let whichever
    /// arrived first silence the other. It was the wrong way round, too — the
    /// poll only ever wrote into a *free* slot, so a dismissable "couldn't
    /// reveal the file" suppressed "this repository has stopped being
    /// readable" for as long as it stood, which is the more urgent of the two
    /// and the one the user cannot make go away by fixing anything. The Tauri
    /// client keeps `notice` and `pollError` apart for the same reason.
    var errorMessage: String?

    /// Consecutive silent-refresh failures. One is usually a transient lock
    /// mid-write and stays invisible, but a streak means the repository is
    /// genuinely unreadable (deleted, permissions, corrupted), so the poll
    /// surfaces it — the error path the toolbar Refresh button used to be
    /// the only way to reach.
    private var quietFailureStreak = 0
    private static let quietFailureThreshold = 3

    /// The poll's own banner: this repository has stopped being readable.
    ///
    /// No ✕, and `private(set)` so nothing outside can fake one: its recovery
    /// retires it, and a dismissal would hide a repository that is still
    /// unreadable. Shown above `errorMessage` when both stand, which is the
    /// order the Tauri strip stacks them in.
    private(set) var pollFailure: String?

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

    /// Bumped by every `open(at:)`, at the moment it is *asked for*.
    ///
    /// `loadDepth` counts how many reads are running; this says which
    /// repository the app is trying to be on, which is the different question
    /// two overlapping opens need answered. Every publish below is gated on
    /// it, so a read that started before a switch stands down instead of
    /// landing one repository's status and history against another's path —
    /// and because it is claimed on entry rather than when a read resolves,
    /// the repository that wins is the one the user asked for last, not the
    /// one whose `git log` happened to finish first.
    private var openGeneration = 0

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
    ///
    /// The outcome distinguishes the three ways this ends, because a caller
    /// acts on them differently: `.superseded` is not a failure and has no
    /// message, and treating it as one would report an error about a
    /// repository the user has already navigated away from.
    @discardableResult
    func open(at url: URL) async -> OpenOutcome {
        openGeneration += 1
        let generation = openGeneration
        beginLoad()
        defer { finishLoad() }

        do {
            let root = try await GitBridge.repoRoot(of: url.path(percentEncoded: false))
            // Read the name before publishing anything rather than after
            // `repoPath`: it is a basename, so it costs nothing to wait for,
            // and it means this open's first write is also its first
            // observable effect — everything above the guard can be abandoned.
            let name = await GitBridge.name(of: root)
            guard generation == openGeneration else { return .superseded }
            repoPath = root
            repoName = name
            errorMessage = nil
            // Drop the previous repository's history *with* its path, not when
            // the new log happens to land. `repoPath` is published above and
            // the History pane re-seeds its selection from `commits`, so
            // leaving the old array in place hands the detail pane a sha from
            // one repository to load against another's path — and a new repo
            // whose log then fails would keep showing the old one's commits
            // indefinitely. Cleared here, the pane reads "still loading"
            // (`historyLoaded`) for exactly the window it is true.
            commits = []
            hasMoreHistory = true
            historyLoaded = false
            // A fresh repository starts at page one, whatever depth the
            // previous one had been scrolled to.
            await loadRepoData(root, historyLimit: Self.historyPageSize)
            return generation == openGeneration ? .opened : .superseded
        } catch {
            guard generation == openGeneration else { return .superseded }
            // Leave any previously open repo intact — a failed open should not
            // blank out what the user was already looking at.
            errorMessage = error.displayMessage
            return .failed
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
    /// failure to re-read is theirs to see rather than one tick of a streak.
    /// The file a discard rewrote may be the one the diff pane is showing, and
    /// its status *letters* read the same before and after — what tells the
    /// pane to look again is `FileEntry.statStamp`, which the rewrite moved.
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
            errorMessage = nil
        } catch {
            errorMessage = error.displayMessage
        }
        // This read has just asked the repository directly, so the poll's
        // streak and its banner both describe a question already answered —
        // either it succeeded, or `errorMessage` above now carries the same
        // news in the same place and two lines would say one thing twice.
        quietFailureStreak = 0
        pollFailure = nil
        return false
    }

    /// The background poll's tick — the silent counterpart of `refresh()`,
    /// mirroring the Tauri client's 2 s loop: no `isLoading` (the progress
    /// bar must not flash every 2 s), one-off failures swallowed
    /// (`errorMessage` stays whatever the last real action left — only a
    /// failure *streak* surfaces a banner, and only the poll's recovery
    /// clears it), history refetched only when HEAD actually moved (how
    /// commits made in an outside terminal appear), and the status published
    /// only when it changed — so an idle tick repaints nothing.
    ///
    /// This is also the refocus path, and it needs no forcing flag: a file
    /// edited on disk while the app was away comes back with a moved
    /// `FileEntry.statStamp`, which makes the status differ and re-keys the
    /// open diff on its own.
    func refreshQuietly() async {
        guard let repoPath else { return }
        let newStatus: RepoStatus
        do {
            newStatus = try await GitBridge.status(of: repoPath)
        } catch {
            quietFailureStreak += 1
            if quietFailureStreak >= Self.quietFailureThreshold {
                pollFailure = error.displayMessage
            }
            return
        }
        quietFailureStreak = 0
        // The repo is readable again; retire the poll's own banner.
        pollFailure = nil

        let headMoved = newStatus.headSha != status?.headSha
        if newStatus != status {
            status = newStatus
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
        let generation = openGeneration
        async let statusResult = GitBridge.status(of: path)
        async let logResult = GitBridge.log(of: path, limit: historyLimit)

        do {
            let (newStatus, newCommits) = try await (statusResult, logResult)
            // A repository switch started while these two reads were in
            // flight, so `path` is no longer the open repository and this
            // answer belongs to nobody. The switch runs its own load; leaving
            // the tail below unrun is deliberate, since that load owns the
            // banner and the failure streak now.
            guard generation == openGeneration else { return }
            status = newStatus
            commits = newCommits
            hasMoreHistory = newCommits.count == Int(historyLimit)
            historyLoaded = true
            errorMessage = nil
        } catch {
            guard generation == openGeneration else { return }
            errorMessage = error.displayMessage
        }
        // The streak and the poll's banner describe one repository's
        // readability, and this load may be a *different* repository: carrying
        // either over would let the previous repo's failures speak for this
        // one. Retired whatever the outcome — on a failure `errorMessage`
        // above already says so, in the same place.
        quietFailureStreak = 0
        pollFailure = nil
    }
}
