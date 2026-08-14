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

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            header
            if store.hasSession {
                TerminalSessionView(
                    repoPath: repoPath,
                    isExpanded: store.isExpanded,
                    onStarted: { store.activeShellLabel = $0.shellLabel },
                    onExit: { store.closeSession() }
                )
                // Structural identity: a bumped generation or a switched
                // repo unmounts the old session (killing its PTY) and
                // mounts a fresh one.
                .id("\(repoPath):\(store.generation)")
                .frame(maxWidth: .infinity)
                .frame(height: store.isExpanded ? 280 : 0)
                .background(.black)
                .clipped()
            }
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            // The label doubles as the toggle and carries ⌘` — like the
            // Tauri client, where first expand lazily spawns the shell.
            Button(action: store.toggle) {
                HStack(spacing: 6) {
                    Image(systemName: "terminal")
                    Text(store.activeShellLabel.isEmpty ? "Terminal" : store.activeShellLabel)
                        .font(.caption)
                }
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .keyboardShortcut("`", modifiers: .command)
            .help("Toggle terminal (⌘`)")

            Spacer(minLength: 0)

            Button {
                store.startNewSession()
            } label: {
                Image(systemName: "plus")
            }
            .help("New terminal session")

            Button(action: store.toggle) {
                Image(systemName: store.isExpanded ? "chevron.down" : "chevron.up")
            }
            .help(store.isExpanded ? "Minimize terminal" : "Expand terminal")

            Button {
                store.closeSession()
            } label: {
                Image(systemName: "xmark")
            }
            .disabled(!store.hasSession)
            .help("Kill terminal session")
        }
        .buttonStyle(.borderless)
        .controlSize(.small)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(.bar)
    }
}
