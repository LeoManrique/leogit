import Foundation

/// Circuit breaker for background network work — the native port of the Tauri
/// client's `services/connectivity.ts`, with the same numbers: after two
/// consecutive failures the breaker opens for 30 s, doubling per further
/// failure up to a 5 min cap; any success closes it again. Background fetches
/// consult it before touching the network, so an unreachable remote (offline,
/// VPN down, dead host) is retried on a backoff instead of hammered on every
/// tick.
///
/// The OS half lives in `NetworkPathObserver` (Network.framework's
/// `NWPathMonitor` — the analogue of the Tauri `navigator.onLine` +
/// `online` event this type once shipped without):
/// `RepoDirectoryStore.shouldAttemptBackground` composes
/// `isOnline && shouldAttempt` exactly like `shouldAttemptBackground()` in
/// `connectivity.ts`, and the observer's recovery kick calls `reset()`.
/// This type stays pure backoff math with zero OS imports — signals feed
/// it, they never live in it.
@MainActor
final class ConnectivityBreaker {
    private static let failureThreshold = 2
    private static let baseBackoff: TimeInterval = 30
    private static let maxBackoff: TimeInterval = 5 * 60

    private var consecutiveFailures = 0
    private var openUntil = Date.distantPast

    /// Whether background network work should be attempted right now.
    var shouldAttempt: Bool { Date.now >= openUntil }

    /// Close the breaker immediately — the OS said the network is back, so
    /// an open backoff window no longer describes reality. Not a success
    /// report: nothing was fetched, so nothing is being claimed beyond
    /// "stop waiting".
    func reset() {
        consecutiveFailures = 0
        openUntil = .distantPast
    }

    /// Report one background fetch's outcome. Only real attempts belong here —
    /// a fetch skipped because there was no remote to reach is not a signal
    /// about connectivity.
    func record(success: Bool) {
        guard !success else {
            reset()
            return
        }
        consecutiveFailures += 1
        guard consecutiveFailures >= Self.failureThreshold else { return }
        let doublings = consecutiveFailures - Self.failureThreshold
        let backoff = min(Self.baseBackoff * pow(2, Double(doublings)), Self.maxBackoff)
        openUntil = Date.now.addingTimeInterval(backoff)
    }
}
