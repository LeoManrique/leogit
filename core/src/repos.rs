//! The repository *list*: which repos a client should offer, and how a typed
//! query narrows them.
//!
//! Both rules were written twice — once in TypeScript, once in Swift — and both
//! had drifted, which is the whole reason they live here now. Neither is about
//! git plumbing (that is [`super::git`]) nor about the settings file (that is
//! [`super::config`]); this module composes the two into what a picker shows.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Membership: discovery ∪ the existence-checked MRU
// ---------------------------------------------------------------------------

/// Merge discovered repos with the recently-opened list, keeping only entries
/// that still exist on disk.
///
/// Discovery alone forgets everything the user reached another way — a clone, a
/// CLI open, a folder picked outside the scan paths — every restart, even
/// though the MRU that remembers them is already on disk. The MRU alone goes
/// stale: a moved or deleted folder keeps a row (and, when the list also feeds
/// the background sweep, a time-boxed fetch per dead entry per tier interval).
/// Taken together they answer honestly, which is why neither client should
/// assemble this itself.
///
/// Order is membership only — discovery order first, then the surviving recents
/// in MRU order. Ranking a picker's rows is a per-client presentation choice.
#[must_use]
pub fn union_known_repos(discovered: Vec<String>, recents: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = discovered.iter().map(String::as_str).collect();
    let mut merged = Vec::with_capacity(discovered.len() + recents.len());
    let mut extra: Vec<String> = Vec::new();
    for recent in recents {
        if seen.contains(recent.as_str()) {
            continue;
        }
        // A path that no longer exists is not a repo the user can open, and
        // leaving it in means a dead row and a doomed background fetch.
        if !Path::new(recent).is_dir() {
            continue;
        }
        seen.insert(recent.as_str());
        extra.push(recent.clone());
    }
    merged.extend(discovered);
    merged.extend(extra);
    merged
}

/// Every repo a client should list: discovery over `scan_paths` unioned with
/// the persisted MRU.
///
/// # Errors
/// When discovery fails. A state file that can't be read is *not* an error —
/// the discovered list is still a usable answer, so the MRU half degrades to
/// empty rather than blanking the picker.
pub fn known_repos(scan_paths: Vec<String>, max_depth: u32) -> Result<Vec<String>, String> {
    let discovered = super::git::discover_repos(scan_paths, max_depth)?;
    let recents = super::config::load_state()
        .ok()
        .and_then(|s| s.recent_repos)
        .unwrap_or_default();
    Ok(union_known_repos(discovered, &recents))
}

// ---------------------------------------------------------------------------
// Search: how a typed query narrows the list
// ---------------------------------------------------------------------------

/// How a query matched a row, strongest first. Declaration order *is* the
/// ranking — [`Ord`] is derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RepoMatch {
    ExactName,
    NamePrefix,
    NameSubstring,
    /// The query is the run of initials of the name's words (`lg` → `leo-git`).
    NameInitials,
    /// The query's characters appear in order, not necessarily together.
    NameSubsequence,
    /// Nothing in the name matched, but the path below its scan folder did.
    PathSubstring,
}

/// One row as the picker knows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRow {
    pub path: String,
    /// Every label the user might reasonably type for this row — a basename,
    /// and where it's known, the GitHub `owner/name`. Matching all of them is
    /// what the two implementations disagreed about: one searched a single
    /// basename while the *labels it displayed* were the owner-qualified ones,
    /// so typing what was on screen found nothing.
    pub names: Vec<String>,
}

/// Rank one row against `query`, or `None` when it doesn't match.
///
/// `scan_folders` are the roots a path is shown relative to; the home directory
/// is always treated as one, so a repo opened from outside every scan folder
/// still matches on the part of its path the user thinks of as its location
/// rather than on `/Users/<name>/`.
#[must_use]
pub fn match_repo(
    query: &str,
    path: &str,
    names: &[String],
    scan_folders: &[String],
) -> Option<RepoMatch> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }

    let best = names
        .iter()
        .filter_map(|name| match_name(&needle, &name.to_lowercase()))
        .min();
    if best.is_some() {
        return best;
    }

    searchable_path(path, scan_folders)
        .contains(&needle)
        .then_some(RepoMatch::PathSubstring)
}

fn match_name(needle: &str, name: &str) -> Option<RepoMatch> {
    if name == needle {
        return Some(RepoMatch::ExactName);
    }
    if name.starts_with(needle) {
        return Some(RepoMatch::NamePrefix);
    }
    if name.contains(needle) {
        return Some(RepoMatch::NameSubstring);
    }
    if initials(name).starts_with(needle) {
        return Some(RepoMatch::NameInitials);
    }
    if is_subsequence(needle, name) {
        return Some(RepoMatch::NameSubsequence);
    }
    None
}

/// First character of each alphanumeric run: `leo-git 2` → `lg2`.
fn initials(name: &str) -> String {
    name.split(|c: char| !c.is_alphanumeric())
        .filter_map(|word| word.chars().next())
        .collect()
}

/// The part of `path` worth searching: everything below the deepest root it
/// sits under, lowercased.
///
/// Both the roots and the path are normalized before they're compared — a
/// separator-flipped or differently-cased scan folder failed the raw prefix
/// test one implementation used, which silently made the whole absolute path
/// searchable and turned `users` into a query that matched every repo.
fn searchable_path(path: &str, scan_folders: &[String]) -> String {
    let target = normalize(path);
    let home = super::paths::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut cut = 0;
    for folder in scan_folders
        .iter()
        .map(String::as_str)
        .chain([home.as_str()])
    {
        if folder.is_empty() {
            continue;
        }
        let mut prefix = normalize(folder);
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        if prefix.len() > cut && target.starts_with(&prefix) {
            cut = prefix.len();
        }
    }
    target[cut..].to_string()
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut wanted = needle.chars().peekable();
    for c in haystack.chars() {
        if wanted.peek() == Some(&c) {
            wanted.next();
        }
    }
    wanted.peek().is_none()
}

/// Narrow and rank `rows` against `query`, returning the matching paths.
///
/// The batch form is what the hosts call: one crossing per keystroke rather
/// than one per row, which is what makes a shared rule affordable for a list
/// that re-filters as the user types. A blank query matches everything, in the
/// order given. The sort is stable, so rows of equal match quality keep the
/// caller's own ordering — a picker's MRU or active-first arrangement survives
/// filtering instead of being scrambled by it.
#[must_use]
pub fn filter_repos(query: &str, rows: &[RepoRow], scan_folders: &[String]) -> Vec<String> {
    if query.trim().is_empty() {
        return rows.iter().map(|r| r.path.clone()).collect();
    }
    let mut ranked: Vec<(RepoMatch, &str)> = rows
        .iter()
        .filter_map(|row| {
            match_repo(query, &row.path, &row.names, scan_folders).map(|m| (m, row.path.as_str()))
        })
        .collect();
    ranked.sort_by_key(|(m, _)| *m);
    ranked.into_iter().map(|(_, p)| p.to_string()).collect()
}

// ---------------------------------------------------------------------------
// Clone target derivation
// ---------------------------------------------------------------------------

/// Where a clone will land, and what it will be called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneTarget {
    /// The URL to hand `git clone`, with shorthand expanded.
    pub normalized_url: String,
    /// The folder name the clone creates.
    pub repo_name: String,
    /// The absolute path that folder lands at.
    pub target_path: String,
}

/// Join a destination folder and a repo name into the path a clone creates.
///
/// `None` when either half is blank — which is also the Clone button's enable
/// condition, so the button and the preview can't disagree. Trailing slashes on
/// `parent` collapse, except on a bare root (`/`), which *is* its slash.
#[must_use]
pub fn clone_target_path(parent: &str, repo_name: &str) -> Option<String> {
    let repo_name = repo_name.trim();
    let parent = parent.trim();
    if repo_name.is_empty() || parent.is_empty() {
        return None;
    }
    let trimmed = parent.trim_end_matches('/');
    let base = if trimmed.is_empty() { "/" } else { trimmed };
    Some(if base.ends_with('/') {
        format!("{base}{repo_name}")
    } else {
        format!("{base}/{repo_name}")
    })
}

/// Work out what `raw_url` names and where cloning it under `parent` would put
/// it.
///
/// `None` means the input isn't something we can clone — which is what the
/// Clone button gates on, so the app stops offering an action that is going to
/// fail. The rules, in order:
///
/// * surrounding whitespace goes (an untrimmed path created a literal `" repo"`
///   directory and persisted the poisoned destination for next time);
/// * trailing slashes go, so `owner/repo/` works;
/// * a trailing `.git` goes, case-insensitively, from the *name* — the URL
///   keeps whatever form the user typed unless it's shorthand;
/// * `owner/repo` expands to a github.com URL;
/// * a scheme-less `github.com/owner/repo` gets `https://`, rather than being
///   handed to git as a relative path.
#[must_use]
pub fn derive_clone_target(raw_url: &str, parent: &str) -> Option<CloneTarget> {
    let trimmed = raw_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let without_git = strip_dot_git(trimmed);
    let normalized_url = if is_owner_repo_shorthand(without_git) {
        format!("https://github.com/{without_git}")
    } else if let Some(rest) = host_relative(without_git) {
        format!("https://{rest}")
    } else {
        trimmed.to_string()
    };

    let repo_name = last_segment(without_git)?;
    let target_path = clone_target_path(parent, &repo_name)?;
    Some(CloneTarget {
        normalized_url,
        repo_name,
        target_path,
    })
}

/// Drop a trailing `.git`, case-insensitively.
///
/// `len - 4` is a *byte* offset, and this runs on a URL the user is still
/// typing: a multi-byte character ending the field puts that offset mid-character,
/// where slicing panics. `is_char_boundary` is the guard — an unpaired
/// continuation byte simply means the tail can't be `.git` anyway.
fn strip_dot_git(url: &str) -> &str {
    let Some(head_len) = url.len().checked_sub(4) else {
        return url;
    };
    if head_len > 0
        && url.is_char_boundary(head_len)
        && url[head_len..].eq_ignore_ascii_case(".git")
    {
        &url[..head_len]
    } else {
        url
    }
}

/// `owner/repo` and nothing else — no scheme, no host, exactly one separator.
fn is_owner_repo_shorthand(value: &str) -> bool {
    let mut parts = value.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !owner.is_empty()
        && !repo.is_empty()
        && [owner, repo].iter().all(|p| {
            p.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
        })
}

/// A scheme-less `host.tld/path` — pasted straight from a browser's address
/// bar, which drops the scheme when it displays a URL.
fn host_relative(value: &str) -> Option<&str> {
    if value.contains("://") || value.contains('@') {
        return None;
    }
    let host = value.split('/').next()?;
    (host.contains('.') && value.split('/').count() > 1).then_some(value)
}

/// The final path or SSH segment: what the clone's folder gets called.
fn last_segment(value: &str) -> Option<String> {
    let segment = value.rsplit(['/', ':']).next()?.trim();
    (!segment.is_empty()).then(|| segment.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(entries: &[(&str, &[&str])]) -> Vec<RepoRow> {
        entries
            .iter()
            .map(|(path, names)| RepoRow {
                path: (*path).to_string(),
                names: names.iter().map(|n| (*n).to_string()).collect(),
            })
            .collect()
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    // -----------------------------------------------------------------------
    // union_known_repos (H-6)
    // -----------------------------------------------------------------------

    /// A repo the user opened from outside every scan folder keeps its row —
    /// the MRU already remembers it, so forgetting it was throwing away state
    /// the app had already persisted.
    #[test]
    fn known_repos_keep_recents_discovery_cannot_see() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&outside).expect("create");
        let outside = outside.to_string_lossy().into_owned();

        let merged = union_known_repos(
            strings(&["/scan/a", "/scan/b"]),
            &[outside.clone(), "/scan/a".to_string()],
        );

        assert_eq!(
            merged,
            vec!["/scan/a".to_string(), "/scan/b".to_string(), outside],
            "discovery first, then the recents it missed, each listed once"
        );
    }

    /// A recent that no longer exists is not a repo anyone can open, and
    /// leaving it in also costs a doomed background fetch every tier interval.
    #[test]
    fn known_repos_drop_recents_that_no_longer_exist() {
        let merged =
            union_known_repos(strings(&["/scan/a"]), &["/definitely/not/here".to_string()]);
        assert_eq!(merged, vec!["/scan/a".to_string()]);
    }

    // -----------------------------------------------------------------------
    // match_repo / filter_repos (H-5)
    // -----------------------------------------------------------------------

    /// The tiers, strongest to weakest, over a single label.
    #[test]
    fn match_tiers_rank_from_exact_to_path() {
        let names = strings(&["leo-git"]);
        let folders = strings(&["/dev"]);
        let m = |q: &str| match_repo(q, "/dev/leo-git", &names, &folders);

        assert_eq!(m("leo-git"), Some(RepoMatch::ExactName));
        assert_eq!(m("leo"), Some(RepoMatch::NamePrefix));
        assert_eq!(m("-gi"), Some(RepoMatch::NameSubstring));
        assert_eq!(m("lg"), Some(RepoMatch::NameInitials));
        assert_eq!(m("lgt"), Some(RepoMatch::NameSubsequence));
        assert_eq!(m("zzz"), None);
        assert!(
            RepoMatch::ExactName < RepoMatch::PathSubstring,
            "stronger sorts first"
        );
    }

    /// Searching every label the row can display is the drift this closes: a
    /// list showing `owner/name` while searching only the basename found
    /// nothing when the user typed what was on screen.
    #[test]
    fn every_label_is_searchable_and_the_best_tier_wins() {
        let names = strings(&["leogit", "leomanrique/leogit"]);
        let folders = strings(&["/dev"]);

        assert_eq!(
            match_repo("leomanrique", "/dev/leogit", &names, &folders),
            Some(RepoMatch::NamePrefix),
            "the owner-qualified label matches too"
        );
        assert_eq!(
            match_repo("leogit", "/dev/leogit", &names, &folders),
            Some(RepoMatch::ExactName),
            "and the strongest tier across labels is the one reported"
        );
    }

    /// A path is searched below its deepest root, and the home folder counts as
    /// one — otherwise a repo outside every scan folder matches on
    /// `/users/<name>/`, which is in every path and therefore means nothing.
    #[test]
    fn path_search_trims_the_deepest_root() {
        let names = strings(&["proj"]);
        let folders = strings(&["/dev", "/dev/work"]);

        assert_eq!(
            match_repo("work", "/dev/work/proj", &names, &folders),
            None,
            "the deeper root wins, so its own name is no longer searchable"
        );
        assert_eq!(
            match_repo("nested", "/dev/nested/proj", &names, &folders),
            Some(RepoMatch::PathSubstring),
            "what remains below the root still matches"
        );
    }

    /// Comparison is normalized on both sides. A scan folder recorded with a
    /// different case or separator used to fail the prefix test outright, which
    /// silently made the whole absolute path searchable.
    #[test]
    fn path_search_normalizes_case_and_separators() {
        let names = strings(&["proj"]);
        assert_eq!(
            match_repo("sub", r"C:\Dev\sub\proj", &names, &strings(&[r"c:\dev"])),
            Some(RepoMatch::PathSubstring)
        );
        assert_eq!(
            match_repo("dev", r"C:\Dev\sub\proj", &names, &strings(&[r"c:\dev"])),
            None,
            "the root itself is trimmed away, whatever case it was stored in"
        );
    }

    /// Filtering is stable, so the caller's own ordering survives inside each
    /// tier — a picker that pinned the active repo first keeps it first.
    #[test]
    fn filtering_is_stable_within_a_tier_and_blank_matches_everything() {
        let list = rows(&[
            ("/dev/alpha", &["alpha"]),
            ("/dev/alps", &["alps"]),
            ("/dev/beta", &["beta"]),
        ]);
        let folders = strings(&["/dev"]);

        assert_eq!(
            filter_repos("al", &list, &folders),
            vec!["/dev/alpha".to_string(), "/dev/alps".to_string()],
            "both prefix-match, and input order breaks the tie"
        );
        assert_eq!(
            filter_repos("  ", &list, &folders).len(),
            3,
            "a blank query is not a filter"
        );
        assert!(filter_repos("zzz", &list, &folders).is_empty());
    }

    // -----------------------------------------------------------------------
    // derive_clone_target (H-4)
    // -----------------------------------------------------------------------

    /// The full matrix the two hand-written copies disagreed on, plus the two
    /// shapes both of them accepted and then failed to clone.
    #[test]
    fn clone_targets_derive_across_the_url_matrix() {
        let cases: &[(&str, &str, &str)] = &[
            // (input, expected url, expected name)
            (
                "https://github.com/owner/repo.git",
                "https://github.com/owner/repo.git",
                "repo",
            ),
            (
                "https://github.com/owner/repo",
                "https://github.com/owner/repo",
                "repo",
            ),
            (
                "git@github.com:owner/repo.git",
                "git@github.com:owner/repo.git",
                "repo",
            ),
            // Shorthand expands — and its `.git` goes with it, which one copy
            // kept and then handed to git as part of the repo path.
            ("owner/repo", "https://github.com/owner/repo", "repo"),
            ("owner/repo.git", "https://github.com/owner/repo", "repo"),
            // A trailing slash used to enable Clone and then fail.
            ("owner/repo/", "https://github.com/owner/repo", "repo"),
            // Pasted from an address bar, scheme dropped by the browser.
            (
                "github.com/owner/repo",
                "https://github.com/owner/repo",
                "repo",
            ),
            // Surrounding whitespace created a literal " repo" directory.
            ("  owner/repo  ", "https://github.com/owner/repo", "repo"),
        ];

        for (input, url, name) in cases {
            let derived = derive_clone_target(input, "/dev")
                .unwrap_or_else(|| panic!("{input:?} should be cloneable"));
            assert_eq!(&derived.normalized_url, url, "url for {input:?}");
            assert_eq!(&derived.repo_name, name, "name for {input:?}");
            assert_eq!(derived.target_path, format!("/dev/{name}"));
        }
    }

    /// This runs on every keystroke of a URL field, so it has to survive
    /// half-typed input — including a multi-byte character sitting where the
    /// `.git` check wants to slice, which is a byte offset, not a character
    /// one. It used to panic there: a crash natively, and a promise that never
    /// settles in the `WebView` host.
    #[test]
    fn clone_target_survives_multi_byte_input() {
        for input in [
            "日本",
            "日本語",
            "ab日本",
            "😀ab",
            "https://github.com/owner/日本語",
        ] {
            let _ = derive_clone_target(input, "/dev");
        }
        assert_eq!(
            derive_clone_target("https://github.com/owner/日本語", "/dev")
                .map(|t| t.repo_name)
                .as_deref(),
            Some("日本語"),
            "a non-ASCII repo name is still derivable"
        );
    }

    /// Nothing to clone means the button stays off, rather than enabling an
    /// action that fails once it runs.
    #[test]
    fn clone_target_is_none_when_there_is_nothing_to_clone() {
        assert_eq!(derive_clone_target("", "/dev"), None);
        assert_eq!(derive_clone_target("   ", "/dev"), None);
        assert_eq!(
            derive_clone_target("owner/repo", "  "),
            None,
            "no destination, no target"
        );
    }

    /// Destination joining collapses trailing slashes but keeps a bare root,
    /// which *is* its slash — stripping it produced a relative path.
    #[test]
    fn clone_paths_join_without_doubling_or_losing_the_root() {
        assert_eq!(
            clone_target_path("/dev/", "repo").as_deref(),
            Some("/dev/repo")
        );
        assert_eq!(
            clone_target_path("/dev///", "repo").as_deref(),
            Some("/dev/repo")
        );
        assert_eq!(clone_target_path("/", "repo").as_deref(), Some("/repo"));
        assert_eq!(clone_target_path("", "repo"), None);
        assert_eq!(clone_target_path("/dev", " "), None);
    }
}
