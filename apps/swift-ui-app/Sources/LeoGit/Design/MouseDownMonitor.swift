import AppKit
import SwiftUI

extension View {
    /// Call `action` for every mouse-down in this app — either button, anywhere
    /// in any of its windows — for as long as `isActive`. The press is only
    /// observed: it goes on to whatever it was aimed at.
    ///
    /// For a mode that any click ends. A clear view laid over the content to
    /// catch the click would also catch the scroll wheel, and a list under it
    /// could no longer be scrolled.
    func onMouseDown(
        while isActive: Bool,
        perform action: @escaping @MainActor () -> Void
    ) -> some View {
        modifier(MouseDownMonitor(isActive: isActive, action: action))
    }
}

private struct MouseDownMonitor: ViewModifier {
    let isActive: Bool
    let action: @MainActor () -> Void

    @State private var monitor: Any?

    func body(content: Content) -> some View {
        content
            .onChange(of: isActive, initial: true) { _, active in
                if active { install() } else { remove() }
            }
            .onDisappear(perform: remove)
    }

    private func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(
            matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]
        ) { event in
            // AppKit runs a local monitor on the main thread, which its
            // block's type has no way to say.
            MainActor.assumeIsolated { action() }
            return event
        }
    }

    private func remove() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
}
