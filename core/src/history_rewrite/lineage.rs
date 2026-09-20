//! Where commits sit on the current branch — what squash and reorder both read
//! before they write a todo.

use std::collections::HashSet;

use super::merge_refusal;
use crate::git::{is_object_id, run_git, run_git_optional};

const NOT_ON_THE_BRANCH: &str = "The selected commits are not all on the current branch.";

/// Commits of the current branch, placed on it.
pub(super) struct Lineage {
    /// Every commit from the oldest placed one up to `HEAD`, oldest first.
    pub range: Vec<String>,
    /// The selected commits, oldest first.
    pub selected: Vec<String>,
}

impl Lineage {
    /// Place `shas` — in any order — on the current branch, together with
    /// `landmark`: a commit the range has to reach that is not one of the
    /// selected ones, which is a reorder's destination.
    ///
    /// The oldest is the one commit all the others descend from, which on one
    /// line of history is their octopus merge base; the order of the rest is
    /// read off the walk from `HEAD` down to it, never off the order they
    /// arrived in.
    ///
    /// The inner `Err` is a refusal — a merge commit in that stretch, which is
    /// then not one line of history to place anything on — and is data, as it
    /// is for the preflight.
    ///
    /// # Errors
    /// When no commit is named, an id is not an object id, or the commits are
    /// not all on the current branch.
    pub(super) fn of(
        repo_path: &str,
        shas: &[String],
        landmark: Option<&str>,
    ) -> Result<Result<Self, String>, String> {
        if let Some(odd) = shas
            .iter()
            .map(String::as_str)
            .chain(landmark)
            .find(|sha| !is_object_id(sha))
        {
            return Err(format!("Not a commit id: {odd}"));
        }
        // git prints ids in lowercase, and takes them in either case.
        let shas: Vec<String> = shas.iter().map(|sha| sha.to_ascii_lowercase()).collect();
        let landmark = landmark.map(str::to_ascii_lowercase);
        let wanted: HashSet<&str> = shas.iter().map(String::as_str).collect();
        if wanted.is_empty() {
            return Err("No commits are selected.".to_string());
        }

        let mut args = vec!["merge-base", "--octopus"];
        args.extend(wanted.iter().copied());
        args.extend(landmark.as_deref());
        // An id that names a tree, or no object at all, fails here with git's
        // own words for it; so do commits that share no ancestor.
        let oldest = run_git(repo_path, &args).map_err(|said| {
            eprintln!("[history_rewrite] could not place the commits: {said}");
            NOT_ON_THE_BRANCH.to_string()
        })?;
        // On one line of history the base of the commits is one of them. When
        // it is a commit nobody named they sit on lines that forked, and when
        // `HEAD` does not reach it they are somewhere else altogether — and
        // the merge question below, asked from `HEAD` down, would answer about
        // a stretch nobody chose.
        let named = wanted.contains(oldest.as_str()) || landmark.as_deref() == Some(&oldest);
        let reached =
            run_git_optional(repo_path, &["merge-base", "--is-ancestor", &oldest, "HEAD"])?
                .is_some();
        if !named || !reached {
            return Err(NOT_ON_THE_BRANCH.to_string());
        }
        if let Some(merge) = merge_refusal(repo_path, &oldest)? {
            return Ok(Err(merge));
        }
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
        // These fail together when a commit is not on this branch: the merge
        // base is then a commit nobody named, or the walk misses one.
        let landmark_placed = landmark.is_none_or(|landmark| range.contains(&landmark));
        if selected.len() != wanted.len() || !landmark_placed || range.first() != Some(&oldest) {
            return Err(NOT_ON_THE_BRANCH.to_string());
        }
        Ok(Ok(Self { range, selected }))
    }
}
