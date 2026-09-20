import SwiftUI

/// "Reorder N Commits?" — asked only of a move that rewrites commits already
/// on the upstream. Reorder has no sheet of its own for that warning to be a
/// caption in, as squash's is; every other move runs straight from the list.
///
/// Holds itself open under a busy label and keeps a refusal inside itself, as
/// `SquashSheet` does. The button keeps the accent: a reorder is a change with
/// a way back — Abort, the reflog — and red is for what loses work.
struct ReorderSheet: View {
    /// The commits to move, newest first.
    let commits: [CommitInfo]
    /// The commit they land just under; `nil` is the tip.
    let beforeSha: String?
    /// The upstream the commits are already on. Empty when core says they are
    /// pushed before a status read has named it.
    let pushedTo: String
    let store: HistoryActionStore
    let repoPath: String
    /// Called with every outcome git was actually asked for, after the sheet
    /// has dismissed itself where that is the right answer.
    let onFinished: (HistoryActionOutcome) async -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var errorMessage: String?

    private var verb: String {
        commits.count == 1 ? "Reorder Commit" : "Reorder \(commits.count) Commits"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("\(verb)?")
                .font(.title3.weight(.semibold))

            Text(
                pushedTo.isEmpty
                    ? "This rewrites commits that are already pushed."
                    : "This rewrites commits that are already on “\(pushedTo)”."
            )
            .font(.callout)
            .fixedSize(horizontal: false, vertical: true)

            Text("After reordering, the next push is a force push.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if let errorMessage { RefusalText(message: errorMessage) }

            if store.isBlocked, !store.isRunning { WriteBlockedNote() }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(store.isRunning)
                Button(store.isRunning ? "Reordering…" : verb, action: reorder)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(store.isBlocked)
            }
        }
        .padding(16)
        .frame(width: 460)
        .interactiveDismissDisabled(store.isRunning)
    }

    private func reorder() {
        guard !store.isBlocked else { return }
        errorMessage = nil
        Task {
            let outcome = await store.reorder(
                commits.map(\.sha),
                under: beforeSha,
                repoPath: repoPath
            )
            switch outcome {
            case .landed, .stoppedOnConflict:
                dismiss()
                await onFinished(outcome)
            case let .failed(message):
                errorMessage = message
                await onFinished(outcome)
            // Nothing was attempted, so the sheet stays as it is and says why.
            case .refusedBusy:
                errorMessage = RepositoryWriteGate.busyMessage
            }
        }
    }
}
