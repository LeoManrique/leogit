import Foundation

/// The single point where Swift calls into Rust.
///
/// Every `leogit-core` function is blocking — it shells out to `git` and waits —
/// so each wrapper is marked `@concurrent` (SE-0461). Under the target's
/// approachable-concurrency settings a plain `nonisolated async` function would
/// inherit the *caller's* executor and therefore block the main actor; the
/// attribute is the explicit opt-in that pushes the work onto the global
/// concurrent executor instead.
///
/// Wrappers are deliberately named differently from the generated free functions
/// (`repoRoot(of:)` vs. `resolveRepoRoot(path:)`) so there is never an ambiguity
/// between the bridge and the bindings it wraps — the generated Swift is compiled
/// into this same module, so both live in one namespace.
enum GitBridge {
    // MARK: - Bootstrap

    /// Replace this process's `PATH` with the user's interactive login
    /// `PATH`, so spawned tools (`git`, `gh`, the `claude` CLI) resolve when
    /// the app is launched from Finder rather than a terminal. Must be the
    /// app's first Rust call, made in `App.init` before any other thread
    /// could be reading the environment — the same contract as the Tauri
    /// host, which calls it at the top of `main`. Deliberately synchronous
    /// for exactly that reason.
    static func bootstrapPathEnvironment() {
        fixPathEnv()
    }

    /// Resolve any path inside a repository to that repository's root.
    @concurrent
    static func repoRoot(of path: String) async throws -> String {
        try resolveRepoRoot(path: path)
    }

    /// Directory name shown as the repository's title.
    @concurrent
    static func name(of path: String) async -> String {
        repoDisplayName(path: path)
    }

    /// Branch metadata plus the working-tree file list.
    @concurrent
    static func status(of repoPath: String) async throws -> RepoStatus {
        try getStatus(repoPath: repoPath)
    }

    /// A page of history, newest first.
    @concurrent
    static func log(of repoPath: String, limit: Int32, skip: Int32 = 0) async throws -> [CommitInfo] {
        try getLog(repoPath: repoPath, options: LogOptions(maxCount: limit, skip: skip))
    }

    /// Version of the linked Rust bridge — proves which build the UI is talking to.
    @concurrent
    static func version() async -> String {
        coreVersion()
    }

    /// Raw unified diff for one working-tree file: `HEAD` against the working
    /// tree, staged and unstaged combined. Untracked files diff against
    /// `/dev/null`, so a brand-new file still yields hunks.
    @concurrent
    static func rawDiff(of repoPath: String, for file: FileEntry) async throws -> String {
        try getDiff(repoPath: repoPath, file: file)
    }

    /// Structure a raw diff into hunks of typed lines, or `nil` when there is
    /// nothing textual to show.
    @concurrent
    static func parsedDiff(from raw: String) async -> DiffPayload? {
        parseDiff(raw: raw)
    }

    /// Syntax tokens for a parsed diff — one entry per flattened line, empty
    /// where the tokenizer has nothing to say. `source` lets the tokenizer
    /// read complete blobs so multi-line constructs stay correct.
    @concurrent
    static func diffTokens(for fileDiff: FileDiff, source: BlobSource?) async -> [[Token]] {
        tokenizeDiff(fileDiff: fileDiff, source: source)
    }

    /// The full commit message: summary, optional description, and
    /// `Co-authored-by` trailers, joined the way `commitChanges` expects.
    @concurrent
    static func commitMessage(summary: String, description: String, coAuthors: [String]) async -> String {
        formatCommitMessage(summary: summary, description: description, coAuthors: coAuthors)
    }

    /// Commit exactly `files` with `message`, regardless of prior index state:
    /// core resets the index and re-stages the given files itself, so there
    /// is no separate staging step. With `amend`, an empty file list is a
    /// message-only amend.
    @concurrent
    static func commitChanges(
        in repoPath: String,
        message: String,
        files: [FileEntry],
        amend: Bool = false
    ) async throws {
        try commit(repoPath: repoPath, message: message, files: files, amend: amend)
    }

    // MARK: - Branches

    /// Local and remote branches in one flat list, most recent commit first.
    /// Remote entries use their short form (`origin/feature`).
    @concurrent
    static func branches(in repoPath: String) async throws -> [BranchInfo] {
        try listBranches(repoPath: repoPath)
    }

    /// Create `name` off `HEAD` without checking it out; callers chain
    /// `checkout` so "New Branch" lands the user on it, exactly like the
    /// Tauri client.
    @concurrent
    static func newBranch(in repoPath: String, named name: String) async throws {
        try createBranch(repoPath: repoPath, name: name, startPoint: "")
    }

    /// Check out `branch`. A remote-only name (`origin/feature`) becomes a
    /// local tracking branch instead of detaching HEAD; a dirty working tree
    /// is git's call — its refusal is surfaced verbatim.
    @concurrent
    static func checkout(in repoPath: String, branch: String) async throws {
        try switchBranch(repoPath: repoPath, branch: branch)
    }

    /// Delete a local branch — always forced (`git branch -D`), so the UI
    /// owns the confirmation.
    @concurrent
    static func removeBranch(in repoPath: String, named name: String) async throws {
        try deleteBranch(repoPath: repoPath, name: name)
    }

    // MARK: - Merge

    /// Merge `branch` into the current branch. A conflict is data, not a
    /// thrown error: `success == false`, git's text in `errorMessage`, and
    /// the conflicted paths listed.
    @concurrent
    static func merge(in repoPath: String, branch: String) async throws -> MergeResult {
        try mergeBranch(repoPath: repoPath, branch: branch)
    }

    /// Stage `branch`'s combined changes (`git merge --squash`) without
    /// committing; `commitSquash` completes the flow.
    @concurrent
    static func squashMerge(in repoPath: String, branch: String) async throws -> MergeResult {
        try mergeSquash(repoPath: repoPath, branch: branch)
    }

    /// Commit a successful squash merge with git's auto-generated
    /// "Squashed commit of the following:" message.
    @concurrent
    static func commitSquash(in repoPath: String) async throws {
        try commitSquashMerge(repoPath: repoPath)
    }

    /// Abort an in-progress merge, restoring the pre-merge working tree.
    @concurrent
    static func abortMerge(in repoPath: String) async throws {
        try mergeAbort(repoPath: repoPath)
    }

    /// Whether a merge is in progress (`MERGE_HEAD` exists) — drives the
    /// merging indicator and the Abort Merge affordance.
    @concurrent
    static func mergeInProgress(in repoPath: String) async throws -> Bool {
        try isMerging(repoPath: repoPath)
    }

    /// How many commits merging `branch` would bring in — the merge sheet's
    /// preview number.
    @concurrent
    static func commitsToMerge(in repoPath: String, from branch: String) async throws -> Int32 {
        try countCommitsToMerge(repoPath: repoPath, targetBranch: branch)
    }

    // MARK: - Sync

    /// The repository's first remote name, or the literal "origin" when none
    /// is configured. Resolved immediately before each network operation,
    /// never cached — matching the Tauri handlers.
    @concurrent
    static func remoteName(in repoPath: String) async throws -> String {
        try getRemote(repoPath: repoPath)
    }

    /// `git fetch --prune`: refresh remote-tracking refs — and the
    /// ahead/behind counts derived from them — without touching the working
    /// tree. Fetch streams no progress; core's fetch path has no sink.
    @concurrent
    static func fetchRemote(in repoPath: String, remote: String) async throws {
        try await fetch(repoPath: repoPath, remote: remote)
    }

    /// `git pull --ff --progress`. Fast-forward only: a diverged branch fails
    /// with git's own message instead of merging or rebasing. Progress ticks
    /// arrive on a Rust background thread — `onProgress` must hop to whatever
    /// isolation it needs.
    @concurrent
    static func pullRemote(
        in repoPath: String,
        remote: String,
        onProgress: @escaping @Sendable (SyncProgress) -> Void
    ) async throws {
        try await pull(repoPath: repoPath, remote: remote, listener: ProgressRelay(onProgress))
    }

    /// `git push --progress [--set-upstream] [--force-with-lease]`.
    /// `setUpstream` must be `!status.hasUpstream` — that flag is only true
    /// when real tracking configuration exists, and a first push without
    /// `--set-upstream` leaves the branch permanently untracked. With-lease
    /// is the only force mode core offers; there is no bare `--force`.
    @concurrent
    static func pushRemote(
        in repoPath: String,
        remote: String,
        branch: String,
        setUpstream: Bool,
        forceWithLease: Bool,
        onProgress: @escaping @Sendable (SyncProgress) -> Void
    ) async throws {
        try await push(
            repoPath: repoPath,
            remote: remote,
            branch: branch,
            setUpstream: setUpstream,
            forceWithLease: forceWithLease,
            listener: ProgressRelay(onProgress)
        )
    }

    // MARK: - Clone

    /// `git clone --progress` into `targetPath` — the FULL destination
    /// (parent plus a folder name the caller derives from the URL), never a
    /// parent directory. Core expands `~`, refuses an existing path, and
    /// creates the parent. Returns the absolute path of the fresh clone,
    /// ready to open. Progress ticks arrive on a Rust background thread,
    /// exactly like pull/push.
    @concurrent
    static func cloneRepository(
        url: String,
        into targetPath: String,
        onProgress: @escaping @Sendable (SyncProgress) -> Void
    ) async throws -> String {
        try await cloneRepo(url: url, targetPath: targetPath, listener: ProgressRelay(onProgress))
    }

    /// The signed-in user's GitHub repositories via the `gh` CLI, most
    /// recently pushed first — the Clone sheet's GitHub tab. Failures carry
    /// a dialog-ready message (gh missing / unauthenticated / timed out).
    @concurrent
    static func githubRepositories(limit: UInt32) async throws -> [GhRepo] {
        try await ghRepoList(limit: limit)
    }

    /// Clone `owner/name` through the GitHub CLI, whose stored auth covers
    /// private repositories without a prompt. No progress stream: `gh repo
    /// clone` reports nothing parseable, so callers show an indeterminate
    /// bar — the Tauri dialog does the same.
    @concurrent
    static func githubClone(nameWithOwner: String, into targetPath: String) async throws -> String {
        try await ghClone(nameWithOwner: nameWithOwner, targetPath: targetPath)
    }

    /// Persist the folder the user last cloned into — the parent directory,
    /// not the repo path — so the next Clone sheet (in either client)
    /// pre-fills it.
    @concurrent
    static func setLastCloneDir(_ dir: String) async throws {
        _ = try patchState(
            patch: ReposStatePatch(
                lastOpenedRepo: nil,
                lastCloneDir: dir,
                repoSortMode: nil,
                cloneSortMode: nil
            )
        )
    }

    /// Persist the GitHub tab's sort toggle (`"recent"` | `"name"`) — the
    /// same state the Tauri dialog's toggle round-trips.
    @concurrent
    static func setCloneSortMode(_ mode: String) async throws {
        _ = try patchState(
            patch: ReposStatePatch(
                lastOpenedRepo: nil,
                lastCloneDir: nil,
                repoSortMode: nil,
                cloneSortMode: mode
            )
        )
    }

    // MARK: - Settings

    /// Persist the whole configuration file. Callers must load fresh, apply
    /// their edits, and save — core rewrites the entire file, so saving a
    /// `Config` loaded before the user started editing would clobber
    /// changes the other client made in between.
    @concurrent
    static func saveAppConfig(_ config: Config) async throws {
        try saveConfig(config: config)
    }

    /// The launchable shells on this machine, best first — the Settings
    /// shell picker's rows. Probe-based (every row's executable exists) and
    /// never empty.
    @concurrent
    static func shellOptions() async -> [ShellOption] {
        listShells()
    }

    // MARK: - AI commit message

    /// The combined unified diff of exactly `files` — the input Generate
    /// hands to the AI provider. An empty selection yields an empty string,
    /// which `generateMessage` then rejects as "no files selected".
    @concurrent
    static func selectedDiff(in repoPath: String, files: [FileEntry]) async throws -> String {
        try getSelectedDiff(repoPath: repoPath, files: files)
    }

    /// The AI settings from the shared `~/.config/leogit/config.toml`, via
    /// the same config→provider mapping the Tauri client performs before
    /// every generate call. Read fresh each time, never cached, so an edit
    /// to the file — or a save from the Tauri client — takes effect on the
    /// next use.
    @concurrent
    static func aiConfig() async throws -> AiProviderConfig {
        try loadAiConfig()
    }

    /// Persist the provider picker's choice (`"claude"` | `"ollama"`) into
    /// the shared config file, leaving every other setting untouched.
    @concurrent
    static func setAIProvider(_ provider: String) async throws {
        try saveAiProvider(provider: provider)
    }

    /// Generate a commit message from `diff` via `config.provider` — the
    /// local `claude` CLI or a self-hosted Ollama instance. Plain
    /// request/response bounded by core's 120 s timeout; there is no
    /// streaming and no cancel, matching the Tauri client.
    @concurrent
    static func generateMessage(diff: String, config: AiProviderConfig) async throws -> CommitMessage {
        try await generateCommitMessage(diff: diff, provider: config.provider, config: config)
    }

    // MARK: - Repo directory & background refresh

    /// The whole shared configuration file, read fresh — the auto-fetch loop
    /// takes `autoFetch`/`fetchIntervalMs` from it, discovery takes
    /// `scanPaths`/`scanDepth`. Re-read on every repo switch, like the Tauri
    /// client, so edits from either client apply without a restart.
    @concurrent
    static func appConfig() async throws -> Config {
        try loadConfig()
    }

    /// The shared repos-state file: the repo to restore on launch plus the
    /// most-recently-opened list.
    @concurrent
    static func reposState() async throws -> ReposState {
        try loadState()
    }

    /// Persist `repoPath` as the repo to restore on the next launch — of
    /// either client; the state file is shared with the Tauri app.
    @concurrent
    static func setLastOpened(repoPath: String) async throws {
        _ = try patchState(
            patch: ReposStatePatch(
                lastOpenedRepo: repoPath,
                lastCloneDir: nil,
                repoSortMode: nil,
                cloneSortMode: nil
            )
        )
    }

    /// Move `repoPath` to the front of the most-recently-used list (core
    /// de-dupes and caps it) and return the updated state.
    @concurrent
    @discardableResult
    static func recordRecent(repoPath: String) async throws -> ReposState {
        try recordRecentRepo(path: repoPath)
    }

    /// Walk the configured scan folders for git repositories — a pure
    /// filesystem walk, no git subprocesses. An empty list falls back to
    /// core's default folders.
    @concurrent
    static func discoverRepositories(scanPaths: [String], depth: UInt32) async throws -> [String] {
        try discoverRepos(scanPaths: scanPaths, maxDepth: depth)
    }

    /// The folders discovery would actually walk, tilde-expanded — for the
    /// picker's "we looked here" empty state.
    @concurrent
    static func scanFolders(for scanPaths: [String]) async -> [String] {
        effectiveScanPaths(scanPaths: scanPaths)
    }

    /// Per-repo badge summary: dirty / ahead / behind. With `fetching`, the
    /// remote-tracking refs refresh first under core's short background
    /// timeouts (12 s cap); a failed fetch is reported in `fetched`, never
    /// thrown, and stale counts still come back.
    @concurrent
    static func syncSummary(of repoPath: String, fetching: Bool) async throws -> RepoSync {
        try await repoSyncStatus(repoPath: repoPath, doFetch: fetching)
    }

    // MARK: - Embedded terminal

    /// Spawn a shell in a fresh PTY at the repository root, 80×24 until the
    /// first resize. `shellID` is the shared config's `terminal_shell`; nil
    /// or an uninstalled id resolves to the best shell on this machine. The
    /// listener's callbacks arrive on the session's Rust reader thread —
    /// never the main one — and its `onClosed` is the only end-of-session
    /// signal, for a self-exiting shell and a `killTerminal` alike.
    @concurrent
    static func launchTerminal(
        in repoPath: String,
        shellID: String?,
        listener: TerminalEventListener
    ) async throws -> StartedTerminal {
        try startTerminal(listener: listener, repoPath: repoPath, shellId: shellID)
    }

    /// Write keystrokes (or a paste) to the session's PTY. Blocking, and
    /// deliberately synchronous: input must reach the shell in typing order,
    /// which the caller's serial I/O queue guarantees and unordered `Task`s
    /// would not. Never call on the main actor.
    static func sendTerminalInput(pid: UInt32, data: String) throws {
        try writeTerminal(pid: pid, data: data)
    }

    /// Propagate the emulator's grid size to the PTY (`SIGWINCH`s the
    /// child). Blocking; terminal I/O queue only.
    static func resizeTerminalGrid(pid: UInt32, cols: UInt16, rows: UInt16) throws {
        try resizeTerminal(pid: pid, cols: cols, rows: rows)
    }

    /// Kill the session's child. The end of the session is the listener's
    /// `onClosed` — emitted once the reader thread sees EOF — never this
    /// return. Blocking; terminal I/O queue only.
    static func killTerminal(pid: UInt32) throws {
        try closeTerminal(pid: pid)
    }
}

/// Bridges the generated `SyncProgressListener` callback protocol to a plain
/// closure. Rust invokes `onProgress` from core's stderr-reader thread — never
/// the main one — which the protocol encodes by requiring `Sendable`.
private final class ProgressRelay: SyncProgressListener {
    private let deliver: @Sendable (SyncProgress) -> Void

    init(_ deliver: @escaping @Sendable (SyncProgress) -> Void) {
        self.deliver = deliver
    }

    func onProgress(progress: SyncProgress) {
        deliver(progress)
    }
}

extension GitError {
    /// The failure text core produced.
    ///
    /// UniFFI synthesises `errorDescription` as `String(reflecting:)`, which
    /// renders as `GitError.Failed(message: "…")` — debugger output, not
    /// something to show a user. Core's message is already human-readable, so
    /// unwrap it directly.
    var message: String {
        switch self {
        case let .Failed(message): message
        }
    }
}

extension Error {
    /// Display text for any error surfaced by the bridge.
    var displayMessage: String {
        (self as? GitError)?.message ?? localizedDescription
    }
}
