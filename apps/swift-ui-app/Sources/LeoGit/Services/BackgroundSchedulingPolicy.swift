import AppKit
import SwiftUI

/// The one answer to "may background work run right now?" — every background
/// loop names the predicate it obeys instead of composing its own boolean
/// out of scattered state. This table is the policy (the GH-Desktop split,
/// plus a native improvement — the Tauri client runs everything always):
///
/// | Work                                   | Pauses on network op | Pauses when app inactive | Pauses when window not visible |
/// |----------------------------------------|----------------------|--------------------------|--------------------------------|
/// | Status poll (active repo, local)       | yes                  | no — slows 2 s → 10 s    | no — slows to 30 s             |
/// | Auto-fetch loop (active repo, network) | yes                  | no                       | no — interval stretched ×3     |
/// | Tier scheduler + sweeps (other repos)  | yes                  | yes                      | yes                            |
/// | Relative-date tick (History list)      | no — reads no git    | no                       | yes                            |
///
/// Rationale: a visible-but-not-key window keeps telling the truth (the
/// audit's "stale in plain sight" case — the web clients never had this
/// failure mode because DOM timers don't know about key windows), and a
/// hidden window keeps refreshing slowly so refocusing reveals a current
/// screen instead of a sudden catch-up — the cadence ladder makes hidden
/// work cheap rather than absent (GH Desktop fetches at one flat interval
/// regardless; we stretch it). The multi-repo fetch fan-out is the only
/// genuinely deferrable work, and the refocus resync remains its catch-up
/// path.
///
/// Also owns the App Nap side effect: the `AppNapSuppressor` assertion is
/// held exactly while a repo is open and some background work is allowed to
/// run, re-evaluated on every input change so it can never outlive its
/// reason. Without the assertion, an unfocused app's `Task.sleep` timers get
/// coalesced by App Nap and the whole policy would silently not apply.
@MainActor
@Observable
final class BackgroundSchedulingPolicy {
    // MARK: Inputs

    /// A push/pull/fetch/publish holds the single network slot — background
    /// git work stands aside so `git status` can't race a transfer's lock
    /// files. Mirrored in by `SyncStore` whenever the slot changes hands
    /// (the native replacement for each Tauri loop capturing
    /// `activeNetworkOp`).
    var networkOpInFlight = false {
        didSet { updateAppNapAssertion() }
    }

    /// Whether LeoGit is the frontmost app, tracked via
    /// `NSApplication.didBecomeActive/didResignActive`.
    private(set) var isAppActive = true {
        didSet { updateAppNapAssertion() }
    }

    /// Whether the repository window is at least partly on screen, tracked
    /// via that window's `NSWindow.didChangeOcclusionStateNotification`
    /// (`occlusionState.contains(.visible)` — false when fully occluded or
    /// miniaturized). Slows the active repo's cadences and pauses the
    /// multi-repo sweeps; it gates no active-repo work outright.
    /// Deliberately not `NSApp.keyWindow`: the key window is nil exactly in
    /// the visible-but-not-key case this policy exists to keep fresh.
    /// Defaults to true so nothing stalls before the window reports in
    /// through `trackWindowVisibility(with:)`.
    private(set) var isWindowVisible = true {
        didSet { updateAppNapAssertion() }
    }

    /// Whether a repository is open — the welcome screen runs no background
    /// loops, so it must not hold the App Nap assertion. Fed by
    /// `ContentView`; only the assertion reads it (the loops it would gate
    /// don't exist without a repo).
    var isRepoOpen = false {
        didSet { updateAppNapAssertion() }
    }

    // MARK: Predicates (the table's rows)

    /// The active repo's status poll: never stops for focus or visibility —
    /// `statusPollInterval` slows instead, so a refocused window is already
    /// current rather than catching up in front of the user.
    var canPollStatus: Bool { !networkOpInFlight }

    /// The active repo's auto-fetch loop: keeps ahead/behind honest whether
    /// or not the window is on screen — `autoFetchInterval(configured:)`
    /// stretches the cadence while hidden instead of pausing it.
    var canAutoFetch: Bool { !networkOpInFlight }

    /// The other repos' badge machinery — tier scheduler, visible-row sweep,
    /// refocus sweep: the deferrable fan-out, paused on blur like GH
    /// Desktop's indicator sweep; the refocus resync is its catch-up path.
    var canRunRepoSweeps: Bool { !networkOpInFlight && isWindowVisible && isAppActive }

    /// Status poll cadence ladder (divergence from Tauri's flat 2 s —
    /// FRONTEND.md §8): 2 s frontmost, 10 s visible-but-inactive, 30 s
    /// hidden. The hidden tick is what makes refocus reveal a current
    /// screen; the refocus resync still covers the final seconds.
    var statusPollInterval: Duration {
        guard isWindowVisible else { return .seconds(30) }
        return isAppActive ? .seconds(2) : .seconds(10)
    }

    /// Auto-fetch cadence: the configured interval while the window is on
    /// screen, stretched ×3 while hidden — fresher than pausing, cheaper
    /// than GH Desktop's flat always-on interval.
    func autoFetchInterval(configured: Duration) -> Duration {
        isWindowVisible ? configured : configured * 3
    }

    /// The History list's relative-date tick (FRONTEND §6.12).
    ///
    /// Visibility alone, and deliberately *not* `isAppActive`: this is the
    /// same "stale in plain sight" case the status poll's ladder exists for —
    /// a visible window that is merely not frontmost is being read, so its
    /// labels have to keep ageing. A hidden window has no labels to age.
    ///
    /// The third gate the Tauri client needs — *is the History pane even
    /// showing?* — is structural here and needs no predicate: the tab switch
    /// takes `HistorySidebar` out of the hierarchy, which tears its ticking
    /// task down with it.
    ///
    /// It reads no git state and spawns nothing, so unlike every predicate
    /// above it ignores `networkOpInFlight`, and it is left out of the App
    /// Nap assertion: a repaint of rows already on screen is not work worth
    /// keeping a sleeping Mac awake for.
    var canTickRelativeDates: Bool { isWindowVisible }

    // MARK: Wiring

    @ObservationIgnored private let appNap = AppNapSuppressor()
    @ObservationIgnored private var activationTokens: [any NSObjectProtocol] = []
    @ObservationIgnored private var occlusionToken: (any NSObjectProtocol)?
    @ObservationIgnored private weak var trackedWindow: NSWindow?

    init() {
        isAppActive = NSApp.isActive
        // These AppKit notifications post on the main thread and `.main`
        // pins delivery there, so the hop is an assumption, not a dispatch.
        activationTokens = [
            NotificationCenter.default.addObserver(
                forName: NSApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.isAppActive = true }
            },
            NotificationCenter.default.addObserver(
                forName: NSApplication.didResignActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.isAppActive = false }
            },
        ]
    }

    // Isolated so the non-Sendable tokens are reachable (a plain deinit is
    // nonisolated under Swift 6). Block observers are not auto-removed;
    // without this a dropped policy would leave its blocks registered.
    isolated deinit {
        for token in activationTokens {
            NotificationCenter.default.removeObserver(token)
        }
        if let occlusionToken {
            NotificationCenter.default.removeObserver(occlusionToken)
        }
    }

    /// Follow `window`'s occlusion state (nil — the repo window went away —
    /// reads as not visible). Called by the accessor below whenever the
    /// SwiftUI hierarchy lands in a window; re-registering is idempotent.
    func track(window: NSWindow?) {
        guard window !== trackedWindow else { return }
        if let occlusionToken {
            NotificationCenter.default.removeObserver(occlusionToken)
            self.occlusionToken = nil
        }
        trackedWindow = window
        guard let window else {
            isWindowVisible = false
            return
        }
        isWindowVisible = window.occlusionState.contains(.visible)
        occlusionToken = NotificationCenter.default.addObserver(
            forName: NSWindow.didChangeOcclusionStateNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let tracked = self.trackedWindow else { return }
                self.isWindowVisible = tracked.occlusionState.contains(.visible)
            }
        }
    }

    /// Held exactly while (a repo is open) ∧ (some background work may run):
    /// funneling every input change through here is what makes the
    /// assertion's lifetime provably match its reason.
    private func updateAppNapAssertion() {
        appNap.setSuppressing(isRepoOpen && (canPollStatus || canAutoFetch))
    }
}

extension View {
    /// Feeds the policy's `isWindowVisible` from whatever window ends up
    /// hosting this hierarchy — SwiftUI exposes no `NSWindow`, so a
    /// zero-sized background AppKit view reports it on attach.
    func trackWindowVisibility(with policy: BackgroundSchedulingPolicy) -> some View {
        background(WindowAccessor(onWindowChange: { policy.track(window: $0) }))
    }
}

private struct WindowAccessor: NSViewRepresentable {
    let onWindowChange: (NSWindow?) -> Void

    func makeNSView(context: Context) -> WindowReportingView {
        let view = WindowReportingView()
        view.onWindowChange = onWindowChange
        return view
    }

    func updateNSView(_ nsView: WindowReportingView, context: Context) {
        nsView.onWindowChange = onWindowChange
    }
}

/// `viewDidMoveToWindow` is the one AppKit hook that fires both when the
/// hierarchy first lands in its window and if it ever moves to another.
private final class WindowReportingView: NSView {
    var onWindowChange: (NSWindow?) -> Void = { _ in }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindowChange(window)
    }
}
