import SwiftUI

/// A failure of FRONTEND §6.13's **first** class: an operation the user asked
/// for and was waiting on did not happen.
///
/// The second class — a failure that was never the user's task — belongs in
/// the screen's `ErrorBanner` instead, and nothing here should be used for it.
struct ActionFailure: Identifiable {
    let id = UUID()

    /// Core's own words. Never re-worded on the way here: the client does not
    /// know more about why git refused than git does.
    let message: String

    /// Re-run the exact attempt that failed, when the attempt can simply be
    /// made again. `nil` where a retry would need different input first (a
    /// name to change, a file to close), so the alert offers only a dismissal
    /// rather than a button that reproduces the same failure.
    let retry: (() -> Void)?

    init(_ message: String, retry: (() -> Void)? = nil) {
        self.message = message
        self.retry = retry
    }
}

extension View {
    /// Present §6.13's modal for `failure`, clearing it on dismissal.
    ///
    /// One modifier rather than a presentation per call site — the contract
    /// asks for the classification to live in one place per client, because a
    /// "report the failure" shape copied from the site next door is exactly
    /// how the Tauri client came to seize the window for *couldn't reveal the
    /// file in Finder*. A call site chooses between this and the strip; it
    /// does not restate what either one looks like.
    ///
    /// **A sheet, and not the `.alert` this used to be.** What lands here is
    /// git's own text, and git's refusals are multi-line: a rejected push is a
    /// `! [rejected]` line, the ref it was about, and three `hint:` lines
    /// naming the fix. `NSAlert` renders its informative text as one reflowed
    /// paragraph that cannot be selected, so the client was destroying the
    /// shape of the message *and* making the ref names uncopyable — on the one
    /// class of failure whose text is meant to be acted on. No modifier fixes
    /// that: a SwiftUI `.alert`'s message is bridged to that AppKit label, so
    /// neither `.font` nor `.textSelection` reaches it. The Tauri client has
    /// always shown this class in a modal carrying a `<pre>`; this is the same
    /// modal, and STYLE.md's rule for it — mono, selectable, capped in height
    /// and scrollable — is finally true in both.
    func actionFailureSheet(_ failure: Binding<ActionFailure?>) -> some View {
        // `.sheet(item:)` clears the binding on dismissal itself, so there is
        // no second place that has to remember to.
        sheet(item: failure) { presented in
            ActionFailureSheet(failure: presented)
        }
    }
}

/// §6.13's first class, on screen: what the user asked for did not happen,
/// with git's account of why in the words git used.
private struct ActionFailureSheet: View {
    let failure: ActionFailure

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Error", systemImage: "exclamationmark.triangle.fill")
                .font(.title3.weight(.semibold))
                .foregroundStyle(.orange)

            ScrollView {
                Text(failure.message)
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
                    // Wrap rather than widen: a long `hint:` line should not
                    // stretch the sheet across the display.
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            // Capped, so a rejection with a dozen hint lines cannot push the
            // buttons off the bottom of the sheet. Short messages — most of
            // them — take only the height they need.
            .frame(maxHeight: 220)

            HStack {
                Spacer()
                if let retry = failure.retry {
                    Button("OK") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                    // Return retries where a retry exists: it is the answer
                    // the user came for, and it repeats an attempt they made
                    // on purpose a moment ago.
                    Button("Try Again") {
                        dismiss()
                        retry()
                    }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                } else {
                    Button("OK") { dismiss() }
                        .buttonStyle(.borderedProminent)
                        .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(16)
        .frame(width: 460)
    }
}
