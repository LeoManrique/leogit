import Foundation

/// The way back from the last History action: what the strip says, and when it
/// stops offering. It lives in client memory only — after a restart the
/// branch's reflog is the way back. Mirrors the Tauri client's `undoOffer.ts`.
struct UndoOffer: Equatable {
    /// The History actions that can be taken back.
    enum Action {
        /// Onto the named branch.
        case cherryPick(target: String)
        case squash
        case reorder
    }

    /// What the action did, as the strip states it.
    let sentence: String
    /// What core needs to put the branch back.
    let point: UndoPoint
    /// The commits the action was asked for, newest first: what History
    /// selects again once they are back.
    let restores: [String]

    /// The offer an action leaves behind. `asked` are the commits it was asked
    /// for, newest first.
    init(after action: Action, of asked: [String], point: UndoPoint) {
        sentence = Self.landedSentence(action, count: asked.count)
        self.point = point
        restores = asked
    }

    /// What the strip says an action did to `count` commits.
    static func landedSentence(_ action: Action, count: Int) -> String {
        let commits = count == 1 ? "1 commit" : "\(count) commits"
        return switch action {
        case let .cherryPick(target): "Cherry-picked \(commits) onto “\(target)”."
        case .squash: "Squashed \(commits) into one."
        case .reorder: "Reordered \(commits)."
        }
    }

    /// Whether the offer still stands under `status`. It falls once the status
    /// shows the branch checked out at any commit but the one the action left
    /// it on — a commit, an amend, a pull or another rewrite has moved it, here
    /// or in a terminal — because an Undo that can only be refused is not left
    /// on screen to fail. While another branch is checked out, or HEAD is
    /// detached, the status cannot see that branch's tip: the offer stays, and
    /// core is the judge.
    func stillStands(under status: RepoStatus) -> Bool {
        if status.detached || status.branch != point.branch { return true }
        return status.headSha == point.afterSha
    }
}
