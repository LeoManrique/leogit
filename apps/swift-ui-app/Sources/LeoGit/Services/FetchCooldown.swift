import Foundation

/// Per-repository memory of when a `git fetch` last reached the remote, so an
/// automatic fetch can decline to repeat one that just happened.
///
/// Only *background* work consults it. A fetch or pull the user asked for is
/// the user saying the answer might be wrong, so it always runs — it simply
/// opens a new window on the way out, because what it brought back is fresh.
/// A push opens none: it sends, and learns nothing about the remote it did not
/// already know.
///
/// The window is deliberately shorter than the shortest cadence that would
/// otherwise refresh a badge (the tier loop's 2 min), so this can never leave
/// anything staler than it already would have been: it removes duplicate round
/// trips, never a refresh. The duplicates are real — the top tier fetches four
/// repositories, and thirty seconds later a refocus sweep fetches the same
/// four; switching A → B → A refetches A seconds after opening it.
///
/// Only a fetch that actually reached the remote leaves a stamp, mirroring the
/// rule the sweep throttles beside it follow: an unreachable remote is retried
/// at the next opportunity rather than suppressed for a minute, and *that*
/// case is the connectivity breaker's to answer.
///
/// Time is a `ContinuousClock`, the one place this departs from the `Date`
/// throttles it sits next to. It counts through system sleep, so a wake-up
/// resync after a night away always fetches, while being immune to a wall clock
/// that steps backwards — which would otherwise hold the window open for the
/// size of the jump. Its instants are process-local by definition, so they are
/// never persisted and never shared with the Tauri client, which keeps its own.
@MainActor
final class FetchCooldown {
    private static let window: Duration = .seconds(60)

    private var lastFetchedAt: [String: ContinuousClock.Instant] = [:]

    /// Record that `path`'s remote answered just now.
    func note(_ path: String) {
        lastFetchedAt[path] = .now
    }

    /// Whether `path` was fetched recently enough that another round trip would
    /// buy nothing a caller does not already have.
    func isFresh(_ path: String) -> Bool {
        guard let last = lastFetchedAt[path] else { return false }
        return last.duration(to: .now) < Self.window
    }

    /// One line for a background fetch this window turned away, naming what was
    /// dropped, the repository, and how fresh the answer being kept is — the
    /// two call sites share it so their wording cannot drift apart.
    func logSkip(_ path: String, _ what: String) {
        let age = lastFetchedAt[path]?.duration(to: .now).components.seconds ?? 0
        print("[fetch] \(what) for \(path): fetched \(age)s ago")
    }
}
