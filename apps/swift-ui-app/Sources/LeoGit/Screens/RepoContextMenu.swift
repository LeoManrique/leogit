import SwiftUI

/// A repository's context actions, defined once and attached wherever a
/// repository is *named* on screen — today the toolbar chip, which acts on the
/// open repository.
///
/// One builder rather than a menu written into each surface, for the same
/// reason `RepoPickerList` is one view rather than two lists: the copies drift,
/// and an action reachable from one surface but not its twin is the drift this
/// codebase keeps finding. The picker rows are the second call site, and the
/// only reason `terminal` is optional is that they will not have one.
///
/// Item order follows the house rule the changed-file menu sets
/// (`ChangesSidebar.singleRowMenu`): what changes something first, then the
/// copies, then the hand-offs to another program.
struct RepoContextMenu: View {
    let repoPath: String

    /// The name to copy — whatever label the calling surface is already
    /// showing. Passed in rather than derived here so the item copies the
    /// words under the pointer, instead of a second name that could disagree
    /// with them (the chip shows the folder name; a row can show the remote's).
    let displayName: String

    /// The terminal toggle, on the surfaces that have a terminal to toggle.
    /// The dock is `cwd`-ed to the *open* repository, so a surface listing
    /// repositories that are not open passes `nil`: offering it there would
    /// either lie about where the shell lands or force a switch nobody asked
    /// for.
    var terminal: TerminalCommand?

    /// A hand-off that failed. Reported and stepped over rather than
    /// presented, the way `ChangesSidebar` reports its own: taking the window
    /// because Finder would not open is a bigger interruption than the thing
    /// that failed.
    let onNotice: (String) -> Void

    var body: some View {
        if let terminal {
            // Wording mirrors View ▸ Show/Hide Terminal (`AppMenus.swift`), so
            // the two ways to reach the dock read as one item in two places.
            Button(terminal.isExpanded ? "Hide Terminal" : "Show Terminal") {
                terminal.toggle()
            }

            Divider()
        }

        Button("Copy Repository Path") { Clipboard.copy(repoPath) }
        Button("Copy Repository Name") { Clipboard.copy(displayName) }

        Divider()

        Button("Reveal in Finder") {
            Task {
                do {
                    // The repository folder itself: core joins the relative
                    // path onto the repository root, so joining nothing
                    // leaves the root — no core change needed for the
                    // directory case.
                    try await GitBridge.revealInFileManager(in: repoPath, relativePath: "")
                } catch {
                    onNotice(error.displayMessage)
                }
            }
        }
    }
}
