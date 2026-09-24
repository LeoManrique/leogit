import AppKit
import SwiftUI

extension View {
    /// A right-click (and control-click) menu for a toolbar chip — the one
    /// place SwiftUI's `.contextMenu` never shows.
    ///
    /// AppKit's toolbar answers every context click inside its bounds itself:
    /// `NSToolbarView` hit-tests the click and, for any view that is not one
    /// of its few known menu owners, shows its own display-mode menu ("Icon
    /// and Text / Icon Only"). SwiftUI does host each `ToolbarItem` in a real
    /// view, but neither a `.contextMenu` inside it nor an `NSView` with a
    /// `menu` is ever asked. So the click is claimed one step earlier, where
    /// every click passes through `NSApplication.sendEvent`: a local event
    /// monitor, shared by every chip (`ToolbarContextMenuRouter`).
    ///
    /// The items are ordinary SwiftUI menu content, rendered through
    /// `NSHostingMenu`, so a chip can share one menu view with the list rows
    /// naming the same thing. That menu is hosted on its own, outside the
    /// window's hierarchy: its items take their inputs from what they capture,
    /// never from `@Environment`.
    func toolbarContextMenu<MenuItems: View>(
        @ViewBuilder _ items: @escaping () -> MenuItems
    ) -> some View {
        background(ToolbarContextMenuAnchor(makeMenu: { NSHostingMenu(rootView: items()) }))
    }
}

/// The chip's marker in AppKit's world: a view behind the chip, sized by
/// SwiftUI to the chip's whole toolbar item, that tells the router where the
/// chip is and which menu it has. Holding the builder rather than a built menu keeps the menu as fresh as the
/// chip — `updateNSView` swaps the closure on every SwiftUI update, and the
/// menu is only built when a click asks for it.
private struct ToolbarContextMenuAnchor: NSViewRepresentable {
    let makeMenu: () -> NSMenu

    func makeNSView(context: Context) -> ToolbarContextMenuAnchorView {
        let view = ToolbarContextMenuAnchorView()
        view.makeMenu = makeMenu
        return view
    }

    func updateNSView(_ nsView: ToolbarContextMenuAnchorView, context: Context) {
        nsView.makeMenu = makeMenu
    }
}

private final class ToolbarContextMenuAnchorView: NSView {
    var makeMenu: () -> NSMenu = { NSMenu() }

    /// Never the target of a click: left-clicks, hover and tooltips all
    /// belong to the chip in front. The router finds the anchor by geometry,
    /// not by hit-testing.
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    /// Fires both when the chip lands in a toolbar and when it leaves one —
    /// SwiftUI builds a fresh `NSToolbar`, with fresh item views, every time
    /// the repository screen replaces Welcome.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        ToolbarContextMenuRouter.shared.anchor(self, didMoveTo: window)
    }
}

/// The router behind every `toolbarContextMenu`, and its one local event
/// monitor. It owns two things AppKit's toolbar would otherwise decide:
///
/// - a context click on an anchored chip opens that chip's menu, and the event
///   stops there;
/// - everywhere else in the toolbar — the sync button, the spacers, the empty
///   bar — the click goes on to AppKit, whose display-mode menu is switched
///   off. The toolbar is not customizable and its title is removed so the
///   chips' names can carry it, so "Icon Only" would only undo that.
///
/// Installed with the first anchor and never removed: the toolbar outlives any
/// one repository, and an idle monitor costs a type check per click and a scan
/// of a two-entry table per right-click.
@MainActor
private final class ToolbarContextMenuRouter {
    static let shared = ToolbarContextMenuRouter()

    private let anchors = NSHashTable<ToolbarContextMenuAnchorView>.weakObjects()
    private var monitor: Any?

    func anchor(_ anchor: ToolbarContextMenuAnchorView, didMoveTo window: NSWindow?) {
        guard let window else {
            anchors.remove(anchor)
            return
        }
        anchors.add(anchor)
        // An anchor lives inside a toolbar item, so by the time it reaches a
        // window that window's toolbar exists — unlike a hook on the screen
        // itself, which can run before SwiftUI has installed it. Set per
        // instance, because every Welcome → repository swap brings a new one
        // with the flag back on.
        disableDisplayModeMenu(of: window.toolbar)
        installMonitorIfNeeded()
    }

    private func disableDisplayModeMenu(of toolbar: NSToolbar?) {
        guard let toolbar, toolbar.allowsDisplayModeCustomization else { return }
        toolbar.allowsDisplayModeCustomization = false
        print("[toolbar] display-mode menu disabled on toolbar \(ObjectIdentifier(toolbar).hashValue)")
    }

    private func installMonitorIfNeeded() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { event in
            self.claimContextClick(event) ? nil : event
        }
        print("[toolbar] context-menu monitor installed")
    }

    /// Opens the chip's menu for a context click that lands on an anchored
    /// chip, and reports whether it did — a claimed event goes no further.
    private func claimContextClick(_ event: NSEvent) -> Bool {
        let isContextClick = event.type == .rightMouseDown
            || (event.type == .leftMouseDown && event.modifierFlags.contains(.control))
        // The monitor sees a click before the window does, so it must do the
        // refusing a window under a sheet or a modal would otherwise do: a
        // chip's menu is no way around a question the window is still asking.
        guard isContextClick, let window = event.window,
              window.attachedSheet == nil, NSApp.modalWindow == nil
        else { return false }
        let point = event.locationInWindow
        // Only anchors are ever matched, and every anchor sits in a toolbar
        // item, so the content area's own `.contextMenu`s are never touched.
        // SwiftUI sizes the anchor to the chip's whole toolbar item, which is
        // exactly its glass capsule — so the capsule's padding counts as the
        // chip, and the bar around it stays AppKit's. A hidden toolbar keeps
        // its views in the window, so a hidden anchor is no chip.
        guard let anchor = anchors.allObjects.first(where: { anchor in
            anchor.window === window
                && !anchor.isHiddenOrHasHiddenAncestor
                && anchor.convert(anchor.bounds, to: nil).contains(point)
        }) else { return false }
        NSMenu.popUpContextMenu(anchor.makeMenu(), with: event, for: anchor)
        return true
    }
}
