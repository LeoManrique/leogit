import Foundation

/// Where a reorder's insertion line may sit in the History list, and what each
/// place means to core. Pure, and mirrored by the Tauri client's
/// `utils/reorderPlacement.ts`.
///
/// A *slot* is a gap between rows: slot `i` is the one above row `i`, and slot
/// `commits.count` is the one under the last loaded row. Moving commits to a
/// slot reads as "take the moved rows out, and put them back where the line
/// is".
enum ReorderPlacement {
    /// The slots that would leave the branch as it is: the ones touching a
    /// contiguous selection, and inside it. `nil` for a selection with gaps —
    /// gathering it anywhere is a change — and for one that is not in the list
    /// at all.
    private static func unchangedSlots(
        in commits: [CommitInfo],
        moving: Set<String>
    ) -> ClosedRange<Int>? {
        let rows = commits.indices.filter { moving.contains(commits[$0].sha) }
        guard let first = rows.first, let last = rows.last,
              last - first + 1 == rows.count
        else { return nil }
        return first...(last + 1)
    }

    /// Whether putting `moving` into `slot` would leave the branch as it is.
    static func changesNothing(in commits: [CommitInfo], moving: Set<String>, slot: Int) -> Bool {
        unchangedSlots(in: commits, moving: moving)?.contains(slot) ?? false
    }

    /// The slot the line appears in when the mode arms: the one above the
    /// topmost moved row — where the commits are now, so that a ⏎ pressed at
    /// once asks for as little as it can.
    static func homeSlot(in commits: [CommitInfo], moving: Set<String>) -> Int {
        commits.firstIndex { moving.contains($0.sha) } ?? 0
    }

    /// The slots the line may rest in, top to bottom, for moving `moving`.
    ///
    /// - **No slot under a merge commit.** Core replays the branch from the
    ///   older of the moved commits and the destination, and git cannot replay
    ///   a merge from a flat todo — so the line stops above the newest merge.
    ///   (The menu item is already off when a merge sits at or above a moved
    ///   commit — `HistoryRange.replaysMergeCommit`.)
    /// - **No slot that would change nothing, except home**, so that every
    ///   step of the line is a different move and ↓ from home is "one row
    ///   down" however many rows are moving.
    ///
    /// `commits` is the History list, newest first and append-only from HEAD.
    /// The slot under the last loaded row is a real destination even when
    /// older history exists — core takes any commit of the branch as one — and
    /// paging only ever adds slots below it.
    static func slots(in commits: [CommitInfo], moving: Set<String>) -> [Int] {
        guard commits.contains(where: { moving.contains($0.sha) }) else { return [] }
        let lowest = commits.firstIndex { $0.parents.count > 1 } ?? commits.count
        let home = homeSlot(in: commits, moving: moving)
        let unchanged = unchangedSlots(in: commits, moving: moving)

        return (0...lowest).filter { slot in
            slot == home || !(unchanged?.contains(slot) ?? false)
        }
    }

    /// Whether `moving` has anywhere to go: a slot other than the one it is in.
    static func canReorder(in commits: [CommitInfo], moving: Set<String>) -> Bool {
        slots(in: commits, moving: moving).count > 1
    }

    /// The next slot up (`-1`) or down (`1`) from `slot`; `slot` itself at an
    /// end.
    static func adjacentSlot(in slots: [Int], to slot: Int, step: Int) -> Int {
        guard let at = slots.firstIndex(of: slot), slots.indices.contains(at + step) else {
            return slot
        }
        return slots[at + step]
    }

    /// A slot as core names a destination: the commit the moved block lands
    /// just *under* — the row above the line — or `nil` for the top slot, the
    /// tip.
    static func destination(in commits: [CommitInfo], slot: Int) -> String? {
        slot > 0 && slot <= commits.count ? commits[slot - 1].sha : nil
    }
}
