import SwiftUI

/// Shown until a repository is open: the app's name over the same repository
/// list the toolbar switcher shows.
///
/// It is a *picker*, not a splash screen. It used to be a logo and two buttons
/// with no discovery run behind them, so a fresh install — the one launch with
/// no repository to restore — met a screen that knew about every repository on
/// the machine and offered none of them. The list is the answer, and it is why
/// this screen no longer carries an `Open Repository…` action: that existed
/// only because there was no list here, and a per-folder open would let the
/// rows disagree with the scan paths that are supposed to define them.
struct WelcomeView: View {
    let coreVersion: String
    let directory: RepoDirectoryStore
    let identifiers: RepoIdentifierStore
    let onSelect: (String) -> Void
    let onClone: () -> Void
    let onChooseFolders: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "point.3.filled.connected.trianglepath.dotted")
                .font(.system(size: 40))
                .foregroundStyle(.tint)

            Text("LeoGit")
                .font(.largeTitle.weight(.semibold))

            RepoPickerList(
                // Nothing is open yet, so no row is checkmarked and none is
                // pinned to the top.
                activePath: nil,
                directory: directory,
                identifiers: identifiers,
                // No repository is open, so there is no transfer to hold
                // switching back.
                switchBlockedReason: nil,
                listMaxHeight: 320,
                onSelect: onSelect,
                onClone: onClone,
                onChooseFolders: onChooseFolders
            )
            .frame(maxWidth: 420)
            .background(.background.secondary, in: .rect(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.separator))

            if !coreVersion.isEmpty {
                // Proves the Rust bridge answered before any repo was opened.
                Text("core \(coreVersion)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
