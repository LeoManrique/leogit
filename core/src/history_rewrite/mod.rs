//! The History actions that *start* a multi-step git operation.
//!
//! This module holds what they share — what has to be true before one may
//! begin ([`rewrite_preflight`]) and what one comes to ([`RewriteResult`]) —
//! and each action has a module of its own: [`cherry_pick`] copies commits onto
//! another branch, [`squash`] folds commits of the current branch into one and
//! [`reorder`] moves them to another place in it. Those two rewrite the branch
//! by replaying it from a todo: [`lineage`] places the commits on the branch,
//! and [`replay`] is the driver.
//!
//! What an operation looks like once it is open — and how to continue or abort
//! it — is [`operation`](super::operation)'s. The two meet in one rule: when an
//! action here stops on a conflict it leaves git's own state behind, so the
//! embedded terminal, the composer's Continue and the branch menu's Abort all
//! work on it exactly as they would on an operation begun by hand.

use serde::{Deserialize, Serialize};

use super::git::{current_branch, git_dir, has_commits, is_object_id, run_git, run_git_optional};
use super::git_version;
use super::operation;

mod cherry_pick;
#[cfg(test)]
mod fixtures;
mod lineage;
mod reorder;
mod replay;
mod squash;

pub use cherry_pick::cherry_pick_commits;
pub use reorder::{reorder_commits, reorder_preflight};
pub use squash::{SquashDraft, squash_commits, squash_draft};

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

impl RewritePreflight {
    fn ready(rewrites_pushed: bool) -> Self {
        Self {
            blocked: None,
            rewrites_pushed,
        }
    }

    fn refused(reason: String) -> Self {
        eprintln!("[history_rewrite] preflight refused: {reason}");
        Self {
            blocked: Some(reason),
            rewrites_pushed: false,
        }
    }

    /// The answer for an action that would replay the current branch —
    /// `branch`, which [`branch_ready_for_an_action`] has passed — from
    /// `oldest` up.
    fn for_a_replay(repo_path: &str, branch: &str, oldest: &str) -> Result<Self, String> {
        Ok(match replay_refusal(repo_path, oldest)? {
            Some(reason) => Self::refused(reason),
            None => Self::ready(is_on_upstream(repo_path, branch, oldest)),
        })
    }
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

impl RewriteResult {
    /// The commits landed. `landed` is the new selection and the undo point —
    /// or the reason they could not be read back, which costs both and not the
    /// success: once the commits have landed nothing may turn the answer into
    /// an `Err`, which would claim the repository is back where it began.
    fn landed(landed: Result<(Vec<String>, UndoPoint), String>) -> Self {
        let (selection, undo) = match landed {
            Ok((selection, undo)) => (selection, Some(undo)),
            Err(unread) => {
                eprintln!("[history_rewrite] done, but the new tip could not be read: {unread}");
                (Vec::new(), None)
            }
        };
        Self {
            success: true,
            conflicts: Vec::new(),
            error_message: None,
            selection,
            undo,
        }
    }

    /// There was nothing to do, and nothing was run: the commits asked for are
    /// the `selection` as they stand, and there is no step to undo.
    fn unchanged(selection: Vec<String>) -> Self {
        Self {
            success: true,
            conflicts: Vec::new(),
            error_message: None,
            selection,
            undo: None,
        }
    }

    /// Stopped on a conflict, with the operation left open: the unmerged paths
    /// and what git said, verbatim.
    fn stopped(conflicts: Vec<String>, said: &str) -> Self {
        Self {
            success: false,
            conflicts,
            error_message: Some(said.to_string()),
            selection: Vec::new(),
            undo: None,
        }
    }
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
/// overwritten), then what only a replay can run into — a merge commit among
/// the replayed commits, and a shallow clone that ends on the oldest of them.
///
/// # Errors
/// When git cannot be run. A refusal is `blocked`, not an `Err`.
pub fn rewrite_preflight(
    repo_path: &str,
    replayed_from: Option<&str>,
) -> Result<RewritePreflight, String> {
    let branch = match branch_ready_for_an_action(repo_path)? {
        Ok(branch) => branch,
        Err(reason) => return Ok(RewritePreflight::refused(reason)),
    };
    match replayed_from {
        Some(oldest) => RewritePreflight::for_a_replay(repo_path, &branch, oldest),
        None => Ok(RewritePreflight::ready(false)),
    }
}

/// The half of the preflight every History action shares: the branch an action
/// may start on, or the reason none may. An action asks this **before it reads
/// anything else off the repository**, so a detached `HEAD` is refused as that
/// and not as whatever git makes of the question that came first.
fn branch_ready_for_an_action(repo_path: &str) -> Result<Result<String, String>, String> {
    if let Err(too_old) = git_version::require_floor() {
        return Ok(Err(too_old));
    }
    if let Some(open) = operation::in_progress(git_dir(repo_path).as_deref()) {
        return Ok(Err(format!(
            "A {} is in progress. Continue or abort it first.",
            open.subcommand()
        )));
    }
    let Some(branch) = current_branch(repo_path)? else {
        return Ok(Err(
            "HEAD is detached. Check out a branch first.".to_string()
        ));
    };
    if !has_commits(repo_path) {
        return Ok(Err(format!("“{branch}” has no commits yet.")));
    }
    if let Some(dirty) = tracked_changes(repo_path)? {
        return Ok(Err(dirty));
    }
    Ok(Ok(branch))
}

/// Why the current branch cannot be replayed from `oldest` up, or `None` when
/// it can.
fn replay_refusal(repo_path: &str, oldest: &str) -> Result<Option<String>, String> {
    if !is_object_id(oldest) {
        return Err(format!("Not a commit id: {oldest}"));
    }
    if let Some(merge) = merge_refusal(repo_path, oldest)? {
        return Ok(Some(merge));
    }
    if parent_of(repo_path, oldest)?.is_none() && records_a_parent(repo_path, oldest)? {
        return Ok(Some(format!(
            "This is a shallow clone, and its history ends at {}. Commits cannot be replayed \
             from the commit a shallow clone ends on — fetch the history below it first \
             (git fetch --unshallow).",
            short(oldest)
        )));
    }
    Ok(None)
}

/// An object id as a refusal names it.
fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// The refusal for a merge commit anywhere from `HEAD` down to `oldest`, itself
/// included. Only a range without one is a single line of history, which is
/// what a todo of `pick` lines can replay — and what [`lineage`] can place
/// commits on.
fn merge_refusal(repo_path: &str, oldest: &str) -> Result<Option<String>, String> {
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
    Ok((!merge.is_empty()).then(|| {
        format!(
            "A merge commit ({}) is among the commits this would replay. History with a \
             merge in it cannot be squashed or reordered.",
            short(&merge)
        )
    }))
}

/// The commit `sha` sits on — what a replay from `sha` is rebased onto — or
/// `None` when it is a root. **As git's history walk sees it**, which in a
/// shallow clone is not as the commit records it: the commit the clone ends on
/// has its parents hidden, and reads as a root it is not. The preflight refuses
/// that case ([`records_a_parent`]); a replay `--root` from there would succeed
/// and cut the branch off from everything below.
fn parent_of(repo_path: &str, sha: &str) -> Result<Option<String>, String> {
    let listed = run_git(repo_path, &["rev-list", "--parents", "-1", sha])?;
    Ok(listed.split_whitespace().nth(1).map(str::to_string))
}

/// Whether the commit object itself names a parent, whatever the history walk
/// makes of it.
fn records_a_parent(repo_path: &str, sha: &str) -> Result<bool, String> {
    let object = run_git(repo_path, &["cat-file", "commit", sha])?;
    Ok(object
        .lines()
        .take_while(|header| !header.is_empty())
        .any(|header| header.starts_with("parent ")))
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

#[cfg(test)]
mod tests {
    use super::fixtures::{linear_repo, sha};
    use super::*;
    use crate::operation::abort_operation;
    use crate::test_support::{commit_file, conflicting_repo, git};
    use std::fs;

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
