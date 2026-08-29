import SwiftUI

/// "Check Out This Commit?" — the confirmation in front of detaching HEAD.
///
/// A sheet rather than a `confirmationDialog`, for the reasons `DiscardSheet`
/// is one. A checkout is not instant on a large tree, and a dialog that
/// dismisses on the click has nowhere to say it is still working — so both
/// buttons lock and the confirming one becomes "Checking out…", which is also
/// what stops a second checkout being issued to race the first on
/// `index.lock`. And it can fail — a dirty file the checkout would overwrite
/// is the common one — which FRONTEND §6.13's refinement keeps *inside* the
/// dialog that raised it rather than in a modal stacked over it: the sheet is
/// already the retry surface, and it is still naming the commit.
struct CheckoutCommitSheet: View {
    /// The commit being checked out, snapshotted when the sheet opened.
    let commit: CommitInfo

    /// Run the checkout. Answers with core's error text, or `nil` once HEAD is
    /// on the commit.
    let onCheckout: () async -> String?

    @Environment(\.dismiss) private var dismiss

    @State private var isCheckingOut = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Check Out This Commit?")
                .font(.title3.weight(.semibold))

            VStack(alignment: .leading, spacing: 2) {
                Text(commit.shortSha)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                Text(commit.summary)
                    .font(.callout)
                    .lineLimit(2)
            }

            Text(
                "This detaches HEAD: you'll be on no branch until you pick one from the "
                    + "branch menu. Commits made meanwhile are easy to lose."
            )
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
                    .disabled(isCheckingOut)
                Button(isCheckingOut ? "Checking out…" : "Check Out", action: checkOut)
                    .buttonStyle(.borderedProminent)
                    .disabled(isCheckingOut)
            }
        }
        .padding(16)
        .frame(width: 420)
        // Nothing calls a checkout off once git is rewriting the working tree,
        // so the sheet stops being dismissible rather than leaving the
        // operation running with no window attached to it.
        .interactiveDismissDisabled(isCheckingOut)
    }

    private func checkOut() {
        guard !isCheckingOut else { return }
        isCheckingOut = true
        errorMessage = nil
        Task {
            defer { isCheckingOut = false }
            if let failure = await onCheckout() {
                errorMessage = failure
            } else {
                dismiss()
            }
        }
    }
}
