import Foundation

/// A network operation against the remote. One runs at a time — the same
/// single-slot mutual exclusion as the Tauri client's `activeNetworkOp`: no
/// push while pulling, no second push, and callers pause their own background
/// refreshes while the slot is taken.
enum NetworkOperation {
    case fetch
    case pull
    case push
    case publish
}

/// Why the open repository is being fetched in the background right now.
///
/// `scheduled` is the auto-fetch loop's own tick — a cadence the user chose in
/// Settings (5 s to an hour, 30 s by default), so it is honoured exactly and
/// never answered from the fetch cooldown, whose window is longer than most of
/// that range and would quietly halve it.
///
/// `catchUp` is every other automatic trigger: the open warm-up, the wake-up
/// resync, the connectivity kick. Each fires on an event rather than a clock,
/// and each can land seconds after a fetch that already answered the same
/// question — which is what the cooldown is for.
enum FetchTrigger {
    case scheduled
    case catchUp
}

/// Observable state for the sync flow: which network operation is in flight
/// plus its latest progress tick.
///
/// Mutations answer an `OpOutcome` instead of storing their failure
/// it, the same contract as `BranchStore` — the toolbar raises an alert.
/// Refreshing the working tree afterwards is the caller's job; there is no
/// completion event from core, so "done" is simply the mutation returning
/// (the Tauri client drives its busy state off the same await).
@MainActor
@Observable
final class SyncStore {
    /// The single in-flight operation; the sync buttons disable while set.
    /// Mirrored into the scheduling policy on every hand-off, so the
    /// background loops pause without capturing this store — the native
    /// replacement for each Tauri loop reading `activeNetworkOp`.
    private(set) var activeOperation: NetworkOperation? {
        didSet { schedulingPolicy.networkOpInFlight = activeOperation != nil }
    }

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

    /// The repositories with a silent fetch in flight, which take no
    /// `activeOperation` — it is not a user operation and must not disable the
    /// sync buttons or pause the poll. Four triggers reach `silentFetch` (the
    /// open warm-up, the fetch loop's tick, the wake-up resync and the
    /// connectivity kick) and nothing stopped two of them running `git fetch`
    /// in the same `.git` at once: the loser hits a ref lock, fails, and is
    /// charged to the breaker as if the network were down (F15).
    ///
    /// Keyed by path rather than a single flag, for the same reason
    /// `RepoDirectoryStore.inFlight` is: the race being prevented is two
    /// fetches in *one* `.git`, and a fetch of A runs on the background budget
    /// — up to 12 s — which easily outlives the switch to B. A process-wide
    /// claim would have let A's straggler turn away B's warm-up, leaving the
    /// repository the user just opened unfetched. Entries survive `reset()`
    /// for that same reason: A's fetch is still running after the switch, and
    /// the slot is released by its own `defer`, never by a switch.
    private var silentFetches: Set<String> = []

    /// Where `activeOperation`'s occupancy is published for the background
    /// loops. Injected: the policy and this store are created together by
    /// `ContentView`, and a store that could exist unpoliced would let a
    /// transfer run while background git work races it.
    private let schedulingPolicy: BackgroundSchedulingPolicy

    /// The process's one per-repository fetch stamp, owned by
    /// `RepoDirectoryStore` and injected here so the active repository's
    /// fetches and the tier sweeps read and write the same map rather than two
    /// that disagree.
    private let fetchCooldown: FetchCooldown

    init(schedulingPolicy: BackgroundSchedulingPolicy, fetchCooldown: FetchCooldown) {
        self.schedulingPolicy = schedulingPolicy
        self.fetchCooldown = fetchCooldown
    }

    /// Forget everything on repo switch.
    func reset() {
        activeOperation = nil
        progressPercent = nil
        progressText = nil
        generation += 1
    }

    /// `git fetch` the repository's remote, updating ahead/behind counts
    /// without touching the working tree.
    ///
    /// The user asked, so the fetch cooldown is never consulted — but a fetch
    /// that reached the remote opens one, because the background work that does
    /// consult it now holds a fresh answer.
    func fetch(repoPath: String) async -> OpOutcome {
        let outcome = await run(.fetch) {
            guard let remote = try await GitBridge.remoteName(in: repoPath) else {
                throw GitError.Failed(message: "This repository has no remote to fetch from.")
            }
            try await GitBridge.fetchRemote(in: repoPath, remote: remote, background: false)
        }
        if case .succeeded = outcome {
            fetchCooldown.note(repoPath)
        }
        return outcome
    }

    /// One fetch attempt that claims no slot and surfaces no errors — the
    /// open-a-repo warm-up the Tauri client also runs at startup, so the
    /// behind badge reflects the remote within moments.
    ///
    /// `nil` means **no attempt was made**: the transfer slot was taken,
    /// another silent fetch of this same repository already holds its slot, a
    /// `.catchUp` trigger found the last answer still fresh, or resolving the
    /// remote name failed
    /// locally. None
    /// of those says anything about the network, so callers must not report
    /// them to the connectivity breaker — a local `git remote` failure counted
    /// as an unreachable remote is the same class of poisoning D-2 was about.
    /// `true`/`false` mean a fetch actually ran and did or didn't reach the
    /// remote; only then does the caller refresh status, and only then does the
    /// breaker hear about it. (The Tauri client's `fetchActiveRemote` draws the
    /// same line, returning early without `recordResult` when `get_remote`
    /// fails.)
    func silentFetch(repoPath: String, trigger: FetchTrigger) async -> Bool? {
        guard activeOperation == nil, !silentFetches.contains(repoPath) else { return nil }
        if trigger == .catchUp, fetchCooldown.isFresh(repoPath) {
            fetchCooldown.logSkip(repoPath, "skipping the open repository's fetch")
            return nil
        }
        silentFetches.insert(repoPath)
        defer { silentFetches.remove(repoPath) }
        // `try?` flattens the throw and the "no remote" answer into one `nil`,
        // which is right here: both mean no fetch was attempted, and neither
        // says anything about the network.
        guard let remote = try? await GitBridge.remoteName(in: repoPath) else { return nil }
        // Nobody is waiting on this one, so it runs on the background budget:
        // an unreachable remote gives up in 12 s instead of holding the single
        // network slot — and every other repo's refresh behind it — for ten
        // minutes.
        let reached =
            (try? await GitBridge.fetchRemote(in: repoPath, remote: remote, background: true))
            != nil
        if reached {
            fetchCooldown.note(repoPath)
        }
        return reached
    }

    /// `git pull --ff` from the repository's remote, streaming progress.
    ///
    /// Its fetch half opens a cooldown window exactly as a bare fetch does; a
    /// push opens none, having learned nothing new about the remote.
    func pull(repoPath: String) async -> OpOutcome {
        let outcome = await run(.pull) {
            guard let remote = try await GitBridge.remoteName(in: repoPath) else {
                throw GitError.Failed(message: "This repository has no remote to pull from.")
            }
            try await GitBridge.pullRemote(
                in: repoPath,
                remote: remote,
                onProgress: self.progressHandler()
            )
        }
        if case .succeeded = outcome {
            fetchCooldown.note(repoPath)
        }
        return outcome
    }

    /// `git push` the given branch, streaming progress. `setUpstream` is
    /// derived from `RepoStatus.hasUpstream` by the caller; `forceWithLease`
    /// only ever arrives from the confirmed force-push dialog.
    func push(
        repoPath: String,
        branch: String,
        setUpstream: Bool,
        forceWithLease: Bool = false
    ) async -> OpOutcome {
        await run(.push) {
            // Unreachable through the UI — the ladder offers Publish, not
            // Push, for a repo with no remote — but saying so beats inventing
            // a remote name and letting git fail with something less clear.
            guard let remote = try await GitBridge.remoteName(in: repoPath) else {
                throw GitError.Failed(message: "This repository has no remote to push to.")
            }
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

    /// Publish a remote-less repository to GitHub via `gh repo create` —
    /// create the repo, wire `origin`, push the current branch. Claims the
    /// same single slot as push/pull (the Tauri client's `'publish'` op), so
    /// every background refresh pauses for its duration. `gh` streams no
    /// parseable progress: the banner stays indeterminate.
    func publish(
        repoPath: String,
        name: String,
        description: String,
        isPrivate: Bool
    ) async -> OpOutcome {
        await run(.publish) {
            try await GitBridge.publishToGitHub(
                repoPath: repoPath,
                name: name,
                description: description,
                isPrivate: isPrivate
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
    ) async -> OpOutcome {
        guard activeOperation == nil else { return .refusedBusy }
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
            return .succeeded
        } catch {
            return .failed(error.displayMessage)
        }
    }
}
