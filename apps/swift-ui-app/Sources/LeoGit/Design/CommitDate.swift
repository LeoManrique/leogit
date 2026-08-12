import Foundation

/// Parsing for the timestamps `leogit-core` puts on commits.
///
/// Core normalises git's `--date=raw` output into ISO-8601 with a *basic* UTC
/// offset — `2026-08-12T10:30:00+0200`, no colon in the offset. Swift's default
/// `.iso8601` strategy expects exactly that shape, so no custom style is needed;
/// the colon-separated variant would need `timeZoneSeparator: .colon`.
enum CommitDate {
    /// Parse one of core's `author_date` / `committer_date` strings.
    ///
    /// Returns `nil` rather than throwing: a malformed timestamp should degrade
    /// to a blank cell, never take down the history list.
    static func parse(_ raw: String) -> Date? {
        try? Date(raw, strategy: .iso8601)
    }

    /// "2 hours ago" / "yesterday", or the raw string if it cannot be parsed.
    ///
    /// The result is a snapshot, not live-updating — recompute on refresh.
    static func relative(_ raw: String) -> String {
        guard let date = parse(raw) else { return raw }
        return date.formatted(.relative(presentation: .named))
    }

    /// Absolute local time, for the row tooltip.
    static func absolute(_ raw: String) -> String {
        guard let date = parse(raw) else { return raw }
        return date.formatted(date: .abbreviated, time: .shortened)
    }
}
