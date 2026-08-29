import SwiftUI

/// "Force Push with Lease?" — the confirmation in front of the one transfer
/// that can destroy someone else's commits.
///
/// A sheet rather than a `confirmationDialog`, for the reasons `DiscardSheet`
/// and `CheckoutCommitSheet` are sheets, and here they are sharper than
/// anywhere else. A push is the slowest thing this app does, and a dialog that
/// dismisses on the click leaves the user watching a toolbar button for an
/// answer to a question they were just asked in the middle of the window. And
/// a lease is *made* to be refused: someone pushed since your last fetch is
/// the expected failure, not the exceptional one, and FRONTEND §6.13's
/// refinement keeps it inside the dialog that raised it — fetch, then press
/// the same button again, one dismissal away instead of two.
struct ForcePushSheet: View {
    /// Where the push would land, named from git's own tracking configuration
    /// (`RepoStatus.upstream`) rather than composed from the remote and the
    /// local branch name — those differ whenever the upstream branch is named
    /// something else, and the dialog would then promise to overwrite a branch
    /// git was never going to touch.
    let upstream: String

    /// Run the push, answering what actually happened.
    ///
    /// The `refusedBusy` case is the reason this is an `OpOutcome` and not a
    /// `String?`: the single network slot can change hands between this sheet
    /// opening and the button being pressed — a background auto-fetch is the
    /// usual culprit — and a sheet that closed on "no error" would report a
    /// force push that never ran, which is the one operation whose consequences
    /// the user most needs to be right about.
    let onForcePush: () async -> OpOutcome

    @Environment(\.dismiss) private var dismiss

    @State private var isPushing = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Force Push with Lease?")
                .font(.title3.weight(.semibold))

            Text("This will overwrite “\(upstream)” with your local branch.")
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)

            Text(
                "With-lease refuses the push if someone else has pushed since your last "
                    + "fetch — safer than a plain force, but it cannot be undone once it "
                    + "succeeds."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

            if let errorMessage {
                // Git's own rejection, kept as git wrote it: monospaced so its
                // ref names line up, selectable so the ref that moved can be
                // copied into a fetch, and scrollable so a long hint block
                // cannot push the buttons off the sheet (STYLE.md).
                ScrollView {
                    Text(errorMessage)
                        .font(.caption.monospaced())
                        .foregroundStyle(.red)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 120)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isPushing)
                Button(isPushing ? "Force-pushing…" : "Force Push", action: forcePush)
                    .buttonStyle(.borderedProminent)
                    .tint(.red)
                    .disabled(isPushing)
            }
        }
        .padding(16)
        .frame(width: 460)
        // A transfer in flight owns the single network slot and cannot be
        // called off, so the sheet stops being dismissible rather than leaving
        // it running with no window attached to it.
        .interactiveDismissDisabled(isPushing)
    }

    private func forcePush() {
        guard !isPushing else { return }
        isPushing = true
        errorMessage = nil
        Task {
            defer { isPushing = false }
            switch await onForcePush() {
            case .succeeded:
                dismiss()
            case let .failed(message):
                errorMessage = message
            case .refusedBusy:
                // Not git's words, because git was never asked — and saying so
                // beats both closing (a push that did not happen) and staying
                // silent under a button that visibly did nothing.
                errorMessage = "Another network operation is in progress. Try again in a moment."
            }
        }
    }
}
