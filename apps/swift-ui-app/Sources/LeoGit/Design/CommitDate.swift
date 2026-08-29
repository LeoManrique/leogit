import Foundation

/// Parsing and presentation for the timestamps `leogit-core` puts on commits.
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

    /// A commit's age in the shared vocabulary — `just now`, `5 minutes ago`,
    /// `3 days ago`, `2 years ago` — or the raw string if it cannot be parsed.
    ///
    /// Spelled out rather than handed to `Date.RelativeFormatStyle`, which is
    /// the one thing it looks like it should be. `.relative(presentation:
    /// .named)` renders the near tiers as *yesterday* / *last week*, and the
    /// Tauri client renders them as *1 day ago* — the same commit reading two
    /// different ways in the two clients, which FRONTEND §6.12 pins as one
    /// vocabulary. GitHub Desktop breaks that tie (the plan's §7 rule) and it
    /// counts: *5 months ago* is a duration, *last week* is a landmark, and a
    /// history list is read for durations. The cost is that these strings are
    /// English, where the format style would have been localised — no loss
    /// today, since every other string in both clients is hard-coded English
    /// too, and a real localisation pass would have to take the whole app.
    ///
    /// `now` is a parameter rather than read inside, because the list ticks it:
    /// see `HistorySidebar`'s `relativeDateClock`.
    static func relative(_ raw: String, now: Date = .now) -> String {
        guard let date = parse(raw) else { return raw }
        // Clamped before it is ever narrowed to `Int`. Git stores commit times
        // as raw int64 seconds and accepts absurd ones, so a hand-edited or
        // corrupt commit can hand us a date far enough out that the conversion
        // would trap — a crash in the history list over a cosmetic label.
        let seconds = min(max(now.timeIntervalSince(date), -Self.clamp), Self.clamp)
        let minutes = whole(seconds, per: 60)
        let hours = whole(seconds, per: 3600)
        let days = whole(seconds, per: 86400)

        // Ordered widest-last, and each tier measured from the original
        // interval rather than from the tier above it, so nothing accumulates
        // rounding. A future timestamp lands in the first branch and reads
        // "just now" rather than a negative count.
        if minutes < 1 { return "just now" }
        if minutes < 60 { return ago(minutes, "minute") }
        if hours < 24 { return ago(hours, "hour") }
        if days < 30 { return ago(days, "day") }
        // A year is the last tier, so months carry everything under it — the
        // 30-day month and 365-day year are deliberate approximations: this
        // label answers "roughly how long ago", and the exact date is one
        // hover away in the tooltip.
        if days < 365 { return ago(days / 30, "month") }
        return ago(days / 365, "year")
    }

    /// Absolute local time, for the row tooltip and the detail card.
    static func absolute(_ raw: String) -> String {
        guard let date = parse(raw) else { return raw }
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    /// Far enough out that no real commit reaches it, near enough that every
    /// division below stays inside `Int`.
    private static let clamp: TimeInterval = 1e15

    private static func whole(_ seconds: TimeInterval, per divisor: Double) -> Int {
        Int((seconds / divisor).rounded(.down))
    }

    private static func ago(_ count: Int, _ unit: String) -> String {
        "\(count) \(unit)\(count == 1 ? "" : "s") ago"
    }
}
