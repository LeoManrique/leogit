import SwiftUI

/// A draggable horizontal rule between two stacked panes, resizing the one
/// *below* it: dragging up makes that pane taller. The visible line is a
/// stock `Divider`; the grab zone is padded to 7 pt so it's catchable without
/// a hunt (the Tauri sidebar handle's ±3 px reach), and hovering shows the
/// system row-resize pointer, which is how the affordance gets discovered.
///
/// The owner supplies the height and its bounds; this view only turns
/// pointer motion into a clamped value.
struct RowResizeHandle: View {
    @Binding var height: CGFloat
    let range: ClosedRange<CGFloat>

    /// The height when the drag began. Every update is start − translation
    /// rather than an accumulated delta, so a drag that overshoots a bound
    /// and comes back tracks the pointer instead of leaving a dead zone.
    @State private var heightAtDragStart: CGFloat?

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
                        height = min(
                            max(start - value.translation.height, range.lowerBound),
                            range.upperBound
                        )
                    }
                    .onEnded { _ in heightAtDragStart = nil }
            )
    }
}
