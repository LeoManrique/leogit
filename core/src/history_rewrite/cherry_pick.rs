//! Cherry-picking commits onto another branch.

use super::{RewriteResult, UndoStart, rewrite_preflight, switch_to};
use crate::git::{
    current_branch, git_dir, is_object_id, ls_files_unmerged, run_git, run_git_combined,
    run_git_combined_with_env,
};
use crate::operation::{self, OperationInProgress};

/// Copy `shas` onto the local branch `target_branch`, which becomes the
/// checked-out branch.
///
/// `shas` arrive newest first, as History lists them, and are picked oldest
/// first. Checking out the target and picking are one call so their order —
/// and the way back when the pick fails for a reason that is not a conflict —
/// live in one place.
///
/// * **Success** leaves the user on the target with the new commits at its tip.
/// * **A conflict** (`success` false) leaves the user on the target with the
///   cherry-pick open, for Continue or Abort.
/// * **Any other failure** is an `Err` with git's text, after the sequence has
///   been aborted and the source branch checked out again. If that way back
///   fails too, the message says where the user has been left.
///
/// **A conflict is `CHERRY_PICK_HEAD` plus unmerged paths**, read off the
/// repository rather than off git's translated text. The file alone is not
/// enough: a signer that cannot run (`commit.gpgsign` with no `gpg` on the
/// app's `PATH`) and a failing `prepare-commit-msg` hook both stop the sequence
/// on a commit with nothing to resolve, and Continue would re-run the same
/// failure forever. Those are failures, and go back like any other.
///
/// `--empty=keep` keeps a pick that turns out redundant instead of stopping the
/// sequence on it for a decision. `-m 1` lets the selection hold merge commits,
/// which land as ordinary single-parent commits.
///
/// # Errors
/// When the preflight refuses; when `target_branch` is the current branch or
/// not a local branch; when a sha is not an object id; when the target cannot
/// be checked out; or when the pick fails for a reason that is not a conflict.
pub fn cherry_pick_commits(
    repo_path: &str,
    shas: &[String],
    target_branch: &str,
) -> Result<RewriteResult, String> {
    if shas.is_empty() {
        return Err("No commits were selected.".to_string());
    }
    // Ids travel as arguments: anything else could be read as an option.
    if let Some(odd) = shas.iter().find(|sha| !is_object_id(sha)) {
        return Err(format!("Not a commit id: {odd}"));
    }
    if let Some(reason) = rewrite_preflight(repo_path, None)?.blocked {
        return Err(reason);
    }
    // The preflight has just refused a detached HEAD.
    let source = current_branch(repo_path)?.ok_or("HEAD is detached.")?;
    if source == target_branch {
        return Err(format!(
            "“{target_branch}” is the branch these commits are already on."
        ));
    }
    let before_sha = run_git(
        repo_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{target_branch}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("There is no local branch named “{target_branch}”."))?;
    let start = UndoStart {
        branch: target_branch.to_string(),
        before_sha,
        return_branch: Some(source.clone()),
    };

    if let Err(said) = switch_to(repo_path, target_branch) {
        // Picking now would land the copies wherever HEAD is instead.
        eprintln!("[history_rewrite] switch to {target_branch} refused: {said}");
        let refusal = format!("Could not check out “{target_branch}”:\n{said}");
        // The usual refusal moves nothing. One that left HEAD somewhere else
        // is taken back, so the `Err` means what it always means.
        return Err(match current_branch(repo_path) {
            Ok(Some(branch)) if branch == source => refusal,
            _ => match switch_to(repo_path, &source) {
                Ok(()) => refusal,
                Err(stranded) => format!(
                    "{refusal}\n\n“{source}” could not be checked out again either:\n{stranded}"
                ),
            },
        });
    }

    let mut args = vec!["cherry-pick", "--empty=keep", "-m", "1"];
    args.extend(shas.iter().rev().map(String::as_str));
    eprintln!(
        "[history_rewrite] cherry-pick {} commit(s) from {source} onto {target_branch}",
        shas.len()
    );
    let (picked, said) = run_git_combined_with_env(repo_path, &args, &[("GIT_EDITOR", ":")])?;

    if picked {
        let landed = run_git(repo_path, &["rev-parse", "HEAD"]).and_then(|after_sha| {
            let range = format!("{}..HEAD", start.before_sha);
            let listed = run_git(repo_path, &["rev-list", &range])?;
            Ok((
                listed.lines().map(str::to_string).collect(),
                start.landed_on(after_sha),
            ))
        });
        return Ok(RewriteResult::landed(landed));
    }

    let said = said.trim();
    let dir = git_dir(repo_path);
    let stopped_on_a_commit = dir
        .as_deref()
        .is_some_and(|dir| dir.join("CHERRY_PICK_HEAD").exists());
    let conflicts = if stopped_on_a_commit {
        ls_files_unmerged(repo_path)
    } else {
        Vec::new()
    };
    if !conflicts.is_empty() {
        eprintln!("[history_rewrite] cherry-pick stopped on a conflict: {said}");
        return Ok(RewriteResult::stopped(conflicts, said, start));
    }

    eprintln!("[history_rewrite] cherry-pick failed: {said}");
    Err(match return_to(repo_path, &source, dir.as_deref()) {
        Ok(()) => said.to_string(),
        Err(stranded) => format!("{said}\n\n{stranded}"),
    })
}

/// The way back after a pick that failed without a conflict: end the sequence
/// — a failure in the middle of one leaves it open with the earlier picks
/// already committed, and git refuses to switch branches meanwhile — then check
/// the source branch out again. `--abort`, never `--quit`: only the first puts
/// the target's tip back.
///
/// The `Err` is a sentence saying where the user has been left, since either
/// step can itself be refused (an untracked file in the way of the checkout).
fn return_to(
    repo_path: &str,
    source: &str,
    git_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    if operation::in_progress(git_dir) == Some(OperationInProgress::CherryPick) {
        let aborted = run_git_combined(repo_path, &["cherry-pick", "--abort"]);
        if !matches!(aborted, Ok((true, _))) {
            let said = aborted.map_or_else(|unrun| unrun, |(_, said)| said);
            return Err(format!(
                "The cherry-pick could not be aborted either, so it is still open:\n{}",
                said.trim()
            ));
        }
    }
    switch_to(repo_path, source).map_err(|said| {
        format!(
            "“{source}” could not be checked out again, so you are on the target branch:\n{said}"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::UndoPoint;
    #[cfg(unix)]
    use super::super::fixtures::failing_hook;
    use super::super::fixtures::{branch, linear_repo, sha};
    use super::*;
    use crate::git::get_status;
    use crate::operation::abort_operation;
    use crate::test_support::{commit_file, conflicting_repo, git, git_stdout, subjects};
    use std::fs;

    #[test]
    fn cherry_pick_copies_commits_oldest_first_and_keeps_empty_ones() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        // A commit that is empty to begin with, and one the target already
        // holds: neither may stop the sequence.
        git(dir, &["commit", "-q", "--allow-empty", "-m", "empty"]);
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1"), sha(dir, "HEAD~2")];
        let before = sha(dir, "target");

        let result = cherry_pick_commits(&repo, &picks, "target").expect("pick");

        assert!(result.success && result.conflicts.is_empty());
        assert_eq!(branch(dir), "target", "the user lands on the target");
        assert_eq!(subjects(dir), ["empty", "three", "two", "one"]);
        assert_eq!(
            result.selection,
            [sha(dir, "HEAD"), sha(dir, "HEAD~1"), sha(dir, "HEAD~2")],
            "the new ids, newest first"
        );
        assert_eq!(
            result.undo,
            Some(UndoPoint {
                branch: "target".to_string(),
                before_sha: before,
                after_sha: sha(dir, "HEAD"),
                return_branch: Some("main".to_string()),
            })
        );
    }

    #[test]
    fn cherry_pick_takes_a_merge_commit_among_ordinary_ones() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        git(dir, &["checkout", "-q", "-b", "topic", "HEAD~1"]);
        commit_file(dir, "t.txt", "t\n", "topic work");
        git(dir, &["checkout", "-q", "main"]);
        git(
            dir,
            &["merge", "-q", "--no-ff", "-m", "merge topic", "topic"],
        );
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];

        let result = cherry_pick_commits(&repo, &picks, "target").expect("pick");

        assert!(result.success);
        assert_eq!(subjects(dir), ["merge topic", "three", "one"]);
        assert!(dir.join("t.txt").exists(), "the merge's change came across");
    }

    #[test]
    fn cherry_pick_conflict_is_data_and_abort_restores_the_target() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        let side_tip = sha(dir, "side");
        // Both of main's commits: `base` is already on `side` and is kept as an
        // empty pick, then `main edits shared` conflicts with side's edit.
        let picks = vec![sha(dir, "main"), sha(dir, "main~1")];

        let result = cherry_pick_commits(&repo, &picks, "side").expect("a conflict is data");

        assert!(!result.success);
        assert_eq!(result.conflicts, ["shared.txt"]);
        assert!(result.error_message.is_some_and(|said| !said.is_empty()));
        assert!(result.selection.is_empty() && result.undo.is_none());
        assert_eq!(
            result.start,
            Some(UndoStart {
                branch: "side".to_string(),
                before_sha: side_tip.clone(),
                return_branch: Some("main".to_string()),
            }),
            "where the pick began, for an undo once it has been continued"
        );
        assert_eq!(branch(dir), "side", "left on the target, mid-operation");
        assert_eq!(
            get_status(repo.clone()).expect("status").operation,
            Some(OperationInProgress::CherryPick)
        );

        abort_operation(&repo).expect("abort");
        assert_eq!(
            sha(dir, "side"),
            side_tip,
            "the earlier pick is rolled back"
        );
    }

    #[test]
    fn cherry_pick_that_fails_without_a_conflict_returns_to_the_source() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let target_tip = sha(dir, "target");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];
        // An untracked file the *second* pick would create: the first lands,
        // then git refuses with the sequence open and no conflict.
        git(dir, &["rm", "-q", "--cached", "c.txt"]);
        git(dir, &["commit", "-q", "-m", "stop tracking c"]);

        let refusal = cherry_pick_commits(&repo, &picks, "target").expect_err("refused");

        assert!(refusal.contains("c.txt"), "git's own text: {refusal}");
        assert_eq!(branch(dir), "main", "back where it began");
        assert_eq!(sha(dir, "target"), target_tip, "and the first pick undone");
        assert_eq!(get_status(repo).expect("status").operation, None);
    }

    #[cfg(unix)]
    #[test]
    fn a_pick_stopped_with_nothing_to_resolve_is_a_failure_not_a_conflict() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let target_tip = sha(dir, "target");
        // The sequencer runs this hook, and its failure stops the pick on the
        // commit — `CHERRY_PICK_HEAD` set — with no file in conflict. Continue
        // would run the hook again, so this must not read as a conflict.
        failing_hook(dir, "prepare-commit-msg", "no-message-for-you");

        let refusal = cherry_pick_commits(&repo, &[sha(dir, "HEAD")], "target")
            .expect_err("a failure, not conflict data");

        assert!(
            refusal.contains("prepare-commit-msg"),
            "git's text: {refusal}"
        );
        assert_eq!(branch(dir), "main", "back where it began");
        assert_eq!(sha(dir, "target"), target_tip);
        assert_eq!(get_status(repo).expect("status").operation, None);
    }

    #[cfg(unix)]
    #[test]
    fn a_switch_is_judged_by_where_head_is_not_by_its_exit_status() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        // `git switch` switches and *then* fails on this hook — what a git-lfs
        // hook does when `git-lfs` is not on the app's PATH.
        failing_hook(dir, "post-checkout", "lfs-not-found");

        let result = cherry_pick_commits(&repo, &[sha(dir, "HEAD")], "target").expect("picked");

        assert!(result.success);
        assert_eq!(branch(dir), "target");
        assert_eq!(subjects(dir), ["three", "one"]);
    }

    #[test]
    fn cherry_pick_refuses_before_it_moves_anything() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let tip = vec![sha(dir, "HEAD")];

        assert!(cherry_pick_commits(&repo, &[], "target").is_err());
        assert!(
            cherry_pick_commits(&repo, &tip, "main").is_err(),
            "onto itself"
        );
        assert!(cherry_pick_commits(&repo, &tip, "nowhere").is_err());
        let option = vec!["--abort".to_string()];
        assert!(cherry_pick_commits(&repo, &option, "target").is_err());

        fs::write(dir.join("a.txt"), "edited\n").expect("edit");
        let dirty = cherry_pick_commits(&repo, &tip, "target").expect_err("dirty tree");
        assert!(dirty.contains("a.txt"), "{dirty}");
        assert_eq!(branch(dir), "main");
    }

    #[test]
    fn cherry_pick_never_creates_the_target_from_a_remote_branch() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        git(
            dir,
            &["update-ref", "refs/remotes/origin/elsewhere", "target"],
        );
        let tip = vec![sha(dir, "HEAD")];

        assert!(cherry_pick_commits(&repo, &tip, "elsewhere").is_err());
        assert_eq!(git_stdout(dir, &["branch", "--list", "elsewhere"]), "");
    }
}
