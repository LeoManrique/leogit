import Foundation

/// What a sync control's chevron can offer beside its face.
enum SyncMenuAction {
    case fetch
    case pull
    case forcePush
}

/// The presentation half of core's `SyncProposal`, which is where the ladder
/// itself lives — one implementation for both clients, carried on
/// `RepoStatus.proposal` so the toolbar button, the ⌘P menu item and the Tauri
/// header can never disagree about what the repository needs next.
///
/// One property per aspect and every `switch` exhaustive, so a rung core adds
/// stops the build at each of them — and they are all here, not spread over the
/// view that draws them. Mirrors the Tauri client's `SYNC_FACES`
/// (`Header.svelte`). Only presentation stays on this side: the two controls
/// are shaped differently, and which state earns a chevron is a macOS question
/// rather than a policy one.
extension SyncProposal {
    /// The state word: the button face's title when idle, and the menu
    /// item's title always (a disabled "Pull" mid-pull reads better in a
    /// menu than "Pulling…").
    var title: String {
        switch self {
        case .loading, .fetch: "Fetch"
        case .detached: "Push"
        case .publishRepository: "Publish"
        case .publishBranch: "Publish Branch"
        case .forcePush: "Force Push"
        case .pull: "Pull"
        case .push: "Push"
        }
    }

    var symbol: String {
        switch self {
        case .loading, .fetch: "arrow.triangle.2.circlepath"
        case .detached, .push: "arrow.up"
        case .publishRepository: "icloud.and.arrow.up"
        case .publishBranch: "arrow.up.circle"
        case .forcePush: "arrow.up.to.line"
        case .pull: "arrow.down"
        }
    }

    /// Whether the proposal can be run at all — the two informational states
    /// have nothing to do, and both the button and the menu item say so by
    /// staying disabled.
    var isActionable: Bool {
        switch self {
        case .loading, .detached: false
        case .publishRepository, .publishBranch, .forcePush, .pull, .push, .fetch: true
        }
    }

    /// What the chevron offers: only what the face does not, so an empty list
    /// is a plain button. Publishing a repository has no secondary action, and
    /// in the fetch state the one item would *be* the face.
    ///
    /// A diverged branch keeps both ways out within reach, whichever of them
    /// core put on the face: Pull under Force Push — git's test says the remote
    /// holds nothing this branch never had, not that the user wants it gone —
    /// and the force push under Pull, where the control drops it again for a
    /// branch with nothing to push.
    var menu: [SyncMenuAction] {
        switch self {
        case .loading, .detached, .publishRepository, .fetch: []
        case .publishBranch, .push: [.fetch]
        case .forcePush: [.pull, .fetch]
        case .pull: [.fetch, .forcePush]
        }
    }

    /// The tooltip, given the pending counts a rung may want to name.
    func help(ahead: Int32, behind: Int32) -> String {
        switch self {
        case .loading:
            "Loading repository status"
        case .detached:
            "Detached HEAD — check out a branch to push"
        case .publishRepository:
            "Publish this repository to GitHub — creates the remote repo and pushes this branch"
        case .publishBranch:
            "Publish this branch to the remote and start tracking it"
        case .forcePush:
            "This branch was rewritten: replace \(Self.commits(behind)) on the remote "
                + "with \(Self.commits(ahead)) from here"
        case .pull:
            "Pull \(Self.commits(behind)) from the remote"
        case .push:
            "Push \(Self.commits(ahead)) to the remote"
        case .fetch:
            "Fetch from every remote — updates the ahead/behind counts without touching your files"
        }
    }

    private static func commits(_ count: Int32) -> String {
        "\(count) commit\(count == 1 ? "" : "s")"
    }
}
