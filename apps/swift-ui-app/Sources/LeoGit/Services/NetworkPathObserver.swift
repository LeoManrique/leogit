import Network
import Observation

/// The OS's answer to "is there a network right now?" — the native analogue
/// of the Tauri client's `navigator.onLine` check and `online`-event
/// recovery kick, which `ConnectivityBreaker` shipped without. One
/// `NWPathMonitor` feeds `isOnline`; the offline→online edge fires
/// `onRecover`, the hook the owner uses to reset the breaker and catch up
/// instead of waiting out a backoff window.
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

    /// Called on the offline→online edge, on the main actor. Registered by
    /// the repository screen; nil (welcome, no repo) means nothing to catch
    /// up.
    @ObservationIgnored var onRecover: (@MainActor () -> Void)?

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

    private func update(isOnline: Bool) {
        guard isOnline != self.isOnline else { return }
        self.isOnline = isOnline
        if isOnline {
            onRecover?()
        }
    }
}
