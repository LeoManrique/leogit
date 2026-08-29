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

    var localBranches: [BranchInfo] { branches.filter { !$0.isRemote } }
    var remoteBranches: [BranchInfo] { branches.filter(\.isRemote) }

    /// Reload the branch list. Failures keep the previous list — the same
    /// silent-refresh contract as the Tauri client, where a listing hiccup
    /// must not blank a menu the user is looking at.
    func load(repoPath: String) async {
        if let fresh = try? await GitBridge.branches(in: repoPath) {
            branches = fresh
        }
    }

    /// Forget everything on repo switch.
    func reset() {
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
