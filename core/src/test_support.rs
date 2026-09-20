//! Throwaway repositories for the git-driving modules' tests.

use std::path::Path;
use std::process::Command;

/// Run git in `dir` and insist it succeeds — for arranging a test, where a
/// failure is a broken test rather than a result to assert on.
pub(crate) fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("spawn git")
        .success();
    assert!(ok, "git {args:?} failed");
}

/// Git's trimmed stdout, for reading a fact back out of a test repository.
pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Initialise a throwaway repo with a committer identity. Local config
/// disables commit signing so the tests don't depend on the developer's
/// global git setup.
pub(crate) fn init_test_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test User"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}
