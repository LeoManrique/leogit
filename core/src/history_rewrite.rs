//! The History actions that *start* a multi-step git operation: what has to be
//! true before one may begin, and cherry-picking commits onto another branch.
//!
//! What an operation looks like once it is open — and how to continue or abort
//! it — is [`operation`](super::operation)'s. The two meet in one rule: when an
//! action here stops on a conflict it leaves git's own state behind, so the
//! embedded terminal, the composer's Continue and the branch menu's Abort all
//! work on it exactly as they would on an operation begun by hand.

use serde::{Deserialize, Serialize};

use super::git::{
    current_branch, git_dir, has_commits, is_object_id, ls_files_unmerged, run_git,
    run_git_combined, run_git_combined_with_env, run_git_optional,
};
use super::git_version;
use super::operation::{self, OperationInProgress};

/// How many changed files a dirty-tree refusal names before it counts the rest.
const DIRTY_FILES_NAMED: usize = 10;

/// Whether a History action may start, and what it would do to pushed work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewritePreflight {
    /// Why the action cannot start, as one message for the user — `None` when
    /// it can. A refusal is data rather than an `Err`: it is an ordinary answer
    /// to "may I?", and the client asks before it opens any dialog.
    pub blocked: Option<String>,
    /// At least one commit the action would replay is already on the upstream,
    /// so the next push after it is a force push. Always `false` for an action
    /// that replays nothing on the current branch.
    pub rewrites_pushed: bool,
}

/// What a History action came to. Shaped like
/// [`MergeResult`](super::git::MergeResult): stopping on a conflict is an
/// ordinary outcome of replaying commits, so it is data — `success` false,
/// git's own text verbatim and the conflicted paths, **with the operation left
/// open** for Continue or Abort. An `Err` always means the repository is back
/// where it began, as far as git allowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteResult {
    pub success: bool,
    pub conflicts: Vec<String>,
    pub error_message: Option<String>,
    /// The new ids of the commits acted on, newest first — what to select in
    /// History instead of jumping to the tip. Empty unless `success`.
    pub selection: Vec<String>,
    /// How to put the branch back. `None` unless `success` — and, with an empty
    /// `selection`, on a success whose new tip could not be read back.
    pub undo: Option<UndoPoint>,
}

/// The one ref a History action moved, before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoPoint {
    pub branch: String,
    pub before_sha: String,
    pub after_sha: String,
    /// The branch the action left to do its work — cherry-pick's source.
    pub return_branch: Option<String>,
}

/// Whether a History action may start on the current branch.
///
/// `replayed_from` is the oldest commit the action would replay on the current
/// branch — the oldest selected commit for a squash or a reorder — and `None`
/// for an action that replays nothing here, which is cherry-pick: it copies
/// commits elsewhere, so a merge among them is fine (`-m 1`) and nothing pushed
/// is rewritten.
///
/// Refused, in this order: a git below the floor, an operation already in
/// progress, a detached or unborn `HEAD`, tracked changes (staged or not —
/// untracked files pass, and git's own refusal is data if one would be
/// overwritten), and a merge commit among the replayed commits.
///
/// # Errors
/// When git cannot be run. A refusal is `blocked`, not an `Err`.
pub fn rewrite_preflight(
    repo_path: &str,
    replayed_from: Option<&str>,
) -> Result<RewritePreflight, String> {
    let blocked = |reason: String| {
        eprintln!("[history_rewrite] preflight refused: {reason}");
        Ok(RewritePreflight {
            blocked: Some(reason),
            rewrites_pushed: false,
        })
    };

    if let Err(too_old) = git_version::require_floor() {
        return blocked(too_old);
    }
    if let Some(open) = operation::in_progress(git_dir(repo_path).as_deref()) {
        return blocked(format!(
            "A {} is in progress. Continue or abort it first.",
            open.subcommand()
        ));
    }
    let Some(branch) = current_branch(repo_path)? else {
        return blocked("HEAD is detached. Check out a branch first.".to_string());
    };
    if !has_commits(repo_path) {
        return blocked(format!("“{branch}” has no commits yet."));
    }
    if let Some(dirty) = tracked_changes(repo_path)? {
        return blocked(dirty);
    }

    let Some(oldest) = replayed_from else {
        return Ok(RewritePreflight {
            blocked: None,
            rewrites_pushed: false,
        });
    };
    if !is_object_id(oldest) {
        return Err(format!("Not a commit id: {oldest}"));
    }
    // `^@` is every parent of the commit, so this is "merges from HEAD down to
    // and including it" — and unlike `<oldest>^..HEAD` it is not a fatal error
    // when the commit is the root. `rev-list` exits 0 either way; what it
    // printed is the answer.
    let merge = run_git(
        repo_path,
        &[
            "rev-list",
            "-1",
            "--merges",
            "HEAD",
            "--not",
            &format!("{oldest}^@"),
        ],
    )?;
    if !merge.is_empty() {
        let short = &merge[..merge.len().min(7)];
        return blocked(format!(
            "A merge commit ({short}) is among the commits this would replay. History with a \
             merge in it cannot be squashed or reordered."
        ));
    }
    Ok(RewritePreflight {
        blocked: None,
        rewrites_pushed: is_on_upstream(repo_path, &branch, oldest),
    })
}

/// A refusal naming the tracked files with uncommitted changes, or `None` when
/// there are none. Untracked files are left out because git lets every
/// operation here start over them, and a submodule that is merely dirty inside
/// because git ignores that too — refusing it would block what git would run.
fn tracked_changes(repo_path: &str) -> Result<Option<String>, String> {
    let listing = run_git(
        repo_path,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=no",
            "--ignore-submodules=dirty",
        ],
    )?;
    // `-z` ends each entry with a NUL and leaves the paths as they are, where
    // the line format quotes them and writes a rename as `old -> new`. An entry
    // is `XY <path>`; a rename or copy is followed by one more field, the path
    // it came from, which is not a change of its own.
    let mut changed = Vec::new();
    let mut fields = listing.split('\0');
    while let Some(entry) = fields.next() {
        let Some(path) = entry.get(3..) else { continue };
        let renamed = entry.as_bytes()[..2]
            .iter()
            .any(|code| matches!(code, b'R' | b'C'));
        if renamed {
            fields.next();
        }
        changed.push(path.to_string());
    }
    if changed.is_empty() {
        return Ok(None);
    }
    let unnamed = changed.len().saturating_sub(DIRTY_FILES_NAMED);
    changed.truncate(DIRTY_FILES_NAMED);
    if unnamed > 0 {
        changed.push(format!("… and {unnamed} more"));
    }
    Ok(Some(format!(
        "There are uncommitted changes to:\n{}\n\nCommit or discard them, then try again.",
        changed.join("\n")
    )))
}

/// Whether `sha` is already on the upstream of `branch`. The replayed range is
/// linear — the preflight has refused a merge in it — so its oldest commit
/// being on the upstream is exactly "at least one replayed commit is pushed",
/// and someone else's push, which moves the upstream without containing `sha`,
/// does not count. Every doubt reads as "not pushed": no upstream configured,
/// or one that was never fetched.
fn is_on_upstream(repo_path: &str, branch: &str, sha: &str) -> bool {
    let upstream = run_git(
        repo_path,
        &[
            "for-each-ref",
            "--format=%(upstream)",
            &format!("refs/heads/{branch}"),
        ],
    )
    .unwrap_or_default();
    if upstream.is_empty() {
        return false;
    }
    // Exit 0 is yes, 1 is no — which `run_git_optional` reads as `None` — and
    // anything else (the upstream ref does not exist here) is a doubt.
    match run_git_optional(repo_path, &["merge-base", "--is-ancestor", sha, &upstream]) {
        Ok(answer) => answer.is_some(),
        Err(doubt) => {
            eprintln!("[history_rewrite] could not ask whether {sha} is pushed: {doubt}");
            false
        }
    }
}

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
        // The commits have landed, so nothing from here on may turn the answer
        // into an `Err` — that would claim the repository is back where it
        // began. A tip that cannot be read back costs the selection and the
        // undo point, not the success.
        let landed = run_git(repo_path, &["rev-parse", "HEAD"]).and_then(|after_sha| {
            let listed = run_git(repo_path, &["rev-list", &format!("{before_sha}..HEAD")])?;
            Ok((after_sha, listed))
        });
        let (selection, undo) = match landed {
            Ok((after_sha, listed)) => (
                listed.lines().map(str::to_string).collect(),
                Some(UndoPoint {
                    branch: target_branch.to_string(),
                    before_sha,
                    after_sha,
                    return_branch: Some(source),
                }),
            ),
            Err(unread) => {
                eprintln!("[history_rewrite] picked, but the new tip could not be read: {unread}");
                (Vec::new(), None)
            }
        };
        return Ok(RewriteResult {
            success: true,
            conflicts: Vec::new(),
            error_message: None,
            selection,
            undo,
        });
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
        return Ok(RewriteResult {
            success: false,
            conflicts,
            error_message: Some(said.to_string()),
            selection: Vec::new(),
            undo: None,
        });
    }

    eprintln!("[history_rewrite] cherry-pick failed: {said}");
    Err(match return_to(repo_path, &source, dir.as_deref()) {
        Ok(()) => said.to_string(),
        Err(stranded) => format!("{said}\n\n{stranded}"),
    })
}

/// Check the local branch `branch` out, answering by **where HEAD is
/// afterwards** rather than by git's exit status: `git switch` switches and
/// *then* exits non-zero when a `post-checkout` hook fails — git-lfs installs
/// one, and it fails wherever `git-lfs` is not on the app's `PATH` — so the
/// status alone would report a checkout that happened as one that did not.
///
/// `switch --no-guess --` can only ever land on the existing local branch:
/// `checkout refs/heads/<name>` would detach, and a bare `switch <name>` may
/// create the branch from a remote-tracking ref of that name.
///
/// The `Err` is what git said.
fn switch_to(repo_path: &str, branch: &str) -> Result<(), String> {
    let (switched, said) = run_git_combined(repo_path, &["switch", "--no-guess", "--", branch])?;
    if switched {
        return Ok(());
    }
    if current_branch(repo_path)?.as_deref() == Some(branch) {
        eprintln!(
            "[history_rewrite] on {branch}, though the switch complained: {}",
            said.trim()
        );
        return Ok(());
    }
    Err(said.trim().to_string())
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
    use super::*;
    use crate::git::get_status;
    use crate::operation::abort_operation;
    use crate::test_support::{
        commit_file, conflicting_repo, git, git_stdout, init_test_repo, subjects,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};

    /// `main` with three commits and an untouched `target` branched off the
    /// first, `main` checked out.
    fn linear_repo() -> (TempDir, String) {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "a.txt", "a\n", "one");
        git(dir, &["branch", "target"]);
        commit_file(dir, "b.txt", "b\n", "two");
        commit_file(dir, "c.txt", "c\n", "three");
        let repo_path = dir.to_str().expect("utf-8 path").to_string();
        (tmp, repo_path)
    }

    fn sha(dir: &Path, rev: &str) -> String {
        git_stdout(dir, &["rev-parse", rev])
    }

    fn branch(dir: &Path) -> String {
        git_stdout(dir, &["symbolic-ref", "--short", "HEAD"])
    }

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

    /// Install an executable hook that prints `says` and fails.
    #[cfg(unix)]
    fn failing_hook(dir: &Path, name: &str, says: &str) {
        use std::os::unix::fs::PermissionsExt;
        let hooks = dir.join(".git/hooks");
        fs::create_dir_all(&hooks).expect("hooks dir");
        let hook = hooks.join(name);
        fs::write(&hook, format!("#!/bin/sh\necho {says} >&2\nexit 1\n")).expect("hook");
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod");
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
    fn a_dirty_tree_refusal_names_a_renamed_file_by_its_new_name() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        git(dir, &["mv", "a.txt", "renamed \"a\".txt"]);

        let preflight = rewrite_preflight(&repo, None).expect("preflight");

        let why = preflight.blocked.expect("blocked");
        assert!(why.contains("\nrenamed \"a\".txt\n"), "{why}");
        assert!(!why.contains("->"), "{why}");
        assert!(
            !why.contains("\na.txt"),
            "the old name is not a change: {why}"
        );
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

    #[test]
    fn rewrite_refuses_a_merge_commit_in_the_range_and_a_dirty_tree() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        let oldest = sha(dir, "HEAD~1");
        assert_eq!(
            rewrite_preflight(&repo, Some(&oldest)).expect("preflight"),
            RewritePreflight {
                blocked: None,
                rewrites_pushed: false
            }
        );

        // Untracked files pass; a tracked edit does not.
        fs::write(dir.join("new.txt"), "n\n").expect("write");
        assert_eq!(
            rewrite_preflight(&repo, Some(&oldest)).expect("p").blocked,
            None
        );
        fs::write(dir.join("b.txt"), "edited\n").expect("edit");
        let dirty = rewrite_preflight(&repo, Some(&oldest)).expect("preflight");
        assert!(dirty.blocked.is_some_and(|why| why.contains("b.txt")));
        git(dir, &["checkout", "-q", "--", "b.txt"]);

        git(dir, &["checkout", "-q", "-b", "topic", "HEAD~1"]);
        commit_file(dir, "t.txt", "t\n", "topic work");
        git(dir, &["checkout", "-q", "main"]);
        git(
            dir,
            &["merge", "-q", "--no-ff", "-m", "merge topic", "topic"],
        );
        let merged = rewrite_preflight(&repo, Some(&oldest)).expect("preflight");
        assert!(merged.blocked.is_some_and(|why| why.contains("merge")));
        // The same range is fine for an action that replays nothing here.
        assert_eq!(rewrite_preflight(&repo, None).expect("p").blocked, None);
    }

    #[test]
    fn preflight_refuses_an_open_operation_and_a_detached_head() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        crate::test_support::git_stopping(dir, &["merge", "side"]);
        let open = rewrite_preflight(&repo, None).expect("preflight");
        assert!(open.blocked.is_some_and(|why| why.contains("merge")));
        abort_operation(&repo).expect("abort");

        git(dir, &["checkout", "-q", "--detach"]);
        let detached = rewrite_preflight(&repo, None).expect("preflight");
        assert!(detached.blocked.is_some_and(|why| why.contains("detached")));
    }

    #[test]
    fn preflight_reaches_the_root_commit_without_failing() {
        let (tmp, repo) = linear_repo();
        let root = sha(tmp.path(), "HEAD~2");
        assert_eq!(
            rewrite_preflight(&repo, Some(&root)).expect("p").blocked,
            None
        );
    }

    #[test]
    fn rewrites_pushed_is_false_without_an_upstream() {
        let (tmp, repo) = linear_repo();
        let oldest = sha(tmp.path(), "HEAD~1");
        let preflight = rewrite_preflight(&repo, Some(&oldest)).expect("preflight");
        assert!(!preflight.rewrites_pushed);
    }

    #[test]
    fn rewrites_pushed_ignores_a_foreign_push() {
        let (tmp, repo) = linear_repo();
        let dir = tmp.path();
        // `origin/main` holds "two" and then someone else's commit; "three" is
        // ours and unpushed.
        git(dir, &["checkout", "-q", "-b", "theirs", "HEAD~1"]);
        commit_file(dir, "theirs.txt", "x\n", "a foreign push");
        git(dir, &["checkout", "-q", "main"]);
        git(dir, &["update-ref", "refs/remotes/origin/main", "theirs"]);
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

        let unpushed = rewrite_preflight(&repo, Some(&sha(dir, "HEAD"))).expect("p");
        assert!(!unpushed.rewrites_pushed, "the upstream moved, not ours");
        let pushed = rewrite_preflight(&repo, Some(&sha(dir, "HEAD~1"))).expect("p");
        assert!(pushed.rewrites_pushed, "“two” is on the upstream");
    }
}
