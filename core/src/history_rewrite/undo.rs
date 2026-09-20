//! Taking a History action back.

use serde::{Deserialize, Serialize};

use super::{UndoPoint, open_operation_refusal, switch_to, tracked_changes};
use crate::git::{current_branch, is_object_id, run_git_combined, run_git_optional};

/// What an undo came to. Three answers, as for the actions themselves: the
/// branch is back (`undone`); the point has **expired** (`undone` false) — the
/// branch is not where the action left it, so this undo can never work again
/// and the offer should go; and an `Err`, for a refusal that may not hold next
/// time (tracked changes, an operation in progress) or a failure of git's.
/// Nothing has moved unless `undone`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoResult {
    pub undone: bool,
    /// When `undone`: what did not follow — the branch the action had left
    /// could not be checked out again. Otherwise: why the point has expired.
    pub message: Option<String>,
}

impl UndoResult {
    fn undone(message: Option<String>) -> Self {
        Self {
            undone: true,
            message,
        }
    }

    fn expired(reason: String) -> Self {
        eprintln!("[history_rewrite] undo point expired: {reason}");
        Self {
            undone: false,
            message: Some(reason),
        }
    }
}

/// Put `point.branch` back on `point.before_sha`.
///
/// The check is on the **branch**, not on `HEAD`: after a cherry-pick the user
/// may already be back on the source branch, and the undo is still good there.
/// It is refused unless the branch's tip is still exactly `after_sha` — a
/// commit made since is not reset over.
///
/// * **The branch is checked out here.** The tree must hold no tracked changes;
///   then `git reset --keep`, which moves the branch, the index and the working
///   tree as `--hard` does on a clean tree, but refuses rather than overwrite
///   an untracked file in its way — `--hard` deletes it, and `--merge` throws a
///   staged change away. (`switch -C` is as careful, but runs `post-checkout`,
///   and exits 1 on a hook's failure after it has moved everything.) Then, for
///   a cherry-pick, the branch it had left is checked out again.
/// * **It is not.** `git branch -f`, because it is the one command that knows
///   every way another worktree can hold a branch — checked out there, or
///   detached in the middle of a rebase or a bisect of it — and refuses. A bare
///   `update-ref` moves the ref under that worktree, whose index and files then
///   describe a commit it is no longer on; `%(worktreepath)` and `worktree
///   list` both report the rebasing worktree as detached and the branch as
///   free.
///
/// # Errors
/// When an id is not an object id; while an operation is in progress; over
/// tracked changes or an untracked file in the way; when another worktree
/// holds the branch; or when git fails. Nothing has moved — a reset that moved
/// the files and then could not move the branch is put back
/// ([`after_a_failed_reset`]), and the text says so if that failed too.
pub fn undo_operation(repo_path: &str, point: &UndoPoint) -> Result<UndoResult, String> {
    let UndoPoint {
        branch,
        before_sha,
        after_sha,
        return_branch,
    } = point;
    // Ids travel as arguments: anything else could be read as an option.
    if let Some(odd) = [before_sha, after_sha]
        .into_iter()
        .find(|sha| !is_object_id(sha))
    {
        return Err(format!("Not a commit id: {odd}"));
    }
    if let Some(open) = open_operation_refusal(repo_path) {
        return Err(open);
    }

    let branch_ref = format!("refs/heads/{branch}");
    let Some(tip) = run_git_optional(
        repo_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{branch_ref}^{{commit}}"),
        ],
    )?
    else {
        return Ok(UndoResult::expired(format!(
            "There is no local branch named “{branch}” any more."
        )));
    };
    if !tip.eq_ignore_ascii_case(after_sha) {
        return Ok(UndoResult::expired(format!(
            "“{branch}” has changed since, so this can no longer be undone here. Where the \
             branch has been is still in its reflog (git reflog {branch})."
        )));
    }
    let before_commit = format!("{before_sha}^{{commit}}");
    let kept = run_git_optional(
        repo_path,
        &["rev-parse", "--verify", "--quiet", &before_commit],
    )?;
    if kept.is_none() {
        return Ok(UndoResult::expired(format!(
            "The commit “{branch}” was on before is no longer in this repository."
        )));
    }

    if current_branch(repo_path)?.as_deref() != Some(branch.as_str()) {
        return move_a_branch_that_is_not_checked_out(repo_path, point);
    }

    if let Some(dirty) = tracked_changes(repo_path)? {
        return Err(dirty);
    }
    // `reset --keep` trusts the index's stat data and refuses a file whose
    // content is unchanged but whose inode is not ("Entry … not uptodate") —
    // an editor's save-by-rename, a build that rewrites in place. `status`
    // would have refreshed it, were it allowed to write here
    // (`GIT_OPTIONAL_LOCKS=0`). Its exit status says whether anything is
    // modified, which has just been answered.
    run_git_combined(repo_path, &["update-index", "-q", "--refresh"])?;
    eprintln!("[history_rewrite] undo: {branch} back to {before_sha}");
    let (reset, said) = run_git_combined(repo_path, &["reset", "--keep", before_sha])?;
    if !reset {
        return Err(after_a_failed_reset(repo_path, point, said.trim()));
    }
    // Only a cherry-pick left a branch to do its work, and only an undo made
    // from the target goes back: anywhere else the user chose where they are.
    let stranded = return_branch
        .as_deref()
        .filter(|source| source != branch)
        .and_then(|source| {
            switch_to(repo_path, source).err().map(|said| {
                eprintln!("[history_rewrite] undone, but {source} was not checked out: {said}");
                format!("Undone, but “{source}” could not be checked out again:\n{said}")
            })
        });
    Ok(UndoResult::undone(stranded))
}

/// What to say once `reset --keep` has failed — after putting right what it may
/// have left half done.
///
/// The reset writes the index and the files **first** and the ref **last**, so
/// a ref it cannot lock — a stale `<branch>.lock` from a git that crashed, a
/// `reference-transaction` hook that says no — leaves the tree on `before_sha`
/// under a branch still on `after_sha`. Every file the undo touched then reads
/// as a staged change, and the next try is refused over them. A refusal made up
/// front (an untracked file in the way) leaves the index as `HEAD` has it,
/// which is how the two are told apart: the tree held no tracked changes a
/// moment ago, so an index that differs now is this reset's doing.
///
/// The way back is the same two-way merge run the other way round,
/// `read-tree -m -u <before> <after>`: it moves no ref, so whatever stopped the
/// reset does not stop it, and like the reset it overwrites nothing that is
/// not git's. (`reset --keep <after>` does not do it — `HEAD` is on `after`
/// already, so it resets the index and leaves the files where they are.)
fn after_a_failed_reset(repo_path: &str, point: &UndoPoint, said: &str) -> String {
    let index_is_heads =
        run_git_combined(repo_path, &["diff-index", "--quiet", "--cached", "HEAD"]);
    if matches!(index_is_heads, Ok((true, _))) {
        return said.to_string();
    }
    eprintln!(
        "[history_rewrite] undo: the reset moved the tree and not the branch; putting it back"
    );
    let UndoPoint {
        before_sha,
        after_sha,
        ..
    } = point;
    match run_git_combined(repo_path, &["read-tree", "-m", "-u", before_sha, after_sha]) {
        Ok((true, _)) => format!("{said}\n\nNothing has changed: the files are back as they were."),
        Ok((false, why)) | Err(why) => {
            eprintln!("[history_rewrite] undo: the tree could not be put back: {why}");
            format!(
                "{said}\n\nThe branch has not moved, but its files are now those of the commit it \
                 was on before, and they could not be put back:\n{}\n\nThey read as uncommitted \
                 changes; discarding them all puts the files back.",
                why.trim()
            )
        }
    }
}

/// The undo of a branch `HEAD` is not on: no index and no files to move, so the
/// ref alone — which `branch -f` refuses to move under another worktree.
fn move_a_branch_that_is_not_checked_out(
    repo_path: &str,
    point: &UndoPoint,
) -> Result<UndoResult, String> {
    let UndoPoint {
        branch, before_sha, ..
    } = point;
    eprintln!("[history_rewrite] undo: {branch} back to {before_sha}, not checked out");
    let (moved, said) = run_git_combined(repo_path, &["branch", "-f", "--", branch, before_sha])?;
    if moved {
        Ok(UndoResult::undone(None))
    } else {
        Err(said.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{branch, edits_of_one_line, five_commits, linear_repo, sha};
    use super::super::{cherry_pick_commits, reorder_commits, squash_commits};
    use super::*;
    use crate::git::get_status;
    use crate::sync_ladder::SyncProposal;
    use crate::test_support::{commit_file, git, git_stdout, published_repo, subjects};
    use std::fs;
    use std::path::Path;

    /// `two` and `three` of [`linear_repo`] copied onto `target`, which is left
    /// checked out: the point to undo, and the two ids it names.
    fn picked_onto_target(dir: &Path, repo: &str) -> UndoPoint {
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];
        let result = cherry_pick_commits(repo, &picks, "target").expect("pick");
        result.undo.expect("a pick that landed can be undone")
    }

    fn is_clean(dir: &Path) -> bool {
        git_stdout(dir, &["status", "--porcelain"]).is_empty()
    }

    #[test]
    fn undo_of_a_squash_puts_the_branch_and_the_tree_back() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let before = sha(dir, "HEAD");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~2")];
        let squashed = squash_commits(&repo, &picks, "three and five").expect("squash");
        assert_eq!(subjects(dir), ["four", "three and five", "two", "one"]);

        let result = undo_operation(&repo, &squashed.undo.expect("undo point")).expect("undo");

        assert_eq!(result, UndoResult::undone(None));
        assert_eq!(sha(dir, "HEAD"), before, "the very commits, not copies");
        assert_eq!(branch(dir), "main");
        assert!(is_clean(dir));
    }

    #[test]
    fn undo_of_a_reorder_puts_the_branch_back() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let before = sha(dir, "HEAD");
        let moved = reorder_commits(&repo, &[sha(dir, "HEAD~3")], None).expect("reorder");
        assert_eq!(subjects(dir), ["two", "five", "four", "three", "one"]);

        let result = undo_operation(&repo, &moved.undo.expect("undo point")).expect("undo");

        assert!(result.undone);
        assert_eq!(sha(dir, "HEAD"), before);
        assert!(is_clean(dir));
    }

    #[test]
    fn undo_of_a_cherry_pick_made_from_the_target_goes_back_to_the_source() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let before = sha(dir, "target");
        let point = picked_onto_target(dir, &repo);
        assert_eq!(branch(dir), "target");

        let result = undo_operation(&repo, &point).expect("undo");

        assert_eq!(result, UndoResult::undone(None));
        assert_eq!(sha(dir, "target"), before);
        assert_eq!(branch(dir), "main", "where the commits came from");
        assert!(is_clean(dir));
        assert!(dir.join("c.txt").exists(), "main's own files are back");
    }

    #[test]
    fn undo_of_a_cherry_pick_works_from_the_source_branch() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let before = sha(dir, "target");
        let point = picked_onto_target(dir, &repo);
        git(dir, &["checkout", "-q", "main"]);
        let main_tip = sha(dir, "HEAD");

        let result = undo_operation(&repo, &point).expect("undo");

        assert_eq!(result, UndoResult::undone(None));
        assert_eq!(sha(dir, "target"), before, "the ref alone moved");
        assert_eq!(branch(dir), "main");
        assert_eq!(sha(dir, "HEAD"), main_tip);
        assert!(is_clean(dir));
    }

    #[test]
    fn undo_of_a_cherry_pick_works_with_head_detached() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let before = sha(dir, "target");
        let point = picked_onto_target(dir, &repo);
        git(dir, &["checkout", "-q", "--detach", "main"]);

        let result = undo_operation(&repo, &point).expect("undo");

        assert!(result.undone);
        assert_eq!(sha(dir, "target"), before);
        assert_eq!(
            sha(dir, "HEAD"),
            sha(dir, "main"),
            "HEAD stays where it was"
        );
    }

    #[test]
    fn undo_expires_once_the_branch_tip_has_moved() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];
        let point = squash_commits(&repo, &picks, "four and five")
            .expect("squash")
            .undo
            .expect("undo point");
        commit_file(dir, "six.txt", "six\n", "six");
        let tip = sha(dir, "HEAD");

        let result = undo_operation(&repo, &point).expect("an answer, not a failure");

        assert!(!result.undone);
        let why = result.message.expect("why");
        assert!(why.contains("has changed since"), "{why}");
        assert_eq!(
            sha(dir, "HEAD"),
            tip,
            "the commit made since is not reset over"
        );
    }

    #[test]
    fn undo_of_a_branch_that_is_not_checked_out_expires_the_same_way() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let point = picked_onto_target(dir, &repo);
        commit_file(dir, "more.txt", "more\n", "more on target");
        let tip = sha(dir, "target");
        git(dir, &["checkout", "-q", "main"]);

        let result = undo_operation(&repo, &point).expect("an answer, not a failure");

        assert!(!result.undone);
        assert_eq!(sha(dir, "target"), tip);
    }

    #[test]
    fn undo_refuses_over_tracked_changes_and_works_once_they_are_gone() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let before = sha(dir, "HEAD");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];
        let point = squash_commits(&repo, &picks, "four and five")
            .expect("squash")
            .undo
            .expect("undo point");
        let after = sha(dir, "HEAD");
        fs::write(dir.join("one.txt"), "edited\n").expect("edit");

        let refused = undo_operation(&repo, &point).expect_err("dirty");

        assert!(refused.contains("uncommitted changes"), "{refused}");
        assert!(refused.contains("one.txt"), "{refused}");
        assert_eq!(sha(dir, "HEAD"), after, "nothing moved");
        assert_eq!(
            fs::read_to_string(dir.join("one.txt")).expect("read"),
            "edited\n"
        );

        git(dir, &["checkout", "--", "one.txt"]);
        assert!(undo_operation(&repo, &point).expect("undo").undone);
        assert_eq!(sha(dir, "HEAD"), before);
    }

    /// What `reset --hard` would delete without a word: a file git does not
    /// know, standing where the undo has to put a tracked one back.
    #[test]
    fn undo_leaves_an_untracked_file_in_its_way_alone() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        git(dir, &["rm", "-q", "a.txt"]);
        git(dir, &["commit", "-q", "-m", "drop a"]);
        let result =
            cherry_pick_commits(&repo, &[sha(dir, "HEAD")], "target").expect("pick the deletion");
        let point = result.undo.expect("undo point");
        assert!(!dir.join("a.txt").exists());
        fs::write(dir.join("a.txt"), "mine, and never committed\n").expect("write");

        let refused = undo_operation(&repo, &point).expect_err("in the way");

        assert!(refused.contains("a.txt"), "{refused}");
        assert_eq!(
            fs::read_to_string(dir.join("a.txt")).expect("read"),
            "mine, and never committed\n"
        );
        assert_eq!(sha(dir, "target"), point.after_sha, "nothing moved");
        assert_eq!(branch(dir), "target");
    }

    /// An editor that saves by renaming a temporary file over the original
    /// leaves the content as it was and the inode not — which `reset --keep`
    /// takes for a modification of any file the undo has to touch, unless the
    /// index is refreshed first. Only an undo that changes files meets it: a
    /// squash and a reorder end on the tree they began with.
    #[test]
    fn undo_works_over_an_index_whose_stat_data_is_stale() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let before = sha(dir, "target");
        let point = picked_onto_target(dir, &repo);
        for name in ["b.txt", "c.txt"] {
            let copy = dir.join(format!("{name}.new"));
            fs::copy(dir.join(name), &copy).expect("copy");
            fs::rename(&copy, dir.join(name)).expect("rename over");
        }

        let result = undo_operation(&repo, &point).expect("undo");

        assert!(result.undone);
        assert_eq!(sha(dir, "target"), before);
        assert_eq!(branch(dir), "main");
        assert!(is_clean(dir));
    }

    #[test]
    fn undo_refuses_while_an_operation_is_open() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let before = sha(dir, "HEAD");
        let stopped = reorder_commits(&repo, &[sha(dir, "HEAD")], Some(&sha(dir, "HEAD~3")))
            .expect("reorder");
        assert!(!stopped.success, "meant to conflict");
        let point = UndoPoint {
            branch: "main".to_string(),
            before_sha: before.clone(),
            after_sha: before,
            return_branch: None,
        };

        let refused = undo_operation(&repo, &point).expect_err("a rebase is open");

        assert!(refused.contains("rebase is in progress"), "{refused}");
    }

    #[test]
    fn undo_refuses_a_branch_checked_out_in_another_worktree() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let point = picked_onto_target(dir, &repo);
        git(dir, &["checkout", "-q", "main"]);
        let elsewhere = tmp.path().join("elsewhere");
        git(
            dir,
            &[
                "worktree",
                "add",
                "-q",
                elsewhere.to_str().expect("utf-8"),
                "target",
            ],
        );

        let refused = undo_operation(&repo, &point).expect_err("held elsewhere");

        assert!(refused.contains("used by worktree"), "{refused}");
        assert_eq!(sha(dir, "target"), point.after_sha, "nothing moved");
    }

    /// The case `%(worktreepath)` and `worktree list` cannot see: the other
    /// worktree is detached, in the middle of rebasing the branch, whose ref
    /// still sits on the tip the undo expects.
    #[test]
    fn undo_refuses_a_branch_another_worktree_is_rebasing() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let point = picked_onto_target(dir, &repo);
        git(dir, &["checkout", "-q", "main"]);
        let elsewhere = tmp.path().join("elsewhere");
        git(
            dir,
            &[
                "worktree",
                "add",
                "-q",
                elsewhere.to_str().expect("utf-8"),
                "target",
            ],
        );
        // A failing `exec` stops the rebase with `target` still to be written.
        crate::test_support::git_stopping(&elsewhere, &["rebase", "-x", "false", "HEAD~1"]);
        assert_eq!(
            git_stdout(
                dir,
                &[
                    "for-each-ref",
                    "--format=%(worktreepath)",
                    "refs/heads/target"
                ]
            ),
            "",
            "git's own listing calls the branch free"
        );
        assert_eq!(sha(dir, "target"), point.after_sha);

        let refused = undo_operation(&repo, &point).expect_err("held elsewhere");

        assert!(refused.contains("used by worktree"), "{refused}");
        assert_eq!(sha(dir, "target"), point.after_sha, "nothing moved");
    }

    #[test]
    fn undo_says_so_when_the_source_branch_cannot_be_checked_out_again() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let before = sha(dir, "target");
        let point = picked_onto_target(dir, &repo);
        git(dir, &["branch", "-q", "-D", "main"]);

        let result = undo_operation(&repo, &point).expect("the undo itself worked");

        assert!(result.undone);
        let note = result.message.expect("a note");
        assert!(
            note.contains("“main” could not be checked out again"),
            "{note}"
        );
        assert_eq!(sha(dir, "target"), before);
        assert_eq!(branch(dir), "target");
    }

    /// `reset --keep` moves the files first and the ref last. A ref it cannot
    /// lock must not leave the old commit's files under the new commit's
    /// branch: that reads as a tree full of staged changes, which then refuse
    /// every further try.
    #[test]
    fn undo_puts_the_files_back_when_the_branch_cannot_be_moved() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let point = picked_onto_target(dir, &repo);
        let lock = dir.join(".git/refs/heads/target.lock");
        fs::write(&lock, "").expect("a lock a crashed git left behind");

        let refusal = undo_operation(&repo, &point).expect_err("the branch is locked");

        assert!(refusal.contains("target.lock"), "{refusal}");
        assert!(refusal.contains("Nothing has changed"), "{refusal}");
        assert_eq!(sha(dir, "target"), point.after_sha);
        assert!(is_clean(dir), "the picked commits' files are back");

        fs::remove_file(&lock).expect("the lock goes");
        let result = undo_operation(&repo, &point).expect("undo");
        assert!(result.undone, "and the same offer still works");
        assert_eq!(sha(dir, "target"), point.before_sha);
    }

    /// A point that names its own branch as the one to go back to asks for no
    /// switch at all — one would run `post-checkout` for nothing.
    #[test]
    fn undo_does_not_switch_to_the_branch_it_is_already_on() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let after = sha(dir, "HEAD");
        let hook = dir.join(".git/hooks/post-checkout");
        fs::create_dir_all(hook.parent().expect("hooks")).expect("hooks dir");
        fs::write(
            &hook,
            "#!/bin/sh\ntouch \"$(git rev-parse --git-dir)/switched\"\n",
        )
        .expect("hook");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("executable");
        }
        let point = UndoPoint {
            branch: "main".into(),
            before_sha: sha(dir, "HEAD~1"),
            after_sha: after,
            return_branch: Some("main".into()),
        };

        let result = undo_operation(&repo, &point).expect("undo");

        assert!(result.undone);
        assert_eq!(result.message, None);
        assert!(!dir.join(".git/switched").exists(), "no checkout was run");
    }

    #[test]
    fn undo_refuses_what_is_not_an_id_and_expires_on_what_is_gone() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let point = |branch: &str, before: &str| UndoPoint {
            branch: branch.to_string(),
            before_sha: before.to_string(),
            after_sha: tip.clone(),
            return_branch: None,
        };

        let odd = undo_operation(&repo, &point("main", "--hard")).expect_err("an option");
        assert!(odd.contains("Not a commit id"), "{odd}");

        let gone = undo_operation(&repo, &point("nope", &sha(dir, "HEAD~1"))).expect("answer");
        assert!(!gone.undone);
        assert!(
            gone.message
                .expect("why")
                .contains("no local branch named “nope”")
        );

        let pruned = "0123456789abcdef0123456789abcdef01234567";
        let lost = undo_operation(&repo, &point("main", pruned)).expect("answer");
        assert!(!lost.undone);
        assert!(
            lost.message
                .expect("why")
                .contains("no longer in this repository")
        );
        assert_eq!(sha(dir, "HEAD"), tip, "nothing moved");
    }

    /// A rewrite of pushed commits proposes a force push; taking it back before
    /// that push leaves nothing to push at all.
    #[test]
    fn undo_of_a_rewrite_of_pushed_commits_is_back_in_sync_with_the_upstream() {
        let (_tmp, mine) = published_repo();
        let repo = mine.to_str().expect("utf-8").to_string();
        let picks = vec![sha(&mine, "HEAD"), sha(&mine, "HEAD~1")];
        let point = squash_commits(&repo, &picks, "first and second")
            .expect("squash")
            .undo
            .expect("undo point");
        let rewritten = get_status(repo.clone()).expect("status");
        assert_eq!(rewritten.proposal, SyncProposal::ForcePush);

        assert!(undo_operation(&repo, &point).expect("undo").undone);

        let status = get_status(repo).expect("status");
        assert_eq!((status.ahead, status.behind), (0, 0));
        assert_eq!(status.proposal, SyncProposal::Fetch);
    }

    /// Taken back *after* the force push, the branch has diverged from the
    /// upstream by its own rewrite once more, and the ladder says so by itself.
    #[test]
    fn undo_after_the_force_push_proposes_the_force_push_again() {
        let (_tmp, mine) = published_repo();
        let repo = mine.to_str().expect("utf-8").to_string();
        let before = sha(&mine, "HEAD");
        let picks = vec![sha(&mine, "HEAD"), sha(&mine, "HEAD~1")];
        let point = squash_commits(&repo, &picks, "first and second")
            .expect("squash")
            .undo
            .expect("undo point");
        git(
            &mine,
            &["push", "-q", "--force-with-lease", "origin", "main"],
        );

        assert!(undo_operation(&repo, &point).expect("undo").undone);

        assert_eq!(sha(&mine, "HEAD"), before);
        let status = get_status(repo).expect("status");
        assert_eq!((status.ahead, status.behind), (2, 1));
        assert_eq!(status.proposal, SyncProposal::ForcePush);
    }
}
