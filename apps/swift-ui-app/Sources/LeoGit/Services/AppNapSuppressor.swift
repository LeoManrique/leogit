import Foundation

/// Owns the `ProcessInfo` activity assertion that opts LeoGit out of App
/// Nap. Any held NSProcessInfo assertion makes the process ineligible for
/// napping (Energy Efficiency Guide's eligibility list); `.background` is
/// the option for app-initiated work — unlike `.userInitiated` it doesn't
/// also block idle system sleep, so the Mac still sleeps normally.
///
/// Why it exists: without the assertion, an unfocused app's `Task.sleep`
/// timers get coalesced and the background loops the scheduling policy
/// deliberately keeps alive (status poll, auto-fetch in a visible window)
/// silently stop ticking. `BackgroundSchedulingPolicy` drives this from its
/// state transitions, so the assertion is held exactly while there is a
/// reason for it.
@MainActor
final class AppNapSuppressor {
    private var token: (any NSObjectProtocol)?

    /// Idempotent: repeated calls with the same value are no-ops, so the
    /// policy can call this on every input change without bookkeeping.
    func setSuppressing(_ shouldSuppress: Bool) {
        if shouldSuppress, token == nil {
            token = ProcessInfo.processInfo.beginActivity(
                options: .background,
                reason: "LeoGit background git refresh"
            )
        } else if !shouldSuppress, let token {
            ProcessInfo.processInfo.endActivity(token)
            self.token = nil
        }
    }

    // Isolated so the non-Sendable token is reachable (a plain deinit is
    // nonisolated under Swift 6); without this cleanup a dropped suppressor
    // would leak its assertion for the process lifetime.
    isolated deinit {
        if let token {
            ProcessInfo.processInfo.endActivity(token)
        }
    }
}
