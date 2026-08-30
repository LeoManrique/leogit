import SwiftUI

/// The terminal panel docked under the main content: a header strip that is
/// always visible while a repository is open, and — once a session exists —
/// the terminal itself, fixed at 280 pt like the Tauri client's.
///
/// Collapsing hides the terminal but keeps the shell running; ✕ kills it;
/// + replaces it with a fresh one. Content above reflows rather than being
/// covered.
struct TerminalDock: View {
    let repoPath: String
    let store: TerminalStore

    /// The emulator's height, matching the Tauri panel's. Named because it is
    /// used twice: once to lay the terminal out and once to decide how much of
    /// it the collapsed panel shows.
    private static let panelHeight: CGFloat = 280

    /// Drives the label toggle. Reading reports whether the panel is open;
    /// writing routes through `toggle()` rather than assigning, so the lazy
    /// first spawn still happens on the way up.
    private var isExpanded: Binding<Bool> {
        Binding(get: { store.isExpanded }, set: { _ in store.toggle() })
    }

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            header
            if store.hasSession {
                TerminalSessionView(
                    repoPath: repoPath,
                    isExpanded: store.isExpanded,
                    onStarted: { store.activeShellLabel = $0.shellLabel },
                    onExit: { store.closeSession() },
                    pendingCommand: store.pendingCommand,
                    onCommandSent: store.commandDidRun
                )
                // Structural identity: a bumped generation or a switched
                // repo unmounts the old session (killing its PTY) and
                // mounts a fresh one.
                .id("\(repoPath):\(store.generation)")
                // SwiftTerm draws edge to edge — its layer has no inset — so
                // the first column sat against the strip above and against
                // the window's own rounded bottom corner, which clipped it.
                // The backdrop below is the same black the view paints, so
                // the gutter reads as part of the terminal.
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .frame(maxWidth: .infinity)
                // The emulator is laid out at the panel's full height whether
                // the panel is showing or not, and collapsing clips it rather
                // than shrinking it. A zero-height frame is still full *width*,
                // so SwiftTerm's degenerate-size bail (which wants both to be
                // zero) never fired: the buffer reflowed to one row on every
                // collapse and back on every expand, wrapping the scrollback
                // and sending a spurious SIGWINCH each way.
                .frame(height: Self.panelHeight)
                .frame(height: store.isExpanded ? Self.panelHeight : 0, alignment: .top)
                .background(.black)
                .clipped()
                // The pinned view now hangs below a collapsed panel instead of
                // having no size at all, so it must stop answering the mouse
                // as well as stop drawing.
                .allowsHitTesting(store.isExpanded)
            }
        }
    }

    /// A macOS accessory bar — the control class AppKit draws recessed for
    /// scope bars (Finder's search scopes, Safari's favourites bar), which is
    /// what a docked panel's strip is. `.accessoryBar` supplies the hover,
    /// press, and on-state treatments, so nothing here paints its own: the
    /// previous hand-styled `.plain`/`.borderless` buttons were inert glyphs
    /// with no feedback at all.
    private var header: some View {
        HStack(spacing: 2) {
            // The shell name doubles as the panel toggle. The first expand
            // lazily spawns the shell, like the Tauri client. A button-styled
            // Toggle rather than a Button so the strip shows the panel's
            // on-state the way a scope bar does.
            //
            // ⌃` belongs to View ▸ Show/Hide Terminal, not to this control: a
            // key equivalent on a button is matched through the responder
            // chain, so the emulator — the one view most likely to hold focus
            // when you want to collapse the panel — could swallow it. A menu
            // equivalent is matched first.
            Toggle(isOn: isExpanded) {
                Label(
                    store.activeShellLabel.isEmpty ? "Terminal" : store.activeShellLabel,
                    systemImage: "apple.terminal"
                )
            }
            .toggleStyle(.button)
            .help("Toggle terminal (⌃`)")

            Spacer(minLength: 0)

            // Icon-only, but built from `Label`s: the titles become the
            // accessibility labels the bare `Image`s never had.
            HStack(spacing: 2) {
                Button {
                    store.startNewSession()
                } label: {
                    Label("New Session", systemImage: "plus")
                }
                .help("New terminal session")

                Button(action: store.toggle) {
                    Label(
                        store.isExpanded ? "Minimize Terminal" : "Expand Terminal",
                        systemImage: store.isExpanded ? "chevron.down" : "chevron.up"
                    )
                }
                .help(store.isExpanded ? "Minimize terminal" : "Expand terminal")

                Button {
                    store.closeSession()
                } label: {
                    Label("Close Session", systemImage: "xmark")
                }
                .disabled(!store.hasSession)
                .help("Kill terminal session")
            }
            .labelStyle(.iconOnly)
        }
        .buttonStyle(.accessoryBar)
        .controlSize(.small)
        .padding(.horizontal, 6)
        .frame(height: 28)
        .background(.bar)
    }
}
