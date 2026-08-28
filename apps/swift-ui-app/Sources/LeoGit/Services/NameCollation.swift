import Foundation

/// How the app's name-ordered lists compare two labels.
///
/// `localizedCaseInsensitiveCompare` is not enough: it still separates `e`
/// from `é`, so an accented name sorts away from the neighbours a user
/// scanning alphabetically expects it beside. This is the Tauri lists'
/// `localeCompare(…, { sensitivity: 'base' })` — case- *and* diacritic-blind —
/// written once so the two clients' A-Z orders agree and the native call
/// sites cannot drift from each other.
/// It answers with a full [`ComparisonResult`] rather than a `Bool`, because
/// ties are the interesting case: Swift's `sort` is not stable, so two rows
/// this call reports as equal would swap places between passes — which, on a
/// list that rebuilds as data streams in, reads as rows twitching under the
/// pointer. Every caller therefore has to see `.orderedSame` and add a tiebreak
/// of its own.
enum NameCollation {
    static func compare(_ lhs: String, _ rhs: String) -> ComparisonResult {
        lhs.compare(
            rhs,
            options: [.caseInsensitive, .diacriticInsensitive],
            range: nil,
            locale: .current
        )
    }
}
