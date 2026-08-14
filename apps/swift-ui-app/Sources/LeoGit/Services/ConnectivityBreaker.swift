import Foundation

/// Circuit breaker for background network work — the native port of the Tauri
/// client's `services/connectivity.ts`, with the same numbers: after two
/// consecutive failures the breaker opens for 30 s, doubling per further
/// failure up to a 5 min cap; any success closes it again. Background fetches
/// consult it before touching the network, so an unreachable remote (offline,
/// VPN down, dead host) is retried on a backoff instead of hammered on every
/// tick.
///
/// Deviation from the Tauri client: no OS online/offline signal (its
/// `navigator.onLine` check and `online`-event recovery kick have no free
/// AppKit analogue). The recovery paths here are the backoff expiring and the
/// refocus resync — which the user triggers naturally by coming back to the
/// app once their connection returns.
@MainActor
final class ConnectivityBreaker {
    private static let failureThreshold = 2
    private static let baseBackoff: TimeInterval = 30
    private static let maxBackoff: TimeInterval = 5 * 60

    private var consecutiveFailures = 0
    private var openUntil = Date.distantPast

    /// Whether background network work should be attempted right now.
    var shouldAttempt: Bool { Date.now >= openUntil }

    /// Report one background fetch's outcome. Only real attempts belong here —
    /// a fetch skipped because there was no remote to reach is not a signal
    /// about connectivity.
    func record(success: Bool) {
        guard !success else {
            consecutiveFailures = 0
            openUntil = .distantPast
            return
        }
        consecutiveFailures += 1
        guard consecutiveFailures >= Self.failureThreshold else { return }
        let doublings = consecutiveFailures - Self.failureThreshold
        let backoff = min(Self.baseBackoff * pow(2, Double(doublings)), Self.maxBackoff)
        openUntil = Date.now.addingTimeInterval(backoff)
    }
}
