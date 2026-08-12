import Foundation

/// State and behaviour for composing a commit from the changes list.
///
/// Inclusion is *derived*, not stored: every committable file is included
/// unless its path is in `excludedPaths`. Tracking exclusions instead of
/// inclusions is what the Tauri client does too (its `userDeselected` set),
/// and for the same reason — a status reload rebuilds the file list from
/// scratch, and files that appear after the user last touched a checkbox must
/// default to included without any re-seeding bookkeeping.
@MainActor
@Observable
final class CommitStore {
    /// First line of the commit message. Required to commit.
    var summary = ""

    /// Optional message body, joined below the summary with a blank line.
    var details = ""

    /// Paths the user explicitly unchecked. Deliberately not pruned when a
    /// path leaves the file list — matching the Tauri client — so a file that
    /// briefly disappears (e.g. touched by a formatter) keeps its opt-out.
    private(set) var excludedPaths: Set<String> = []

    private(set) var isCommitting = false

    /// Core's own failure text from the last attempt; cleared on the next
    /// attempt, on success, and on `reset()`.
    private(set) var errorMessage: String?

    /// Whether the parent repository can stage this entry at all. A dirty
    /// submodule can't be committed from here — its changes live inside the
    /// submodule and only a pointer move would be recorded.
    static func isCommittable(_ file: FileEntry) -> Bool {
        !file.submoduleDirty
    }

    func isIncluded(_ file: FileEntry) -> Bool {
        Self.isCommittable(file) && !excludedPaths.contains(file.path)
    }

    /// The subset of `files` the next commit would contain, in list order.
    func includedFiles(from files: [FileEntry]) -> [FileEntry] {
        files.filter { isIncluded($0) }
    }

    func setIncluded(_ file: FileEntry, _ include: Bool) {
        guard Self.isCommittable(file) else { return }
        if include {
            excludedPaths.remove(file.path)
        } else {
            excludedPaths.insert(file.path)
        }
    }

    func setAllIncluded(_ include: Bool, in files: [FileEntry]) {
        let committable = files.filter(Self.isCommittable).map(\.path)
        if include {
            excludedPaths.subtract(committable)
        } else {
            excludedPaths.formUnion(committable)
        }
    }

    /// Forget everything typed and unchecked — for switching repositories.
    func reset() {
        summary = ""
        details = ""
        excludedPaths = []
        errorMessage = nil
    }

    /// Format the message and commit `files`. On success the composer is
    /// cleared and the caller should reload status + history; on failure the
    /// draft is kept for another attempt and `errorMessage` carries core's
    /// own text. Returns whether the commit landed.
    func commit(repoPath: String, files: [FileEntry]) async -> Bool {
        let trimmedSummary = summary.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedSummary.isEmpty, !files.isEmpty, !isCommitting else { return false }
        isCommitting = true
        errorMessage = nil
        defer { isCommitting = false }

        let message = await GitBridge.commitMessage(
            summary: trimmedSummary,
            description: details.trimmingCharacters(in: .whitespacesAndNewlines),
            coAuthors: []
        )
        do {
            try await GitBridge.commitChanges(in: repoPath, message: message, files: files)
            reset()
            return true
        } catch {
            errorMessage = error.displayMessage
            return false
        }
    }
}
