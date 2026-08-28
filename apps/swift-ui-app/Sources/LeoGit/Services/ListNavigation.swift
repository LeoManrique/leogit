import Foundation

/// Keyboard-cursor arithmetic for the app's filterable lists — the native
/// counterpart of the Tauri client's `actions/listNavigation.ts`, kept
/// identical so both clients answer an arrow key the same way.
///
/// The lists this serves keep focus in their *filter field* rather than in the
/// list, so the cursor cannot be a focused row: the user has to be able to
/// keep typing while narrowing, and Return has to act on what the query
/// surfaced. That is why the arithmetic is here rather than left to
/// `List(selection:)`, which only moves a cursor when the list itself is the
/// first responder.
enum ListNavigation {
    /// Where the cursor lands after one arrow key.
    ///
    /// Wraps at both ends, and treats "no cursor" (a negative index) as
    /// sitting just before the first row, so Down opens at the top and Up at
    /// the bottom. An empty list has no cursor to move.
    static func nextIndex(after current: Int, count: Int, delta: Int) -> Int {
        guard count > 0 else { return -1 }
        guard current >= 0 else { return delta > 0 ? 0 : count - 1 }
        return (current + delta + count) % count
    }
}
