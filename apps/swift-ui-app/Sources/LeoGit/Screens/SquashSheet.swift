import SwiftUI

/// "Squash N Commits" — the message the squashed commit will carry, asked for
/// before anything is rewritten.
///
/// The fields are the composer's *components*, never its state: the composer
/// may be holding a draft of its own. They open with core's reading of the
/// selected commits (`SquashDraft`) and are the user's from then on. The
/// co-authors that draft gathered have no field in this app, as they have none
/// in the composer; they are named, and travel to the commit untouched.
///
/// A sheet that holds itself open under a busy label, for the reason
/// `CherryPickSheet` does — and one more: a squash core refused or undid stays
/// here, so the message typed is not lost with it. The cause is usually outside
/// the app (a hook, a signer), after which the same button is pressed again.
struct SquashSheet: View {
    /// The menu's targets, newest first.
    let commits: [CommitInfo]
    let draft: SquashDraft
    /// The upstream the commits are already on, or `nil` when none is pushed.
    /// Empty when core says they are pushed before a status read has named the
    /// upstream, which still gets the warning.
    let pushedTo: String?
    let store: HistoryActionStore
    let repoPath: String
    /// Called with every outcome git was actually asked for, after the sheet
    /// has dismissed itself where that is the right answer.
    let onFinished: (HistoryActionOutcome) async -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var summary: String
    @State private var details: String
    @State private var errorMessage: String?

    init(
        commits: [CommitInfo],
        draft: SquashDraft,
        pushedTo: String?,
        store: HistoryActionStore,
        repoPath: String,
        onFinished: @escaping (HistoryActionOutcome) async -> Void
    ) {
        self.commits = commits
        self.draft = draft
        self.pushedTo = pushedTo
        self.store = store
        self.repoPath = repoPath
        self.onFinished = onFinished
        _summary = State(initialValue: draft.summary)
        _details = State(initialValue: draft.description)
    }

    private var title: String { "Squash \(commits.count) Commits" }

    private var trimmedSummary: String {
        summary.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.title3.weight(.semibold))

            WheelScrollableTextField(prompt: "Summary (required)", text: $summary)
                .disabled(store.isRunning)

            DescriptionEditor(text: $details)
                .frame(height: 160)
                .disabled(store.isRunning)

            if !draft.coAuthors.isEmpty {
                caption("Co-authored by \(draft.coAuthors.joined(separator: ", ")).")
            }

            if let pushedTo {
                caption(
                    (pushedTo.isEmpty
                        ? "These commits are already pushed."
                        : "These commits are already on “\(pushedTo)”.")
                        + " After squashing, the next push is a force push."
                )
            }

            if let errorMessage { RefusalText(message: errorMessage) }

            if store.isBlocked, !store.isRunning { WriteBlockedNote() }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(store.isRunning)
                // ⌘↩, the composer's chord: plain Return belongs to the
                // description, which takes it as a newline before any default
                // button could see it.
                Button(store.isRunning ? "Squashing…" : title, action: squash)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(store.isBlocked || trimmedSummary.isEmpty)
            }
        }
        .padding(16)
        .frame(width: 480)
        .interactiveDismissDisabled(store.isRunning)
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func squash() {
        // The chord arrives here past the button's `.disabled`, so the same
        // gate is asked again.
        guard !store.isBlocked, !trimmedSummary.isEmpty else { return }
        errorMessage = nil
        Task {
            let outcome = await store.squash(
                commits.map(\.sha),
                summary: trimmedSummary,
                description: details.trimmingCharacters(in: .whitespacesAndNewlines),
                coAuthors: draft.coAuthors,
                repoPath: repoPath
            )
            switch outcome {
            case .landed, .stoppedOnConflict:
                dismiss()
                await onFinished(outcome)
            case let .failed(message):
                errorMessage = message
                await onFinished(outcome)
            // Nothing was attempted, so the sheet stays as it is, still
            // offering the squash — and says why, since the slot was taken
            // between the guard above and the claim.
            case .refusedBusy:
                errorMessage = RepositoryWriteGate.busyMessage
            }
        }
    }
}
