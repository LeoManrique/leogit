import Foundation

/// How a History action ended. `OpOutcome`'s three answers plus the one that
/// is neither a success nor a failure to retry: the action stopped on a
/// conflict, which has already moved the repository and left git's operation
/// open for Continue or Abort.
enum HistoryActionOutcome {
    /// Done. The ids are the commits the action produced, newest first — what
    /// History selects instead of jumping to the tip — and `undo` is the way
    /// back, `nil` for a run that changed nothing.
    case landed([String], undo: UndoOffer?)

    /// Stopped on a conflict, with git's own text.
    case stoppedOnConflict(String)

    /// The write slot was held; nothing was attempted and nothing changed.
    case refusedBusy

    /// Refused, or failed for a reason that is not a conflict. Core has put
    /// the repository back where it began, or the text says where it was left.
    case failed(String)
}

/// How an undo ended.
enum UndoOutcome {
    /// The branch is back. The ids are the commits the action had been asked
    /// for, to select again; `note` is what did not follow — the branch the
    /// action had left could not be checked out again.
    case undone(restores: [String], note: String?)

    /// The branch is not where the action left it, so this can never be undone
    /// any more: the offer is gone, and this is why.
    case expired(String)

    /// The write slot was held; nothing was attempted and nothing changed.
    case refusedBusy

    /// A refusal that may not hold next time — tracked changes, an operation
    /// in progress — or git's own failure. Nothing moved, and the offer stands.
    case failed(String)
}

/// Whether a squash may open its sheet, and with what.
enum SquashReadiness {
    /// The message the sheet opens with, and whether any of the commits is
    /// already on the upstream — so the next push would be a force push.
    case ready(draft: SquashDraft, rewritesPushed: Bool)

    /// Core's reason it cannot start.
    case refused(String)
}

/// Whether a reorder may run, once its destination is known.
enum ReorderReadiness {
    /// It may. `rewritesPushed` says a commit it would replay is already on
    /// the upstream — reorder has no sheet of its own to say so in, so that is
    /// what makes it ask first.
    case ready(rewritesPushed: Bool)

    /// Core's reason it cannot start.
    case refused(String)
}

/// The History actions that replay commits — what starts them and what takes
/// them back, under the window's one write slot — and the two things about
/// them git keeps no record of: which branch a cherry-pick came from, and the
/// way back from the last action.
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

    /// The way back from the last action that moved a branch, while it is still
    /// good. Retired by its ✕, by an undo, and by one data-driven rule —
    /// `retireUndoUnlessStanding(in:)`.
    private(set) var undoOffer: UndoOffer?

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
        undoOffer = nil
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
    ) async -> HistoryActionOutcome {
        guard let claim = gate.claim() else { return .refusedBusy }
        isRunning = true
        defer {
            isRunning = false
            gate.release(claim)
        }
        do {
            let result = try await GitBridge.cherryPick(in: repoPath, shas: shas, onto: target)
            if result.success {
                return .landing(result, after: .cherryPick(target: target), of: shas)
            }
            cherryPickReturn = CherryPickReturn(source: source, target: target)
            return .stoppedOnConflict(
                result.errorMessage ?? "The cherry-pick stopped on a conflict in “\(target)”."
            )
        } catch {
            return .failed(error.displayMessage)
        }
    }

    /// Ask core whether `shas` can be squashed, and for the message the sheet
    /// opens with. The preflight is given the oldest of them — the last, since
    /// they arrive newest first — which is what makes core look for a merge in
    /// the stretch a squash replays and say whether pushed commits are in it.
    func prepareSquash(_ shas: [String], repoPath: String) async -> SquashReadiness {
        do {
            let preflight = try await GitBridge.historyActionPreflight(
                in: repoPath,
                replayedFrom: shas.last
            )
            if let blocked = preflight.blocked { return .refused(blocked) }
            let draft = try await GitBridge.draftSquash(in: repoPath, shas: shas)
            return .ready(draft: draft, rewritesPushed: preflight.rewritesPushed)
        } catch {
            return .refused(error.displayMessage)
        }
    }

    /// Fold `shas` into the oldest of them under the message the sheet holds.
    /// The three parts go through the composer's own formatter, so co-authors
    /// take one path to a commit message.
    func squash(
        _ shas: [String],
        summary: String,
        description: String,
        coAuthors: [String],
        repoPath: String
    ) async -> HistoryActionOutcome {
        guard let claim = gate.claim() else { return .refusedBusy }
        isRunning = true
        defer {
            isRunning = false
            gate.release(claim)
        }
        do {
            let message = await GitBridge.commitMessage(
                summary: summary,
                description: description,
                coAuthors: coAuthors
            )
            let result = try await GitBridge.squash(in: repoPath, shas: shas, message: message)
            if result.success {
                return .landing(result, after: .squash, of: shas)
            }
            return .stoppedOnConflict(result.errorMessage ?? "The squash stopped on a conflict.")
        } catch {
            print("[history] squash failed: \(error.displayMessage)")
            return .failed(error.displayMessage)
        }
    }

    /// Ask core whether `shas` can be moved to just under `beforeSha` (`nil`
    /// is the tip). Not the shared preflight: what a reorder replays — and so
    /// whether a merge or a pushed commit is in it — depends on the
    /// destination.
    func prepareReorder(
        _ shas: [String],
        under beforeSha: String?,
        repoPath: String
    ) async -> ReorderReadiness {
        do {
            let preflight = try await GitBridge.preflightReorder(
                in: repoPath,
                shas: shas,
                under: beforeSha
            )
            if let blocked = preflight.blocked { return .refused(blocked) }
            return .ready(rewritesPushed: preflight.rewritesPushed)
        } catch {
            return .refused(error.displayMessage)
        }
    }

    /// Move `shas` as one block to just under `beforeSha`, or to the tip.
    func reorder(
        _ shas: [String],
        under beforeSha: String?,
        repoPath: String
    ) async -> HistoryActionOutcome {
        guard let claim = gate.claim() else { return .refusedBusy }
        isRunning = true
        defer {
            isRunning = false
            gate.release(claim)
        }
        do {
            let result = try await GitBridge.reorder(in: repoPath, shas: shas, under: beforeSha)
            if result.success {
                return .landing(result, after: .reorder, of: shas)
            }
            return .stoppedOnConflict(result.errorMessage ?? "The reorder stopped on a conflict.")
        } catch {
            print("[history] reorder failed: \(error.displayMessage)")
            return .failed(error.displayMessage)
        }
    }

    // MARK: Undo

    /// Put an action's way back on offer. Called once the repository has been
    /// re-read: the rule that retires an offer reads the status, and the one
    /// from before the action still shows the old tip. A run that changed
    /// nothing offers nothing, and leaves the previous offer as good as it was.
    func offer(_ undo: UndoOffer?) {
        if let undo { undoOffer = undo }
    }

    /// The strip's ✕.
    func dismissUndo() {
        undoOffer = nil
    }

    /// The rule that retires an offer: a status that shows its branch somewhere
    /// the action did not leave it. A commit, an amend, a pull or a rebase —
    /// here or in a terminal — all arrive this way, so nothing that moves a
    /// branch has to remember to clear it.
    func retireUndoUnlessStanding(in status: RepoStatus?) {
        guard let undoOffer, let status else { return }
        if !undoOffer.stillStands(under: status) { self.undoOffer = nil }
    }

    /// Take the offered action back.
    func undo(repoPath: String) async -> UndoOutcome {
        guard let offer = undoOffer else { return .failed("There is nothing to undo.") }
        guard let claim = gate.claim() else { return .refusedBusy }
        defer { gate.release(claim) }
        do {
            let result = try await GitBridge.undo(in: repoPath, point: offer.point)
            // Undone or expired, this offer is spent — unless the window has
            // moved on meanwhile and holds another.
            if undoOffer == offer { undoOffer = nil }
            if result.undone {
                return .undone(restores: offer.restores, note: result.message)
            }
            return .expired(result.message ?? "This can no longer be undone.")
        } catch {
            print("[history] undo failed: \(error.displayMessage)")
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

extension HistoryActionOutcome {
    /// A clean run of `action` over `asked` (newest first): what it produced,
    /// and its way back when core handed one over.
    fileprivate static func landing(
        _ result: RewriteResult,
        after action: UndoOffer.Action,
        of asked: [String]
    ) -> Self {
        .landed(
            result.selection,
            undo: result.undo.map { UndoOffer(after: action, of: asked, point: $0) }
        )
    }
}
