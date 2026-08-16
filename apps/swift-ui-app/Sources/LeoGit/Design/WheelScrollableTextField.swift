import AppKit
import SwiftUI

/// A single-line text field whose overflowing text can be panned with the
/// trackpad or a wheel.
///
/// The stock field (SwiftUI's `TextField` is an `NSTextField` underneath)
/// moves its text only with the caret, so a long line — an AI-generated
/// summary, say — can't be swiped through. The Tauri client maps wheel
/// deltas onto the input's `scrollLeft` for the same reason; this is that
/// handler's AppKit counterpart: the system control with one event
/// forwarded, no custom chrome. Scrolling works while the field is being
/// edited — unfocused, `NSTextField` draws static truncated text and has
/// nothing to scroll.
struct WheelScrollableTextField: NSViewRepresentable {
    /// Placeholder, shown whenever the field is empty.
    let prompt: String
    @Binding var text: String

    func makeNSView(context: Context) -> ScrollingTextField {
        let field = ScrollingTextField()
        field.delegate = context.coordinator
        field.isBezeled = true
        field.bezelStyle = .squareBezel
        field.font = .preferredFont(forTextStyle: .body)
        field.usesSingleLineMode = true
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        return field
    }

    func updateNSView(_ field: ScrollingTextField, context: Context) {
        field.placeholderString = prompt
        // Only external writes (Generate's result, the post-commit reset) may
        // replace the value — resetting it on the field's own edits would
        // throw the caret to the end.
        if field.stringValue != text {
            field.stringValue = text
        }
        field.isEnabled = context.environment.isEnabled
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        private let text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            text.wrappedValue = field.stringValue
        }
    }
}

/// `NSTextField` that pans its field editor on scroll gestures.
final class ScrollingTextField: NSTextField {
    // The field must never demand its content's width from layout — its
    // width is the composer's business, and a content-sized ideal width
    // would let a long summary push the split divider.
    override var intrinsicContentSize: NSSize {
        var size = super.intrinsicContentSize
        size.width = NSView.noIntrinsicMetric
        return size
    }

    override func scrollWheel(with event: NSEvent) {
        // While editing, the field editor sits in a clip view sized to the
        // field; panning is moving that clip's origin over the editor.
        guard let editor = currentEditor(), let clip = editor.superview as? NSClipView else {
            super.scrollWheel(with: event)
            return
        }
        let overflow = editor.frame.width - clip.bounds.width
        guard overflow > 0 else {
            // No overflow: let the event reach the surrounding scroll views,
            // the Tauri handler's "only then preventDefault" rule.
            super.scrollWheel(with: event)
            return
        }
        // Dominant axis, so a plain vertical wheel can pan the line too.
        let delta = abs(event.scrollingDeltaX) >= abs(event.scrollingDeltaY)
            ? event.scrollingDeltaX
            : event.scrollingDeltaY
        guard delta != 0 else { return }
        var origin = clip.bounds.origin
        origin.x = min(max(0, origin.x - delta), overflow)
        clip.scroll(to: origin)
    }
}
