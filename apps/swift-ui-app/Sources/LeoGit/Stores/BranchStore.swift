import Foundation

/// A branch's short name is unique within one listing (locals are plain,
/// remotes are prefixed `origin/…`), so it identifies the row.
extension BranchInfo: Identifiable {
    public var id: String { name }
}

/// Observable state for branch management: the branch list plus every
/// mutation the branch menu offers (switch, create, delete, merge, abort).
///
/// Mutations return their outcome instead of storing it, because different
/// surfaces present failures differently — the menu raises an alert, the
/// create sheet shows the text inline and stays open. After any successful
/// mutation the list is reloaded here; refreshing the *working tree* (status,
/// history) is the caller's job, since only it knows whether HEAD moved.
@MainActor
@Observable
final class BranchStore {
    private(set) var branches: [BranchInfo] = []

    /// The window's one write slot. Every mutation here claims it, so a branch
    /// operation and a commit, a Continue or a History action never overlap.
    private let gate: RepositoryWriteGate

    /// One of *this store's* operations is running. What a surface words its
    /// own progress from — the spinner in the create and merge sheets, the
    /// dimmed chip — and never what gates a start: see `isBlocked`.
    ///
    /// A count rather than a flag: a bridge call cannot be cancelled, so an
    /// operation on the repository the window has left can still be finishing
    /// under one begun on the new repository, and must not announce its end.
    var isRunning: Bool { running > 0 }
    private var running = 0

    /// A repository write is running — this store's or anyone else's — so no
    /// branch operation can start. Items and buttons disable on this; they do
    /// not claim to be working on it, since the work may be a commit's.
    var isBlocked: Bool { gate.isHeld }

    init(gate: RepositoryWriteGate) {
        self.gate = gate
    }

    /// The repository the published `branches` is allowed to describe.
    ///
    /// `list_branches` is a blocking bridge call, so cancelling the task that
    /// asked for it does not stop it: tearing down repository A's
    /// `.task(id: repoPath)` leaves A's listing running, and it lands *after*
    /// B has opened. Without a record of which repository a list describes, the
    /// menu would then offer A's branches while pointed at B — and clicking one
    /// runs `git checkout <A-branch>` in B, which is a real checkout of a
    /// branch that may not exist there, or worse, one that does.
    ///
    /// This is the guard `loadGeneration` cannot be. Recency only orders
    /// requests; it cannot tell which repository one is *for*. A merge on A
    /// that finishes after the user has switched to B calls `load(A)` from
    /// `run`, which is by then the newest request — so on recency alone A's
    /// list wins and B's is discarded, which is the wrong answer arrived at by
    /// a correct-looking rule.
    @ObservationIgnored private var currentRepo: String?

    /// Which listing the published `branches` is allowed to come from.
    ///
    /// The other half, and it closes a different race: two listings for *one*
    /// repository, landing out of order. The toolbar menu reloads on open, the
    /// poll reloads when HEAD moves, and a branch action reloads when it
    /// finishes — so several `load(A)` calls are ordinary, and the one that
    /// finishes last is not necessarily the one asked for last.
    /// `RepoStore.openGeneration` is the same device for the same reason.
    ///
    /// Bumped by `reset(for:)` — the repo switch — *and* by every `load`.
    @ObservationIgnored private var loadGeneration = 0

    var localBranches: [BranchInfo] { branches.filter { !$0.isRemote } }
    var remoteBranches: [BranchInfo] { branches.filter(\.isRemote) }

    /// Reload the branch list. Failures keep the previous list — the same
    /// silent-refresh contract as the Tauri client, where a listing hiccup
    /// must not blank a menu the user is looking at.
    ///
    /// A result for a repository the app has left, or one that is no longer the
    /// newest asked for, is dropped rather than published; see `currentRepo`
    /// and `loadGeneration` for which of the two closes which race.
    ///
    /// A load for another repository is refused *before* it claims a
    /// generation, not merely discarded when it returns. Claiming one on the
    /// way to being thrown away would invalidate the current repository's
    /// listing while that listing is still in flight — leaving the menu empty,
    /// by way of a guard whose whole job was to keep it right.
    func load(repoPath: String) async {
        guard repoPath == currentRepo else { return }
        loadGeneration += 1
        let generation = loadGeneration
        let fresh = try? await GitBridge.branches(in: repoPath)
        // Both guards again, after the await: a switch can land while the
        // listing is out, and so can a newer load for this same repository.
        guard repoPath == currentRepo, generation == loadGeneration, let fresh else { return }
        branches = fresh
    }

    /// Forget everything on repo switch, and adopt `repoPath` as the repository
    /// this store now speaks for.
    ///
    /// Bumps the generation as well as blanking the list: a listing for the
    /// repository being left can still be in flight, and clearing an array
    /// that is about to be refilled by it clears nothing.
    ///
    /// Takes the path rather than deriving it, because the caller — the screen's
    /// per-repository setup — is the thing that *knows* the switch happened;
    /// a store that guessed from the first `load` to arrive would be trusting
    /// the very ordering this guards against.
    func reset(for repoPath: String) {
        loadGeneration += 1
        currentRepo = repoPath
        branches = []
    }

    /// Check out `branch` (a remote-only name becomes a tracking branch).
    func switchTo(_ branch: String, repoPath: String) async -> OpOutcome {
        await run(repoPath: repoPath) {
            try await GitBridge.checkout(in: repoPath, branch: branch)
        }
    }

    /// The two-call "New Branch" flow: create off `HEAD`, then land on it.
    func createAndSwitch(named name: String, repoPath: String) async -> OpOutcome {
        await run(repoPath: repoPath) {
            try await GitBridge.newBranch(in: repoPath, named: name)
            try await GitBridge.checkout(in: repoPath, branch: name)
        }
    }

    /// Force-delete a local branch; the confirmation already happened.
    func delete(_ name: String, repoPath: String) async -> OpOutcome {
        await run(repoPath: repoPath) {
            try await GitBridge.removeBranch(in: repoPath, named: name)
        }
    }

    /// Abort the operation in progress — a merge, rebase, cherry-pick or
    /// revert. `said` is git's own text when the abort worked but did less
    /// than a full rewind (HEAD was moved by hand mid-sequence): something to
    /// read, not a failure.
    ///
    /// `source` is the branch a cherry-pick begun here came from, when that is
    /// what is being aborted: the pick checked its target out to do its work,
    /// and the abort has put that target back, so it ends on the branch the
    /// commits came from rather than stranding the user on one they never
    /// chose. Failing to get back there is `stranded`, beside a `succeeded`
    /// outcome rather than in place of it: the abort worked, saying it had not
    /// would send the user to retry an abort of nothing, and `said` still has
    /// to reach them.
    func abortOperation(
        returningTo source: String?,
        repoPath: String
    ) async -> (outcome: OpOutcome, said: String?, stranded: String?) {
        var said: String?
        var stranded: String?
        let outcome = await run(repoPath: repoPath) {
            said = try await GitBridge.abortStoppedOperation(in: repoPath)
            guard let source else { return }
            do {
                try await GitBridge.checkout(in: repoPath, branch: source)
            } catch {
                stranded =
                    "The cherry-pick was aborted, but “\(source)” could not be checked out again:\n\(error.displayMessage)"
            }
        }
        return (outcome, said, stranded)
    }

    /// Merge `source` into the current branch. Squash is the same two-call
    /// sequence as the Tauri handler: stage via `merge --squash`, then commit
    /// with git's generated message. A conflicted merge reports its text here
    /// while the conflicted files land in the ordinary changes list.
    func merge(_ source: String, squash: Bool, repoPath: String) async -> OpOutcome {
        await run(repoPath: repoPath) {
            let result = squash
                ? try await GitBridge.squashMerge(in: repoPath, branch: source)
                : try await GitBridge.merge(in: repoPath, branch: source)
            guard result.success else {
                throw GitError.Failed(message: result.errorMessage ?? "Merge failed.")
            }
            if squash {
                try await GitBridge.commitSquash(in: repoPath)
            }
        }
    }

    /// Busy-guard one mutation, reload the branch list, and report which of
    /// the three things happened.
    ///
    /// The guard answers `refusedBusy` rather than `succeeded`'s old `nil`:
    /// serializing the operations is what stops two checkouts contending on
    /// `index.lock`, but a guard that lies about what it did just moves the
    /// damage from git to the UI.
    ///
    /// The reload below is `load(repoPath:)` and inherits its guards, which is
    /// the point: a merge on the repository the user has since left still
    /// finishes, and its reload then does nothing rather than publishing a
    /// listing of the wrong repository or dropping the right one. The outcome
    /// still goes back to whoever asked, so the merge is reported where it was
    /// started.
    ///
    /// `body` is non-escaping and runs before this returns, on purpose:
    /// `abortOperation` hands git's words back through a variable it captures.
    private func run(
        repoPath: String,
        _ body: () async throws -> Void
    ) async -> OpOutcome {
        guard let claim = gate.claim() else { return .refusedBusy }
        running += 1
        defer {
            running -= 1
            gate.release(claim)
        }
        do {
            try await body()
            await load(repoPath: repoPath)
            return .succeeded
        } catch {
            // A failed merge still changed the world (MERGE_HEAD, conflicted
            // index), so reload here too; the caller refreshes status.
            await load(repoPath: repoPath)
            return .failed(error.displayMessage)
        }
    }
}
