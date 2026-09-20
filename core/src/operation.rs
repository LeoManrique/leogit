//! The multi-step git operation a repository is stopped in the middle of — a
//! merge, a rebase, a cherry-pick or a revert — and the two ways out of it:
//! carry on, or abort.
//!
//! Which operation is open is read from the files git itself keeps in the git
//! dir, so it costs no subprocess on the status poll and it is true of an
//! operation begun in a terminal as much as one begun here. That second half is
//! the point: the app must never show a repository it cannot get out of.

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::git::{
    git_add, git_dir, ls_files_unmerged, run_git, run_git_combined, run_git_combined_with_env,
    unquote_path,
};

/// A multi-step operation git is stopped in the middle of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationInProgress {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl OperationInProgress {
    /// The git subcommand that continues, skips and aborts this operation —
    /// which is also the plain word for it in a sentence.
    pub(crate) fn subcommand(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }

    /// The file that exists while one commit of a sequence is waiting to be
    /// committed — stopped on a conflict, or applied with `--no-commit` — as
    /// opposed to the sequence merely being open. `git revert -n` leaves its
    /// file behind even when it applies cleanly, which `git cherry-pick -n`
    /// does not: git itself calls that state "currently reverting", and
    /// continuing it commits what was staged.
    fn stopped_commit_file(self) -> Option<&'static str> {
        match self {
            Self::CherryPick => Some("CHERRY_PICK_HEAD"),
            Self::Revert => Some("REVERT_HEAD"),
            Self::Merge | Self::Rebase => None,
        }
    }
}

/// What continuing an operation came to. Shaped like
/// [`MergeResult`](super::git::MergeResult): stopping on a further conflict is
/// an ordinary outcome of replaying commits, so it is data — `success` false,
/// git's own text verbatim, and the conflicted paths — and only "git could not
/// be asked" or "this was refused before git was asked" is an `Err`, which
/// leaves the index exactly as it was.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationOutcome {
    pub success: bool,
    pub conflicts: Vec<String>,
    pub error_message: Option<String>,
    /// The stopped commit was dropped rather than committed, because its
    /// resolution left nothing to commit. Git says nothing when it skips — a
    /// rebase does so by itself, without being asked — so without this a commit
    /// that never landed reads as one that did.
    pub skipped: bool,
}

/// Which operation `git_dir` records as open, if any. `None` for the git dir
/// itself (no repository could be resolved) reads as "nothing in progress", so
/// a non-repo path degrades to a calm answer rather than an error.
///
/// The order is load-bearing, and each rung was checked against git:
///
/// * **Rebase first.** A rebase that stops on a `merge` line of its todo
///   (`--rebase-merges`) leaves `MERGE_HEAD` beside `rebase-merge/`, and
///   `git merge --abort` there clears the one and strands the other.
/// * **`rebase-apply/applying` is `git am`, not a rebase.** Every `git rebase`
///   command refuses there, so it is reported as nothing rather than as a
///   rebase whose buttons could only fail.
/// * **The sequencer's todo says which sequence it is.** Cherry-pick and
///   revert share `sequencer/`, and git does not keep them apart for us:
///   `cherry-pick --abort` silently aborts a revert. The todo outlives
///   `CHERRY_PICK_HEAD` — after a conflicted pick is committed by hand the
///   head file is gone while the sequence is still open — so it is asked
///   before the head files, which only a single-commit pick or revert needs.
/// * **Merge last**, being the only state none of the others can also be.
#[must_use]
pub fn in_progress(git_dir: Option<&Path>) -> Option<OperationInProgress> {
    let dir = git_dir?;
    if dir.join("rebase-merge").is_dir() {
        return Some(OperationInProgress::Rebase);
    }
    let apply = dir.join("rebase-apply");
    if apply.is_dir() {
        return (!apply.join("applying").exists()).then_some(OperationInProgress::Rebase);
    }
    if let Ok(todo) = std::fs::read_to_string(dir.join("sequencer").join("todo")) {
        let first_command = todo
            .lines()
            .map(str::trim_start)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .and_then(|line| line.split_whitespace().next());
        return Some(match first_command {
            Some("revert") => OperationInProgress::Revert,
            _ => OperationInProgress::CherryPick,
        });
    }
    if dir.join("REVERT_HEAD").exists() {
        return Some(OperationInProgress::Revert);
    }
    if dir.join("CHERRY_PICK_HEAD").exists() {
        return Some(OperationInProgress::CherryPick);
    }
    dir.join("MERGE_HEAD")
        .exists()
        .then_some(OperationInProgress::Merge)
}

const NOTHING_IN_PROGRESS: &str = "No merge, rebase, cherry-pick or revert is in progress.";

/// How many conflicted paths one `git diff --check` is asked about.
const MARKER_CHECK_BATCH: usize = 500;

/// Stage the resolved conflicts and carry the operation in progress on.
///
/// Staging is limited to the paths git reported as unmerged. Deliberately not
/// `git add -u`: a file edited for some other reason while the operation was
/// paused would be swept into the replayed commit. The replayed commit's own
/// cleanly-merged changes are already in the index, which is why this never
/// resets it either.
///
/// A pick or revert whose resolution left nothing to commit is skipped rather
/// than continued. `--continue` refuses there and asks for a decision; a rebase
/// in the same position drops the commit without asking or saying so, and taking
/// the target's side of every conflict is the user having already said the
/// commit adds nothing. `--empty=keep` does not cover it — that is about commits
/// that were empty to begin with, or that a new order made empty. Either way the
/// outcome says `skipped`: after a reorder the commit that went may be the very
/// one the user was moving.
///
/// # Errors
/// When nothing is in progress; when a conflicted file still holds conflict
/// markers; when a rebase is asked to continue over unstaged edits (git refuses
/// that with a message naming the wrong cause); or when git cannot be run.
pub fn continue_operation(repo_path: &str) -> Result<OperationOutcome, String> {
    let dir = git_dir(repo_path);
    let operation = in_progress(dir.as_deref()).ok_or(NOTHING_IN_PROGRESS)?;

    // Every refusal comes before the first write, so an `Err` leaves the index
    // as it found it and a retry starts from the same place.
    let unmerged = ls_files_unmerged(repo_path);
    refuse_leftover_markers(repo_path, &unmerged)?;
    if operation == OperationInProgress::Rebase {
        refuse_unstaged_changes(repo_path, &unmerged)?;
    }
    git_add(repo_path, &unmerged)?;

    let skipped = resolved_to_nothing(repo_path, dir.as_deref(), operation);
    // A rebase drops the commit by itself on `--continue`.
    let skips_itself = operation == OperationInProgress::Rebase;
    let step = if skipped && !skips_itself {
        "--skip"
    } else {
        "--continue"
    };
    let subcommand = operation.subcommand();
    eprintln!(
        "[operation] git {subcommand} {step} ({} resolved)",
        unmerged.len()
    );
    // `:` is the editor that accepts whatever message it is handed — the
    // replayed commit's own — so nothing waits on a terminal that isn't there.
    let (ok, said) =
        run_git_combined_with_env(repo_path, &[subcommand, step], &[("GIT_EDITOR", ":")])?;
    if ok {
        return Ok(OperationOutcome {
            success: true,
            conflicts: Vec::new(),
            error_message: None,
            skipped,
        });
    }
    let said = without_progress(&said);
    eprintln!("[operation] git {subcommand} {step} stopped: {said}");
    Ok(OperationOutcome {
        success: false,
        conflicts: ls_files_unmerged(repo_path),
        error_message: Some(said),
        skipped,
    })
}

/// What git said, without its progress meter. A rebase writes
/// `Rebasing (2/5)` over itself with carriage returns even when nothing is
/// watching, so a line of its output arrives with every step it took glued to
/// the front; what follows the last carriage return is the line as a terminal
/// would have left it.
pub(crate) fn without_progress(said: &str) -> String {
    said.trim()
        .lines()
        .map(|line| line.rsplit('\r').next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Abort the operation in progress.
///
/// Rewinds exactly as far as git does. A multi-commit pick normally rolls back
/// to the tip it started from — unless HEAD was moved by hand in between, when
/// git clears the sequence, keeps HEAD and says so (`You seem to have moved
/// HEAD. Not rewinding`). That text comes back as `Some`: the abort worked, and
/// the user is not where they started, which is theirs to read. This never
/// resets over commits the user made themselves.
///
/// # Errors
/// When nothing is in progress, or git refuses the abort.
pub fn abort_operation(repo_path: &str) -> Result<Option<String>, String> {
    let operation = in_progress(git_dir(repo_path).as_deref()).ok_or(NOTHING_IN_PROGRESS)?;
    let subcommand = operation.subcommand();
    let (ok, said) = run_git_combined(repo_path, &[subcommand, "--abort"])?;
    let said = said.trim();
    if !ok {
        eprintln!("[operation] git {subcommand} --abort failed: {said}");
        return Err(format!("git {subcommand} --abort failed: {said}"));
    }
    eprintln!("[operation] git {subcommand} --abort");
    Ok((!said.is_empty()).then(|| said.to_string()))
}

/// Refuse while an unmerged file still holds a conflict's opening or closing
/// marker, naming the files.
///
/// `git diff --check` finds them, asked about the unmerged paths only and read
/// line by line rather than by exit status: run bare it also reports trailing
/// whitespace, in any dirty file. It looks at added lines only, so a fixture
/// that legitimately contains marker text is not mistaken for a conflict.
///
/// A flagged line that is a bare `=======` does not count by itself. It is
/// also a Markdown heading underline, and a refusal nobody can get past is a
/// worse failure than one stray separator reaching a commit.
fn refuse_leftover_markers(repo_path: &str, unmerged: &[String]) -> Result<(), String> {
    if unmerged.is_empty() {
        return Ok(());
    }
    let mut unresolved: Vec<String> = Vec::new();
    // In batches, because the paths travel as arguments — `git diff` reads no
    // pathspec from stdin — and a large conflicted merge must not outgrow the
    // argument limit.
    for batch in unmerged.chunks(MARKER_CHECK_BATCH) {
        let mut args = vec!["--literal-pathspecs", "diff", "--check", "--"];
        args.extend(batch.iter().map(String::as_str));
        // The C locale keeps git's own `fatal:` prefix readable below.
        let (clean, report) = run_git_combined_with_env(repo_path, &args, &[("LC_ALL", "C")])?;
        // `--check` exits non-zero for its findings too, so a failure is told
        // by what it printed. A check that could not run must not read as
        // "no markers": the doubt goes to not committing a conflict.
        if !clean && report.lines().any(|line| line.starts_with("fatal:")) {
            return Err(format!(
                "Could not check the resolved files for conflict markers: {}",
                report.trim()
            ));
        }
        for line in report.lines() {
            let Some(location) = line.strip_suffix(": leftover conflict marker") else {
                continue;
            };
            // `--check` prints paths raw, never quoted, so a reported line is
            // matched against the paths already in hand rather than parsed:
            // that is exact whatever the name contains, colons included.
            let flagged = batch.iter().find_map(|path| {
                let line_number = location.strip_prefix(path.as_str())?.strip_prefix(':')?;
                Some((path, line_number.parse::<usize>().ok()?))
            });
            // A flagged line no held path accounts for is named as git wrote
            // it: the doubt goes the same way.
            let (name, is_marker) = match flagged {
                Some((path, line_number)) => (
                    path.as_str(),
                    opens_or_closes_a_conflict(repo_path, path, line_number),
                ),
                None => (location, true),
            };
            if is_marker && !unresolved.iter().any(|known| known == name) {
                unresolved.push(name.to_string());
            }
        }
    }
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Conflict markers are still in:\n{}\n\nResolve them, then continue.",
        unresolved.join("\n")
    ))
}

/// Whether line `line_number` (1-based) of a worktree file is a `<<<<<<<` or
/// `>>>>>>>` marker. A line that cannot be read counts as one: git flagged it,
/// and the doubt goes to not committing a conflict.
fn opens_or_closes_a_conflict(repo_path: &str, path: &str, line_number: usize) -> bool {
    let Ok(bytes) = std::fs::read(Path::new(repo_path).join(path)) else {
        return true;
    };
    let text = String::from_utf8_lossy(&bytes);
    let Some(line) = line_number.checked_sub(1).and_then(|i| text.lines().nth(i)) else {
        return true;
    };
    line.starts_with("<<<<<<<") || line.starts_with(">>>>>>>")
}

/// Refuse to continue a rebase over unstaged edits to tracked files, naming
/// them. Git refuses this itself — a merge or a pick does not mind — but with
/// "You must edit all merge conflicts and then mark them as resolved", which
/// sends the user looking for conflicts that are no longer there. Submodules
/// are ignored because git's own check ignores them, and untracked files
/// because it allows them.
///
/// Asked before the resolutions are staged, so the `unmerged` paths — which
/// `git diff` lists too, once per conflicted stage — are left out: staging is
/// what is about to happen to them.
fn refuse_unstaged_changes(repo_path: &str, unmerged: &[String]) -> Result<(), String> {
    let changed = run_git(repo_path, &["diff", "--name-only", "--ignore-submodules"])?;
    let mut names: Vec<String> = Vec::new();
    for name in changed.lines().map(unquote_path) {
        if !unmerged.contains(&name) && !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        return Ok(());
    }
    Err(format!(
        "A rebase cannot continue while other files have unstaged changes:\n{}\n\nCommit them \
         after the rebase, or discard them, then continue.",
        names.join("\n")
    ))
}

/// Whether the operation is stopped on a commit whose resolution left the index
/// identical to `HEAD`. Asked of the index rather than read out of git's
/// refusal, which would mean matching translated prose.
///
/// A merge concludes with a commit whatever it holds.
fn resolved_to_nothing(
    repo_path: &str,
    git_dir: Option<&Path>,
    operation: OperationInProgress,
) -> bool {
    let Some(dir) = git_dir else {
        return false;
    };
    let a_commit_is_waiting = match operation.stopped_commit_file() {
        Some(file) => dir.join(file).exists(),
        None => operation == OperationInProgress::Rebase && rebase_stopped_on_a_pick(dir),
    };
    a_commit_is_waiting
        && run_git_combined(repo_path, &["diff", "--cached", "--quiet"])
            .is_ok_and(|(nothing_staged, _)| nothing_staged)
}

/// Whether a rebase is stopped on a `pick` that has yet to become a commit —
/// the one stop where a resolution that comes to nothing makes git drop the
/// commit. Two of the rebase's own files say so between them: `stopped-sha`
/// names the commit a stop is about and is gone once that commit is made, which
/// leaves out a `break` and a failed `exec`; `amend` is there when continuing
/// would amend `HEAD`, which leaves out an `edit` stop and a conflicted `fixup`
/// or `squash` — those fold into a commit that already exists, and lose
/// nothing. Whether this continue staged the conflict is no guide: a resolution
/// staged from a terminal leaves nothing unmerged, and the commit goes all the
/// same.
fn rebase_stopped_on_a_pick(git_dir: &Path) -> bool {
    let rebase = git_dir.join("rebase-merge");
    rebase.join("stopped-sha").exists() && !rebase.join("amend").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::get_status;
    use crate::test_support::{
        commit_file, conflicting_repo, git, git_stdout, git_stopping, init_test_repo, subjects,
    };
    use std::fs;
    use tempfile::tempdir;

    fn operation_of(repo_path: &str) -> Option<OperationInProgress> {
        get_status(repo_path.to_string()).expect("status").operation
    }

    #[test]
    fn status_reports_a_merge_in_progress_and_its_end() {
        let (tmp, repo) = conflicting_repo();
        assert_eq!(operation_of(&repo), None, "a clean branch is at rest");

        git_stopping(tmp.path(), &["merge", "side"]);
        assert_eq!(operation_of(&repo), Some(OperationInProgress::Merge));

        assert_eq!(abort_operation(&repo).expect("abort"), None);
        assert_eq!(operation_of(&repo), None, "aborting ends the merge");
        assert_eq!(
            fs::read_to_string(tmp.path().join("shared.txt")).expect("read"),
            "main\n",
            "and restores the tree"
        );
    }

    #[test]
    fn status_reports_a_rebase_and_a_cherry_pick_in_progress_and_their_end() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        let tip = git_stdout(dir, &["rev-parse", "HEAD"]);

        git_stopping(dir, &["rebase", "side"]);
        assert_eq!(operation_of(&repo), Some(OperationInProgress::Rebase));
        abort_operation(&repo).expect("abort rebase");
        assert_eq!(operation_of(&repo), None);
        assert_eq!(
            git_stdout(dir, &["symbolic-ref", "--short", "HEAD"]),
            "main",
            "aborting a rebase puts the branch back under HEAD"
        );

        git_stopping(dir, &["cherry-pick", "side~1", "side"]);
        assert_eq!(operation_of(&repo), Some(OperationInProgress::CherryPick));
        abort_operation(&repo).expect("abort pick");
        assert_eq!(operation_of(&repo), None);
        assert_eq!(git_stdout(dir, &["rev-parse", "HEAD"]), tip);
    }

    /// Revert shares the sequencer with cherry-pick, and git will happily let
    /// `cherry-pick --abort` end one. Both the single-commit form (a head file
    /// and no sequencer) and the sequence (a todo of `revert` lines) have to
    /// read as a revert.
    #[test]
    fn a_revert_is_not_mistaken_for_a_cherry_pick() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        // Reverting `base` conflicts: `main` has rewritten the same line since.
        let base = git_stdout(dir, &["rev-parse", "HEAD~1"]);

        git_stopping(dir, &["revert", "--no-edit", &base]);
        assert!(
            !dir.join(".git/sequencer").exists(),
            "one commit: no sequencer"
        );
        assert_eq!(operation_of(&repo), Some(OperationInProgress::Revert));
        abort_operation(&repo).expect("abort single revert");
        assert_eq!(operation_of(&repo), None);

        commit_file(dir, "later.txt", "l\n", "later");
        git_stopping(dir, &["revert", "--no-edit", "HEAD", &base]);
        assert!(dir.join(".git/sequencer/todo").exists(), "a sequence");
        assert_eq!(operation_of(&repo), Some(OperationInProgress::Revert));
        abort_operation(&repo).expect("abort revert sequence");
        assert_eq!(operation_of(&repo), None);
    }

    #[test]
    fn a_hand_committed_pick_still_reads_as_a_cherry_pick_in_progress() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["cherry-pick", "side~1", "side"]);

        fs::write(dir.join("shared.txt"), "both\n").expect("resolve");
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "--no-edit"]);
        assert!(!dir.join(".git/CHERRY_PICK_HEAD").exists());
        assert_eq!(operation_of(&repo), Some(OperationInProgress::CherryPick));

        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
        assert_eq!(operation_of(&repo), None);
        assert_eq!(subjects(dir)[0], "side adds a file", "the rest was picked");
    }

    #[test]
    fn abort_after_a_hand_made_commit_clears_the_sequence_and_reports_gits_warning() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["cherry-pick", "side~1", "side"]);
        fs::write(dir.join("shared.txt"), "both\n").expect("resolve");
        git(dir, &["add", "shared.txt"]);
        git(dir, &["commit", "-q", "--no-edit"]);
        let hand_made = git_stdout(dir, &["rev-parse", "HEAD"]);

        let said = abort_operation(&repo)
            .expect("the abort itself works")
            .expect("and git says it did not rewind");
        assert!(said.contains("moved HEAD"), "git's own words: {said}");
        assert_eq!(operation_of(&repo), None);
        assert_eq!(
            git_stdout(dir, &["rev-parse", "HEAD"]),
            hand_made,
            "the commit the user made is not reset over"
        );
    }

    #[test]
    fn continue_refuses_leftover_conflict_markers() {
        let (tmp, repo) = conflicting_repo();
        git_stopping(tmp.path(), &["rebase", "side"]);

        let refusal = continue_operation(&repo).expect_err("markers are still there");
        assert!(refusal.contains("shared.txt"), "names the file: {refusal}");
        assert_eq!(
            operation_of(&repo),
            Some(OperationInProgress::Rebase),
            "and nothing moved"
        );
        assert!(
            !ls_files_unmerged(&repo).is_empty(),
            "the unresolved file was not staged"
        );
    }

    /// `git diff --check` prints a path raw where `ls-files` quotes it, and a
    /// name may hold the very colon the report is delimited by.
    #[test]
    fn continue_names_a_conflicted_file_whose_name_git_would_quote() {
        let name = "Capítulo \"x\": 2.md";
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, name, "base\n", "base");
        git(dir, &["checkout", "-q", "-b", "side"]);
        commit_file(dir, name, "side\n", "side edits it");
        git(dir, &["checkout", "-q", "main"]);
        commit_file(dir, name, "main\n", "main edits it");
        let repo = dir.to_str().expect("utf-8 path").to_string();
        git_stopping(dir, &["merge", "side"]);

        let refusal = continue_operation(&repo).expect_err("markers are still there");
        assert!(refusal.contains(name), "names the file as it is: {refusal}");

        fs::write(dir.join(name), "both\n").expect("resolve");
        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(!outcome.skipped);
        assert_eq!(git_stdout(dir, &["show", &format!("HEAD:{name}")]), "both");
    }

    /// A kept `=======` is a Markdown heading underline as well as a
    /// separator; on its own it must not make Continue impossible.
    #[test]
    fn continue_accepts_a_resolution_that_keeps_a_heading_underline() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["cherry-pick", "side~1"]);
        fs::write(dir.join("shared.txt"), "Heading\n=======\n").expect("resolve");

        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
    }

    #[test]
    fn continue_stages_the_resolution_and_finishes_a_rebase() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["rebase", "side"]);
        fs::write(dir.join("shared.txt"), "both\n").expect("resolve");

        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(outcome.conflicts.is_empty());
        assert_eq!(operation_of(&repo), None);
        assert_eq!(
            subjects(dir),
            [
                "main edits shared",
                "side adds a file",
                "side edits shared",
                "base"
            ],
            "the replayed commit keeps its own message"
        );
        assert_eq!(git_stdout(dir, &["show", "HEAD:shared.txt"]), "both");
    }

    #[test]
    fn continue_leaves_an_unrelated_edit_out_of_the_replayed_commit() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        commit_file(dir, "other.txt", "o\n", "main adds other");
        git_stopping(dir, &["cherry-pick", "side~1"]);
        fs::write(dir.join("shared.txt"), "both\n").expect("resolve");
        fs::write(dir.join("other.txt"), "edited while paused\n").expect("unrelated edit");

        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
        assert_eq!(
            git_stdout(dir, &["show", "--format=", "--name-only", "HEAD"]),
            "shared.txt",
            "only the resolved file is in the picked commit"
        );
        assert_eq!(
            git_stdout(dir, &["status", "--porcelain"]),
            "M other.txt",
            "the other edit is still just an edit"
        );
    }

    /// Git refuses this too, but by claiming there are unresolved conflicts.
    #[test]
    fn continuing_a_rebase_over_an_unrelated_edit_names_the_real_cause() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["rebase", "side"]);
        fs::write(dir.join("shared.txt"), "both\n").expect("resolve");
        // Tracked at the stop: the rebase is standing on `side`'s tip.
        fs::write(dir.join("side-only.txt"), "edited while paused\n").expect("unrelated edit");

        let refusal = continue_operation(&repo).expect_err("unstaged edit");
        assert!(
            refusal.contains("side-only.txt"),
            "names the file: {refusal}"
        );
        assert!(
            !refusal.contains("shared.txt"),
            "and only that one: {refusal}"
        );
        assert_eq!(operation_of(&repo), Some(OperationInProgress::Rebase));
        assert_eq!(
            ls_files_unmerged(&repo),
            ["shared.txt"],
            "a refusal stages nothing"
        );
    }

    #[test]
    fn continue_stages_a_modify_delete_resolution() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        init_test_repo(dir);
        git(dir, &["checkout", "-q", "-b", "main"]);
        commit_file(dir, "doomed.txt", "base\n", "base");
        commit_file(dir, "kept.txt", "base\n", "base two");
        git(dir, &["checkout", "-q", "-b", "side"]);
        commit_file(dir, "doomed.txt", "side\n", "side edits doomed");
        commit_file(dir, "kept.txt", "side\n", "side edits kept");
        git(dir, &["checkout", "-q", "main"]);
        git(dir, &["rm", "-q", "doomed.txt", "kept.txt"]);
        git(dir, &["commit", "-q", "-m", "main deletes both"]);
        let repo = dir.to_str().expect("utf-8 path").to_string();

        // Deleted here, modified there — resolved one way each.
        git_stopping(dir, &["cherry-pick", "side~1"]);
        fs::remove_file(dir.join("doomed.txt")).expect("resolve by deleting");
        let first = continue_operation(&repo).expect("continue");
        assert!(first.success, "{:?}", first.error_message);
        assert_eq!(operation_of(&repo), None);
        assert!(!dir.join("doomed.txt").exists());

        git_stopping(dir, &["cherry-pick", "side"]);
        let second = continue_operation(&repo).expect("continue");
        assert!(second.success, "{:?}", second.error_message);
        assert_eq!(git_stdout(dir, &["show", "HEAD:kept.txt"]), "side");
    }

    #[test]
    fn a_continue_that_conflicts_again_is_data() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git(dir, &["checkout", "-q", "side"]);
        commit_file(dir, "shared.txt", "side again\n", "side edits shared again");
        git(dir, &["checkout", "-q", "main"]);

        git_stopping(dir, &["cherry-pick", "side~2", "side"]);
        fs::write(dir.join("shared.txt"), "first resolution\n").expect("resolve");

        let outcome = continue_operation(&repo).expect("continue runs");
        assert!(!outcome.success, "the second pick conflicts as well");
        assert_eq!(outcome.conflicts, ["shared.txt"]);
        assert!(outcome.error_message.is_some());
        assert_eq!(operation_of(&repo), Some(OperationInProgress::CherryPick));
        assert_eq!(
            subjects(dir)[0],
            "side edits shared",
            "the first pick landed"
        );
    }

    /// `cherry-pick --continue` refuses a pick that resolved to nothing and
    /// asks the user to choose; a rebase drops it without asking. Both drop it
    /// here.
    #[test]
    fn a_pick_resolved_to_nothing_is_skipped_and_the_sequence_goes_on() {
        let (tmp, repo) = conflicting_repo();
        let dir = tmp.path();
        git_stopping(dir, &["cherry-pick", "side~1", "side"]);
        fs::write(dir.join("shared.txt"), "main\n").expect("take main's side");

        let outcome = continue_operation(&repo).expect("continue");
        assert!(outcome.success, "{:?}", outcome.error_message);
        assert!(outcome.skipped, "and says the commit was dropped");
        assert_eq!(operation_of(&repo), None);
        assert_eq!(
            subjects(dir),
            ["side adds a file", "main edits shared", "base"],
            "the emptied pick is gone and the one after it landed"
        );
    }

    #[test]
    fn an_am_in_progress_is_not_reported_as_a_rebase() {
        let tmp = tempdir().expect("tempdir");
        let apply = tmp.path().join("rebase-apply");
        fs::create_dir(&apply).expect("mkdir");
        fs::write(apply.join("applying"), "").expect("touch");
        assert_eq!(in_progress(Some(tmp.path())), None);

        fs::remove_file(apply.join("applying")).expect("rm");
        fs::write(apply.join("rebasing"), "").expect("touch");
        assert_eq!(
            in_progress(Some(tmp.path())),
            Some(OperationInProgress::Rebase)
        );
    }

    #[test]
    fn continue_and_abort_refuse_when_nothing_is_in_progress() {
        let (_tmp, repo) = conflicting_repo();
        assert_eq!(
            continue_operation(&repo).expect_err("nothing to continue"),
            NOTHING_IN_PROGRESS
        );
        assert_eq!(
            abort_operation(&repo).expect_err("nothing to abort"),
            NOTHING_IN_PROGRESS
        );
    }
}
