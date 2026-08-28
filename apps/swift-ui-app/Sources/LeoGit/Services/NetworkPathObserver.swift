import Network
import Observation

/// The OS's answer to "is there a network right now?" — the native analogue
/// of the Tauri client's `navigator.onLine` check and `online`-event
/// recovery kick, which `ConnectivityBreaker` shipped without. One
/// `NWPathMonitor` feeds `isOnline`, and the offline→online edge runs every
/// handler registered through `onRecover(_:perform:)` — the repository
/// screen's catch-up, which resets the breaker instead of waiting out a
/// backoff window, and the release check's retry.
///
/// This type only observes — composing the signal with the breaker
/// (`RepoDirectoryStore.shouldAttemptBackground`) is the owner's job, so the
/// breaker stays pure backoff math and this stays pure OS plumbing.
@MainActor
@Observable
final class NetworkPathObserver {
    /// Whether the current network path is usable (`.satisfied`). Starts
    /// true — the monitor delivers the real state moments after `start`,
    /// and a false start would fire a spurious recovery kick on launch.
    private(set) var isOnline = true

    /// What runs on the offline→online edge, on the main actor, keyed by
    /// subscriber.
    ///
    /// More than one thing wants that edge — the repository screen's catch-up
    /// and the update check's retry — and each is registered from a `.task`
    /// that can run again, so the key makes re-registration replace rather
    /// than stack. A second `NWPathMonitor` per subscriber would be the other
    /// way to do it, and would pay the OS twice for one answer.
    @ObservationIgnored private var recoveryHandlers: [String: @MainActor () -> Void] = [:]

    @ObservationIgnored private let monitor = NWPathMonitor()

    init() {
        // The handler runs on the monitor's private queue; only the
        // Sendable verdict crosses to the main actor.
        monitor.pathUpdateHandler = { [weak self] path in
            let satisfied = path.status == .satisfied
            Task { @MainActor [weak self] in
                self?.update(isOnline: satisfied)
            }
        }
        monitor.start(queue: DispatchQueue(label: "leogit.network-path"))
    }

    deinit {
        // NWPathMonitor is Sendable, so the nonisolated deinit may touch it.
        monitor.cancel()
    }

    /// Register (or replace) what `key` runs when the network comes back.
    func onRecover(_ key: String, perform handler: @escaping @MainActor () -> Void) {
        recoveryHandlers[key] = handler
    }

    private func update(isOnline: Bool) {
        guard isOnline != self.isOnline else { return }
        self.isOnline = isOnline
        if isOnline {
            for handler in recoveryHandlers.values { handler() }
        }
    }
}
