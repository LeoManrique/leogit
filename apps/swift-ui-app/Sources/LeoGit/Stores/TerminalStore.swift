import Foundation
import Observation

/// Dock state for the embedded terminal: which session generation is alive
/// (0 = none), whether the panel is expanded, and the launched shell's
/// display name — the Tauri client's `MainLayout` terminal state, gathered
/// into one type.
///
/// The session itself (PTY id, SwiftTerm view, I/O queue) lives in
/// `TerminalSessionView`, mounted while `generation > 0` and identified by
/// `repoPath:generation` — so bumping the generation, or switching repos,
/// swaps in a fresh session through structural identity: the SwiftUI
/// analogue of the `{#key}` remount the Tauri client uses.
@MainActor
@Observable
final class TerminalStore {
    /// 0 = no live session; greater values identify the current one. Bumped
    /// by New Session, zeroed by close.
    private(set) var generation = 0

    /// Whether the panel shows the terminal or only the header strip.
    /// Collapsing hides the view but keeps the PTY running — the Tauri
    /// client's `display: none`, as opposed to closing.
    private(set) var isExpanded = false

    /// Header label for the shell that actually launched, from
    /// `StartedTerminal.shellLabel` — what you got is never a guess. Empty
    /// until the session reports in.
    var activeShellLabel = ""

    /// A command waiting to be typed into the shell, cleared by the session
    /// view once it has been. The panel is what owns the terminal, so a
    /// caller elsewhere in the screen — the commit composer offering to fix
    /// an unready AI provider — asks here rather than reaching for a session
    /// that may not exist yet.
    private(set) var pendingCommand: String?

    var hasSession: Bool { generation > 0 }

    /// Show the terminal and type `command` into it, spawning the shell first
    /// if this is the first expand.
    ///
    /// The app hands a fix to its own terminal rather than running it
    /// silently: an interactive `claude auth login` needs a real terminal to
    /// print its URL into, and asking for an auth code in app chrome is a
    /// habit worth not teaching. The command is visible, and the user can see
    /// exactly what ran.
    func run(_ command: String) {
        if generation == 0 { generation = 1 }
        isExpanded = true
        pendingCommand = command
    }

    /// The session typed `pendingCommand`; forget it so a later redraw can't
    /// replay it.
    func commandDidRun() {
        pendingCommand = nil
    }

    /// ⌘` and the header buttons. The PTY is lazy: none exists until the
    /// first expand asks for one.
    func toggle() {
        if !isExpanded, generation == 0 {
            generation = 1
        }
        isExpanded.toggle()
    }

    /// Replace the current session with a fresh shell (the header's +). The
    /// outgoing session's view unmounts and kills its own PTY.
    func startNewSession() {
        activeShellLabel = ""
        generation += 1
        isExpanded = true
    }

    /// Drop the session and collapse. The header's ✕, the shell exiting by
    /// itself, and switching repos all land here — sessions never outlive
    /// the repo they were opened in.
    func closeSession() {
        generation = 0
        isExpanded = false
        activeShellLabel = ""
        // Anything still waiting for a shell dies with the panel rather than
        // being typed into whichever session opens next.
        pendingCommand = nil
    }
}
