import Foundation
import Observation

/// The once-per-session release check behind the update chip.
///
/// Releases don't ship often enough to poll for, so this is not a poll: one
/// *answer* ends it, and the interval below is a **retry** interval that only
/// ever fires while every attempt so far has failed. "You are current" is an
/// answer too — a check that comes back empty stops the loop exactly like one
/// that finds something.
///
/// Deliberately **not** gated on the connectivity breaker, only on the OS
/// path monitor. The breaker guards git remotes, and a rate-limited GitHub API
/// answer says nothing about those; worse, the breaker can be open for reasons
/// that have nothing to do with reachability, which would silently suppress
/// the check for the rest of the session. The outcome does not feed the
/// breaker either — the same argument in the other direction.
@MainActor
@Observable
final class UpdateStore {
    /// A release newer than this build, once one is known.
    private(set) var available: UpdateInfo?

    /// Dismissal is session-scoped and deliberately not persisted: like the
    /// check itself it resets on relaunch, so a skipped release resurfaces on
    /// the next start rather than being forgotten forever.
    var isDismissed = false

    /// What the chip shows, which is the only thing any view needs to ask.
    var visible: UpdateInfo? { isDismissed ? nil : available }

    /// Long enough that a machine that is offline for an afternoon costs a
    /// handful of requests, short enough that a laptop opened on a train
    /// finds out within the sitting.
    private static let retryInterval: Duration = .seconds(30 * 60)

    /// True once any attempt has come back with an answer, success or "you
    /// are current". The loop exits on it.
    @ObservationIgnored private var hasAnswer = false

    @ObservationIgnored private var loop: Task<Void, Never>?

    /// Start checking, and keep retrying until one attempt answers.
    ///
    /// Idempotent, so a view whose `.task` re-runs does not start a second
    /// loop. `isOnline` is read per attempt through the closure rather than
    /// captured, so the gate reflects the network at the moment of the
    /// attempt rather than at the moment of the call.
    func start(isOnline: @escaping @MainActor () -> Bool) {
        guard loop == nil, !hasAnswer else { return }
        loop = Task { @MainActor in
            while !Task.isCancelled && !hasAnswer {
                await attempt(isOnline: isOnline)
                if hasAnswer { break }
                try? await Task.sleep(for: Self.retryInterval)
            }
            // Deliberately does not clear `loop`: a cancelled loop finishes
            // *after* its replacement has been stored, so clearing from here
            // would drop the reference to a task still running and let the
            // next recovery start a third. `hasAnswer` is what ends this for
            // good, and the only place `loop` is cleared is where it is
            // replaced.
        }
    }

    /// The offline→online edge: launching offline — a plane, a captive portal
    /// — is exactly when the first attempt was skipped, and waiting out the
    /// retry interval after the network returns would be half an hour of
    /// knowing better and saying nothing.
    func networkDidRecover(isOnline: @escaping @MainActor () -> Bool) {
        guard !hasAnswer else { return }
        loop?.cancel()
        loop = nil
        start(isOnline: isOnline)
    }

    /// One attempt. A failure is not an error the user is shown — it is
    /// "couldn't check", which is what the retry is for.
    private func attempt(isOnline: @MainActor () -> Bool) async {
        // While the OS says offline don't even spawn the request: it would
        // only time out and burn a retry slot.
        guard isOnline() else { return }
        do {
            let info = try await GitBridge.latestRelease()
            hasAnswer = true
            if let info {
                print("[update] v\(info.version) available")
                available = info
            }
        } catch {
            print("[update] check failed, will retry: \(error.displayMessage)")
        }
    }
}
