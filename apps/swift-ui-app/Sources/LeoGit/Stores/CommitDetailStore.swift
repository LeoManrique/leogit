import Foundation

/// State for one selected commit's detail: its changed files, its +/− totals,
/// and which file's diff is showing.
///
/// Metadata needs no loading — it rides in the `CommitInfo` the history list
/// already holds — so selecting a commit fetches only the file list and, as
/// non-critical chrome, the line totals. The Tauri client makes the same
/// split: files awaited, stats fired-and-forgotten with failures swallowed.
/// One deliberate improvement over it: a failed file-list read shows an error
/// state here instead of silently rendering as "no files".
@MainActor
@Observable
final class CommitDetailStore {
    private(set) var files: [FileEntry] = []
    private(set) var stats: CommitStats?
    private(set) var isLoading = false
    private(set) var errorMessage: String?

    /// The file whose diff the detail pane shows; auto-set to the first file
    /// when a commit loads, like the Changes tab's list.
    var selectedPath: String?

    /// Guards against a superseded load writing over a newer one — the
    /// blocking FFI calls cannot be interrupted, so a stale task may resume
    /// here after `.task(id:)` has already started its replacement.
    private var generation = 0

    /// Load the detail for the commit `sha`, replacing whatever was shown.
    func load(repoPath: String, sha: String) async {
        generation += 1
        let current = generation
        isLoading = true
        files = []
        stats = nil
        selectedPath = nil
        errorMessage = nil

        async let statsResult = GitBridge.commitStats(in: repoPath, sha: sha)

        do {
            let loaded = try await GitBridge.commitFiles(in: repoPath, sha: sha)
            guard current == generation else { return }
            files = loaded
            selectedPath = loaded.first?.path
            isLoading = false
        } catch {
            guard current == generation else { return }
            errorMessage = error.displayMessage
            isLoading = false
        }

        // Totals arrive whenever they arrive; a failure just leaves the
        // header without a +/− badge, never an error.
        if let stats = try? await statsResult {
            guard current == generation else { return }
            self.stats = stats
        }
    }
}
