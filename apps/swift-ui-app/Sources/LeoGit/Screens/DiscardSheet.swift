import SwiftUI

/// "Discard Changes?" — the confirmation in front of the one row action that
/// destroys work.
///
/// A sheet rather than a `confirmationDialog` for two reasons a dialog cannot
/// serve. The discard is not instant on thirty files, and a dialog that
/// dismisses on the click has nowhere to say it is still working — so both
/// buttons lock and the destructive one becomes "Discarding…". And the discard
/// can fail, on an `index.lock` race or a file the OS refuses to trash, which
/// FRONTEND §6.13's refinement keeps *inside* the dialog that raised it: the
/// sheet is already the retry surface a modal would be offering, and it is
/// still naming the files the failure was about.
struct DiscardSheet: View {
    let repoPath: String

    /// What the discard will act on, snapshotted when the sheet opened — never
    /// re-read from the live list, so a status tick mid-confirmation cannot
    /// widen what the user agreed to.
    let files: [FileEntry]

    /// Called once the working tree has actually changed.
    let onDiscarded: () async -> Void

    @Environment(\.dismiss) private var dismiss

    /// What discarding would do, as core decides it. `nil` until the answer
    /// arrives — the sheet opens on the click and fills its outcome line a
    /// moment later rather than guessing in the meantime.
    @State private var plan: DiscardPlan?
    @State private var isDiscarding = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Discard Changes?")
                .font(.title3.weight(.semibold))

            question

            Text(outcome)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if let errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isDiscarding)
                Button(isDiscarding ? "Discarding…" : "Discard Changes", action: discard)
                    .buttonStyle(.borderedProminent)
                    .tint(.red)
                    .disabled(isDiscarding)
            }
        }
        .padding(16)
        .frame(width: 420)
        // Nothing calls a discard off once it has started writing, so the
        // sheet stops being dismissible rather than leaving a half-finished
        // operation with no window attached to it.
        .interactiveDismissDisabled(isDiscarding)
        .task {
            plan = await GitBridge.discardPlan(in: repoPath, files: files)
        }
    }

    @ViewBuilder
    private var question: some View {
        if files.count == 1, let file = files.first {
            VStack(alignment: .leading, spacing: 2) {
                Text("Are you sure you want to discard all changes to this file?")
                    .font(.callout)
                Text(file.path)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(3)
                    .truncationMode(.middle)
            }
        } else {
            Text("Are you sure you want to discard all changes to \(files.count) selected files?")
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// What discarding actually does, path class by path class — because which
    /// one a file falls into is not visible from its row, and the two outcomes
    /// are not equally recoverable.
    ///
    /// The answer comes from core, from real `HEAD` membership, and is the same
    /// decision the discard itself runs on. A status letter cannot stand in for
    /// it: a staged re-add of a path that exists in HEAD is restorable, a
    /// rename whose original is *not* in HEAD is not, and under an unborn HEAD
    /// nothing is — three cases a guess got wrong, each of them a promise the
    /// action then broke.
    private var outcome: String {
        guard let plan else { return "Working out what this will do…" }
        let restored = plan.restore.count
        let trashed = plan.trash.count
        switch (restored, trashed) {
        case (0, 0):
            return "There is nothing to discard."
        case (_, 0) where restored == 1:
            return "It goes back to its committed state."
        case (_, 0):
            return "All \(restored) go back to their committed state."
        case (0, _) where trashed == 1:
            return "It was never committed, so there is nothing to restore it to — "
                + "it moves to the Trash instead."
        case (0, _):
            return "None of the \(trashed) were ever committed, so they move to the Trash "
                + "rather than being restored."
        default:
            let back = restored == 1 ? "1 file goes" : "\(restored) files go"
            let away = trashed == 1 ? "1 moves" : "\(trashed) move"
            return "\(back) back to the last commit; \(away) to the Trash."
        }
    }

    private func discard() {
        guard !isDiscarding else { return }
        isDiscarding = true
        errorMessage = nil
        Task {
            defer { isDiscarding = false }
            do {
                // One call for the whole set, not one per row: core runs at
                // most three git subprocesses however many files it is given.
                try await GitBridge.discardChanges(in: repoPath, files: files)
                dismiss()
                await onDiscarded()
            } catch {
                errorMessage = error.displayMessage
                // A refusal is not proof that nothing happened: core restores
                // from HEAD and trashes in separate steps, so the first can
                // land and the second fail. Re-read the tree either way, and
                // re-ask what a retry would now do — an outcome line describing
                // a tree that no longer exists is worse than none.
                await onDiscarded()
                plan = await GitBridge.discardPlan(in: repoPath, files: files)
            }
        }
    }
}
