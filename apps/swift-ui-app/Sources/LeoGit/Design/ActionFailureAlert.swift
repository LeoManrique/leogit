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
    /// One modifier rather than an `.alert` per call site — the contract asks
    /// for the classification to live in one place per client, because a
    /// "report the failure" shape copied from the site next door is exactly
    /// how the Tauri client came to seize the window for *couldn't reveal the
    /// file in Finder*. A call site chooses between this and the strip; it
    /// does not restate what either one looks like.
    func actionFailureAlert(_ failure: Binding<ActionFailure?>) -> some View {
        alert(
            "Error",
            isPresented: Binding(
                get: { failure.wrappedValue != nil },
                set: { if !$0 { failure.wrappedValue = nil } }
            ),
            presenting: failure.wrappedValue
        ) { presented in
            if let retry = presented.retry {
                Button("Try Again") { retry() }
            }
            // AppKit makes the first button the default and the cancel-role
            // one Escape's, so Return retries where a retry exists and Escape
            // always dismisses. Retrying is the answer the user came for, and
            // it repeats an attempt they made on purpose a moment ago.
            Button("OK", role: .cancel) {}
        } message: { presented in
            Text(presented.message)
        }
    }
}
