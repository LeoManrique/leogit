//! What the sync control should offer to do next — one proposal at a time,
//! picked by a strict precedence ladder over a [`RepoStatus`] — and the one
//! question on that ladder a status cannot answer by itself: whether a
//! diverged branch diverged because *this* branch moved away from commits it
//! once held.
//!
//! That question is answered from the branch's reflog, so nothing is remembered
//! between calls: an amend or a rebase done in a terminal, or before a restart,
//! reads the same as one done here. The force push asks it again at push time
//! ([`force_push_lease`]), about the same ref and through the same test, which
//! is what keeps a proposed Force Push from being one the push then refuses.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Mutex;

use super::git::{RepoStatus, run_git, run_git_optional, run_git_with_stdin};

/// What the sync control should offer to do next.
///
/// One state at a time. Pull outranks push, so a diverged branch proposes the
/// step that has to happen first — unless the divergence is this branch's own
/// rewrite, where pulling would merge the rewritten commits back in beside
/// their replacements and the step that has to happen is the force push. The
/// pending counts stay visible beside the control meanwhile.
///
/// It lives in core because four surfaces read it: each client's sync control
/// and each client's keyboard/menu route to the same action. Written twice it
/// would be four chances to disagree about what the repository needs next —
/// and the clients *had* drifted, one deriving the ladder as three loose
/// booleans that could all be true at once.
///
/// Titles, icons, and which states get a chevron stay per-platform: the two
/// controls are shaped differently and that is presentation, not policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncProposal {
    /// Nothing is known about the repository yet. A neutral, disabled Fetch —
    /// so the control never flashes "Publish" at a repository whose first
    /// status read simply hasn't landed.
    Loading,
    /// HEAD points at a commit rather than a branch: there is no branch to
    /// push or pull, and the way out is the branch picker.
    Detached,
    /// No remote at all — create the GitHub repository and push in one shot.
    PublishRepository,
    /// A remote exists but this branch tracks nothing, so its first push has
    /// to carry `--set-upstream`.
    PublishBranch,
    /// Diverged, and everything the upstream holds is something this branch
    /// held before it was rewritten — an amend, a rebase, a squash of commits
    /// already pushed. The push that publishes the rewrite is a force push.
    ForcePush,
    /// Behind the upstream. Pulling comes first, whatever else is pending.
    Pull,
    /// Ahead only.
    Push,
    /// In sync: the manual "check the remote", which touches no files.
    Fetch,
}

/// Run the sync ladder over a repository status.
///
/// Total: every status maps to exactly one proposal, which is what makes the
/// impossible combinations unrepresentable rather than merely unhandled —
/// "publishable and behind" cannot both be live the way three independent
/// booleans could.
///
/// `rewritten_away` answers the one question the status does not carry —
/// [`upstream_was_rewritten_away`], for a real repository. It is a closure
/// because it costs subprocesses: the ladder calls it on the diverged rung
/// only, so every other status is answered from the fields alone.
///
/// A host that has no status *at all* yet answers [`SyncProposal::Loading`]
/// itself; that is a fact about the host's own load, not about the repository.
#[must_use]
pub fn propose(status: &RepoStatus, rewritten_away: impl FnOnce() -> bool) -> SyncProposal {
    // A detached HEAD reports an empty branch name too, so it has to be told
    // apart from a status nobody has filled in before the emptiness test.
    if status.branch.is_empty() && !status.detached {
        return SyncProposal::Loading;
    }
    if status.detached {
        // Deliberately ahead of the remote checks: on a detached HEAD,
        // publishing would offer to push a branch that does not exist, and
        // the honest state wins.
        SyncProposal::Detached
    } else if !status.has_remote {
        SyncProposal::PublishRepository
    } else if !status.has_upstream {
        SyncProposal::PublishBranch
    } else if status.behind > 0 {
        // Behind *only* is never a force push, whatever the reflog says: after
        // a `pull --rebase` the old upstream tip is in the reflog too, and the
        // branch simply fast-forwards.
        if status.ahead > 0 && rewritten_away() {
            SyncProposal::ForcePush
        } else {
            SyncProposal::Pull
        }
    } else if status.ahead > 0 {
        SyncProposal::Push
    } else {
        SyncProposal::Fetch
    }
}

/// Whether `branch`'s upstream holds nothing this branch has not already held
/// — so that having diverged from it means the branch moved away from those
/// commits here, and not that somebody pushed something new.
///
/// The test is [`reflog_holds`] on the upstream's tip. An amended commit, a
/// rebased or squashed range and a `reset` past a pulled commit all leave the
/// old tip in the reflog — whoever wrote the commits left behind, which is why
/// the confirmation says they go. A commit somebody else pushed that was never
/// pulled was never under this branch's tip, and no entry reaches it.
///
/// It is asked only about a push that goes where the divergence was measured,
/// [`measured_push_target`].
///
/// **Every doubt reads as `false`**, which leaves the ladder proposing Pull and
/// the force push one menu away: no reflog (`core.logAllRefUpdates=false`), the
/// rewrite having aged out of it (`gc.reflogExpireUnreachable`), an entry whose
/// object was pruned, git failing to run. Three local subprocesses, spent only
/// while the branch is diverged; nothing is cached, because the honest cache
/// key — the upstream's sha — costs one of them, and a reflog can change under
/// an unchanged pair of tips.
pub(crate) fn upstream_was_rewritten_away(repo_path: &str, branch: &str) -> bool {
    let held = measured_push_target(repo_path, branch).and_then(|target| match target {
        Some(upstream) => reflog_holds(repo_path, branch, &upstream),
        None => Ok(false),
    });
    match held {
        Ok(held) => held,
        Err(doubt) => {
            complain_once(&format!(
                "[sync_ladder] could not tell whether {branch} was rewritten, proposing Pull: {doubt}"
            ));
            false
        }
    }
}

/// The lease a force push of `branch` to `remote` carries:
/// `--force-with-lease=<remote branch>:<the commit it may replace>`.
///
/// The commit is the remote-tracking ref's tip, and it is pinned only after
/// [`reflog_holds`] has shown this branch once held it. Both halves matter. A
/// bare `--force-with-lease` compares the remote branch with the
/// remote-tracking ref at push time — which the automatic fetches keep moving,
/// so after one of them it passes over a commit somebody else pushed and this
/// branch never held. Naming the commit makes the lease about what was
/// *checked*, not about whatever the ref says a moment later.
///
/// Deliberately not git's `--force-if-includes`, though the test is the same
/// one: git 2.54 bounds its reflog walk by the newest entry of the
/// *remote-tracking* ref's reflog and reads that timestamp uninitialised when
/// there is none — so in a fresh clone, whose `refs/remotes/origin/*` have no
/// reflog until the first fetch that moves them, it refuses the plainest amend
/// of a pushed commit. (An explicit lease value also turns that flag into a
/// no-op, so the two cannot be combined.)
///
/// # Errors
/// When the remote branch was never fetched, when this branch's reflog does
/// not show it held the remote-tracking tip — somebody else's unpulled commit,
/// but also a repository that keeps no reflog — or when git can't run.
pub(crate) fn force_push_lease(
    repo_path: &str,
    remote: &str,
    branch: &str,
) -> Result<String, String> {
    let tracking_ref = tracking_ref(remote, branch);
    // `--quiet` makes a ref that is not there exit 1 in silence, which is the
    // one answer told apart from git failing.
    let Some(tip) = run_git_optional(
        repo_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{tracking_ref}^{{commit}}"),
        ],
    )?
    else {
        return Err(format!(
            "{remote}/{branch} has never been fetched here, so there is no known commit for a force push to replace. Fetch first."
        ));
    };

    if !reflog_holds(repo_path, branch, &tip)? {
        return Err(format!(
            "{remote}/{branch} holds commits this branch never contained, and a force push would delete them. Pull them in, or look at them first."
        ));
    }
    Ok(format!("--force-with-lease=refs/heads/{branch}:{tip}"))
}

/// The remote-tracking ref a push of `branch` to `remote` lands under, with the
/// fetch refspec `git clone` and `git remote add` write. A remote configured
/// with another one is not proposed a force push and cannot send one; both say
/// so rather than guess at a ref.
fn tracking_ref(remote: &str, branch: &str) -> String {
    format!("refs/remotes/{remote}/{branch}")
}

/// `branch`'s upstream ref, when a push from here lands on the branch it
/// stands for — and `None` when the divergence was measured against one ref
/// and the push would move another, where forcing it ends nothing.
///
/// A push here names the remote [`get_push_remote`](super::git::get_push_remote)
/// picks and the branch once, which git reads as `<branch>:<branch>`. So the
/// upstream has to live on git's push remote for the branch (`%(push:remotename)`
/// — `pushRemote`, then `remote.pushDefault`, then the tracking remote, whatever
/// `push.default` says; a fork workflow names the fork there), under this
/// branch's own name, in the [`tracking_ref`] the force push will read.
fn measured_push_target(repo_path: &str, branch: &str) -> Result<Option<String>, String> {
    let local_ref = format!("refs/heads/{branch}");
    let described = run_git(
        repo_path,
        &[
            "for-each-ref",
            "--format=%(upstream)%00%(upstream:remoteref)%00%(push:remotename)",
            &local_ref,
        ],
    )?;
    let mut facts = described.split('\0');
    let (Some(upstream), Some(upstream_branch), Some(push_remote)) =
        (facts.next(), facts.next(), facts.next())
    else {
        return Ok(None);
    };
    let lands_on_upstream = !push_remote.is_empty()
        && upstream_branch == local_ref
        && upstream == tracking_ref(push_remote, branch);
    Ok(lands_on_upstream.then(|| upstream.to_string()))
}

/// Whether `commit` is reachable from some entry of `branch`'s reflog: whether
/// this branch, at some point this repository still remembers, contained it.
fn reflog_holds(repo_path: &str, branch: &str, commit: &str) -> Result<bool, String> {
    let local_ref = format!("refs/heads/{branch}");

    // `rev-list -g`, not `log -g` or `reflog show`: it is plumbing, so
    // `log.showSignature` and `i18n.logOutputEncoding` cannot put anything but
    // object names on its stdout. An entry whose object is gone is skipped by
    // git, which only removes an exclusion below — towards Pull.
    let reflog = run_git(repo_path, &["rev-list", "-g", &local_ref])?;
    if reflog.is_empty() {
        return Ok(false);
    }

    // "Is the commit reachable from any of these?" — asked as "what is left of
    // it once all of them are excluded", on stdin because a reflog has no
    // length limit and an argument list does. Deliberately without
    // `--ignore-missing`: it would also forgive a missing *commit*, and an
    // empty answer to a question about nothing reads as a force push.
    let mut revisions = format!("{commit}\n");
    for entry in reflog.lines() {
        revisions.push('^');
        revisions.push_str(entry.trim());
        revisions.push('\n');
    }
    let output = run_git_with_stdin(
        repo_path,
        &["rev-list", "-1", "--stdin"],
        revisions.as_bytes(),
    )?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(output.stdout.iter().all(u8::is_ascii_whitespace))
}

/// Log a complaint the first time it is made, and never again.
///
/// The probe runs on the status poll, so a repository it cannot answer for
/// would otherwise say so every two seconds for as long as it stays diverged —
/// and two such repositories open at once would take turns.
fn complain_once(complaint: &str) {
    static MADE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let mut made = MADE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if made.insert(complaint.to_string()) {
        eprintln!("{complaint}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::get_status;
    use crate::test_support::{clone_of, commit_file, git, git_stdout, published_repo};
    use std::path::Path;

    /// A status with everything settled: on a branch, tracking a reachable
    /// upstream, nothing pending. Each test perturbs the one field it is about.
    fn synced_status() -> RepoStatus {
        RepoStatus {
            branch: "main".to_string(),
            upstream: "origin/main".to_string(),
            has_upstream: true,
            ahead: 0,
            behind: 0,
            files: Vec::new(),
            has_remote: true,
            unpushed_shas: Vec::new(),
            detached: false,
            head_sha: "a".repeat(40),
            operation: None,
            proposal: SyncProposal::Fetch,
        }
    }

    /// For a rung that must be decided without asking the reflog at all.
    fn never_asked() -> bool {
        panic!("this rung is answered from the status alone");
    }

    /// The proposal a real repository's status carries.
    fn proposal_of(repo: &Path) -> SyncProposal {
        get_status(repo.to_string_lossy().into_owned())
            .expect("status")
            .proposal
    }

    /// The ladder's precedence, top to bottom, each rung asserted against a
    /// status that also satisfies every rung below it — which is the property
    /// three independent booleans could not express.
    #[test]
    fn sync_ladder_follows_its_precedence() {
        assert_eq!(propose(&synced_status(), never_asked), SyncProposal::Fetch);

        let ahead = RepoStatus {
            ahead: 2,
            ..synced_status()
        };
        assert_eq!(propose(&ahead, never_asked), SyncProposal::Push);

        // Diverged by somebody else's push: pull outranks push, so the step
        // that has to happen first is the one proposed.
        let diverged = RepoStatus {
            ahead: 2,
            behind: 3,
            ..synced_status()
        };
        assert_eq!(propose(&diverged, || false), SyncProposal::Pull);

        // Diverged by this branch's own rewrite: there is nothing to pull.
        assert_eq!(propose(&diverged, || true), SyncProposal::ForcePush);

        // An untracked branch outranks its own inferred counts: the first push
        // must set the upstream before anything can be pulled into it.
        let untracked = RepoStatus {
            has_upstream: false,
            upstream: String::new(),
            ahead: 2,
            behind: 3,
            ..synced_status()
        };
        assert_eq!(
            propose(&untracked, never_asked),
            SyncProposal::PublishBranch
        );

        // No remote at all outranks the untracked branch, since there is
        // nothing to set an upstream to.
        let no_remote = RepoStatus {
            has_remote: false,
            has_upstream: false,
            upstream: String::new(),
            ..synced_status()
        };
        assert_eq!(
            propose(&no_remote, never_asked),
            SyncProposal::PublishRepository
        );

        // And a detached HEAD outranks every remote question, because there is
        // no branch for any of them to be about.
        let detached = RepoStatus {
            detached: true,
            branch: String::new(),
            has_remote: false,
            has_upstream: false,
            upstream: String::new(),
            ..synced_status()
        };
        assert_eq!(propose(&detached, never_asked), SyncProposal::Detached);
    }

    /// After a `pull --rebase` the old upstream tip sits in the reflog, so the
    /// reflog alone would say "rewritten" — but a branch that is only behind
    /// fast-forwards, and must never be offered a force push.
    #[test]
    fn sync_ladder_never_asks_the_reflog_about_a_branch_that_is_only_behind() {
        let behind = RepoStatus {
            behind: 3,
            ..synced_status()
        };
        assert_eq!(propose(&behind, never_asked), SyncProposal::Pull);
    }

    /// The empty status a client holds before its first read must not look
    /// like a repository with no remote, or the control flashes "Publish" at
    /// every repo on the way in.
    #[test]
    fn sync_ladder_waits_for_a_real_status() {
        let unloaded = RepoStatus {
            branch: String::new(),
            upstream: String::new(),
            has_upstream: false,
            has_remote: false,
            head_sha: String::new(),
            ..synced_status()
        };
        assert_eq!(propose(&unloaded, never_asked), SyncProposal::Loading);
    }

    /// A freshly initialised repository — a real branch, no commits, no
    /// remote — is a publish candidate, not an unloaded status.
    #[test]
    fn sync_ladder_offers_publish_for_an_unborn_repository() {
        let unborn = RepoStatus {
            has_remote: false,
            has_upstream: false,
            upstream: String::new(),
            head_sha: String::new(),
            ..synced_status()
        };
        assert_eq!(
            propose(&unborn, never_asked),
            SyncProposal::PublishRepository
        );
    }

    #[test]
    fn sync_proposes_force_push_after_a_rewrite_and_pull_after_a_foreign_push() {
        let (tmp, mine) = published_repo();

        // Amending a pushed commit diverges the branch 1/1 from its own past.
        git(
            &mine,
            &["commit", "-q", "--amend", "-m", "second, reworded"],
        );
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);

        // Somebody else pushes on top of the old tip. Until it is fetched the
        // upstream has not moved here, and the answer stands; once it is, the
        // upstream holds a commit this branch never did.
        let theirs = clone_of(tmp.path(), "theirs");
        commit_file(&theirs, "theirs.txt", "theirs\n", "their commit");
        git(&theirs, &["push", "-q", "origin", "main"]);
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);
        git(&mine, &["fetch", "-q", "origin"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);
    }

    /// The plain divergence the ladder always had: their push, my commit, no
    /// rewrite anywhere.
    #[test]
    fn sync_proposes_pull_when_both_sides_only_added_commits() {
        let (tmp, mine) = published_repo();
        let theirs = clone_of(tmp.path(), "theirs");
        commit_file(&theirs, "theirs.txt", "theirs\n", "their commit");
        git(&theirs, &["push", "-q", "origin", "main"]);

        commit_file(&mine, "mine.txt", "mine\n", "my commit");
        git(&mine, &["fetch", "-q", "origin"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);
    }

    /// A rebase of pushed commits — the case the History actions produce — read
    /// from a repository that kept no memory of who ran it.
    #[test]
    fn sync_proposes_force_push_after_a_rebase_of_pushed_commits() {
        let (_tmp, mine) = published_repo();
        commit_file(&mine, "third.txt", "third\n", "third");
        git(&mine, &["push", "-q", "origin", "main"]);

        // Drop the middle commit: "third" is replayed onto "first".
        git(
            &mine,
            &["rebase", "-q", "--onto", "HEAD~2", "HEAD~1", "main"],
        );
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);
    }

    #[test]
    fn sync_proposes_pull_when_the_reflog_cannot_answer() {
        let (_tmp, mine) = published_repo();
        git(
            &mine,
            &["commit", "-q", "--amend", "-m", "second, reworded"],
        );
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);

        // The rewrite ages out of the reflog: the old tip is no longer
        // anything this repository can show it held.
        git(&mine, &["reflog", "expire", "--expire=now", "--all"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);
    }

    /// A fork workflow measures the divergence against `origin` and pushes to
    /// `fork`: forcing `fork` would not end it, so it is not the proposal.
    #[test]
    fn sync_proposes_pull_when_the_push_goes_somewhere_else() {
        let (tmp, mine) = published_repo();
        git(
            &mine,
            &["commit", "-q", "--amend", "-m", "second, reworded"],
        );
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);

        git(tmp.path(), &["init", "-q", "--bare", "fork.git"]);
        git(&mine, &["remote", "add", "fork", "../fork.git"]);
        git(&mine, &["config", "remote.pushDefault", "fork"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);

        // `push.default=upstream` makes git's own `@{push}` the upstream again,
        // but a push from here still names the fork.
        git(&mine, &["config", "push.default", "upstream"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);

        git(&mine, &["config", "--unset", "remote.pushDefault"]);
        git(&mine, &["config", "branch.main.pushRemote", "fork"]);
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);
    }

    /// A push here names the local branch, so it lands on the remote branch of
    /// the *same* name — not on a differently named upstream, which is the ref
    /// the divergence was measured against.
    #[test]
    fn sync_proposes_pull_when_the_upstream_branch_has_another_name() {
        let (_tmp, mine) = published_repo();
        git(
            &mine,
            &["commit", "-q", "--amend", "-m", "second, reworded"],
        );
        assert_eq!(proposal_of(&mine), SyncProposal::ForcePush);

        // The reflog and the upstream both move with the rename, so nothing
        // but the name changed.
        git(&mine, &["branch", "-m", "main", "renamed"]);
        assert_eq!(
            git_stdout(&mine, &["rev-parse", "--abbrev-ref", "renamed@{upstream}"]),
            "origin/main"
        );
        assert_eq!(proposal_of(&mine), SyncProposal::Pull);
    }
}
