import Foundation

/// The words for an operation in progress, in the three forms the UI needs.
///
/// One table rather than a `switch` at each site: the branch chip, the abort
/// item, its confirmation and the composer all name the same operation, and a
/// fifth site that forgot a case would show a rebase as a merge. Mirrors the
/// Tauri client's `operationWords.ts`.
struct OperationWords: Equatable, Sendable {
    /// Sentence form: "the rebase in progress".
    let noun: String
    /// Title Case, for buttons and menu items: "Abort Rebase…".
    let title: String
    /// What the repository is doing, for the branch chip: "· rebasing".
    let gerund: String

    /// What to say when a Continue dropped the stopped commit instead of
    /// making it (`OperationOutcome.skipped`). Git is silent there, and
    /// "nothing new in History" would otherwise read as the Continue having
    /// done nothing.
    var skippedNotice: String {
        "The resolution left nothing to commit, so the \(noun) skipped that commit."
    }
}

extension OperationInProgress {
    var words: OperationWords {
        switch self {
        case .merge: OperationWords(noun: "merge", title: "Merge", gerund: "merging")
        case .rebase: OperationWords(noun: "rebase", title: "Rebase", gerund: "rebasing")
        case .cherryPick:
            OperationWords(noun: "cherry-pick", title: "Cherry-pick", gerund: "cherry-picking")
        case .revert: OperationWords(noun: "revert", title: "Revert", gerund: "reverting")
        }
    }

    /// Whether the operation replays commits that already have a message, so
    /// the composer offers **Continue** in place of **Commit**. A merge is the
    /// exception: concluding it *is* a commit, with a message the user writes.
    var continuesFromComposer: Bool { self != .merge }
}
