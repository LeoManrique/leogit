import SwiftUI

/// The one-line strip above the split — the app's only banner shape
/// (`STYLE.md`, *Status indicators*; FRONTEND §6.13's second class). It states
/// something about the repository without taking the window, so the last good
/// data stays on screen behind it.
///
/// Three conditions use it, each a line of its own so none can silence
/// another: the poll's "this repository stopped being readable", a dismissable
/// notice, and the way back from the last History action. Call sites pass
/// content and a tone; they do not pass style.
struct StatusStrip: View {
    enum Tone {
        /// Something went wrong or needs reading: the orange triangle on a wash
        /// of the same hue.
        case warning
        /// The app reporting its own finished work: a quiet checkmark and no
        /// wash, because nothing is wrong.
        case done
    }

    /// The one thing a strip offers to do, as a link after the sentence.
    struct Action {
        let label: String
        /// Why it cannot run right now; makes the link inert and becomes its help.
        var blocked: String?
        let run: () -> Void
    }

    let tone: Tone
    let message: String
    var action: Action?

    /// Retire the strip by hand. Omitted for the status poll's own, whose
    /// recovery retires it — a ✕ there would hide a repository that is still
    /// unreadable. Everything else needs one, because nothing else will.
    var onDismiss: (() -> Void)?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                glyph
                Text(message)
                    .font(.callout)
                    .textSelection(.enabled)

                if let action {
                    // Dimmed and inert rather than `.disabled`, as the
                    // repository picker's blocked rows are: a disabled control
                    // takes no pointer events, so the reason would never show.
                    Button(action.label) {
                        if action.blocked == nil { action.run() }
                    }
                    .buttonStyle(.link)
                    .font(.callout)
                    .opacity(action.blocked == nil ? 1 : 0.55)
                    .help(action.blocked ?? "")
                    .accessibilityHint(action.blocked ?? "")
                }
                Spacer(minLength: 0)

                if let onDismiss {
                    Button(action: onDismiss) {
                        Image(systemName: "xmark")
                            .font(.caption.weight(.semibold))
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help("Dismiss")
                    .accessibilityLabel("Dismiss")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(tone == .warning ? Color.orange.opacity(0.12) : Color.clear)

            // The wash is the warning's own edge; without one, a hairline is
            // what keeps the line from reading as part of the list below it.
            if tone == .done { Divider() }
        }
    }

    @ViewBuilder
    private var glyph: some View {
        switch tone {
        case .warning:
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
        case .done:
            Image(systemName: "checkmark.circle")
                .foregroundStyle(.secondary)
        }
    }
}
