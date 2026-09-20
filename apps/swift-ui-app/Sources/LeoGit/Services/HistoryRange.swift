import Foundation

/// What a rewrite of the current branch would replay, read off the loaded
/// History rows. Pure, and mirrored by the Tauri client's
/// `utils/historyRange.ts`.
enum HistoryRange {
    /// Whether rewriting `selected` would replay a merge commit: one sits
    /// anywhere from HEAD down to the oldest selected commit. A squash or a
    /// reorder replays that whole stretch, and git cannot replay a merge from
    /// a flat todo.
    ///
    /// `commits` is the History list — newest first, append-only from HEAD —
    /// so every row above a selected one is loaded and the answer needs no git
    /// call. It is the menu's early answer only; core's preflight asks git and
    /// stays the authority.
    static func replaysMergeCommit(in commits: [CommitInfo], selected: Set<String>) -> Bool {
        guard let oldest = commits.lastIndex(where: { selected.contains($0.sha) }) else {
            return false
        }
        return commits[...oldest].contains { $0.parents.count > 1 }
    }
}
