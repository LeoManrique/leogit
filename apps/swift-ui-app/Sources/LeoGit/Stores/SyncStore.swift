import Foundation

/// A network operation against the remote. One runs at a time — the same
/// single-slot mutual exclusion as the Tauri client's `activeNetworkOp`: no
/// push while pulling, no second push, and callers pause their own background
/// refreshes while the slot is taken.
enum NetworkOperation {
    case fetch
    case pull
    case push
}

/// Observable state for the sync flow: which network operation is in flight
/// plus its latest progress tick.
///
/// Mutations return core's error text (`nil` on success) instead of storing
/// it, the same contract as `BranchStore` — the toolbar raises an alert.
/// Refreshing the working tree afterwards is the caller's job; there is no
/// completion event from core, so "done" is simply the mutation returning
/// (the Tauri client drives its busy state off the same await).
@MainActor
@Observable
final class SyncStore {
    /// The single in-flight operation; the sync buttons disable while set.
    private(set) var activeOperation: NetworkOperation?

    /// Aggregate progress of the in-flight operation, 0–100. `nil` before the
    /// first tick and for fetch, which streams no progress — show an
    /// indeterminate bar then. Cleared the moment the operation ends, success
    /// or failure, so the bar never parks at its last value.
    private(set) var progressPercent: Double?

    /// The raw git progress line, shown verbatim
    /// ("Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s").
    private(set) var progressText: String?

    /// Invalidates progress ticks from an operation that already ended: the
    /// stderr-reader thread can deliver a straggler after the await resolves,
    /// which must not repaint a dismissed bar.
    private var generation = 0

    /// Forget everything on repo switch.
    func reset() {
        activeOperation = nil
        progressPercent = nil
        progressText = nil
        generation += 1
    }

    /// `git fetch` the repository's remote, updating ahead/behind counts
    /// without touching the working tree.
    func fetch(repoPath: String) async -> String? {
        await run(.fetch) {
            let remote = try await GitBridge.remoteName(in: repoPath)
            try await GitBridge.fetchRemote(in: repoPath, remote: remote)
        }
    }

    /// One fetch attempt that claims no slot and surfaces no errors — the
    /// open-a-repo warm-up the Tauri client also runs at startup, so the
    /// behind badge reflects the remote within moments. Returns whether the
    /// fetch reached the remote (callers refresh status only then).
    func silentFetch(repoPath: String) async -> Bool {
        guard activeOperation == nil else { return false }
        guard let remote = try? await GitBridge.remoteName(in: repoPath) else { return false }
        return (try? await GitBridge.fetchRemote(in: repoPath, remote: remote)) != nil
    }

    /// `git pull --ff` from the repository's remote, streaming progress.
    func pull(repoPath: String) async -> String? {
        await run(.pull) {
            let remote = try await GitBridge.remoteName(in: repoPath)
            try await GitBridge.pullRemote(
                in: repoPath,
                remote: remote,
                onProgress: self.progressHandler()
            )
        }
    }

    /// `git push` the given branch, streaming progress. `setUpstream` is
    /// derived from `RepoStatus.hasUpstream` by the caller; `forceWithLease`
    /// only ever arrives from the confirmed force-push dialog.
    func push(
        repoPath: String,
        branch: String,
        setUpstream: Bool,
        forceWithLease: Bool = false
    ) async -> String? {
        await run(.push) {
            let remote = try await GitBridge.remoteName(in: repoPath)
            try await GitBridge.pushRemote(
                in: repoPath,
                remote: remote,
                branch: branch,
                setUpstream: setUpstream,
                forceWithLease: forceWithLease,
                onProgress: self.progressHandler()
            )
        }
    }

    /// A progress closure bound to the current generation: ticks arrive on a
    /// Rust background thread, hop to the main actor here, and are dropped if
    /// their operation already ended.
    private func progressHandler() -> @Sendable (SyncProgress) -> Void {
        let expected = generation
        return { [weak self] tick in
            Task { @MainActor [weak self] in
                guard let self, self.generation == expected, self.activeOperation != nil else {
                    return
                }
                self.progressPercent = Double(tick.percent)
                self.progressText = tick.text
            }
        }
    }

    /// Claim the operation slot, run one mutation, and map any failure to its
    /// display text. Progress is cleared at both ends: a new operation starts
    /// from zero, and neither success nor failure leaves a parked bar behind.
    private func run(
        _ operation: NetworkOperation,
        _ body: () async throws -> Void
    ) async -> String? {
        guard activeOperation == nil else { return nil }
        activeOperation = operation
        progressPercent = nil
        progressText = nil
        defer {
            generation += 1
            activeOperation = nil
            progressPercent = nil
            progressText = nil
        }
        do {
            try await body()
            return nil
        } catch {
            return error.displayMessage
        }
    }
}
