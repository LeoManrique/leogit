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

    /// One branch operation at a time; the menu disables itself while set.
    private(set) var isBusy = false

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
        isBusy = false
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

    /// Abort an in-progress merge, restoring the pre-merge working tree.
    func abortMerge(repoPath: String) async -> OpOutcome {
        await run(repoPath: repoPath) {
            try await GitBridge.abortMerge(in: repoPath)
        }
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
    private func run(
        repoPath: String,
        _ body: () async throws -> Void
    ) async -> OpOutcome {
        guard !isBusy else { return .refusedBusy }
        isBusy = true
        defer { isBusy = false }
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
