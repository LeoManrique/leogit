import SwiftUI

/// A repository's context actions, defined once and attached wherever a
/// repository is *named* on screen — today the toolbar chip, which acts on the
/// open repository.
///
/// One builder rather than a menu written into each surface, for the same
/// reason `RepoPickerList` is one view rather than two lists: the copies drift,
/// and an action reachable from one surface but not its twin is the drift this
/// codebase keeps finding.
struct RepoContextMenu: View {
    let repoPath: String

    /// The name to copy — whatever label the calling surface is already
    /// showing. Passed in rather than derived here so the item copies the
    /// words under the pointer, instead of a second name that could disagree
    /// with them.
    let displayName: String

    var body: some View {
        Button("Copy Repo Name") { Clipboard.copy(displayName) }
        Button("Copy Repo Path") { Clipboard.copy(repoPath) }
    }
}
