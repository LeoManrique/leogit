import SwiftUI

/// A draggable horizontal rule between two stacked panes, resizing the one
/// *below* it: dragging up makes that pane taller. The visible line is a
/// stock `Divider`; the grab zone is padded to 7 pt so it's catchable without
/// a hunt (the Tauri sidebar handle's ±3 px reach), and hovering shows the
/// system row-resize pointer, which is how the affordance gets discovered.
///
/// It is also reachable from the keyboard, which is the only way it exists for
/// anyone who cannot make a 7 pt drag: with macOS keyboard navigation on, Tab
/// to it, then ↑/↓ to move it 16 pt at a time and Home/End to send it to
/// either bound. That setting is the gate because the handle takes focus like
/// a button and not like a field (see `.focusable` below) — and without it Tab
/// reaches no non-text control anyway, so nothing is actually lost. VoiceOver
/// gets the same two steps as an adjustable element, so the handle is one
/// control however it is reached.
///
/// The owner supplies the height and its bounds; this view only turns pointer
/// motion and key presses into a clamped value, and calls `onCommit` when a
/// gesture ends so the owner can persist **once** rather than on every frame of
/// a drag.
struct RowResizeHandle: View {
    @Binding var height: CGFloat
    let range: ClosedRange<CGFloat>

    /// The height is settled: a drag let go, or a key moved it. The owner
    /// persists here, not on every change — a drag writes sixty values a
    /// second and only the last one is worth keeping.
    var onCommit: () -> Void = {}

    /// The height when the drag began. Every update is start − translation
    /// rather than an accumulated delta, so a drag that overshoots a bound
    /// and comes back tracks the pointer instead of leaving a dead zone.
    @State private var heightAtDragStart: CGFloat?

    /// One key press, in points. The Tauri handle's `RESIZE_STEP`.
    private static let keyStep: CGFloat = 16

    var body: some View {
        Divider()
            .padding(.vertical, 3)
            .contentShape(.rect)
            .pointerStyle(.rowResize)
            .gesture(
                // Global space: the handle itself moves with every update,
                // so a local translation would measure against a moving
                // origin.
                DragGesture(minimumDistance: 0, coordinateSpace: .global)
                    .onChanged { value in
                        let start = heightAtDragStart ?? height
                        heightAtDragStart = start
                        apply(start - value.translation.height)
                    }
                    .onEnded { _ in
                        heightAtDragStart = nil
                        onCommit()
                    }
            )
            // Button-like, not field-like: `.automatic` would enable `.edit`
            // too, which makes the divider an unconditional first-responder
            // candidate — with a clean tree it is the only focusable view in
            // the sidebar, so window activation hands it focus and paints the
            // ring across the whole gutter. `.activate` scopes it to users who
            // turned keyboard navigation on, which is who the keys are for.
            .focusable(interactions: .activate)
            .onKeyPress(keys: [.upArrow, .downArrow, .home, .end]) { press in
                switch press.key {
                // Up grows the pane below, which is the direction the divider
                // itself travels — the drag says the same thing.
                case .upArrow: apply(height + Self.keyStep)
                case .downArrow: apply(height - Self.keyStep)
                case .home: apply(range.lowerBound)
                case .end: apply(range.upperBound)
                default: return .ignored
                }
                onCommit()
                return .handled
            }
            .accessibilityElement()
            .accessibilityLabel("Resize commit section")
            .accessibilityValue(Text("\(Int(height)) points"))
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: apply(height + Self.keyStep)
                case .decrement: apply(height - Self.keyStep)
                @unknown default: return
                }
                onCommit()
            }
    }

    private func apply(_ value: CGFloat) {
        height = min(max(value, range.lowerBound), range.upperBound)
    }
}
