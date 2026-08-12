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
    /// How many commits the first page loads. History paging comes later; this
    /// slice only needs enough rows to prove the bridge carries real data.
    private static let historyPageSize: Int32 = 100

    private(set) var repoPath: String?
    private(set) var repoName = ""
    private(set) var status: RepoStatus?
    private(set) var commits: [CommitInfo] = []
    private(set) var isLoading = false
    private(set) var coreVersionText = ""

    /// Whether a merge is in progress (`MERGE_HEAD` exists). Refreshed with
    /// status, like the Tauri client folds `is_merging` into its poll; drives
    /// the subtitle badge and the branch menu's Abort Merge item.
    private(set) var isMerging = false

    /// Bumped on every successful status load. Views that derive from the
    /// working tree beyond `status` itself — the open diff, most notably —
    /// key their reload on this, since a refresh can change a file's diff
    /// without changing its row in the file list.
    private(set) var statusEpoch = 0

    /// Set when the last operation failed; surfaced as a banner and cleared on
    /// the next successful load.
    var errorMessage: String?

    var isRepoOpen: Bool { repoPath != nil }

    /// Open the repository containing `url`, then load its status and history.
    ///
    /// Accepts a subdirectory as well as a repository root, matching the
    /// `leogit <path>` CLI behaviour.
    func open(at url: URL) async {
        isLoading = true
        defer { isLoading = false }

        do {
            let root = try await GitBridge.repoRoot(of: url.path(percentEncoded: false))
            repoPath = root
            repoName = await GitBridge.name(of: root)
            errorMessage = nil
            await loadRepoData(root)
        } catch {
            // Leave any previously open repo intact — a failed open should not
            // blank out what the user was already looking at.
            errorMessage = error.displayMessage
        }
    }

    /// Re-read status and history for the already-open repository.
    func refresh() async {
        guard let repoPath else { return }
        isLoading = true
        defer { isLoading = false }
        await loadRepoData(repoPath)
    }

    /// Close the repository and return to the welcome screen.
    func close() {
        repoPath = nil
        repoName = ""
        status = nil
        commits = []
        isMerging = false
        errorMessage = nil
    }

    /// Report which Rust build the UI is linked against.
    func loadCoreVersion() async {
        coreVersionText = await GitBridge.version()
    }

    /// Status and history are independent reads, so run them concurrently and
    /// let each report its own failure.
    private func loadRepoData(_ path: String) async {
        async let statusResult = GitBridge.status(of: path)
        async let logResult = GitBridge.log(of: path, limit: Self.historyPageSize)
        async let mergingResult = GitBridge.mergeInProgress(in: path)

        do {
            let (newStatus, newCommits) = try await (statusResult, logResult)
            status = newStatus
            commits = newCommits
            statusEpoch += 1
            errorMessage = nil
        } catch {
            errorMessage = error.displayMessage
        }
        // Best-effort, like the Tauri client's poll: failure reads as "not
        // merging" rather than an error.
        isMerging = (try? await mergingResult) ?? false
    }
}
