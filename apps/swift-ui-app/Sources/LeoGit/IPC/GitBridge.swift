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
