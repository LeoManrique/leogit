//! The commit composer's exclusion set: which changed files the user has opted
//! out of, and how long an opt-out outlives the path that named it.
//!
//! Inclusion is *derived* in both clients — every committable file is included
//! unless the user unchecked it — so what each client actually stores is the
//! set of paths that were unchecked. A status read then rebuilds the file list
//! from scratch, and the question this module answers is what to do with an
//! opt-out whose path is no longer in that list.
//!
//! The two answers the clients had were both wrong, in opposite directions.
//! Dropping the opt-out the moment its path disappears re-includes a file the
//! user deliberately unchecked, silently, because a formatter rewrote it
//! between two ticks and `git status` happened to read it mid-write — and the
//! next commit takes it. Keeping the opt-out forever never re-includes
//! anything, but grows a set of paths that will never be seen again for as
//! long as the repository stays open.
//!
//! So: keep the opt-out through a grace window and drop it after. The failure
//! modes are not symmetric — an opt-out that lives too long costs one visible
//! checkbox click, while one that dies too early costs a commit the user did
//! not mean to make and never saw happen — which is why the window exists at
//! all rather than the set simply being pruned.
//!
//! **The window has two terms, and each covers what the other cannot.** It is
//! wall-clock rather than a count of ticks, because the hosts poll on a ladder
//! whose interval changes with what the window is doing (2 s frontmost, 30 s
//! hidden) and "fifteen ticks" would mean anything from 30 seconds to seven
//! minutes; the caller passes the time actually elapsed since it last asked.
//! But elapsed time alone is not enough either, for the mirror-image reason:
//! at the slow rung a single read is charged the whole 30 s, so a purely
//! time-based window would expire on the *first* look — which is precisely the
//! look that can land mid-rewrite. Hence [`EXCLUSION_GRACE_READS`] beside it.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How long an opt-out survives after its path stops appearing in the status
/// file list. Roughly a formatter's round trip and then some: long enough that
/// no plausible rewrite-in-place can re-include a file, short enough that the
/// set does not accumulate across an afternoon's work.
pub const EXCLUSION_GRACE_MS: u32 = 30_000;

/// How many status reads in a row must fail to see a path before it can be
/// dropped, whatever the clock says.
///
/// Time alone cannot express the rule this module exists for. The hosts poll on
/// a ladder whose slowest rung is 30 s and which skips a tick outright while a
/// transfer holds the repository, so a single read can be charged 30 s, or two
/// minutes, in one go — and a window measured only in milliseconds would then
/// drop an opt-out on the *first* time it happened to look. That first look is
/// exactly the dangerous one: it is the read that lands in the half-second a
/// formatter has the file renamed away. Two consecutive misses cannot be one
/// unlucky read, at any cadence, which is the guarantee the window was supposed
/// to give and could not.
pub const EXCLUSION_GRACE_READS: u32 = 2;

/// One path the user has excluded from the next commit, with how long and over
/// how many reads it has gone unseen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusion {
    /// Repository-relative path, as it appears in `FileEntry::path`.
    pub path: String,
    /// Milliseconds this path has been absent from the status file list, in an
    /// unbroken run. Zero while it is present — a path that comes back starts
    /// its grace window over, which is the whole point of the window.
    pub absent_ms: u32,
    /// Consecutive status reads that have not seen this path. Zero while it is
    /// present, and reset by a single reappearance along with `absent_ms`.
    pub absent_reads: u32,
}

/// Age every opt-out against the file list a status read just produced, and
/// drop the ones that have now been gone long enough, *and* over enough reads,
/// to be gone rather than merely mid-rewrite.
///
/// `elapsed_ms` is the wall-clock time since the previous call. Callers that
/// skipped a tick (a transfer held the poll back) pass the whole gap rather
/// than one interval, so a pause cannot extend an opt-out's life — and cannot
/// shorten it below [`EXCLUSION_GRACE_READS`] observations either.
///
/// Input order is preserved, and a `present` entry that nothing excludes is
/// ignored — the caller passes the file list it already has rather than
/// pre-filtering it.
#[must_use]
pub fn reconcile_exclusions(
    excluded: &[Exclusion],
    present: &[String],
    elapsed_ms: u32,
) -> Vec<Exclusion> {
    let present: HashSet<&str> = present.iter().map(String::as_str).collect();
    excluded
        .iter()
        .filter_map(|entry| {
            if present.contains(entry.path.as_str()) {
                return Some(Exclusion {
                    path: entry.path.clone(),
                    absent_ms: 0,
                    absent_reads: 0,
                });
            }
            // Saturating rather than wrapping: a host that was suspended for a
            // week hands us an elapsed that overflows, and the answer to "has
            // this been gone longer than the window?" is plainly yes.
            let absent_ms = entry.absent_ms.saturating_add(elapsed_ms);
            let absent_reads = entry.absent_reads.saturating_add(1);
            let expired = absent_ms >= EXCLUSION_GRACE_MS && absent_reads >= EXCLUSION_GRACE_READS;
            (!expired).then(|| Exclusion {
                path: entry.path.clone(),
                absent_ms,
                absent_reads,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn excluded(triples: &[(&str, u32, u32)]) -> Vec<Exclusion> {
        triples
            .iter()
            .map(|(path, absent_ms, absent_reads)| Exclusion {
                path: (*path).to_string(),
                absent_ms: *absent_ms,
                absent_reads: *absent_reads,
            })
            .collect()
    }

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn a_present_path_keeps_its_opt_out_and_resets_its_clock() {
        let kept = reconcile_exclusions(
            &excluded(&[("a.txt", 12_000, 6)]),
            &paths(&["a.txt"]),
            2_000,
        );
        assert_eq!(kept, excluded(&[("a.txt", 0, 0)]));
    }

    #[test]
    fn an_absent_path_ages_but_survives_inside_the_window() {
        let kept = reconcile_exclusions(
            &excluded(&[("a.txt", 10_000, 5)]),
            &paths(&["b.txt"]),
            2_000,
        );
        assert_eq!(kept, excluded(&[("a.txt", 12_000, 6)]));
    }

    #[test]
    fn an_absent_path_is_dropped_once_both_terms_are_satisfied() {
        let kept = reconcile_exclusions(
            &excluded(&[("a.txt", EXCLUSION_GRACE_MS - 1, EXCLUSION_GRACE_READS)]),
            &paths(&[]),
            1,
        );
        assert!(kept.is_empty(), "grace window should have expired: {kept:?}");
    }

    #[test]
    fn one_read_can_never_prune_however_long_it_is_charged() {
        // The whole reason the read count exists. At the hidden rung one tick
        // is charged the entire 30 s window, and a transfer can hand over
        // minutes in a single lump — so a purely time-based rule would drop an
        // opt-out on the *first* look, which is exactly the look that can land
        // in the half-second a formatter has the file renamed away.
        for elapsed in [EXCLUSION_GRACE_MS, 120_000, u32::MAX] {
            let kept = reconcile_exclusions(&excluded(&[("a.txt", 0, 0)]), &paths(&[]), elapsed);
            assert_eq!(
                kept.len(),
                1,
                "a single absent read pruned at elapsed={elapsed}: {kept:?}"
            );
        }
    }

    #[test]
    fn a_second_absent_read_is_what_finally_prunes() {
        let after_one = reconcile_exclusions(&excluded(&[("a.txt", 0, 0)]), &paths(&[]), 30_000);
        assert_eq!(after_one, excluded(&[("a.txt", 30_000, 1)]));
        let after_two = reconcile_exclusions(&after_one, &paths(&[]), 30_000);
        assert!(after_two.is_empty(), "two misses should prune: {after_two:?}");
    }

    #[test]
    fn a_path_that_comes_back_starts_the_window_over() {
        // The formatter case: the file vanishes for a few ticks while it is
        // rewritten, then reappears. Its opt-out must be exactly as fresh as
        // one that never left, or a long edit session eventually re-includes it.
        let mut set = excluded(&[("a.txt", 0, 0)]);
        for _ in 0..5 {
            set = reconcile_exclusions(&set, &paths(&["b.txt"]), 2_000);
        }
        assert_eq!(set, excluded(&[("a.txt", 10_000, 5)]));
        set = reconcile_exclusions(&set, &paths(&["a.txt"]), 2_000);
        assert_eq!(set, excluded(&[("a.txt", 0, 0)]));
    }

    #[test]
    fn a_poll_held_back_by_a_transfer_cannot_buy_extra_life() {
        // The gap is charged in full, so pausing the tick does not extend an
        // opt-out — it just takes the second read to finish the job.
        let mut set = excluded(&[("a.txt", 0, 0)]);
        set = reconcile_exclusions(&set, &paths(&[]), 600_000);
        assert_eq!(set, excluded(&[("a.txt", 600_000, 1)]));
        set = reconcile_exclusions(&set, &paths(&[]), 2_000);
        assert!(set.is_empty(), "the second read should prune: {set:?}");
    }

    #[test]
    fn a_suspended_host_prunes_rather_than_overflowing() {
        let kept = reconcile_exclusions(
            &excluded(&[("a.txt", u32::MAX, u32::MAX)]),
            &paths(&[]),
            u32::MAX,
        );
        assert!(kept.is_empty(), "saturating add should still prune: {kept:?}");
    }

    #[test]
    fn order_is_preserved_and_unexcluded_files_are_ignored() {
        let kept = reconcile_exclusions(
            &excluded(&[("c.txt", 0, 0), ("a.txt", 0, 0), ("b.txt", 0, 0)]),
            &paths(&["a.txt", "b.txt", "c.txt", "d.txt"]),
            2_000,
        );
        assert_eq!(
            kept.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["c.txt", "a.txt", "b.txt"]
        );
    }

    #[test]
    fn an_empty_set_stays_empty() {
        assert!(reconcile_exclusions(&[], &paths(&["a.txt"]), 2_000).is_empty());
    }
}
