import Foundation

/// How a cherry-pick ended. `OpOutcome`'s three answers plus the one that is
/// neither a success nor a failure to retry: the pick stopped on a conflict,
/// which has already moved the repository onto the target and left the
/// operation open for Continue or Abort.
enum CherryPickOutcome {
    /// Every commit landed. The ids are the new commits, newest first.
    case picked([String])

    /// Stopped on a conflict, with git's own text.
    case stoppedOnConflict(String)

    /// The write slot was held; nothing was attempted and nothing changed.
    case refusedBusy

    /// Refused, or failed for a reason that is not a conflict. Core has put
    /// the repository back where it began, or the text says where it was left.
    case failed(String)
}

/// The History actions that replay commits — what starts them, under the
/// window's one write slot — and the one thing about them git keeps no record
/// of: which branch a cherry-pick came from.
///
/// Like `BranchStore`, an action returns its outcome instead of storing it,
/// and re-reading the repository afterwards is the caller's job.
@MainActor
@Observable
final class HistoryActionStore {
    /// Where a cherry-pick begun here came from and went to.
    struct CherryPickReturn: Equatable {
        let source: String
        let target: String
    }

    private let gate: RepositoryWriteGate

    /// Set while a cherry-pick begun here is stopped on a conflict, so that
    /// aborting it can end on the branch the commits came from. Forgotten by
    /// one data-driven rule — `forgetCherryPickUnlessOpen(in:)`.
    private(set) var cherryPickReturn: CherryPickReturn?

    /// An action of this store's own is running, for wording its progress.
    private(set) var isRunning = false

    /// A repository write is running — this store's or anyone else's — so no
    /// History action can start.
    var isBlocked: Bool { gate.isHeld }

    init(gate: RepositoryWriteGate) {
        self.gate = gate
    }

    /// Forget everything on repo switch.
    func reset() {
        cherryPickReturn = nil
        isRunning = false
    }

    /// Ask core whether a History action can start at all, before any sheet
    /// opens for it. Answers the reason it cannot, or `nil` when it can.
    /// `replayedFrom` is `nil` for cherry-pick, which replays nothing here.
    func refusal(replayedFrom: String?, repoPath: String) async -> String? {
        do {
            return try await GitBridge.historyActionPreflight(
                in: repoPath,
                replayedFrom: replayedFrom
            ).blocked
        } catch {
            return error.displayMessage
        }
    }

    /// Copy `shas` (newest first) from `source`, the checked-out branch, onto
    /// `target`.
    func cherryPick(
        _ shas: [String],
        from source: String,
        onto target: String,
        repoPath: String
    ) async -> CherryPickOutcome {
        guard let claim = gate.claim() else { return .refusedBusy }
        isRunning = true
        defer {
            isRunning = false
            gate.release(claim)
        }
        do {
            let result = try await GitBridge.cherryPick(in: repoPath, shas: shas, onto: target)
            if result.success {
                return .picked(result.selection)
            }
            cherryPickReturn = CherryPickReturn(source: source, target: target)
            return .stoppedOnConflict(
                result.errorMessage ?? "The cherry-pick stopped on a conflict in “\(target)”."
            )
        } catch {
            return .failed(error.displayMessage)
        }
    }

    /// The rule that ends `cherryPickReturn`: a status that no longer shows a
    /// cherry-pick open on the target. Finishing it, aborting it, doing either
    /// from the terminal and a status for another branch all arrive this way,
    /// so nothing that ends a pick has to remember to clear it.
    func forgetCherryPickUnlessOpen(in status: RepoStatus?) {
        guard let pending = cherryPickReturn else { return }
        if status?.operation != .cherryPick || status?.branch != pending.target {
            cherryPickReturn = nil
        }
    }
}
