import Foundation

/// The way back from the last History action: what the strip says, and when it
/// stops offering. It lives in client memory only — after a restart the
/// branch's reflog is the way back. Mirrors the Tauri client's `undoOffer.ts`,
/// as `OpenAction` below does.
struct UndoOffer: Equatable {
    /// The History actions that can be taken back.
    enum Action: Equatable {
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
    /// for, newest first — what an Undo brings back, whatever a Continue made
    /// of them.
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

/// A History action that stopped on a conflict and left its operation open:
/// what is kept until the operation ends, so that a Continue which lands can
/// still be taken back and an Abort knows which branch to return to. Git keeps
/// no record of either. Written only by the app's own action, so an operation
/// begun in a terminal has none.
struct OpenAction: Equatable {
    /// What the action left open: a cherry-pick, or a squash's or reorder's
    /// rebase.
    let operation: OperationInProgress
    /// Where the action began, from core.
    let start: UndoStart
    let action: UndoOffer.Action
    /// The commits the action was asked for, newest first.
    let asked: [String]

    /// Whether this is still the operation `status` shows. It is forgotten the
    /// moment the status shows anything else — aborted, finished or quit, here
    /// or in a terminal, and a status that is not there at all. A cherry-pick
    /// is also held to its branch; a rebase cannot be, because it detaches HEAD
    /// and the status names no branch while one is open.
    func stillStands(under status: RepoStatus?) -> Bool {
        guard let status, status.operation == operation else { return false }
        return operation == .rebase || status.branch == start.branch
    }

    /// The undo point, now that a Continue has carried the operation to its end
    /// and `status` has been read again — or `nil` when none can be vouched
    /// for:
    ///
    /// - the status does not show the action's branch checked out, so its new
    ///   tip cannot be read;
    /// - `startedAt`, where git's own state says the operation began, is not
    ///   where this action began — the open operation was not the one it left
    ///   — or could not be read at all, which vouches for nothing;
    /// - the branch is where it began, so there is nothing to take back.
    ///
    /// A wrong `afterSha` would be harmless — core refuses a tip that is not
    /// the branch's — but nothing in core relates `beforeSha` to it, which is
    /// why the checks here are about the start.
    func undoPoint(startedAt: String?, under status: RepoStatus) -> UndoPoint? {
        guard !status.detached, status.branch == start.branch, !status.headSha.isEmpty else {
            return nil
        }
        guard startedAt == start.beforeSha else { return nil }
        guard status.headSha != start.beforeSha else { return nil }
        return UndoPoint(
            branch: start.branch,
            beforeSha: start.beforeSha,
            afterSha: status.headSha,
            returnBranch: start.returnBranch
        )
    }
}
