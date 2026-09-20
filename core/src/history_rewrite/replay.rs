//! The driver under every action that rewrites the current branch: replay it
//! from a todo this app wrote, through `git rebase -i`.
//!
//! git asks a *sequence editor* to edit the todo it generated; ours replaces
//! it. Nothing is spliced into a shell string — the two paths travel in
//! environment variables, and the editor is one fixed line of `sh`.

use std::fs;

use crate::git::{git_dir, ls_files_unmerged, run_git_combined, run_git_combined_with_env};
use crate::operation::{self, OperationInProgress, without_progress};

/// The replay's message file, relative to the git dir — what a todo's `exec`
/// line hands to `git commit -F`. It lives **inside** `rebase-merge/` so that
/// it shares the rebase's lifetime exactly: it survives any number of conflict
/// rounds, and git deletes it with the rest of that directory when the rebase
/// finishes or is aborted — from this app or from a terminal.
pub(super) const MESSAGE_GIT_PATH: &str = "rebase-merge/leogit-message";

/// Replaces git's todo with ours and drops the message beside it. git runs the
/// editor through `sh` with the todo's path as `$1`, which sits in
/// `rebase-merge/` — the directory the message belongs in.
const SEQUENCE_EDITOR: &str =
    r#"sh -c 'cp "$LEOGIT_TODO" "$1" && cp "$LEOGIT_MESSAGE" "$(dirname "$1")/leogit-message"' --"#;

/// One replay of the current branch.
pub(super) struct Replay<'a> {
    pub repo_path: &'a str,
    /// The commit the replayed range sits on — the parent of the oldest commit
    /// in the todo — or `None` when that commit is the root.
    pub onto: Option<&'a str>,
    /// The todo: full object ids, no comment lines, so neither
    /// `core.commentChar` nor `rebase.abbreviateCommands` has a say in it.
    pub todo: &'a str,
    /// What [`MESSAGE_GIT_PATH`] will hold.
    pub message: &'a str,
}

/// How a replay ended when it did not fail.
pub(super) enum Replayed {
    Done,
    /// Stopped with unmerged paths and the rebase left open: the paths, and
    /// what git said.
    Conflict(Vec<String>, String),
}

impl Replay<'_> {
    /// Run the replay.
    ///
    /// `--empty=keep` because a rewrite must neither stop for a reason the UI
    /// cannot explain nor lose a commit, and `rebase -i` halts on a commit the
    /// new order made empty. `--no-autostash`, `--no-update-refs`,
    /// `--no-rebase-merges` and `--no-autosquash` pin the user settings that
    /// would change what the todo means. `commit.gpgsign` is left alone: a user
    /// who signs wants the replayed commits signed.
    ///
    /// `--reschedule-failed-exec` is for the rounds after this call, which
    /// belong to Continue: an `exec` that fails is otherwise already crossed
    /// off the todo, and the next `--continue` finishes the rebase without it —
    /// for a squash, without the message. git keeps the setting with the
    /// rebase, so it holds from a terminal too.
    ///
    /// **A stop is a conflict only if there are unmerged paths.** A signer that
    /// cannot run, a failing hook and a failing `exec` line all stop the rebase
    /// on a step with nothing to resolve, where Continue would run the same
    /// failure again — those are failures, and the rebase is aborted.
    ///
    /// # Errors
    /// With git's text, the branch back where it began — or a second paragraph
    /// saying the rebase is still open, when the abort was refused too.
    pub(super) fn run(&self) -> Result<Replayed, String> {
        // Unique per run, so two windows on one repository cannot hand each
        // other's todo to git; removed when `scratch` drops, by which time the
        // editor has long since copied both files.
        let scratch = tempfile::tempdir().map_err(|e| format!("temporary directory: {e}"))?;
        let todo_path = scratch.path().join("todo");
        let message_path = scratch.path().join("message");
        fs::write(&todo_path, self.todo).map_err(|e| format!("write the rebase todo: {e}"))?;
        fs::write(&message_path, self.message)
            .map_err(|e| format!("write the commit message: {e}"))?;
        let (Some(todo_path), Some(message_path)) = (todo_path.to_str(), message_path.to_str())
        else {
            return Err("The temporary directory's path is not UTF-8.".to_string());
        };

        let mut args = vec![
            "rebase",
            "-i",
            "--no-autostash",
            "--no-update-refs",
            "--no-rebase-merges",
            "--no-autosquash",
            "--reschedule-failed-exec",
            "--empty=keep",
        ];
        args.push(self.onto.unwrap_or("--root"));
        let (replayed, said) = run_git_combined_with_env(
            self.repo_path,
            &args,
            &[
                ("LEOGIT_TODO", todo_path),
                ("LEOGIT_MESSAGE", message_path),
                ("GIT_SEQUENCE_EDITOR", SEQUENCE_EDITOR),
                ("GIT_EDITOR", ":"),
            ],
        )?;
        if replayed {
            return Ok(Replayed::Done);
        }

        let said = without_progress(&said);
        let said = said.as_str();
        let open = operation::in_progress(git_dir(self.repo_path).as_deref())
            == Some(OperationInProgress::Rebase);
        if !open {
            // Refused before it began: a `pre-rebase` hook, a lock, a base
            // that does not resolve.
            eprintln!("[history_rewrite] rebase refused: {said}");
            return Err(said.to_string());
        }
        let conflicts = ls_files_unmerged(self.repo_path);
        if !conflicts.is_empty() {
            eprintln!("[history_rewrite] rebase stopped on a conflict: {said}");
            return Ok(Replayed::Conflict(conflicts, said.to_string()));
        }

        eprintln!("[history_rewrite] rebase stopped with nothing to resolve: {said}");
        let aborted = run_git_combined(self.repo_path, &["rebase", "--abort"]);
        Err(match aborted {
            Ok((true, _)) => said.to_string(),
            refused => {
                let why = refused.map_or_else(|unrun| unrun, |(_, said)| said);
                format!(
                    "{said}\n\nThe rebase could not be aborted either, so it is still open:\n{}",
                    why.trim()
                )
            }
        })
    }
}
