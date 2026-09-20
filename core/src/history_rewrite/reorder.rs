//! Moving commits of the current branch to another place in it.
//!
//! The moved commits keep their own order and travel as one block, wherever
//! they were: a selection need not be contiguous. **The destination is named by
//! the commit the block lands just under** in a newest-first list — so just
//! *older* than it — and no destination is the tip. That is what a list's
//! insertion line reads as: take the moved rows out, and put them back where
//! the line is.

use std::collections::HashSet;
use std::fmt::Write as _;

use super::lineage::Lineage;
use super::replay::{Replay, Replayed};
use super::{
    RewritePreflight, RewriteResult, UndoStart, branch_ready_for_an_action, parent_of,
    replay_refusal,
};
use crate::git::run_git;

/// A move, worked out against the branch as it stands.
struct Move {
    /// The oldest commit the move displaces: the replay starts here, and every
    /// commit below keeps its id.
    from: String,
    /// The commits to replay — `from` up to `HEAD` — oldest first, in their new
    /// order.
    order: Vec<String>,
    /// Where the moved block sits in the new branch, as the number of commits
    /// above it, and how many commits it is.
    above: usize,
    moved: usize,
}

impl Move {
    /// Work out the move of `lineage.selected` to just under `before`, or to
    /// the tip. `None` when the branch already reads that way.
    ///
    /// **A no-op is judged by the result, not by where the destination lies**:
    /// build the new order and compare it with the old one. "Adjacent to the
    /// selection" is that for a contiguous selection only — `{C, E}` to the tip
    /// of `A B C D E` moves `C`, and `{D, E}` there moves nothing.
    ///
    /// The range reaches down to the older of the oldest moved commit and the
    /// destination, and its leading commits may well stay where they are
    /// (`{B, D}` to just under `C` leaves `B` first). Those are dropped from
    /// the replay rather than left for git to fast-forward over, so that what
    /// the preflight judges — a merge, a shallow boundary, a pushed commit — is
    /// what is rewritten and nothing below it.
    fn plan(lineage: &Lineage, before: Option<&str>) -> Option<Self> {
        let moving: HashSet<&str> = lineage.selected.iter().map(String::as_str).collect();
        let mut order: Vec<&String> = Vec::with_capacity(lineage.range.len());
        for sha in &lineage.range {
            // `before` may itself be moving — the line drawn between two
            // selected rows — and the block lands where it stood all the same.
            if before == Some(sha.as_str()) {
                order.extend(&lineage.selected);
            }
            if !moving.contains(sha.as_str()) {
                order.push(sha);
            }
        }
        if before.is_none() {
            order.extend(&lineage.selected);
        }

        let kept = order
            .iter()
            .zip(&lineage.range)
            .take_while(|(new, old)| **new == *old)
            .count();
        if kept == order.len() {
            return None;
        }
        let block = order
            .iter()
            .position(|sha| **sha == lineage.selected[0])
            .unwrap_or_default();
        Some(Self {
            from: lineage.range[kept].clone(),
            above: order.len() - block - lineage.selected.len(),
            moved: lineage.selected.len(),
            order: order[kept..].iter().map(|sha| (*sha).clone()).collect(),
        })
    }

    /// Every line a `pick` of a full object id, so no user setting has a say
    /// in how the todo is read.
    fn todo(&self) -> String {
        let mut todo = String::new();
        for sha in &self.order {
            let _ = writeln!(todo, "pick {sha}");
        }
        todo
    }
}

/// `shas` and the destination placed on the current branch, and the move
/// between them — `None` when there is nothing to move. The inner `Err` is
/// [`Lineage::of`]'s refusal.
fn planned(
    repo_path: &str,
    shas: &[String],
    before_sha: Option<&str>,
) -> Result<Result<(Lineage, Option<Move>), String>, String> {
    let before = before_sha.map(str::to_ascii_lowercase);
    Ok(
        Lineage::of(repo_path, shas, before.as_deref())?.map(|lineage| {
            let planned = Move::plan(&lineage, before.as_deref());
            (lineage, planned)
        }),
    )
}

/// Whether `shas` may be moved to just under `before_sha` — or to the tip — and
/// whether doing so rewrites pushed commits. [`rewrite_preflight`] cannot answer
/// for a reorder, because where the replay starts depends on the destination.
/// A move that would change nothing is ready, and rewrites nothing.
///
/// [`rewrite_preflight`]: super::rewrite_preflight
///
/// # Errors
/// When git cannot be run, an id is not an object id, or the commits are not
/// all on the current branch. A refusal is `blocked`, not an `Err`.
pub fn reorder_preflight(
    repo_path: &str,
    shas: &[String],
    before_sha: Option<&str>,
) -> Result<RewritePreflight, String> {
    let branch = match branch_ready_for_an_action(repo_path)? {
        Ok(branch) => branch,
        Err(reason) => return Ok(RewritePreflight::refused(reason)),
    };
    match planned(repo_path, shas, before_sha)? {
        Err(reason) => Ok(RewritePreflight::refused(reason)),
        Ok((_, Some(planned))) => RewritePreflight::for_a_replay(repo_path, &branch, &planned.from),
        Ok((_, None)) => Ok(RewritePreflight::ready(false)),
    }
}

/// Move `shas` — commits of the current branch, in any order — to just under
/// `before_sha` in a newest-first list, or to the tip for `None`.
///
/// * **Success** leaves the branch rewritten, with the moved commits as the
///   `selection`. A move that would change nothing is a success too: nothing is
///   run, the `selection` is the commits as they stand, and there is no `undo`.
/// * **A conflict** (`success` false) leaves the rebase open for Continue or
///   Abort.
/// * **Any other failure** is an `Err` with git's text, the rebase aborted and
///   the branch back where it began.
///
/// # Errors
/// When the preflight refuses; when no commit is named, an id is not an object
/// id, or the commits and the destination are not all on the current branch; or
/// when the rebase fails for a reason that is not a conflict.
pub fn reorder_commits(
    repo_path: &str,
    shas: &[String],
    before_sha: Option<&str>,
) -> Result<RewriteResult, String> {
    let branch = branch_ready_for_an_action(repo_path)??;
    let (lineage, planned) = planned(repo_path, shas, before_sha)??;
    let Some(planned) = planned else {
        eprintln!("[history_rewrite] reorder: the commits are already there");
        let mut selection = lineage.selected;
        selection.reverse();
        return Ok(RewriteResult::unchanged(selection));
    };
    if let Some(reason) = replay_refusal(repo_path, &planned.from)? {
        return Err(reason);
    }
    let start = UndoStart::on_the_current_branch(repo_path, branch)?;
    let onto = parent_of(repo_path, &planned.from)?;

    eprintln!(
        "[history_rewrite] reorder {} commit(s) on {}, replaying {} from {}",
        planned.moved,
        start.branch,
        planned.order.len(),
        planned.from
    );
    let replayed = Replay {
        repo_path,
        onto: onto.as_deref(),
        todo: &planned.todo(),
        message: None,
    }
    .run()?;

    match replayed {
        Replayed::Conflict(conflicts, said) => Ok(RewriteResult::stopped(conflicts, &said, start)),
        Replayed::Done => {
            // `--empty=keep` replays every line of the todo into a commit, so
            // the block is where the todo put it, counted from the new tip.
            let reach = (planned.above + planned.moved).to_string();
            let landed = run_git(repo_path, &["rev-list", "-n", &reach, "HEAD"]).and_then(|tip| {
                let tip: Vec<String> = tip.lines().map(str::to_string).collect();
                let (Some(after_sha), Some(selection)) = (tip.first(), tip.get(planned.above..))
                else {
                    return Err("rev-list answered with fewer commits than were moved".to_string());
                };
                Ok((selection.to_vec(), start.landed_on(after_sha.clone())))
            });
            Ok(RewriteResult::landed(landed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::UndoPoint;
    #[cfg(unix)]
    use super::super::fixtures::failing_hook;
    use super::super::fixtures::{branch, edits_of_one_line, five_commits, sha};
    use super::*;
    use crate::git::get_status;
    use crate::operation::{OperationInProgress, abort_operation, continue_operation};
    use crate::test_support::{commit_file, git, git_stdout, subjects};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    /// The ids of `revs`, as a selection arrives: in no particular order.
    fn shas(dir: &Path, revs: &[&str]) -> Vec<String> {
        revs.iter().map(|rev| sha(dir, rev)).collect()
    }

    fn subjects_of(dir: &Path, shas: &[String]) -> Vec<String> {
        shas.iter()
            .map(|sha| git_stdout(dir, &["log", "-1", "--format=%s", sha]))
            .collect()
    }

    #[test]
    fn reorder_moves_commits_to_the_tip_and_into_the_middle() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let one = sha(dir, "HEAD~4");
        let tip = sha(dir, "HEAD");

        // `two` to the tip.
        let moved = reorder_commits(&repo, &shas(dir, &["HEAD~3"]), None).expect("reorder");

        assert!(moved.success && moved.conflicts.is_empty());
        assert_eq!(subjects(dir), ["two", "five", "four", "three", "one"]);
        assert_eq!(moved.selection, [sha(dir, "HEAD")]);
        assert_eq!(
            moved.undo,
            Some(UndoPoint {
                branch: "main".to_string(),
                before_sha: tip,
                after_sha: sha(dir, "HEAD"),
                return_branch: None,
            })
        );
        assert_eq!(sha(dir, "HEAD~4"), one, "older commits keep their ids");

        // And back down, to just under `three` — a destination older than the
        // commit that moves.
        let under = sha(dir, "HEAD~3");
        let back = reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&under)).expect("reorder");

        assert!(back.success);
        assert_eq!(subjects(dir), ["five", "four", "three", "two", "one"]);
        assert_eq!(subjects_of(dir, &back.selection), ["two"]);
        assert_eq!(branch(dir), "main");
        assert!(!dir.join(".git/rebase-merge").exists());
    }

    #[test]
    fn reorder_moves_a_non_contiguous_selection_as_one_block() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        // Click order, not history order: core places them itself.
        let picks = shas(dir, &["HEAD", "HEAD~2"]);
        let under = sha(dir, "HEAD~3");

        let moved = reorder_commits(&repo, &picks, Some(&under)).expect("reorder");

        assert!(moved.success);
        assert_eq!(subjects(dir), ["four", "two", "five", "three", "one"]);
        assert_eq!(
            subjects_of(dir, &moved.selection),
            ["five", "three"],
            "the moved commits, newest first"
        );
    }

    #[test]
    fn reorder_lands_the_block_where_a_selected_destination_stood() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        // The insertion line between `four` and `three`, with `four` and `two`
        // selected: take both out, and put them back at the line.
        let picks = shas(dir, &["HEAD~1", "HEAD~3"]);
        let under = sha(dir, "HEAD~1");

        let moved = reorder_commits(&repo, &picks, Some(&under)).expect("reorder");

        assert!(moved.success);
        assert_eq!(subjects(dir), ["five", "four", "two", "three", "one"]);
        assert_eq!(subjects_of(dir, &moved.selection), ["four", "two"]);
    }

    #[test]
    fn reorder_under_the_first_commit_makes_a_new_root() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let root = sha(dir, "HEAD~4");

        let moved = reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&root)).expect("reorder");

        assert!(moved.success);
        assert_eq!(subjects(dir), ["four", "three", "two", "one", "five"]);
        assert_eq!(moved.selection, [sha(dir, "HEAD~4")]);
        assert_eq!(git_stdout(dir, &["rev-list", "--count", "HEAD"]), "5");
    }

    #[test]
    fn reorder_of_a_no_op_destination_changes_nothing() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let reflog = git_stdout(dir, &["reflog", "show", "HEAD"]);
        let top_two = shas(dir, &["HEAD~1", "HEAD"]);

        // Already at the tip; just under the row above; and between the two.
        for before in [None, Some(sha(dir, "HEAD~1"))] {
            let result = reorder_commits(&repo, &top_two, before.as_deref()).expect("reorder");
            assert!(result.success && result.undo.is_none());
            assert_eq!(result.selection, [sha(dir, "HEAD"), sha(dir, "HEAD~1")]);
        }
        let three = shas(dir, &["HEAD~2"]);
        let above = sha(dir, "HEAD~1");
        let result = reorder_commits(&repo, &three, Some(&above)).expect("reorder");
        assert!(result.success && result.undo.is_none());
        let ready = reorder_preflight(&repo, &three, Some(&above)).expect("preflight");
        assert_eq!(ready, RewritePreflight::ready(false));

        assert_eq!(sha(dir, "HEAD"), tip);
        assert_eq!(
            git_stdout(dir, &["reflog", "show", "HEAD"]),
            reflog,
            "git was not asked to rebase anything"
        );

        // Not a no-op, though the destination is the tip both times: `three`
        // is what moves.
        let split = shas(dir, &["HEAD~2", "HEAD"]);
        let moved = reorder_commits(&repo, &split, None).expect("reorder");
        assert_eq!(subjects(dir), ["five", "three", "four", "two", "one"]);
        assert_eq!(moved.selection, shas(dir, &["HEAD", "HEAD~1"]));
        assert!(moved.undo.is_some());
    }

    #[test]
    fn reorder_replays_only_from_the_first_commit_it_displaces() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let two = sha(dir, "HEAD~3");
        // `two` and `four` to just under `three`: `two` stays first, so the
        // replay starts at `three`.
        let picks = shas(dir, &["HEAD~3", "HEAD~1"]);
        let under = sha(dir, "HEAD~2");
        let planned = planned(&repo, &picks, Some(&under))
            .expect("plan")
            .expect("placed")
            .1;
        assert_eq!(planned.expect("a move").from, under);

        let moved = reorder_commits(&repo, &picks, Some(&under)).expect("reorder");

        assert_eq!(subjects(dir), ["five", "three", "four", "two", "one"]);
        assert_eq!(sha(dir, "HEAD~3"), two, "`two` was not rewritten");
        assert_eq!(moved.selection, [sha(dir, "HEAD~2"), two]);
    }

    #[test]
    fn reorder_keeps_a_commit_that_became_empty() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        crate::test_support::init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "flag.txt", "off\n", "off");
        commit_file(dir, "flag.txt", "on\n", "on");
        commit_file(dir, "flag.txt", "off\n", "off again");
        let repo = dir.to_str().expect("utf-8 path");

        // Under `on`, `off again` turns off what is already off: the new order
        // leaves it nothing to change, which is what `--empty=keep` is for.
        let under = sha(dir, "HEAD~1");
        let moved = reorder_commits(repo, &shas(dir, &["HEAD"]), Some(&under)).expect("reorder");

        assert!(moved.success, "{:?}", moved.error_message);
        assert_eq!(subjects(dir), ["on", "off again", "off"]);
        assert_eq!(moved.selection, [sha(dir, "HEAD~1")]);
        assert_eq!(
            git_stdout(dir, &["rev-parse", "HEAD~1^{tree}"]),
            git_stdout(dir, &["rev-parse", "HEAD~2^{tree}"]),
            "kept, and empty"
        );
    }

    #[test]
    fn reorder_keeps_a_commit_that_was_empty_to_begin_with() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        crate::test_support::init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "keep.txt", "keep\n", "base");
        git(dir, &["commit", "-q", "--allow-empty", "-m", "a marker"]);
        commit_file(dir, "note.txt", "note\n", "adds a note");
        let repo = dir.to_str().expect("utf-8 path");

        // A commit that was empty to begin with is replayed like any other.
        let moved = reorder_commits(repo, &shas(dir, &["HEAD~1"]), None).expect("reorder");

        assert!(moved.success, "{:?}", moved.error_message);
        assert_eq!(subjects(dir), ["a marker", "adds a note", "base"]);
        assert_eq!(moved.selection, [sha(dir, "HEAD")]);
    }

    #[test]
    fn reorder_stops_on_a_conflict_and_continue_finishes_it() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        // `second edit` to just under `first edit`: it lands on a file that
        // still reads `base`.
        let under = sha(dir, "HEAD~2");
        let tip_before = sha(dir, "HEAD");

        let stopped =
            reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&under)).expect("data, not Err");

        assert!(!stopped.success);
        assert_eq!(stopped.conflicts, ["shared.txt"]);
        assert!(stopped.error_message.is_some_and(|said| !said.is_empty()));
        assert!(stopped.selection.is_empty() && stopped.undo.is_none());
        assert_eq!(
            stopped.start,
            Some(UndoStart {
                branch: "main".to_string(),
                before_sha: tip_before,
                return_branch: None,
            })
        );
        assert_eq!(
            get_status(repo.clone()).expect("status").operation,
            Some(OperationInProgress::Rebase)
        );

        fs::write(dir.join("shared.txt"), "resolved\n").expect("resolve");
        let again = continue_operation(&repo).expect("continue");
        // Round two: `first edit` is replayed onto a file it never saw.
        assert!(
            !again.success,
            "the commit replayed next conflicts in its turn"
        );
        assert!(!again.skipped);
        fs::write(dir.join("shared.txt"), "resolved again\n").expect("resolve");
        let outcome = continue_operation(&repo).expect("continue");

        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(!outcome.skipped, "both commits landed with a change");
        assert_eq!(
            subjects(dir),
            ["between", "first edit", "second edit", "base"]
        );
        assert!(!dir.join(".git/rebase-merge").exists());
    }

    #[test]
    fn a_moved_commit_resolved_to_nothing_is_dropped_and_continue_says_so() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let under = sha(dir, "HEAD~2");
        let stopped =
            reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&under)).expect("a conflict");
        assert!(!stopped.success);

        // Taking the side already there leaves `second edit` with nothing to
        // add; git drops it on `--continue` without a word, `--empty=keep` or not.
        fs::write(dir.join("shared.txt"), "base\n").expect("resolve");
        let outcome = continue_operation(&repo).expect("continue");

        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(outcome.skipped, "the commit that went is reported");
        assert_eq!(subjects(dir), ["between", "first edit", "base"]);
    }

    #[test]
    fn a_resolution_staged_from_a_terminal_still_reports_the_dropped_commit() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let under = sha(dir, "HEAD~2");
        let stopped =
            reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&under)).expect("a conflict");
        assert!(!stopped.success);

        // Staged by hand, so Continue finds nothing unmerged to stage — and git
        // drops `second edit` all the same.
        fs::write(dir.join("shared.txt"), "base\n").expect("resolve");
        git(dir, &["add", "shared.txt"]);
        let outcome = continue_operation(&repo).expect("continue");

        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(outcome.skipped, "the commit that went is reported");
        assert_eq!(subjects(dir), ["between", "first edit", "base"]);
    }

    #[test]
    fn an_aborted_reorder_puts_the_branch_back() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let under = sha(dir, "HEAD~2");
        let stopped =
            reorder_commits(&repo, &shas(dir, &["HEAD"]), Some(&under)).expect("a conflict");
        assert!(!stopped.success);

        abort_operation(&repo).expect("abort");

        assert_eq!(sha(dir, "HEAD"), tip);
        assert_eq!(branch(dir), "main");
        assert_eq!(git_stdout(dir, &["status", "--porcelain"]), "");
    }

    #[cfg(unix)]
    #[test]
    fn reorder_stopped_with_nothing_to_resolve_is_a_failure_not_a_conflict() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        failing_hook(dir, "prepare-commit-msg", "no-message-for-you");

        let refusal = reorder_commits(&repo, &shas(dir, &["HEAD~2"]), None).expect_err("a failure");

        assert!(
            refusal.contains("no-message-for-you"),
            "git's text: {refusal}"
        );
        assert_eq!(sha(dir, "HEAD"), tip, "aborted, back where it began");
        assert_eq!(get_status(repo).expect("status").operation, None);
    }

    #[test]
    fn reorder_refuses_before_it_moves_anything() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let one = shas(dir, &["HEAD~2"]);

        assert!(reorder_commits(&repo, &[], None).is_err(), "no commits");
        let option = vec!["--abort".to_string()];
        assert!(reorder_commits(&repo, &option, None).is_err());
        assert!(reorder_commits(&repo, &one, Some("--root")).is_err());

        // A destination on another branch.
        git(dir, &["checkout", "-q", "-b", "side", "HEAD~2"]);
        commit_file(dir, "side.txt", "s\n", "side work");
        let foreign = sha(dir, "HEAD");
        git(dir, &["checkout", "-q", "main"]);
        let refusal = reorder_commits(&repo, &one, Some(&foreign)).expect_err("not on this branch");
        assert!(refusal.contains("current branch"), "{refusal}");
        assert!(reorder_preflight(&repo, &one, Some(&foreign)).is_err());

        fs::write(dir.join("one.txt"), "edited\n").expect("edit");
        let dirty = reorder_commits(&repo, &one, None).expect_err("dirty tree");
        assert!(dirty.contains("one.txt"), "{dirty}");
        let preflight = reorder_preflight(&repo, &one, None).expect("preflight");
        assert!(preflight.blocked.is_some_and(|why| why.contains("one.txt")));
        assert_eq!(sha(dir, "HEAD"), tip);
    }

    #[test]
    fn reorder_refuses_a_merge_between_the_commits_and_the_destination() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        git(dir, &["checkout", "-q", "-b", "topic", "HEAD~1"]);
        commit_file(dir, "t.txt", "t\n", "topic work");
        git(dir, &["checkout", "-q", "main"]);
        git(
            dir,
            &["merge", "-q", "--no-ff", "-m", "merge topic", "topic"],
        );
        commit_file(dir, "six.txt", "six\n", "six");
        let tip = sha(dir, "HEAD");
        // `six` sits above the merge; a destination below it replays the merge.
        let six = shas(dir, &["HEAD"]);
        let under = sha(dir, "HEAD~2");

        let preflight = reorder_preflight(&repo, &six, Some(&under)).expect("preflight");
        assert!(preflight.blocked.is_some_and(|why| why.contains("merge")));
        let refusal = reorder_commits(&repo, &six, Some(&under)).expect_err("refused");
        assert!(refusal.contains("merge"), "{refusal}");
        assert_eq!(sha(dir, "HEAD"), tip);
    }

    #[test]
    fn a_commit_that_is_somewhere_else_is_refused_as_that_and_not_as_a_merge() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        // A merge in the branch's past, so that a merge refusal is there to be
        // given by mistake, and a branch `main` never took in.
        git(dir, &["checkout", "-q", "-b", "topic", "HEAD~1"]);
        commit_file(dir, "t.txt", "t\n", "topic work");
        git(dir, &["checkout", "-q", "main"]);
        git(
            dir,
            &["merge", "-q", "--no-ff", "-m", "merge topic", "topic"],
        );
        commit_file(dir, "six.txt", "six\n", "six");
        git(dir, &["checkout", "-q", "-b", "elsewhere", "HEAD~1^1~3"]);
        commit_file(dir, "e.txt", "e\n", "elsewhere work");
        let elsewhere = sha(dir, "HEAD");
        git(dir, &["checkout", "-q", "main"]);
        let six = shas(dir, &["HEAD"]);
        let not_on_the_branch = "not all on the current branch";

        // A destination on a line that forked, a moved commit `HEAD` does not
        // reach, and an id that names a tree.
        let forked = reorder_commits(&repo, &six, Some(&elsewhere)).expect_err("refused");
        assert!(forked.contains(not_on_the_branch), "{forked}");
        let foreign = reorder_commits(&repo, &[elsewhere], None).expect_err("refused");
        assert!(foreign.contains(not_on_the_branch), "{foreign}");
        let tree = git_stdout(dir, &["rev-parse", "HEAD^{tree}"]);
        let no_commit = reorder_commits(&repo, &six, Some(&tree)).expect_err("refused");
        assert!(no_commit.contains(not_on_the_branch), "{no_commit}");
    }

    #[test]
    fn reorder_refuses_to_replay_from_the_commit_a_shallow_clone_ends_on() {
        let (upstream, _repo) = five_commits();
        let tmp = tempdir().expect("tempdir");
        let url = format!("file://{}", upstream.path().display());
        git(tmp.path(), &["clone", "-q", "--depth", "3", &url, "clone"]);
        let dir = tmp.path().join("clone");
        git(&dir, &["config", "user.name", "t"]);
        git(&dir, &["config", "user.email", "t@t"]);
        let repo = dir.to_str().expect("utf-8 path");
        let tip = sha(&dir, "HEAD");
        // `HEAD~2` is where the clone ends, and reads as a root it is not: a
        // commit moved under it would be replayed `--root`.
        let boundary = sha(&dir, "HEAD~2");
        let five = shas(&dir, &["HEAD"]);

        let refusal = reorder_commits(repo, &five, Some(&boundary)).expect_err("refused");

        assert!(refusal.contains("shallow"), "{refusal}");
        assert_eq!(sha(&dir, "HEAD"), tip);
        let preflight = reorder_preflight(repo, &five, Some(&boundary)).expect("preflight");
        assert!(preflight.blocked.is_some_and(|why| why.contains("shallow")));

        // Just above the boundary the parent is real.
        let above = sha(&dir, "HEAD~1");
        let moved = reorder_commits(repo, &five, Some(&above)).expect("reorder");
        assert!(moved.success);
        assert_eq!(subjects(&dir), ["four", "five", "three"]);

        // The boundary may be one of the moved commits as long as it keeps its
        // place: the replay then starts above it, on a parent that is real.
        let with_boundary = vec![boundary.clone(), sha(&dir, "HEAD")];
        let under = sha(&dir, "HEAD~1");
        let moved = reorder_commits(repo, &with_boundary, Some(&under)).expect("reorder");
        assert!(moved.success, "{:?}", moved.error_message);
        assert_eq!(subjects(&dir), ["five", "four", "three"]);
        assert_eq!(sha(&dir, "HEAD~2"), boundary, "the boundary kept its id");
    }

    #[test]
    fn reorder_preflight_says_when_pushed_commits_would_be_rewritten() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        // `origin/main` holds everything up to `three`.
        git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD~2"]);
        git(dir, &["config", "remote.origin.url", "nowhere"]);
        git(
            dir,
            &[
                "config",
                "remote.origin.fetch",
                "+refs/heads/*:refs/remotes/origin/*",
            ],
        );
        git(dir, &["branch", "-q", "--set-upstream-to=origin/main"]);
        let five = shas(dir, &["HEAD"]);

        let local = reorder_preflight(&repo, &five, Some(&sha(dir, "HEAD~1"))).expect("p");
        assert_eq!(
            local,
            RewritePreflight::ready(false),
            "`four` is not pushed"
        );
        let pushed = reorder_preflight(&repo, &five, Some(&sha(dir, "HEAD~2"))).expect("p");
        assert_eq!(pushed, RewritePreflight::ready(true), "`three` is");
    }

    #[test]
    fn reorder_ignores_the_settings_that_would_bend_the_todo() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        git(dir, &["branch", "bookmark", "HEAD~1"]);
        let bookmark = sha(dir, "bookmark");
        for (key, value) in [
            ("rebase.updateRefs", "true"),
            ("rebase.autoSquash", "true"),
            ("rebase.autoStash", "true"),
            ("rebase.rebaseMerges", "true"),
            ("rebase.abbreviateCommands", "true"),
            ("rebase.missingCommitsCheck", "error"),
            ("core.commentChar", ";"),
            ("sequence.editor", "false"),
            ("core.editor", "false"),
        ] {
            git(dir, &["config", key, value]);
        }

        let moved = reorder_commits(&repo, &shas(dir, &["HEAD~3"]), None).expect("reorder");

        assert!(moved.success);
        assert_eq!(subjects(dir), ["two", "five", "four", "three", "one"]);
        assert_eq!(
            sha(dir, "bookmark"),
            bookmark,
            "another branch is not moved"
        );
    }
}
