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

    /// Where a `leogit <dir>` invocation points, or `nil` when the arguments
    /// name no usable folder — which is a plain launch, not an error. An
    /// existing folder always resolves; `isRepo` is what tells "open this"
    /// from "offer to create one here".
    @concurrent
    static func launchTarget(arguments: [String], workingDirectory: String) async -> LaunchTarget? {
        resolveLaunchTarget(args: arguments, cwd: workingDirectory)
    }

    /// `git init` a folder and answer the path to open. Idempotent: a folder
    /// already inside a repository yields that repository's root instead of
    /// nesting a new one inside it.
    @concurrent
    static func initRepository(at path: String) async throws -> String {
        try initRepo(path: path)
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

    /// What this client asks core to build alongside a parse: the line model,
    /// plus the split layout's row pairing while that layout is the one on
    /// screen. The HTML array exists for a `WebView` host; asking for it here
    /// would only pay to marshal strings this renderer never looks at, and the
    /// pairing costs the same for a reader who is looking at the unified rows.
    /// There is no default — a caller that reads a diff knows which layout it
    /// is about to render, and guessing on its behalf is how the ask drifts
    /// from the arrangement.
    static func diffOptions(sideBySide: Bool, showAnyway: Bool) -> DiffOptions {
        DiffOptions(html: false, sideBySide: sideBySide, showAnyway: showAnyway)
    }

    /// Read and parse one working-tree file's diff: `HEAD` against the working
    /// tree, staged and unstaged combined. Untracked files diff against
    /// `/dev/null`, so a brand-new file still yields hunks.
    ///
    /// `hideWhitespace` runs `git diff -w`; when that leaves nothing to show,
    /// core checks the unfiltered diff so the pane can say the change is there
    /// and the setting is hiding it, rather than implying nothing changed.
    @concurrent
    static func parsedDiff(
        of repoPath: String,
        for file: FileEntry,
        hideWhitespace: Bool,
        options: DiffOptions
    ) async throws -> DiffPayload {
        try getParsedDiff(
            repoPath: repoPath, file: file, hideWhitespace: hideWhitespace, options: options)
    }

    /// Syntax tokens for a parsed diff — one entry per flattened line, empty
    /// where the tokenizer has nothing to say. `source` lets the tokenizer
    /// read complete blobs so multi-line constructs stay correct.
    @concurrent
    static func diffTokens(for fileDiff: FileDiff, source: BlobSource?) async -> [[Token]] {
        tokenizeDiff(fileDiff: fileDiff, source: source)
    }

    // MARK: - Commit history detail

    /// The files a commit changed and its line totals, from one `git log`.
    /// Renames carry `origPath`; the working-tree-only flags (`embedded`,
    /// `submoduleDirty`) are always false here, and binary files appear in the
    /// list without contributing to the totals. First-parent for merges.
    @concurrent
    static func commitDetail(in repoPath: String, sha: String) async throws -> CommitDetail {
        try getCommitDetail(repoPath: repoPath, sha: sha)
    }

    /// Read and parse one file's diff within a commit — the commit against its
    /// first parent, so a merge commit shows coherent per-file changes. Feeds
    /// the same `diffTokens` pipeline as the working tree, with
    /// `BlobSource.commit` so blobs are read at the commit, not from disk.
    @concurrent
    static func parsedCommitDiff(
        in repoPath: String,
        sha: String,
        filePath: String,
        options: DiffOptions
    ) async throws -> DiffPayload {
        try getParsedCommitDiff(
            repoPath: repoPath, sha: sha, filePath: filePath, options: options)
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

    // MARK: - Row context actions

    /// Throw away the working-tree changes to `files`. Tracked entries are
    /// restored from `HEAD`; entries with no committed version — untracked
    /// files and a rename's new side — go to the Trash, recoverable but gone
    /// from the working tree, which is why callers confirm first.
    @concurrent
    static func discardChanges(in repoPath: String, files: [FileEntry]) async throws {
        try discardFiles(repoPath: repoPath, files: files)
    }

    /// What discarding `files` would do to each path — the confirmation
    /// dialog's copy, decided by the same code that performs the action.
    /// A row's status letter cannot answer this: a staged re-add of a path
    /// that exists in HEAD is restorable, and under an unborn HEAD nothing is.
    @concurrent
    static func discardPlan(in repoPath: String, files: [FileEntry]) async -> DiscardPlan {
        classifyDiscard(repoPath: repoPath, files: files)
    }

    /// Add literal file paths to the repository's root `.gitignore`. Core
    /// escapes each path's glob metacharacters, so the rule matches that file
    /// and nothing else, and skips rules already present.
    @concurrent
    static func ignoreFiles(in repoPath: String, paths: [String]) async throws {
        try ignorePaths(repoPath: repoPath, paths: paths)
    }

    /// Add ready-to-write patterns (`*.log`) to the root `.gitignore`,
    /// verbatim — the "ignore every file of this type" action.
    @concurrent
    static func ignorePatterns(in repoPath: String, patterns: [String]) async throws {
        try appendToGitignore(repoPath: repoPath, patterns: patterns)
    }

    /// Reveal a working-tree file in Finder, selected in its folder.
    @concurrent
    static func revealInFileManager(in repoPath: String, relativePath: String) async throws {
        try revealPath(repoPath: repoPath, relPath: relativePath)
    }

    /// Open a working-tree file with whatever application the system
    /// associates with its type.
    @concurrent
    static func openWithDefaultApp(in repoPath: String, relativePath: String) async throws {
        try openPath(repoPath: repoPath, relPath: relativePath)
    }

    /// Open an `https://` URL in the default browser. Routed through core
    /// rather than `NSWorkspace` so both clients hand the address to the OS
    /// behind the same scheme and metacharacter guard.
    @concurrent
    static func openInBrowser(_ url: String) async throws {
        try openUrl(url: url)
    }

    /// Check out a commit by sha, detaching `HEAD` — the next status reports
    /// `detached`, and the branch menu is how the user reattaches.
    @concurrent
    static func checkout(in repoPath: String, commit sha: String) async throws {
        try checkoutCommit(repoPath: repoPath, sha: sha)
    }

    /// Drop the last commit (`git reset --mixed HEAD~1`), leaving its changes
    /// in the working tree to be edited and re-committed. Core refuses on a
    /// repository whose only commit is the initial one.
    @concurrent
    static func undoCommit(in repoPath: String) async throws {
        try undoLastCommit(repoPath: repoPath)
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

    /// How many commits merging `branch` would bring in — the merge sheet's
    /// preview number.
    @concurrent
    static func commitsToMerge(in repoPath: String, from branch: String) async throws -> Int32 {
        try countCommitsToMerge(repoPath: repoPath, targetBranch: branch)
    }

    // MARK: - Sync

    /// The repository's first remote name, or `nil` when it has none.
    /// Resolved immediately before each network operation, never cached —
    /// matching the Tauri handlers. Deliberately does not invent "origin": a
    /// caller that skips on `nil` is skipping a fetch that could only fail,
    /// and whose failure the connectivity breaker would have read as the
    /// network being down.
    @concurrent
    static func remoteName(in repoPath: String) async throws -> String? {
        try getRemote(repoPath: repoPath)
    }

    /// `git fetch --prune`: refresh remote-tracking refs — and the
    /// ahead/behind counts derived from them — without touching the working
    /// tree. Fetch streams no progress; core's fetch path has no sink.
    ///
    /// `background` picks the budget: an automatic fetch nobody is waiting on
    /// fails fast (8/8/12 s) so an unreachable remote can't hold the single
    /// network slot for ten minutes, while a fetch the user asked for keeps
    /// the generous one a large transfer needs.
    @concurrent
    static func fetchRemote(in repoPath: String, remote: String, background: Bool) async throws {
        try await fetch(repoPath: repoPath, remote: remote, background: background)
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
    /// private repositories without a prompt. Progress ticks arrive on the
    /// same seam a URL clone uses — `gh repo clone` forwards `--progress` to
    /// `git clone`, so this reports real numbers rather than an
    /// indeterminate bar.
    @concurrent
    static func githubClone(
        nameWithOwner: String,
        into targetPath: String,
        onProgress: @escaping @Sendable (SyncProgress) -> Void
    ) async throws -> String {
        try await ghClone(
            listener: ProgressRelay(onProgress),
            nameWithOwner: nameWithOwner,
            targetPath: targetPath)
    }

    /// Publish a remote-less repository to GitHub in one shot (`gh repo
    /// create --source … --remote origin --push`): creates the repo under
    /// the signed-in account (`name` may be `owner/name` for an org), wires
    /// it up as `origin`, and pushes the current branch with tracking. The
    /// next status refresh sees `hasRemote`/`hasUpstream` flip — that, not a
    /// return value, is how the UI learns the new state. No progress stream;
    /// the publish flow shows an indeterminate bar.
    @concurrent
    static func publishToGitHub(
        repoPath: String,
        name: String,
        description: String,
        isPrivate: Bool
    ) async throws {
        try await ghPublishRepo(
            repoPath: repoPath, name: name, description: description, isPrivate: isPrivate)
    }

    /// Persist the folder the user last cloned into — the parent directory,
    /// not the repo path — so the next Clone sheet (in either client)
    /// pre-fills it.
    @concurrent
    static func setLastCloneDir(_ dir: String) async throws {
        _ = try patchState(patch: ReposStatePatch(lastCloneDir: dir))
    }

    /// Persist the GitHub tab's sort toggle (`"recent"` | `"name"`) — the
    /// same state the Tauri dialog's toggle round-trips.
    @concurrent
    static func setCloneSortMode(_ mode: String) async throws {
        _ = try patchState(patch: ReposStatePatch(cloneSortMode: mode))
    }

    /// Persist the repo picker's sort toggle (`"recent"` | `"name"`) — the
    /// switcher's clock ⇄ A-Z choice, shared with the Tauri dropdown.
    @concurrent
    static func setRepoSortMode(_ mode: String) async throws {
        _ = try patchState(patch: ReposStatePatch(repoSortMode: mode))
    }

    // MARK: - Settings

    /// Apply a field-wise patch to the configuration and return the result.
    ///
    /// The only writer. A surface patches the fields it owns and nothing else,
    /// so it can no longer revert what the other client changed while its
    /// window was open — the load-fresh-then-edit discipline this used to
    /// require of every caller now lives inside core, under a lock. The
    /// returned config is normalized: hand it back to the form and an
    /// out-of-range entry corrects itself.
    @discardableResult
    @concurrent
    static func patchAppConfig(_ patch: ConfigPatch) async throws -> Config {
        try patchConfig(patch: patch)
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
    /// the shared config file, leaving every other setting untouched. An
    /// unrecognized name normalizes to claude inside core rather than being
    /// rejected here, so no writer can persist one.
    @concurrent
    static func setAIProvider(_ provider: String) async throws {
        try await patchAppConfig(ConfigPatch(aiProvider: provider))
    }

    /// Generate a commit message from `diff` via `config.provider` — the
    /// local `claude` CLI or a self-hosted Ollama instance. Plain
    /// request/response bounded by core's 120 s timeout; there is no
    /// streaming and no cancel, matching the Tauri client.
    @concurrent
    static func generateMessage(diff: String, config: AiProviderConfig) async throws -> CommitMessage {
        try await generateCommitMessage(diff: diff, provider: config.provider, config: config)
    }

    /// Whether `config.provider` could serve a request right now, so the
    /// composer can say why Generate is greyed out rather than letting a
    /// doomed request report it. Two process spawns for Claude
    /// (`--version`, then `auth status`), an HTTP request for Ollama.
    ///
    /// Every probe failure is an answer, not a throw — the only error is a
    /// provider name core doesn't know.
    @concurrent
    static func providerStatus(config: AiProviderConfig) async throws -> ProviderStatus {
        try await checkProviderStatus(provider: config.provider, config: config)
    }

    /// Read a *failed* generate for a provider state the user can fix.
    ///
    /// Not a fallback for `providerStatus(config:)` — for an expired session
    /// it is the only thing that works. Signing out deletes the credentials,
    /// so the probe sees it; an expired session leaves them on disk, so
    /// `claude auth status` still reports a signed-in CLI and only a real
    /// request discovers the refresh failed.
    ///
    /// Pure string reading in core, so there is nothing to await or throw.
    static func providerStatus(fromFailure message: String, provider: String) -> ProviderStatus {
        providerStatusFromFailure(provider: provider, error: message)
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
        _ = try patchState(patch: ReposStatePatch(lastOpenedRepo: repoPath))
    }

    /// Move `repoPath` to the front of the most-recently-used list (core
    /// de-dupes and caps it) and return the updated state.
    @concurrent
    @discardableResult
    static func recordRecent(repoPath: String) async throws -> ReposState {
        try recordRecentRepo(path: repoPath)
    }

    /// Every repository the picker should list: a filesystem walk of the scan
    /// folders (no git subprocesses) unioned with the shared
    /// recently-opened list, minus any entry that no longer exists. An empty
    /// list falls back to core's default folders. The walk runs on a Rust
    /// blocking thread, so a deep scan tree can't park a cooperative one.
    @concurrent
    static func knownRepositories(scanPaths: [String], depth: UInt32) async throws -> [String] {
        try await knownRepos(scanPaths: scanPaths, maxDepth: depth)
    }

    /// The `owner/name` a repository's remote URL parses to, or `nil` when it
    /// has no remote or the URL names no such pair — the picker keeps
    /// labelling that row with its folder name.
    @concurrent
    static func identifier(of repoPath: String) async -> RepoIdentifier? {
        await repoIdentifier(repoPath: repoPath)
    }

    // MARK: - Pure rules
    //
    // Deliberately synchronous, unlike every wrapper above: these touch no
    // filesystem and spawn nothing, so there is no blocking work to move off
    // the caller's executor. The picker rules are also read from a view's
    // computed property, where an `await` would force the answer into state
    // that lags a keystroke behind the field it describes.

    /// Narrow and rank picker rows against a typed query, strongest match
    /// first; ties keep the caller's ordering, so an MRU arrangement survives
    /// filtering. One crossing per keystroke rather than one per row.
    static func matchingRepos(query: String, rows: [RepoRow], scanFolders: [String]) -> [String] {
        filterRepos(query: query, rows: rows, scanFolders: scanFolders)
    }

    /// Which of the commit composer's opt-outs survive the file list a status
    /// read just produced — the ones still present, plus the ones whose path
    /// has been gone for less than core's grace window.
    ///
    /// `elapsedMs` is wall-clock time since the previous call rather than a
    /// count of ticks, because the poll's cadence changes with what the window
    /// is doing: counting ticks would make one grace window mean anything
    /// between 30 seconds and seven minutes.
    static func survivingExclusions(
        _ excluded: [Exclusion],
        present: [String],
        elapsedMs: UInt32
    ) -> [Exclusion] {
        reconcileExclusions(excluded: excluded, present: present, elapsedMs: elapsedMs)
    }

    /// What cloning `rawURL` under `parent` would produce — the URL to hand
    /// git (shorthand expanded), the folder name, and where it lands. `nil`
    /// means there is nothing cloneable, which is also the Clone button's
    /// enable condition, so the preview and the button can't disagree.
    static func cloneTarget(rawURL: String, parent: String) -> CloneTarget? {
        deriveCloneTarget(rawUrl: rawURL, parent: parent)
    }

    /// Where a clone of `repoName` lands under `parent` — the GitHub tab's
    /// half of the same rule.
    static func clonePath(parent: String, repoName: String) -> String? {
        cloneTargetPath(parent: parent, repoName: repoName)
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

    // MARK: - Update check

    /// A release newer than this build, or `nil` when it is current — which
    /// also covers a newer tag whose artifact for this platform has not been
    /// uploaded yet, since offering one the installer cannot complete is
    /// worse than staying quiet.
    ///
    /// Throwing means the *check* failed (offline, rate-limited, GitHub
    /// down), which the caller retries quietly and never shows.
    ///
    /// Already async in the bindings — core drives the request through tokio —
    /// so no `@concurrent` hop is needed or wanted here.
    static func latestRelease() async throws -> UpdateInfo? {
        try await checkForUpdate()
    }

    // MARK: - Embedded terminal

    /// Spawn a shell in a fresh PTY at the repository root, 80×24 until the
    /// first resize. `shellID` is the shared config's `terminal_shell`; nil
    /// or an uninstalled id resolves to the best shell on this machine. The
    /// listener's callbacks arrive on the session's Rust reader thread —
    /// never the main one — and its `onClosed` is the only end-of-session
    /// signal, for a self-exiting shell and a `killTerminal` alike, carrying
    /// the reaped child's exit status (a kill surfaces as a fatal signal).
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
