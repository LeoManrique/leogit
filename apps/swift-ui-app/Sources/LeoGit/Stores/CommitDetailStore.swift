import Foundation

/// State for one selected commit's detail: its changed files, its +/− totals,
/// and which file's diff is showing.
///
/// Metadata needs no loading — it rides in the `CommitInfo` the history list
/// already holds — so selecting a commit is one read: core returns the file
/// list and the line totals from a single `git log`, which is also what keeps
/// the two from ever describing different commits.
@MainActor
@Observable
final class CommitDetailStore {
    private(set) var files: [FileEntry] = []
    private(set) var stats: CommitStats?
    private(set) var isLoading = false
    private(set) var errorMessage: String?

    /// The highlighted rows, by path. A set because the list is the same one
    /// the Changes tab uses and answers the same gestures; nothing here acts on
    /// more than one row, so extending a selection is only a way of reading
    /// down the list.
    var selection: Set<String> = []

    /// The file whose diff the detail pane shows, derived from `selection`
    /// through `FileListSelection` — the one place that rule lives. Auto-set to
    /// the first file when a commit loads, like the Changes tab's list.
    private(set) var selectedPath: String?

    /// Re-derive the shown file after the highlight moved.
    func selectionChanged() {
        selectedPath = FileListSelection.activePath(
            in: selection,
            of: files,
            keeping: selectedPath
        )
    }

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
        selection = []
        selectedPath = nil
        errorMessage = nil

        do {
            let detail = try await GitBridge.commitDetail(in: repoPath, sha: sha)
            guard current == generation else { return }
            files = detail.files
            stats = detail.stats
            selection = detail.files.first.map { [$0.path] } ?? []
            selectedPath = detail.files.first?.path
            isLoading = false
        } catch {
            guard current == generation else { return }
            errorMessage = error.displayMessage
            isLoading = false
        }
    }
}
