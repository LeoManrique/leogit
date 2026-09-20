//! Throwaway repositories for the git-driving modules' tests.

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

/// Keep a git child away from the developer's own configuration, so a test
/// reads the same on every machine. Applied to core's `git_cmd` under test as
/// well as to the helpers here.
pub(crate) fn isolate_from_user_config(cmd: &mut Command) {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("LC_ALL", "C");
}

/// `git <args>` in `dir`, isolated from the developer's configuration.
fn git_in(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir).args(args);
    isolate_from_user_config(&mut cmd);
    cmd
}

/// Run git in `dir` and insist it succeeds — for arranging a test, where a
/// failure is a broken test rather than a result to assert on.
pub(crate) fn git(dir: &Path, args: &[&str]) {
    let ok = git_in(dir, args).status().expect("spawn git").success();
    assert!(ok, "git {args:?} failed");
}

/// Git's trimmed stdout, for reading a fact back out of a test repository.
pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = git_in(dir, args).output().expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Run a git command that is *expected* to stop on a conflict.
pub(crate) fn git_stopping(dir: &Path, args: &[&str]) {
    let status = git_in(dir, args)
        .env("GIT_EDITOR", ":")
        .output()
        .expect("spawn git")
        .status;
    assert!(!status.success(), "git {args:?} was meant to conflict");
}

/// Initialise a throwaway repo with a committer identity. Local config
/// disables commit signing so the tests don't depend on the developer's
/// global git setup.
pub(crate) fn init_test_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    set_identity(dir);
}

/// The committer identity every test repository commits under, with signing
/// off, written to the repository's own config.
fn set_identity(dir: &Path) {
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test User"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

/// Write `content` to `name` and commit just that file.
pub(crate) fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).expect("write");
    git(dir, &["add", "--", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// The subjects on the current branch, newest first.
pub(crate) fn subjects(dir: &Path) -> Vec<String> {
    git_stdout(dir, &["log", "--format=%s"])
        .lines()
        .map(str::to_string)
        .collect()
}

/// A repository whose `main` and `side` both rewrote `shared.txt` from the
/// same base — so merging, rebasing or picking across them conflicts — with
/// `main` checked out. `side` carries a second, independent commit so a
/// sequence has somewhere to go after its conflict.
pub(crate) fn conflicting_repo() -> (TempDir, String) {
    let tmp = tempdir().expect("tempdir");
    let dir = tmp.path();
    init_test_repo(dir);
    git(dir, &["checkout", "-q", "-b", "main"]);
    commit_file(dir, "shared.txt", "base\n", "base");
    git(dir, &["checkout", "-q", "-b", "side"]);
    commit_file(dir, "shared.txt", "side\n", "side edits shared");
    commit_file(dir, "side-only.txt", "s\n", "side adds a file");
    git(dir, &["checkout", "-q", "main"]);
    commit_file(dir, "shared.txt", "main\n", "main edits shared");
    let repo_path = dir.to_str().expect("utf-8 path").to_string();
    (tmp, repo_path)
}

/// A bare `origin.git` and a clone of it, `mine`, side by side in one
/// directory: `main` holds two commits ("first", "second"), both pushed, and
/// tracks `origin/main`. Returns the directory and the path of `mine`;
/// [`clone_of`] adds somebody else's clone beside it.
pub(crate) fn published_repo() -> (TempDir, PathBuf) {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    // Initialised and then pointed at the remote, rather than cloned from it:
    // cloning an empty repository warns on every test's stderr.
    let mine = root.join("mine");
    std::fs::create_dir(&mine).expect("mkdir");
    init_test_repo(&mine);
    git(&mine, &["checkout", "-q", "-b", "main"]);
    git(&mine, &["remote", "add", "origin", "../origin.git"]);
    commit_file(&mine, "first.txt", "first\n", "first");
    commit_file(&mine, "second.txt", "second\n", "second");
    git(&mine, &["push", "-q", "--set-upstream", "origin", "main"]);
    (tmp, mine)
}

/// Another clone of [`published_repo`]'s `origin.git`, named `name`, with the
/// same committer identity every test repository gets.
pub(crate) fn clone_of(root: &Path, name: &str) -> PathBuf {
    git(root, &["clone", "-q", "origin.git", name]);
    let clone = root.join(name);
    set_identity(&clone);
    clone
}
