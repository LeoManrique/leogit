//! Squashing commits of the current branch into one.
//!
//! **The target is the oldest selected commit**: every other selected commit
//! folds into it, where it stands, under a message the user wrote. The
//! selection need not be contiguous — the unselected commits between and above
//! are replayed after the squashed one.

use std::collections::HashSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::replay::{MESSAGE_GIT_PATH, Replay, Replayed};
use super::{RewriteResult, UndoPoint, branch_ready_for_an_action, parent_of, replay_refusal};
use crate::git::{CommitInfo, is_object_id, read_commits, run_git};

/// The message a squash sheet opens with, in the composer's three parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SquashDraft {
    pub summary: String,
    pub description: String,
    /// `Name <email>` values, as `format_commit_message` takes them.
    pub co_authors: Vec<String>,
}

/// The selected commits as the current branch holds them.
struct Lineage {
    /// Every commit from the oldest selected one up to `HEAD`, oldest first.
    range: Vec<String>,
    /// The selected commits, oldest first. The first is the squash target.
    selected: Vec<String>,
}

impl Lineage {
    /// Place `shas` — in any order — on the current branch.
    ///
    /// The oldest is the one commit all the others descend from, which on one
    /// line of history is their octopus merge base; the order of the rest is
    /// read off the walk from `HEAD` down to it, never off the order they
    /// arrived in.
    fn of(repo_path: &str, shas: &[String]) -> Result<Self, String> {
        if let Some(odd) = shas.iter().find(|sha| !is_object_id(sha)) {
            return Err(format!("Not a commit id: {odd}"));
        }
        // git prints ids in lowercase, and takes them in either case.
        let shas: Vec<String> = shas.iter().map(|sha| sha.to_ascii_lowercase()).collect();
        let wanted: HashSet<&str> = shas.iter().map(String::as_str).collect();
        if wanted.len() < 2 {
            return Err("Select at least two commits to squash.".to_string());
        }

        let mut args = vec!["merge-base", "--octopus"];
        args.extend(shas.iter().map(String::as_str));
        let oldest = run_git(repo_path, &args)?;
        // `^@` is every parent, so the walk includes `oldest` itself and is
        // not a fatal error when it is the root.
        let walked = run_git(
            repo_path,
            &[
                "rev-list",
                "--reverse",
                "HEAD",
                "--not",
                &format!("{oldest}^@"),
            ],
        )?;
        let range: Vec<String> = walked.lines().map(str::to_string).collect();
        let selected: Vec<String> = range
            .iter()
            .filter(|sha| wanted.contains(sha.as_str()))
            .cloned()
            .collect();
        // Both fail together when a selected commit is not on this branch: the
        // merge base is then a commit nobody selected, or the walk misses one.
        if selected.len() != wanted.len() || range.first() != Some(&oldest) {
            return Err("The selected commits are not all on the current branch.".to_string());
        }
        Ok(Self { range, selected })
    }

    fn target(&self) -> &str {
        &self.selected[0]
    }
}

/// What the squash sheet opens with: the target's summary; as the description,
/// the target's body followed by every other commit's summary and body, oldest
/// first; and every co-author any of them names, once.
///
/// # Errors
/// As [`squash_commits`], short of running anything.
pub fn squash_draft(repo_path: &str, shas: &[String]) -> Result<SquashDraft, String> {
    let lineage = Lineage::of(repo_path, shas)?;
    let commits = read_commits(repo_path, &lineage.selected)?;
    Ok(draft_of(&commits))
}

fn draft_of(commits: &[CommitInfo]) -> SquashDraft {
    let body = |commit: &CommitInfo| {
        commit
            .body_without_coauthors
            .as_deref()
            .unwrap_or(&commit.body)
            .trim()
            .to_string()
    };
    let Some((target, folded)) = commits.split_first() else {
        return SquashDraft {
            summary: String::new(),
            description: String::new(),
            co_authors: Vec::new(),
        };
    };

    let mut paragraphs = vec![body(target)];
    for commit in folded {
        paragraphs.push(commit.summary.trim().to_string());
        paragraphs.push(body(commit));
    }
    paragraphs.retain(|paragraph| !paragraph.is_empty());

    // One person is one address, however each commit spelled the name.
    let identity = |author: &String| {
        let lowered = author.to_lowercase();
        match (lowered.rfind('<'), lowered.rfind('>')) {
            (Some(open), Some(close)) if open < close => lowered[open + 1..close].to_string(),
            _ => lowered,
        }
    };
    let mut seen = HashSet::new();
    let co_authors = commits
        .iter()
        .flat_map(|commit| &commit.co_authors)
        .filter(|author| seen.insert(identity(author)))
        .cloned()
        .collect();

    SquashDraft {
        summary: target.summary.clone(),
        description: paragraphs.join("\n\n"),
        co_authors,
    }
}

/// The todo that replays `lineage.range` with the selected commits folded into
/// the target, where the target stands.
///
/// **The amend comes straight after the last `fixup`, before the unselected
/// commits are replayed** — that placement is the whole safety of it: at the
/// end of the todo the same line would reword the branch's last commit instead.
/// And because it is a line of the todo, it runs whenever the rebase reaches
/// it, so the message survives any number of conflict rounds and Continue never
/// needs to know about it. `--no-verify` because the replayed picks do not run
/// `pre-commit` either; `--cleanup=whitespace`, as the composer's own commit
/// has it, because a message typed into a field holds no comment lines — under
/// `commit.cleanup=strip` a line that begins with `#` would silently go.
/// Neither flag reaches a `prepare-commit-msg` hook, which git runs for the
/// amend as for every replayed commit and which may rewrite the message — as
/// it may the composer's; that is the repository's own rule, and is left to it.
fn squash_todo(lineage: &Lineage) -> String {
    let folded: HashSet<&str> = lineage.selected.iter().map(String::as_str).collect();
    let mut todo = format!("pick {}\n", lineage.target());
    for sha in &lineage.selected[1..] {
        let _ = writeln!(todo, "fixup {sha}");
    }
    let _ = writeln!(
        todo,
        "exec git commit --amend --no-verify --cleanup=whitespace -q \
         -F \"$(git rev-parse --git-path {MESSAGE_GIT_PATH})\""
    );
    for sha in &lineage.range {
        if !folded.contains(sha.as_str()) {
            let _ = writeln!(todo, "pick {sha}");
        }
    }
    todo
}

/// Fold `shas` — commits of the current branch, in any order — into the oldest
/// of them, under `message`.
///
/// * **Success** leaves the branch rewritten, with the squashed commit as the
///   `selection`.
/// * **A conflict** (`success` false) leaves the rebase open for Continue or
///   Abort; the message is already inside it and lands when the rebase gets
///   there.
/// * **Any other failure** is an `Err` with git's text, the rebase aborted and
///   the branch back where it began.
///
/// # Errors
/// When the preflight refuses; when fewer than two commits are named, one is
/// not an object id, or they are not all on the current branch; when the
/// message is blank; or when the rebase fails for a reason that is not a
/// conflict.
pub fn squash_commits(
    repo_path: &str,
    shas: &[String],
    message: &str,
) -> Result<RewriteResult, String> {
    if message.trim().is_empty() {
        return Err("A squashed commit needs a summary.".to_string());
    }
    // The shared refusals first: they are about the repository, and would
    // otherwise surface as whatever git makes of placing commits on a detached
    // or unborn HEAD.
    let branch = branch_ready_for_an_action(repo_path)??;
    let lineage = Lineage::of(repo_path, shas)?;
    if let Some(reason) = replay_refusal(repo_path, lineage.target())? {
        return Err(reason);
    }
    let before_sha = run_git(repo_path, &["rev-parse", "HEAD"])?;
    let onto = parent_of(repo_path, lineage.target())?;

    eprintln!(
        "[history_rewrite] squash {} commit(s) into {} on {branch}, replaying {}",
        lineage.selected.len(),
        lineage.target(),
        lineage.range.len()
    );
    let replayed = Replay {
        repo_path,
        onto: onto.as_deref(),
        todo: &squash_todo(&lineage),
        message: &format!("{}\n", message.trim_end()),
    }
    .run()?;

    match replayed {
        Replayed::Conflict(conflicts, said) => Ok(RewriteResult::stopped(conflicts, &said)),
        Replayed::Done => {
            // The squashed commit sits under the commits replayed after it.
            let above = lineage.range.len() - lineage.selected.len();
            let landed = run_git(repo_path, &["rev-parse", "HEAD", &format!("HEAD~{above}")])
                .and_then(|tips| {
                    let mut tips = tips.lines().map(str::to_string);
                    let (Some(after_sha), Some(squashed)) = (tips.next(), tips.next()) else {
                        return Err("rev-parse answered with fewer than two ids".to_string());
                    };
                    let undo = UndoPoint {
                        branch,
                        before_sha,
                        after_sha,
                        return_branch: None,
                    };
                    Ok((vec![squashed], undo))
                });
            Ok(RewriteResult::landed(landed))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::super::fixtures::failing_hook;
    use super::super::fixtures::{branch, sha};
    use super::super::rewrite_preflight;
    use super::*;
    use crate::git::get_status;
    use crate::operation::{OperationInProgress, abort_operation, continue_operation};
    use crate::test_support::{commit_file, git, git_stdout, init_test_repo, subjects};
    use std::fs;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};

    /// `main` with five commits, `one` … `five`, each adding its own file.
    fn five_commits() -> (TempDir, String) {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        for name in ["one", "two", "three", "four", "five"] {
            commit_file(dir, &format!("{name}.txt"), &format!("{name}\n"), name);
        }
        let repo_path = dir.to_str().expect("utf-8 path").to_string();
        (tmp, repo_path)
    }

    fn message_of(dir: &Path, rev: &str) -> String {
        git_stdout(dir, &["log", "-1", "--format=%B", rev])
    }

    fn files_of(dir: &Path, rev: &str) -> Vec<String> {
        let listed = git_stdout(
            dir,
            &["show", "--format=", "--name-only", "--no-renames", rev],
        );
        listed.lines().map(str::to_string).collect()
    }

    #[test]
    fn squash_gathers_a_non_contiguous_selection_at_the_target() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let untouched = sha(dir, "HEAD~4");
        let tip = sha(dir, "HEAD");
        // Click order, not history order: core places them itself.
        let picks = vec![sha(dir, "HEAD~3"), sha(dir, "HEAD"), sha(dir, "HEAD~1")];
        let message =
            "two, four and five\n\n# not a comment\n\nCo-authored-by: Ada <ada@example.com>";

        let result = squash_commits(&repo, &picks, message).expect("squash");

        assert!(result.success && result.conflicts.is_empty());
        assert_eq!(
            subjects(dir),
            ["three", "two, four and five", "one"],
            "folded where the oldest stood, the rest replayed after it"
        );
        assert_eq!(message_of(dir, "HEAD~1"), message, "`#` line and all");
        assert_eq!(files_of(dir, "HEAD~1"), ["five.txt", "four.txt", "two.txt"]);
        assert_eq!(
            sha(dir, "HEAD~2"),
            untouched,
            "older commits keep their ids"
        );
        assert_eq!(result.selection, [sha(dir, "HEAD~1")]);
        assert_eq!(
            result.undo,
            Some(UndoPoint {
                branch: "main".to_string(),
                before_sha: tip,
                after_sha: sha(dir, "HEAD"),
                return_branch: None,
            })
        );
        assert_eq!(branch(dir), "main");
        assert!(
            !dir.join(".git/rebase-merge").exists(),
            "and the message file went with the rebase"
        );
    }

    #[test]
    fn squash_reaching_the_first_commit_uses_root() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let picks = vec![sha(dir, "HEAD~3"), sha(dir, "HEAD~4")];

        let result = squash_commits(&repo, &picks, "one and two").expect("squash");

        assert!(result.success);
        assert_eq!(subjects(dir), ["five", "four", "three", "one and two"]);
        assert_eq!(result.selection, [sha(dir, "HEAD~3")]);
        assert_eq!(files_of(dir, "HEAD~3"), ["one.txt", "two.txt"]);
    }

    /// `main`: `base`, then `shared.txt` edited by `first edit`, an unrelated
    /// `between`, and `second edit` of the same line.
    fn edits_of_one_line() -> (TempDir, String) {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "shared.txt", "base\n", "base");
        commit_file(dir, "shared.txt", "first\n", "first edit");
        commit_file(dir, "other.txt", "other\n", "between");
        commit_file(dir, "shared.txt", "second\n", "second edit");
        let repo_path = dir.to_str().expect("utf-8 path").to_string();
        (tmp, repo_path)
    }

    #[test]
    fn squash_keeps_the_message_across_a_conflict() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        // Folding `second edit` into `base` applies "first → second" to a file
        // that reads "base": the rebase stops on the `fixup`, **before** the
        // amend — where a message handed to the first command alone is lost.
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~3")];

        let stopped = squash_commits(&repo, &picks, "base, rewritten").expect("data, not Err");

        assert!(!stopped.success);
        assert_eq!(stopped.conflicts, ["shared.txt"]);
        assert!(stopped.error_message.is_some_and(|said| !said.is_empty()));
        assert!(stopped.selection.is_empty() && stopped.undo.is_none());
        assert_eq!(
            get_status(repo.clone()).expect("status").operation,
            Some(OperationInProgress::Rebase)
        );

        // Round two: `first edit` is replayed after the fold, onto a file it
        // never saw.
        fs::write(dir.join("shared.txt"), "resolved\n").expect("resolve");
        let again = continue_operation(&repo).expect("continue");
        assert!(!again.success, "the replayed commit conflicts in its turn");
        fs::write(dir.join("shared.txt"), "resolved again\n").expect("resolve");
        let done = continue_operation(&repo).expect("continue");

        assert!(done.success, "{:?}", done.error_message);
        assert_eq!(
            subjects(dir),
            ["between", "first edit", "base, rewritten"],
            "the typed message landed, and `second edit` was folded"
        );
        assert!(!dir.join(".git/rebase-merge").exists());
    }

    #[test]
    fn a_failed_amend_is_run_again_by_the_next_continue() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~3")];
        let stopped = squash_commits(&repo, &picks, "typed once").expect("data, not Err");
        assert!(!stopped.success);

        // Whatever makes the amend fail once the rebase is in Continue's hands
        // — a signer, a hook — git must not cross the line off: the next
        // continue would finish the squash under the wrong message.
        let message = dir.join(".git").join(MESSAGE_GIT_PATH);
        let held = dir.join("held-message");
        fs::rename(&message, &held).expect("take the message away");
        fs::write(dir.join("shared.txt"), "resolved\n").expect("resolve");
        let failed = continue_operation(&repo).expect("continue");
        assert!(!failed.success && failed.conflicts.is_empty());

        fs::rename(&held, &message).expect("put it back");
        let mut outcome = continue_operation(&repo).expect("continue");
        if !outcome.success {
            fs::write(dir.join("shared.txt"), "resolved again\n").expect("resolve");
            outcome = continue_operation(&repo).expect("continue");
        }

        assert!(outcome.success, "{:?}", outcome.error_message);
        assert_eq!(subjects(dir), ["between", "first edit", "typed once"]);
    }

    #[test]
    fn a_squash_that_would_come_to_nothing_is_refused_and_undone() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "keep.txt", "keep\n", "base");
        commit_file(dir, "note.txt", "note\n", "adds a note");
        git(dir, &["rm", "-q", "note.txt"]);
        git(dir, &["commit", "-q", "-m", "removes the note"]);
        let tip = sha(dir, "HEAD");
        let repo = dir.to_str().expect("utf-8 path");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];

        // git will not fold a commit into nothing, and stops with nothing to
        // resolve — a failure, so the branch is put back.
        let refusal = squash_commits(repo, &picks, "nothing at all").expect_err("refused");

        assert!(refusal.contains("empty"), "git's text: {refusal}");
        assert_eq!(sha(dir, "HEAD"), tip);
        assert_eq!(branch(dir), "main");
        assert_eq!(git_stdout(dir, &["status", "--porcelain"]), "");
    }

    #[test]
    fn an_aborted_squash_puts_the_branch_back_and_leaves_nothing_behind() {
        let (tmp, repo) = edits_of_one_line();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~3")];
        let stopped = squash_commits(&repo, &picks, "never lands").expect("a conflict is data");
        assert!(!stopped.success);

        abort_operation(&repo).expect("abort");

        assert_eq!(sha(dir, "HEAD"), tip);
        assert_eq!(branch(dir), "main");
        assert!(!dir.join(".git/rebase-merge").exists());
    }

    #[cfg(unix)]
    #[test]
    fn squash_stopped_with_nothing_to_resolve_is_a_failure_not_a_conflict() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        // The sequencer runs this hook for every commit it makes; its failure
        // stops the rebase with no file in conflict, and Continue would only
        // run it again.
        failing_hook(dir, "prepare-commit-msg", "no-message-for-you");
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~2")];

        let refusal = squash_commits(&repo, &picks, "refused").expect_err("a failure");

        assert!(
            refusal.contains("no-message-for-you"),
            "git's text: {refusal}"
        );
        assert_eq!(sha(dir, "HEAD"), tip, "aborted, back where it began");
        assert_eq!(branch(dir), "main");
        assert_eq!(get_status(repo).expect("status").operation, None);
    }

    #[test]
    fn squash_refuses_before_it_moves_anything() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let tip = sha(dir, "HEAD");
        let two = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1")];

        assert!(squash_commits(&repo, &two[..1], "m").is_err(), "one commit");
        let twice = vec![two[0].clone(), two[0].clone()];
        assert!(
            squash_commits(&repo, &twice, "m").is_err(),
            "the same twice"
        );
        assert!(squash_commits(&repo, &two, " \n").is_err(), "no message");
        let option = vec![two[0].clone(), "--abort".to_string()];
        assert!(squash_commits(&repo, &option, "m").is_err());

        // A commit of another branch.
        git(dir, &["checkout", "-q", "-b", "side", "HEAD~2"]);
        commit_file(dir, "side.txt", "s\n", "side work");
        let foreign = vec![two[0].clone(), sha(dir, "HEAD")];
        git(dir, &["checkout", "-q", "main"]);
        let refusal = squash_commits(&repo, &foreign, "m").expect_err("not on this branch");
        assert!(refusal.contains("current branch"), "{refusal}");

        fs::write(dir.join("one.txt"), "edited\n").expect("edit");
        let dirty = squash_commits(&repo, &two, "m").expect_err("dirty tree");
        assert!(dirty.contains("one.txt"), "{dirty}");
        assert_eq!(sha(dir, "HEAD"), tip);
    }

    #[test]
    fn squash_refuses_to_replay_from_the_commit_a_shallow_clone_ends_on() {
        let (upstream, _repo) = five_commits();
        let tmp = tempdir().expect("tempdir");
        let url = format!("file://{}", upstream.path().display());
        git(tmp.path(), &["clone", "-q", "--depth", "3", &url, "clone"]);
        let dir = tmp.path().join("clone");
        git(&dir, &["config", "user.name", "t"]);
        git(&dir, &["config", "user.email", "t@t"]);
        let repo = dir.to_str().expect("utf-8 path");
        let tip = sha(&dir, "HEAD");
        // `HEAD~2` is where the clone ends: git hides its parent, so it reads
        // as a root — and a replay `--root` would cut the branch off there.
        let boundary = vec![sha(&dir, "HEAD"), sha(&dir, "HEAD~2")];

        let refusal = squash_commits(repo, &boundary, "m").expect_err("refused");

        assert!(refusal.contains("shallow"), "{refusal}");
        assert_eq!(sha(&dir, "HEAD"), tip);
        let preflight = rewrite_preflight(repo, Some(&boundary[1])).expect("preflight");
        assert!(preflight.blocked.is_some_and(|why| why.contains("shallow")));

        // Above the boundary the parent is real, and the squash is an ordinary one.
        let above = vec![sha(&dir, "HEAD"), sha(&dir, "HEAD~1")];
        let result = squash_commits(repo, &above, "four and five").expect("squash");
        assert!(result.success);
        assert_eq!(subjects(&dir), ["four and five", "three"]);
    }

    #[test]
    fn squash_takes_object_ids_in_either_case() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        let picks = vec![sha(dir, "HEAD").to_uppercase(), sha(dir, "HEAD~1")];

        let result = squash_commits(&repo, &picks, "four and five").expect("squash");

        assert!(result.success);
        assert_eq!(subjects(dir), ["four and five", "three", "two", "one"]);
    }

    #[test]
    fn squash_ignores_the_settings_that_would_bend_the_todo() {
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
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~2")];

        let result = squash_commits(&repo, &picks, "three and five").expect("squash");

        assert!(result.success);
        assert_eq!(subjects(dir), ["four", "three and five", "two", "one"]);
        assert_eq!(
            sha(dir, "bookmark"),
            bookmark,
            "another branch is not moved"
        );
    }

    #[test]
    fn a_repository_path_with_spaces_and_quotes_is_safe() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path().join("it's a \"repo\"");
        fs::create_dir(&dir).expect("mkdir");
        init_test_repo(&dir);
        git(&dir, &["checkout", "-q", "-b", "main"]);
        commit_file(&dir, "a.txt", "a\n", "one");
        commit_file(&dir, "b.txt", "b\n", "two");
        let picks = vec![sha(&dir, "HEAD"), sha(&dir, "HEAD~1")];

        let result = squash_commits(dir.to_str().expect("utf-8"), &picks, "both").expect("squash");

        assert!(result.success);
        assert_eq!(subjects(&dir), ["both"]);
    }

    #[test]
    fn draft_is_the_targets_summary_then_every_message_oldest_first() {
        let (tmp, repo) = five_commits();
        let dir = tmp.path();
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "six\n\nwhy six\n\nCo-authored-by: Ada <ada@example.com>",
            ],
        );
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "seven\n\nCo-authored-by: ada <ADA@example.com>\nCo-authored-by: Bo <bo@example.com>",
            ],
        );
        // Newest first, as History hands them over.
        let picks = vec![sha(dir, "HEAD"), sha(dir, "HEAD~1"), sha(dir, "HEAD~3")];

        let draft = squash_draft(&repo, &picks).expect("draft");

        assert_eq!(
            draft,
            SquashDraft {
                summary: "four".to_string(),
                description: "six\n\nwhy six\n\nseven".to_string(),
                co_authors: vec![
                    "Ada <ada@example.com>".to_string(),
                    "Bo <bo@example.com>".to_string()
                ],
            }
        );
    }
}
