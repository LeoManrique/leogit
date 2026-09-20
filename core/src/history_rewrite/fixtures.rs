//! Repositories and probes the History actions' tests share.

use crate::test_support::{commit_file, git, git_stdout, init_test_repo};
use std::fs;
use std::path::Path;
use tempfile::{TempDir, tempdir};

/// `main` with three commits and an untouched `target` branched off the
/// first, `main` checked out.
pub(super) fn linear_repo() -> (TempDir, String) {
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

/// `main` with five commits, `one` … `five`, each adding its own file.
pub(super) fn five_commits() -> (TempDir, String) {
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

/// `main`: `base`, then `shared.txt` edited by `first edit`, an unrelated
/// `between`, and `second edit` of the same line.
pub(super) fn edits_of_one_line() -> (TempDir, String) {
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

pub(super) fn sha(dir: &Path, rev: &str) -> String {
    git_stdout(dir, &["rev-parse", rev])
}

pub(super) fn branch(dir: &Path) -> String {
    git_stdout(dir, &["symbolic-ref", "--short", "HEAD"])
}

/// Install an executable hook that prints `says` and fails.
#[cfg(unix)]
pub(super) fn failing_hook(dir: &Path, name: &str, says: &str) {
    use std::os::unix::fs::PermissionsExt;
    let hooks = dir.join(".git/hooks");
    fs::create_dir_all(&hooks).expect("hooks dir");
    let hook = hooks.join(name);
    fs::write(&hook, format!("#!/bin/sh\necho {says} >&2\nexit 1\n")).expect("hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod");
}
