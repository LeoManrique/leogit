import Foundation

/// The one "a repository write is in flight" slot for a window.
///
/// Git has one `index.lock`, so anything that moves HEAD, a ref, the index or
/// the working tree runs one at a time: a second write started under the first
/// either fails on that lock or lands in the middle of it — an Abort confirmed
/// while a long Continue is still replaying commits, a branch switch under a
/// commit. The composer, the branch menu and the History actions are separate
/// stores that share no state, so they share this instead, and every surface
/// that starts a write goes inert on the same fact.
///
/// Network transfers keep their own slot (`SyncStore.activeOperation`): they
/// are waited on differently and carry progress. Appending to `.gitignore` is
/// not a holder either — it is a file edit that takes no git lock.
@MainActor
@Observable
final class RepositoryWriteGate {
    /// A write is running. Surfaces disable on this; the store that owns the
    /// write keeps its own flag for wording its own progress.
    private(set) var isHeld = false

    /// What a surface says while it cannot start because of `isHeld` — one
    /// wording everywhere, and the Tauri client's.
    static let busyMessage = "Another operation is still running."

    /// Proof of having claimed the slot, handed back to release it.
    ///
    /// It remembers which repository's turn it was claimed in, because a bridge
    /// call cannot be cancelled: a write still running for the repository the
    /// window has left finishes late, and releasing by flag alone would free a
    /// slot the *new* repository's write is holding.
    struct Claim {
        fileprivate let generation: Int
    }

    @ObservationIgnored private var generation = 0

    /// Take the slot, or `nil` when a write already holds it. A refusal is
    /// never a success: the caller reports `refusedBusy` and leaves its
    /// surface as it was.
    func claim() -> Claim? {
        guard !isHeld else { return nil }
        isHeld = true
        return Claim(generation: generation)
    }

    /// Give the slot back — from a `defer` beside the claim. A claim from
    /// before the last `reset()` releases nothing.
    func release(_ claim: Claim) {
        guard claim.generation == generation else { return }
        isHeld = false
    }

    /// The window moved to another repository: the slot is free for it, and
    /// whatever the old repository still has running no longer speaks for it.
    func reset() {
        generation += 1
        isHeld = false
    }
}
