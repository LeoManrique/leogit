use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileStatus {
    New,
    Modified,
    Deleted,
    Renamed,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub orig_path: Option<String>,
    pub status: FileStatus,
    pub xy: String,
    pub display_name: String,
    pub display_dir: String,
    /// True when this entry is an embedded git repository (a nested repo with
    /// its own `.git`). Git reports it as a single untracked directory entry —
    /// it never recurses into it, even under `--untracked-files=all` — so the
    /// path keeps a trailing slash. Committing it stages a gitlink (a pointer to
    /// the nested repo's commit), not the folder's files; the UI surfaces that
    /// distinction before letting the user commit. Always false for tracked
    /// changes and ordinary untracked files.
    pub embedded: bool,
    /// True when this entry is a tracked submodule that is dirty *inside* (its
    /// own working tree has modified or untracked content) but whose recorded
    /// commit pointer has NOT moved. There is nothing the parent repo can stage
    /// — `git add` is a no-op — so the change can't be committed from here; the
    /// inner changes must be committed inside the submodule first. The UI
    /// disables this entry rather than letting a commit fail with "staging
    /// produced no changes". A submodule whose pointer *did* move is committable
    /// and leaves this false.
    pub submodule_dirty: bool,
}

/// Aggregate line-change totals for a single commit, summed across every file
/// it touches. Binary files (which `git --numstat` reports as `-`/`-`) are
/// skipped, so the totals reflect only text lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitStats {
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub summary: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub committer_name: String,
    pub committer_date: String,
    pub parents: Vec<String>,
    pub trailers: Vec<String>,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub success: bool,
    pub fast_forward: bool,
    pub conflicts: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOptions {
    pub max_count: i32,
    pub skip: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AheadBehind {
    pub ahead: i32,
    pub behind: i32,
}

/// Lightweight per-repo sync summary used by the repo picker's background
/// scheduler to render pull/push badges without fully opening each repo.
/// Computed from `git status` headers (no working-tree file scan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSync {
    pub ahead: i32,
    pub behind: i32,
    /// Whether the repo has at least one configured remote. Repos with no
    /// remote can never be ahead/behind, so the picker skips their badges.
    pub has_remote: bool,
    /// Whether the optional network fetch actually reached the remote. `true`
    /// when no fetch was requested or there was no remote to reach (nothing to
    /// report), `false` when a requested fetch failed/timed out. The frontend
    /// feeds this to its connectivity circuit breaker so a run of failed
    /// background fetches backs off instead of hammering an unreachable remote.
    pub fetched: bool,
}

/// Full status payload returned by `get_status`.
/// Includes branch metadata parsed from `# branch.*` headers as well as the file list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub branch: String,
    pub upstream: String,
    pub has_upstream: bool,
    pub ahead: i32,
    pub behind: i32,
    pub files: Vec<FileEntry>,
    /// Whether the repo has at least one configured remote. When false the UI
    /// offers "Publish to GitHub" instead of Push, since there's nowhere to push.
    pub has_remote: bool,
    /// SHAs of commits reachable from HEAD but not from the remote tracking
    /// branch — i.e. commits the user still needs to push. Empty when the
    /// branch has no resolvable upstream or is in sync. Used by the History
    /// view to mark unpushed rows.
    pub unpushed_shas: Vec<String>,
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Build a Command for `git` with the standard env vars set.
/// TERM=dumb suppresses pagers/color; GIT_TERMINAL_PROMPT=0 prevents credential prompts
/// from blocking the process indefinitely.
fn git_cmd(repo_path: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path)
        .env("TERM", "dumb")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args);
    super::process::hide_console(&mut cmd);
    cmd
}

/// Run a git command and return the raw stdout bytes.
/// Use this when you need to preserve NUL terminators or binary content.
fn run_git_raw(repo_path: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = git_cmd(repo_path, args)
        .output()
        .map_err(|e| format!("git: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Run a git command and return stdout as a UTF-8 string with trailing whitespace trimmed.
/// Use this for line-oriented git output (NOT for NUL-delimited formats).
fn run_git(repo_path: &str, args: &[&str]) -> Result<String, String> {
    let bytes = run_git_raw(repo_path, args)?;
    Ok(String::from_utf8_lossy(&bytes).trim_end().to_string())
}

/// Run a git command and return combined stdout+stderr, regardless of exit status.
/// The bool indicates whether the command succeeded.
fn run_git_combined(repo_path: &str, args: &[&str]) -> Result<(bool, String), String> {
    let output = git_cmd(repo_path, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("git: {}", e))?;
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), combined))
}

// ---------------------------------------------------------------------------
// Network git: bounded so an offline / flaky connection can't hang a thread
// ---------------------------------------------------------------------------
//
// Two layers of protection, both applied to every remote-touching git op:
//   1. Transport timeouts baked into the command (`git_net_cmd`) so git itself
//      aborts a connect or stalled transfer quickly.
//   2. A hard kill-timeout (`process::run_timed`) as a backstop, in case a
//      transport ignores the knobs above and wedges anyway.
// The commands that use these are also `#[tauri::command(async)]` so they run
// on a worker thread, never the UI thread — a slow remote degrades a badge,
// it never freezes the app.

/// Background badge fetches: tiny, fired on a timer for many repos, so they
/// fail fast — if a remote isn't reachable in a few seconds we abandon and keep
/// the last-known counts.
const NET_BG_CONNECT_SECS: u64 = 8;
const NET_BG_STALL_SECS: u64 = 8;
const NET_BG_TIMEOUT: Duration = Duration::from_secs(12);

/// User-initiated transfers (Pull/Push/Fetch/Clone buttons): generous, because
/// a legitimate large transfer takes a while. The connect/stall knobs still
/// abort a *stalled* connection fast; the hard cap only catches a wedged
/// process that's making no progress at all.
const NET_UI_CONNECT_SECS: u64 = 15;
const NET_UI_STALL_SECS: u64 = 30;
const NET_UI_TIMEOUT: Duration = Duration::from_secs(600);

/// Build a `git` Command for a *network* operation with transport timeouts so an
/// unreachable or stalled remote fails fast instead of hanging:
///   - SSH: `ConnectTimeout` bounds the TCP/handshake; `BatchMode=yes` refuses
///     any interactive prompt (so a missing key errors immediately).
///   - HTTP(S): `http.lowSpeedLimit`/`http.lowSpeedTime` abort a transfer that
///     drops below ~1 KB/s for `stall_secs` (covers a dropped connection mid
///     transfer, which a plain connect timeout would miss).
/// `current_dir` is `None` for `clone` (no repo exists yet).
fn git_net_cmd(
    current_dir: Option<&str>,
    args: &[&str],
    connect_secs: u64,
    stall_secs: u64,
) -> Command {
    let mut cmd = Command::new("git");
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    cmd.env("TERM", "dumb")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(
            "GIT_SSH_COMMAND",
            format!("ssh -o ConnectTimeout={connect_secs} -o BatchMode=yes"),
        )
        .arg("-c")
        .arg("http.lowSpeedLimit=1000")
        .arg("-c")
        .arg(format!("http.lowSpeedTime={stall_secs}"))
        .args(args);
    super::process::hide_console(&mut cmd);
    cmd
}

/// Run a network git op with both the transport timeouts of [`git_net_cmd`] and
/// the hard kill-timeout of [`process::run_timed`]. Returns
/// `(succeeded, combined_output)`.
fn run_git_net(
    current_dir: Option<&str>,
    args: &[&str],
    connect_secs: u64,
    stall_secs: u64,
    timeout: Duration,
) -> Result<(bool, String), String> {
    let cmd = git_net_cmd(current_dir, args, connect_secs, stall_secs);
    let out = super::process::run_timed(cmd, &format!("git {}", args.join(" ")), timeout)?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), combined))
}

/// Returns true if the repository has at least one commit (HEAD resolves to a
/// commit). A fresh repo with an unborn HEAD returns false rather than erroring,
/// letting callers treat "no commits yet" as a valid empty state instead of
/// hitting git's "does not have any commits yet" fatal.
fn has_commits(repo_path: &str) -> bool {
    git_cmd(repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// First configured remote name (e.g. "origin"), or `None` when the repo has
/// no remotes. Used both to gate Push-vs-Publish and to locate the
/// remote-tracking ref for the no-upstream ahead/behind fallback.
fn first_remote(repo_path: &str) -> Option<String> {
    let out = run_git(repo_path, &["remote"]).unwrap_or_default();
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Ahead/behind for a branch that has no explicit upstream, measured against
/// `refs/remotes/<remote>/<branch>` when such a ref exists. Returns the counts
/// alongside the resolved remote-tracking ref so callers can list unpushed
/// commits. `None` when there's no matching remote ref (unpublished branch,
/// detached HEAD, etc.). Shared by `get_status` and `repo_sync_status` so the
/// fallback stays identical in both.
fn remote_tracking_ahead_behind(
    repo_path: &str,
    remote: &str,
    branch: &str,
) -> Option<(i32, i32, String)> {
    let remote_ref = format!("refs/remotes/{remote}/{branch}");
    let exists = git_cmd(repo_path, &["rev-parse", "--verify", "--quiet", &remote_ref])
        .output()
        .is_ok_and(|o| o.status.success());
    if !exists {
        return None;
    }
    let range = format!("HEAD...{remote_ref}");
    let out = run_git(
        repo_path,
        &["rev-list", "--left-right", "--count", &range, "--"],
    )
    .ok()?;
    // Output: "<ahead>\t<behind>" — left side is HEAD, right side is the ref.
    let parts: Vec<&str> = out.split_whitespace().collect();
    if parts.len() == 2 {
        let ahead = parts[0].parse().unwrap_or(0);
        let behind = parts[1].parse().unwrap_or(0);
        Some((ahead, behind, remote_ref))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Path display helpers
// ---------------------------------------------------------------------------

/// Returns the filename and parent directory components of a path.
/// Matches Go's filepath.Base / filepath.Dir; root paths return empty `dir`.
fn extract_display_name_and_dir(path: &str) -> (String, String) {
    let p = Path::new(path);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let dir = match p.parent().and_then(|d| d.to_str()) {
        Some("") | Some(".") | None => String::new(),
        Some(d) => d.to_string(),
    };
    (name, dir)
}

// ---------------------------------------------------------------------------
// FileStatus mapping (porcelain v2 XY codes)
// ---------------------------------------------------------------------------

/// Map the 2-character XY status code to a FileStatus.
/// X = staged status, Y = worktree status. Priority: Conflicted > New > Renamed > Deleted > Modified.
fn status_from_xy(xy: &str) -> FileStatus {
    let bytes = xy.as_bytes();
    if bytes.len() != 2 {
        return FileStatus::Modified;
    }
    let (x, y) = (bytes[0], bytes[1]);
    if x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D') {
        return FileStatus::Conflicted;
    }
    if x == b'?' {
        return FileStatus::New;
    }
    if x == b'A' {
        return FileStatus::New;
    }
    if x == b'R' {
        return FileStatus::Renamed;
    }
    if x == b'D' || y == b'D' {
        return FileStatus::Deleted;
    }
    FileStatus::Modified
}

/// Classify the porcelain-v2 `sub` field (the 3rd field of a changed entry).
///
/// It is 4 chars: `N...` for a non-submodule, or `S<c><m><u>` for a submodule
/// where `c`=`C` if the recorded commit changed, `m`=`M` if it has modified
/// tracked content, and `u`=`U` if it has untracked content.
///
/// Returns true only for the *dirty-but-pointer-unmoved* case: a submodule
/// (`c` is `.`, not `C`) with at least one of `m`/`u` set. That is the one
/// state the parent repo cannot stage — there is no gitlink change to add, so a
/// commit would fail. When the commit moved (`c` is `C`) the gitlink *is*
/// stageable, so this returns false.
fn is_dirty_submodule(sub: &str) -> bool {
    let b = sub.as_bytes();
    b.len() == 4 && b[0] == b'S' && b[1] == b'.' && (b[2] == b'M' || b[3] == b'U')
}

// ---------------------------------------------------------------------------
// get_status: parses porcelain v2 -z output
// ---------------------------------------------------------------------------

/// Parse a type-1 ordinary changed entry: `1 XY sub mH mI mW hH hI <path>`
/// (9 fields total; the 9th field captures the full path, including spaces).
fn parse_ordinary_entry(seg: &str) -> Option<FileEntry> {
    let parts: Vec<&str> = seg.splitn(9, ' ').collect();
    if parts.len() < 9 {
        return None;
    }
    let xy = parts[1].to_string();
    let path = parts[8].to_string();
    let status = status_from_xy(&xy);
    let submodule_dirty = is_dirty_submodule(parts[2]);
    let (display_name, display_dir) = extract_display_name_and_dir(&path);
    Some(FileEntry {
        path,
        orig_path: None,
        status,
        xy,
        display_name,
        display_dir,
        embedded: false,
        submodule_dirty,
    })
}

/// Parse a type-2 rename/copy entry: `2 XY sub mH mI mW hH hI Xscore <newpath>`
/// (10 fields; orig path comes from the following NUL segment).
fn parse_rename_entry(seg: &str, orig_path: String) -> Option<FileEntry> {
    let parts: Vec<&str> = seg.splitn(10, ' ').collect();
    if parts.len() < 10 {
        return None;
    }
    let xy = parts[1].to_string();
    let path = parts[9].to_string();
    let (display_name, display_dir) = extract_display_name_and_dir(&path);
    Some(FileEntry {
        path,
        orig_path: if orig_path.is_empty() {
            None
        } else {
            Some(orig_path)
        },
        status: FileStatus::Renamed,
        xy,
        display_name,
        display_dir,
        embedded: false,
        submodule_dirty: false,
    })
}

/// Parse a type-u unmerged/conflict entry: `u XY sub m1 m2 m3 mW h1 h2 h3 <path>`
/// (11 fields).
fn parse_unmerged_entry(seg: &str) -> Option<FileEntry> {
    let parts: Vec<&str> = seg.splitn(11, ' ').collect();
    if parts.len() < 11 {
        return None;
    }
    let xy = parts[1].to_string();
    let path = parts[10].to_string();
    let (display_name, display_dir) = extract_display_name_and_dir(&path);
    Some(FileEntry {
        path,
        orig_path: None,
        status: FileStatus::Conflicted,
        xy,
        display_name,
        display_dir,
        embedded: false,
        submodule_dirty: false,
    })
}

#[tauri::command]
pub fn get_status(repo_path: String) -> Result<RepoStatus, String> {
    // Get raw bytes — DO NOT trim or convert until we've split on NUL.
    let bytes = run_git_raw(
        &repo_path,
        &[
            "--no-optional-locks",
            "status",
            "--untracked-files=all",
            "--branch",
            "--porcelain=2",
            "-z",
        ],
    )?;

    let mut result = RepoStatus {
        branch: String::new(),
        upstream: String::new(),
        has_upstream: false,
        ahead: 0,
        behind: 0,
        files: Vec::new(),
        has_remote: false,
        unpushed_shas: Vec::new(),
    };

    // Configured remote, queried once and reused below (the no-upstream
    // ahead/behind fallback needs the first remote's name). `has_remote` drives
    // the UI's Push-vs-Publish choice.
    let first_remote = first_remote(&repo_path);
    result.has_remote = first_remote.is_some();

    if bytes.is_empty() {
        return Ok(result);
    }

    // Under `-z`, EVERY porcelain v2 record — `# branch.*` headers included — is
    // NUL-terminated; there are no newlines in the output. We walk the leading
    // header records (those starting with "# "), then split the remainder on NUL
    // for the file entries. The header section ends at the first record that does
    // not begin with "# ".
    let mut rest: &[u8] = &bytes;

    while let Some(sep) = rest.iter().position(|&b| b == b'\0') {
        let record = &rest[..sep];
        if !record.starts_with(b"# ") {
            break;
        }
        let line_str = String::from_utf8_lossy(record);
        if let Some(rem) = line_str.strip_prefix("# branch.head ") {
            let val = rem.trim();
            result.branch = if val == "(detached)" {
                String::new()
            } else {
                val.to_string()
            };
        } else if let Some(rem) = line_str.strip_prefix("# branch.upstream ") {
            result.upstream = rem.trim().to_string();
            result.has_upstream = true;
        } else if let Some(rem) = line_str.strip_prefix("# branch.ab ") {
            let parts: Vec<&str> = rem.split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].trim_start_matches('+').parse().unwrap_or(0);
                let behind = parts[1].trim_start_matches('-').parse().unwrap_or(0);
                result.ahead = ahead;
                result.behind = behind;
            }
        }
        rest = &rest[sep + 1..];
    }

    // Fallback: if branch.head wasn't present (e.g., empty repo), try rev-parse.
    if result.branch.is_empty() {
        if let Ok(b) = run_git(&repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
            if b != "HEAD" {
                result.branch = b;
            }
        }
    }

    // Effective upstream ref — what `rev-list HEAD ^<this>` should compare
    // against. For tracked branches it's the `branch.upstream` value; for
    // untracked branches it falls back to `refs/remotes/<first-remote>/<branch>`
    // if such a ref exists. Stays `None` for detached HEAD, empty repos, or
    // branches with no matching remote ref.
    let mut effective_upstream: Option<String> = if result.has_upstream {
        Some(result.upstream.clone())
    } else {
        None
    };

    // Ahead/behind fallback for branches WITHOUT explicit upstream tracking.
    // git status only emits `# branch.ab` when `branch.<name>.{merge,remote}` are
    // set, so a freshly created local branch (or a clone that never ran
    // `push -u`) will report ahead=behind=0 even when a matching remote ref
    // exists. Match GitHub Desktop: if there's a remote ref at
    // `refs/remotes/<first-remote>/<branch>`, compute ahead/behind against it
    // manually so the Push badge updates.
    //
    // Don't synthesise `has_upstream = true` — that flag still drives whether
    // the next push needs `--set-upstream`, and lying about it would break
    // first-push behaviour.
    if !result.has_upstream && !result.branch.is_empty() {
        if let Some(remote) = first_remote.as_deref() {
            if let Some((ahead, behind, remote_ref)) =
                remote_tracking_ahead_behind(&repo_path, remote, &result.branch)
            {
                result.ahead = ahead;
                result.behind = behind;
                // Surface the synthesised tracking name so the UI / debug logs
                // make sense; this is purely informational and does NOT flip
                // has_upstream.
                result.upstream = format!("{}/{} (inferred)", remote, result.branch);
                effective_upstream = Some(remote_ref);
            }
        }
    }

    // List unpushed commit SHAs so the History view can mark them with an
    // up-arrow, matching GitHub Desktop. Two cases:
    //
    //  - The branch tracks an upstream (real or inferred) and is ahead: the
    //    unpushed set is `HEAD ^<upstream>` — exactly the commits ahead of it.
    //  - The branch has no resolvable upstream — a new local branch never pushed,
    //    with no same-named remote ref (e.g. cloned `main`, branched off, committed)
    //    — but the repo has a remote: fall back to `HEAD --not --remotes`, i.e.
    //    local commits not reachable from ANY remote branch. That marks the new
    //    commits while leaving the shared base (on `origin/main`) unmarked. Without
    //    this fallback the History view showed no arrows at all on an unpublished
    //    branch, since `ahead` stays 0 there. When the repo has a remote but no
    //    remote-tracking refs yet (just `remote add`, never pushed), this correctly
    //    marks every commit — none of them are on the remote.
    //
    // Why `--remotes` (every remote) rather than scoping to the push remote:
    // `--not --remotes` is conservative — it can only ever UNDER-mark (miss an
    // arrow on a commit that also happens to live on some unrelated remote ref),
    // never FALSE-mark a commit that's already pushed. Scoping to a single guessed
    // remote (e.g. the alphabetically-first one) would draw a false arrow on an
    // already-pushed commit whenever that guess isn't the real push target — a
    // worse error than the rare under-mark. The only divergence from GitHub Desktop
    // is a multi-remote/fork setup where a commit was pushed to a non-default
    // remote only; accepted as a known limitation.
    //
    // Both are skipped when there's nothing to compute (an in-sync upstream
    // branch, or a repo with no remotes) to avoid an extra `git rev-list` on the
    // 2s status poll.
    let exclude_upstream = effective_upstream.as_deref().map(|up| format!("^{up}"));
    let unpushed_args: Option<Vec<&str>> = if result.ahead > 0 {
        exclude_upstream
            .as_deref()
            .map(|ex| vec!["rev-list", "HEAD", ex])
    } else if effective_upstream.is_none() && result.has_remote && !result.branch.is_empty() {
        Some(vec!["rev-list", "HEAD", "--not", "--remotes"])
    } else {
        None
    };
    if let Some(args) = unpushed_args {
        if let Ok(out) = run_git(&repo_path, &args) {
            result.unpushed_shas = out
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
        }
    }

    if rest.is_empty() {
        return Ok(result);
    }

    // Split remainder on NUL into segments. Each tracked-change segment is one
    // NUL-terminated record except for type-2 (rename) entries, which produce
    // two consecutive NUL segments: <newpath-line>\0<oldpath>\0.
    let segments: Vec<&[u8]> = rest.split(|&b| b == b'\0').collect();
    let mut i = 0;
    while i < segments.len() {
        let seg = segments[i];
        if seg.is_empty() {
            i += 1;
            continue;
        }
        let seg_str = match std::str::from_utf8(seg) {
            Ok(s) => s,
            Err(_) => {
                // Skip non-UTF-8 entries rather than crash. Real paths on macOS/Linux
                // are typically UTF-8; pathological cases are rare.
                i += 1;
                continue;
            }
        };

        if let Some(rest_seg) = seg_str.strip_prefix("? ") {
            // Untracked file. A trailing slash means git reported a directory
            // rather than a file — under `-uall` that only happens for an
            // embedded git repository, which git won't recurse into.
            let path = rest_seg.to_string();
            let embedded = path.ends_with('/');
            let (display_name, display_dir) = extract_display_name_and_dir(&path);
            result.files.push(FileEntry {
                path,
                orig_path: None,
                status: FileStatus::New,
                xy: "??".to_string(),
                display_name,
                display_dir,
                embedded,
                submodule_dirty: false,
            });
        } else if seg_str.starts_with("1 ") {
            if let Some(e) = parse_ordinary_entry(seg_str) {
                result.files.push(e);
            }
        } else if seg_str.starts_with("2 ") {
            // Rename: orig path is the NEXT NUL segment.
            let orig = if i + 1 < segments.len() {
                let s = String::from_utf8_lossy(segments[i + 1]).to_string();
                i += 1; // consume the extra segment
                s
            } else {
                String::new()
            };
            if let Some(e) = parse_rename_entry(seg_str, orig) {
                result.files.push(e);
            }
        } else if seg_str.starts_with("u ") {
            if let Some(e) = parse_unmerged_entry(seg_str) {
                result.files.push(e);
            }
        }
        // Ignore other prefixes (e.g., "!" ignored entries when --ignored is enabled).

        i += 1;
    }

    sort_file_entries(&mut result.files);

    Ok(result)
}

// ---------------------------------------------------------------------------
// Diff commands
// ---------------------------------------------------------------------------

/// SHA of git's canonical empty tree object. Git always recognizes this hash
/// even when the object isn't physically in the database. Diffing a tracked
/// file against it instead of `HEAD` produces a correct "all lines added" patch
/// on a fresh repo with an unborn HEAD, where `git diff HEAD` fails with
/// "fatal: bad revision 'HEAD'".
const EMPTY_TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn diff_args_for_file<'a>(file: &'a FileEntry, head_ref: &'a str, ignore_ws: bool) -> Vec<&'a str> {
    let untracked = matches!(file.status, FileStatus::New) && file.xy.starts_with('?');
    let mut args: Vec<&str> = vec![
        "diff",
        "--no-ext-diff",
        "--patch-with-raw",
        "--no-color",
    ];
    if untracked {
        args.push("--no-index");
        if ignore_ws {
            args.push("-w");
        }
        args.push("--");
        args.push("/dev/null");
        args.push(&file.path);
    } else {
        args.push(head_ref);
        if ignore_ws {
            args.push("-w");
        }
        args.push("--");
        args.push(&file.path);
    }
    args
}

/// Run a diff command. For untracked files, `git diff --no-index` exits with status 1
/// to signal "files differ", which is expected — we treat that as success.
fn run_diff(repo_path: &str, file: &FileEntry, ignore_ws: bool) -> Result<String, String> {
    // On a fresh repo (unborn HEAD) there is no `HEAD` to diff against, so fall
    // back to the empty tree and the staged/working file shows as fully added.
    let head_ref = if has_commits(repo_path) {
        "HEAD"
    } else {
        EMPTY_TREE_SHA
    };
    let args = diff_args_for_file(file, head_ref, ignore_ws);
    let arg_refs: Vec<&str> = args.iter().copied().collect();
    let output = git_cmd(repo_path, &arg_refs)
        .output()
        .map_err(|e| format!("git diff: {}", e))?;
    let untracked = matches!(file.status, FileStatus::New) && file.xy.starts_with('?');
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        return Ok(stdout);
    }
    // Untracked diffs return exit 1 when content differs — treat as success
    // as long as we got useful output.
    if untracked && !stdout.is_empty() {
        return Ok(stdout);
    }
    if let Some(code) = output.status.code() {
        if untracked && code == 1 {
            return Ok(stdout);
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("git diff failed: {}", stderr.trim()))
}

#[tauri::command]
pub fn get_head_sha(repo_path: String) -> Result<String, String> {
    match run_git(&repo_path, &["rev-parse", "HEAD"]) {
        Ok(sha) => Ok(sha),
        // Empty repo (no commits yet) — return empty string instead of error so
        // callers can use this as a cheap change-detection signal.
        Err(_) => Ok(String::new()),
    }
}

#[tauri::command]
pub fn get_diff(repo_path: String, file: FileEntry) -> Result<String, String> {
    run_diff(&repo_path, &file, false)
}

#[tauri::command]
pub fn get_diff_whitespace_ignored(repo_path: String, file: FileEntry) -> Result<String, String> {
    run_diff(&repo_path, &file, true)
}

#[tauri::command]
pub fn get_commit_diff(
    repo_path: String,
    sha: String,
    file_path: String,
) -> Result<String, String> {
    if file_path.is_empty() {
        // Full commit diff
        run_git(
            &repo_path,
            &[
                "log",
                &sha,
                "-1",
                "--first-parent",
                "-p",
                "--no-color",
                "--format=",
            ],
        )
    } else {
        // Per-file diff inside the commit. Using `git log` with `-p` produces a
        // proper unified diff (NOT the file contents that `git show {sha}:{path}`
        // returns).
        run_git(
            &repo_path,
            &[
                "log",
                &sha,
                "-1",
                "--first-parent",
                "-p",
                "--no-color",
                "--format=",
                "--",
                &file_path,
            ],
        )
    }
}

#[tauri::command]
pub fn get_selected_diff(repo_path: String, files: Vec<FileEntry>) -> Result<String, String> {
    if files.is_empty() {
        return Ok(String::new());
    }
    // Diff each file individually so untracked files (which need --no-index) are
    // handled correctly. Concatenate the results.
    let mut combined = String::new();
    for f in &files {
        if let Ok(d) = run_diff(&repo_path, f, false) {
            if !d.is_empty() {
                combined.push_str(&d);
                combined.push('\n');
            }
        }
    }
    Ok(combined)
}

// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

const LOG_FORMAT: &str = "%H%x01%h%x01%s%x01%b%x01%an%x01%ae%x01%ad%x01%cn%x01%ce%x01%cd%x01%P%x01%(trailers:unfold,only)%x01%D%x00";

#[tauri::command]
pub fn get_log(repo_path: String, opts: LogOptions) -> Result<Vec<CommitInfo>, String> {
    // A fresh repo with an unborn HEAD has no commits to show; `git log` would
    // fail with "does not have any commits yet" (exit 128). Treat it as an empty
    // history so the History tab renders its empty state instead of an error.
    if !has_commits(&repo_path) {
        return Ok(Vec::new());
    }

    let max_count = if opts.max_count <= 0 { 50 } else { opts.max_count };
    let max_arg = format!("--max-count={}", max_count);
    let skip_arg = format!("--skip={}", opts.skip);
    let format_arg = format!("--format={}", LOG_FORMAT);

    let bytes = run_git_raw(
        &repo_path,
        &[
            "log",
            "--date=raw",
            &max_arg,
            &skip_arg,
            &format_arg,
            "--no-show-signature",
            "--no-color",
            "--",
        ],
    )?;

    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let raw = String::from_utf8_lossy(&bytes).to_string();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let mut commits = Vec::new();
    for record in raw.split('\0') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.splitn(13, '\u{1}').collect();
        if fields.len() < 10 {
            continue;
        }

        let sha = fields[0].to_string();
        let short_sha = fields[1].to_string();
        let summary = fields[2].to_string();
        let body = fields[3].trim().to_string();
        let author_name = fields[4].to_string();
        let author_email = fields[5].to_string();
        let author_date = format_raw_date(fields[6]);
        let committer_name = fields[7].to_string();
        let _committer_email = fields[8].to_string();
        let committer_date = format_raw_date(fields[9]);

        let parents = if fields.len() > 10 {
            fields[10]
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        };

        let trailers: Vec<String> = if fields.len() > 11 {
            fields[11]
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        };

        let refs: Vec<String> = if fields.len() > 12 {
            let r = fields[12].trim();
            if r.is_empty() {
                Vec::new()
            } else {
                r.split(',').map(|s| s.trim().to_string()).collect()
            }
        } else {
            Vec::new()
        };

        commits.push(CommitInfo {
            sha,
            short_sha,
            summary,
            body,
            author_name,
            author_email,
            author_date,
            committer_name,
            committer_date,
            parents,
            trailers,
            refs,
        });
    }

    Ok(commits)
}

/// Format a git raw date (`<unix> <tz>`) as an ISO-8601 string.
/// We avoid pulling in chrono and instead emit a simple `YYYY-MM-DDTHH:MM:SS+ZZZZ`
/// representation built from the unix epoch and the supplied timezone offset.
fn format_raw_date(raw: &str) -> String {
    let raw = raw.trim();
    let mut parts = raw.splitn(2, ' ');
    let unix_str = parts.next().unwrap_or("");
    let tz = parts.next().unwrap_or("+0000");

    let unix: i64 = match unix_str.parse() {
        Ok(n) => n,
        Err(_) => return String::new(),
    };

    // Convert to seconds since epoch in the given timezone.
    let (tz_sign, tz_hours, tz_mins) = parse_tz(tz);
    let tz_offset_secs = tz_sign * (tz_hours * 3600 + tz_mins * 60);
    let local = unix + tz_offset_secs;

    let (y, mo, d, h, mi, s) = civil_from_unix(local);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
        y, mo, d, h, mi, s, tz
    )
}

fn parse_tz(tz: &str) -> (i64, i64, i64) {
    // tz like "+0500" or "-0830"
    let bytes = tz.as_bytes();
    if bytes.len() < 5 {
        return (1, 0, 0);
    }
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i64 = std::str::from_utf8(&bytes[1..3])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mins: i64 = std::str::from_utf8(&bytes[3..5])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (sign, hours, mins)
}

/// Convert a unix timestamp to a civil (Y, M, D, h, m, s) tuple using Howard Hinnant's
/// proleptic Gregorian algorithm. Avoids the chrono dependency.
fn civil_from_unix(unix: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let h = (secs / 3600) as u32;
    let mi = ((secs % 3600) / 60) as u32;
    let s = (secs % 60) as u32;

    // Days since 1970-01-01 -> civil date (Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, h, mi, s)
}

// ---------------------------------------------------------------------------
// Commit files
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_commit_files(repo_path: String, sha: String) -> Result<Vec<FileEntry>, String> {
    // `git log --first-parent` (not `diff-tree`) so merge commits diff against
    // their first parent and show their files — `diff-tree` emits nothing for a
    // merge unless given a combined-diff flag. This mirrors `get_commit_diff`,
    // keeping the file list, the per-file diff, and the stats badge in agreement.
    let output = run_git(
        &repo_path,
        &[
            "log",
            &sha,
            "-1",
            "--first-parent",
            "--format=",
            "--name-status",
            "--root",
            "--no-color",
        ],
    )?;

    let mut files = Vec::new();
    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let code = parts[0];
        let (status, path, orig) = if code.starts_with('R') {
            // R100\told\tnew
            if parts.len() < 3 {
                continue;
            }
            (
                FileStatus::Renamed,
                parts[2].to_string(),
                Some(parts[1].to_string()),
            )
        } else if code.starts_with('C') {
            // Copy: C<score>\told\tnew — treat as modified.
            if parts.len() < 3 {
                continue;
            }
            (FileStatus::Modified, parts[2].to_string(), None)
        } else {
            let status = match code {
                "A" => FileStatus::New,
                "D" => FileStatus::Deleted,
                "M" | "T" => FileStatus::Modified,
                _ => FileStatus::Modified,
            };
            (status, parts[1].to_string(), None)
        };

        let (display_name, display_dir) = extract_display_name_and_dir(&path);
        files.push(FileEntry {
            path,
            orig_path: orig,
            status,
            xy: code.to_string(),
            display_name,
            display_dir,
            embedded: false,
            submodule_dirty: false,
        });
    }

    sort_file_entries(&mut files);

    Ok(files)
}

/// Sums the added/removed line counts across every file in a commit so the
/// commit detail header can show a single `+N / -M` badge. Uses `--numstat`,
/// whose `<added>\t<deleted>\t<path>` lines parse cleanly; binary files report
/// `-` in both columns and are skipped. Like `get_commit_files`, it goes through
/// `git log --first-parent` so merge commits report their first-parent totals
/// rather than the empty output `diff-tree` gives for merges.
#[tauri::command]
pub fn get_commit_stats(repo_path: String, sha: String) -> Result<CommitStats, String> {
    let output = run_git(
        &repo_path,
        &[
            "log",
            &sha,
            "-1",
            "--first-parent",
            "--format=",
            "--numstat",
            "--root",
            "--no-color",
        ],
    )?;

    let mut additions: u32 = 0;
    let mut deletions: u32 = 0;
    for line in output.lines() {
        let mut cols = line.split('\t');
        let added = cols.next();
        let deleted = cols.next();
        if let (Some(a), Some(d)) = (added, deleted) {
            // Binary files show `-`; `parse` fails and we skip them.
            if let (Ok(a), Ok(d)) = (a.parse::<u32>(), d.parse::<u32>()) {
                additions = additions.saturating_add(a);
                deletions = deletions.saturating_add(d);
            }
        }
    }

    Ok(CommitStats {
        additions,
        deletions,
    })
}

/// Shared ordering for any file-list panel (working-tree status, commit
/// details, future selection lists). Two-key comparison:
///   1. Root-level files (no `/` in the path) come before any nested file.
///      Mental model: treat the repo root as `.`, which sorts before any
///      directory name. So `README.md` lands at the top, then everything
///      under `blackbox-e2e/`, `desktop/`, etc.
///   2. Within each group, case-insensitive path order so the list reads
///      like Finder / VS Code / GitHub.com instead of git's byte-sorted
///      output (which puts uppercase names ahead of lowercase ones, and
///      dot-prefixed dirs before everything else).
fn sort_file_entries(files: &mut [FileEntry]) {
    files.sort_by(|a, b| {
        let a_root = !a.path.contains('/');
        let b_root = !b.path.contains('/');
        match (a_root, b_root) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
        }
    });
}

// ---------------------------------------------------------------------------
// Branches
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_branches(repo_path: String) -> Result<Vec<BranchInfo>, String> {
    // Use for-each-ref so we can compute is_remote precisely from the full refname
    // and detect HEAD pointers via the "->" substring in the short refname.
    let output = run_git(
        &repo_path,
        &[
            "for-each-ref",
            "refs/heads",
            "refs/remotes",
            "--sort=-committerdate",
            "--format=%(refname:short)|%(HEAD)|%(refname)",
        ],
    )?;

    let mut branches = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].trim().to_string();
        if name.is_empty() {
            continue;
        }
        // Skip HEAD pointer entries like "origin/HEAD -> origin/main".
        if name.contains(" -> ") {
            continue;
        }
        let is_current = parts[1].trim() == "*";
        let full_ref = parts[2].trim();
        let is_remote = full_ref.starts_with("refs/remotes/");

        branches.push(BranchInfo {
            name,
            is_remote,
            is_current,
        });
    }

    Ok(branches)
}

#[tauri::command]
pub fn create_branch(repo_path: String, name: String, start_point: String) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["branch", &name];
    if !start_point.is_empty() {
        args.push(&start_point);
    }
    run_git(&repo_path, &args)?;
    Ok(())
}

/// Whether a local branch (`refs/heads/<name>`) exists. `show-ref --quiet`
/// exits non-zero when the ref is missing, which `run_git` surfaces as `Err`.
fn local_branch_exists(repo_path: &str, name: &str) -> bool {
    run_git(
        repo_path,
        &["show-ref", "--verify", "--quiet", &format!("refs/heads/{name}")],
    )
    .is_ok()
}

/// Whether a remote-tracking branch (`refs/remotes/<name>`) exists, where
/// `name` is the short form shown in the UI (e.g. `origin/feature`).
fn remote_branch_exists(repo_path: &str, name: &str) -> bool {
    run_git(
        repo_path,
        &["show-ref", "--verify", "--quiet", &format!("refs/remotes/{name}")],
    )
    .is_ok()
}

#[tauri::command]
pub fn switch_branch(repo_path: String, branch: String) -> Result<(), String> {
    // A remote-only branch (e.g. `origin/feature`) has to become a local tracking
    // branch — `git checkout origin/feature --` would otherwise treat the ref as a
    // commit-ish and land us in detached HEAD. Guarding on "not already a local
    // branch" means a local branch whose name legitimately contains a slash
    // (`feature/foo`) is never misread as a remote ref.
    if !local_branch_exists(&repo_path, &branch) && remote_branch_exists(&repo_path, &branch) {
        return checkout_tracking_branch(&repo_path, &branch);
    }
    run_git(&repo_path, &["checkout", &branch, "--"])?;
    Ok(())
}

/// Check out a remote branch (`origin/feature`) as a local tracking branch. The
/// local name drops the remote prefix (`origin/feature` -> `feature`,
/// `origin/team/x` -> `team/x`), matching what `git switch <name>`'s DWIM does.
/// If a local branch of that name already exists it's switched to as-is instead
/// of recreated (which would fail), so clicking a second remote's same-named
/// branch simply reuses the existing local branch.
fn checkout_tracking_branch(repo_path: &str, remote_branch: &str) -> Result<(), String> {
    let local_name = remote_branch
        .split_once('/')
        .map_or(remote_branch, |(_, rest)| rest);

    if local_branch_exists(repo_path, local_name) {
        run_git(repo_path, &["checkout", local_name, "--"])?;
    } else {
        run_git(
            repo_path,
            &["checkout", "-b", local_name, "--track", remote_branch],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_branch(repo_path: String, name: String) -> Result<(), String> {
    run_git(&repo_path, &["branch", "-D", &name])?;
    Ok(())
}

#[tauri::command(async)]
pub fn delete_remote_branch(
    repo_path: String,
    remote: String,
    branch: String,
) -> Result<(), String> {
    let refspec = format!(":{}", branch);
    let (ok, combined) = run_git_net(
        Some(&repo_path),
        &["push", &remote, &refspec],
        NET_UI_CONNECT_SECS,
        NET_UI_STALL_SECS,
        NET_UI_TIMEOUT,
    )?;
    if !ok {
        return Err(format!("git push failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn rename_branch(repo_path: String, old_name: String, new_name: String) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["branch", "-m"];
    if !old_name.is_empty() {
        args.push(&old_name);
    }
    args.push(&new_name);
    run_git(&repo_path, &args)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------

fn update_index(repo_path: &str, paths: &[String], force_remove: bool) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["update-index", "--add", "--remove"];
    if force_remove {
        args.push("--force-remove");
    }
    args.extend(["--replace", "-z", "--stdin"]);

    let mut child = git_cmd(repo_path, &args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git update-index: {}", e))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open stdin".to_string())?;
        let mut buf = Vec::new();
        for p in paths {
            buf.extend_from_slice(p.as_bytes());
            buf.push(0);
        }
        stdin
            .write_all(&buf)
            .map_err(|e| format!("write stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("git update-index wait: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git update-index failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn stage_files(repo_path: &str, files: &[FileEntry]) -> Result<(), String> {
    let mut renamed_old: Vec<String> = Vec::new();
    let mut normal: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();

    for f in files {
        match f.status {
            FileStatus::Renamed => {
                if let Some(orig) = &f.orig_path {
                    renamed_old.push(orig.clone());
                }
                normal.push(f.path.clone());
            }
            FileStatus::Deleted => deleted.push(f.path.clone()),
            _ => normal.push(f.path.clone()),
        }
    }

    // Removals (rename source paths, deletions) go through update-index, whose
    // --force-remove is the precise tool for dropping an index entry.
    update_index(repo_path, &renamed_old, true)?;
    // Additions/modifications go through `git add`, not update-index. update-index
    // silently ignores any path that resolves to a directory ("Ignoring path …/"),
    // which left embedded git repositories (nested repos git reports as a single
    // directory entry) unstaged and surfaced the misleading "staging produced no
    // changes" error. `git add` stages a directory's files and an embedded repo as
    // a gitlink, matching the git CLI.
    git_add(repo_path, &normal)?;
    update_index(repo_path, &deleted, true)?;
    Ok(())
}

/// Stage the given paths as additions/modifications via porcelain `git add`.
///
/// Paths are piped NUL-separated through `--pathspec-from-file` to sidestep
/// arg-length and quoting limits, mirroring [`update_index`]. Embedded-repo
/// advice is silenced (`advice.addEmbeddedRepo=false`) because the UI already
/// explains the gitlink before the user gets here.
fn git_add(repo_path: &str, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut child = git_cmd(
        repo_path,
        &[
            "-c",
            "advice.addEmbeddedRepo=false",
            "add",
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("git add: {e}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open stdin".to_string())?;
        let mut buf = Vec::new();
        for p in paths {
            buf.extend_from_slice(p.as_bytes());
            buf.push(0);
        }
        stdin
            .write_all(&buf)
            .map_err(|e| format!("write stdin: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("git add wait: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn commit(
    repo_path: String,
    message: String,
    files: Vec<FileEntry>,
    amend: Option<bool>,
) -> Result<(), String> {
    let amend = amend.unwrap_or(false);

    // When amending you can change just the message; an empty file list is fine.
    if files.is_empty() && !amend {
        return Err("no files selected — select files first".to_string());
    }

    // Clear the staging area so only what the user selected gets committed.
    // A pathspec reset (`reset -- .`) is used instead of `reset HEAD` so this
    // also works on a fresh repo whose HEAD is still unborn — `reset HEAD` fails
    // there with "ambiguous argument 'HEAD'". The repo root is the cwd, so `.`
    // covers the whole index, matching `reset HEAD` on repos that have commits.
    let reset = git_cmd(&repo_path, &["reset", "--", "."])
        .output()
        .map_err(|e| format!("git reset: {}", e))?;
    if !reset.status.success() {
        return Err(format!(
            "git reset failed: {}",
            String::from_utf8_lossy(&reset.stderr).trim()
        ));
    }

    stage_files(&repo_path, &files)?;

    // For a non-amend commit, refusing an empty index is the right call.
    // For an amend, an empty index is the "message only" path and must succeed.
    if !amend && !has_staged_changes(repo_path.clone())? {
        return Err("staging produced no changes".to_string());
    }

    // Pipe the message via stdin to avoid arg-length and shell-quoting issues.
    let mut args: Vec<&str> = vec!["commit", "-F", "-"];
    if amend {
        args.push("--amend");
    }
    let mut child = git_cmd(&repo_path, &args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git commit: {}", e))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open stdin".to_string())?;
        stdin
            .write_all(message.as_bytes())
            .map_err(|e| format!("write stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("git commit wait: {}", e))?;

    if !output.status.success() {
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        return Err(format!("git commit failed: {}", combined.trim()));
    }
    Ok(())
}

/// Undo the most recent commit on the current branch.
///
/// Uses `git reset --mixed HEAD~1`: HEAD moves back to the parent, the index
/// is reset to match (unstaging anything that was staged), and the working
/// tree is left untouched. The undone commit's changes therefore re-appear as
/// unstaged modifications, ready for the user to edit and re-commit. Matches
/// what `git commit` would have set up if the user had never committed.
///
/// Refuses to undo the initial commit (no parent to reset to) — that path
/// requires `git update-ref -d HEAD` plus an index rebuild, which we don't
/// support yet.
#[tauri::command]
pub fn undo_last_commit(repo_path: String) -> Result<(), String> {
    // Verify HEAD has a parent before attempting the reset. `rev-parse HEAD~1`
    // fails on the initial commit with a clear error message we can surface.
    let parent = git_cmd(&repo_path, &["rev-parse", "--verify", "--quiet", "HEAD~1"])
        .output()
        .map_err(|e| format!("git rev-parse: {}", e))?;
    if !parent.status.success() {
        return Err("cannot undo the initial commit".to_string());
    }

    let (ok, combined) = run_git_combined(&repo_path, &["reset", "--mixed", "HEAD~1"])?;
    if !ok {
        return Err(format!("git reset --mixed HEAD~1 failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn has_staged_changes(repo_path: String) -> Result<bool, String> {
    // `git diff --cached --quiet` exits 0 when there are no staged changes,
    // 1 when there are. Any other exit code is a real error.
    let output = git_cmd(&repo_path, &["diff", "--cached", "--quiet"])
        .output()
        .map_err(|e| format!("git diff --cached: {}", e))?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(code) => Err(format!(
            "git diff --cached --quiet exited with status {}: {}",
            code,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        None => Err("git diff --cached --quiet terminated by signal".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Discard / ignore (Changes-tab context menu)
// ---------------------------------------------------------------------------

/// The subset of `paths` that exist as blobs in the `HEAD` tree. Empty on an
/// unborn HEAD (a repo with no commits yet) or when nothing matches.
///
/// Used by [`discard_files`] to tell tracked files (restore from HEAD) from
/// files git has never committed — untracked, freshly `git add`-ed, or the new
/// side of a rename — which have no HEAD version to fall back to and must be
/// removed instead. `-r` makes the pathspecs resolve to nested blobs; `-z`
/// keeps paths with spaces/newlines intact.
fn head_paths(repo_path: &str, paths: &[String]) -> HashSet<String> {
    if paths.is_empty() || !has_commits(repo_path) {
        return HashSet::new();
    }
    let mut args: Vec<&str> = vec!["ls-tree", "-r", "-z", "--name-only", "HEAD", "--"];
    args.extend(paths.iter().map(String::as_str));
    match run_git_raw(repo_path, &args) {
        Ok(bytes) => bytes
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
        Err(_) => HashSet::new(),
    }
}

/// Discard the working-tree changes of `files`, restoring the repo to its
/// committed state for each.
///
/// Tracked files (modified, deleted, conflicted, and the original side of a
/// rename) are restored from `HEAD` in both the index and the working tree.
/// Files with no committed version (untracked, staged additions, and the new
/// side of a rename) can't be "reverted", so their working-tree copy is moved
/// to the OS trash — recoverable, unlike `rm` — and any staged entry is
/// dropped from the index. Runs on a worker thread so a large discard never
/// blocks the UI.
///
/// # Errors
/// Returns `Err` if the underlying `git reset` / `git checkout` fails. A file
/// that can't be moved to the trash (already gone, permissions) is logged and
/// skipped rather than aborting the whole operation.
#[tauri::command]
pub fn discard_files(repo_path: &str, files: Vec<FileEntry>) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    // Every path whose HEAD membership decides how we discard it: the path
    // itself plus the pre-rename original.
    let mut candidates: Vec<String> = Vec::new();
    for f in &files {
        candidates.push(f.path.clone());
        if let Some(orig) = &f.orig_path {
            candidates.push(orig.clone());
        }
    }
    let in_head = head_paths(repo_path, &candidates);

    // `restore` → tracked paths to `git checkout HEAD --`.
    // `trash_and_unstage` → never-committed paths to trash + `git reset --`.
    let mut restore: Vec<String> = Vec::new();
    let mut trash_and_unstage: Vec<String> = Vec::new();
    for f in files {
        match f.orig_path {
            // Rename: bring back the committed original, drop the new path.
            Some(orig) if in_head.contains(&orig) => {
                restore.push(orig);
                trash_and_unstage.push(f.path);
            }
            _ if in_head.contains(&f.path) => restore.push(f.path),
            _ => trash_and_unstage.push(f.path),
        }
    }

    // 1) Move never-committed working-tree files to the trash so an accidental
    //    discard is recoverable. Best-effort per file.
    for rel in &trash_and_unstage {
        let abs = Path::new(repo_path).join(rel);
        if abs.exists() {
            if let Err(e) = trash::delete(&abs) {
                eprintln!("discard_files: could not trash {}: {e}", abs.display());
            }
        }
    }

    // 2) Drop any staged additions from the index. The pathspec form (no
    //    `HEAD`) is unborn-HEAD safe, matching `commit`'s reset above; it's a
    //    harmless no-op for purely untracked paths.
    if !trash_and_unstage.is_empty() {
        let mut args: Vec<&str> = vec!["reset", "--"];
        args.extend(trash_and_unstage.iter().map(String::as_str));
        let (ok, out) = run_git_combined(repo_path, &args)?;
        if !ok {
            return Err(format!("git reset failed: {}", out.trim()));
        }
    }

    // 3) Restore tracked files to their committed state (index + worktree).
    if !restore.is_empty() {
        let mut args: Vec<&str> = vec!["checkout", "HEAD", "--"];
        args.extend(restore.iter().map(String::as_str));
        let (ok, out) = run_git_combined(repo_path, &args)?;
        if !ok {
            return Err(format!("git checkout failed: {}", out.trim()));
        }
    }

    Ok(())
}

/// Append `patterns` to the repo's root `.gitignore`, one per line, creating
/// the file if absent. Lines already present (compared trimmed) are skipped so
/// repeated "Ignore" clicks don't pile up duplicates.
///
/// Callers pass ready-to-write patterns: a literal file path (with its glob
/// metacharacters escaped) or a glob like `*.log`. A trailing newline is
/// ensured before appending so existing rules aren't joined onto.
///
/// # Errors
/// Returns `Err` if the `.gitignore` file can't be written.
#[tauri::command]
pub fn append_to_gitignore(repo_path: &str, patterns: Vec<String>) -> Result<(), String> {
    if patterns.is_empty() {
        return Ok(());
    }
    let path = Path::new(repo_path).join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    // Skip patterns already on a line of the file, and de-dup within this batch
    // (e.g. two selected files sharing a name).
    let present: HashSet<&str> = existing.lines().map(str::trim).collect();
    let mut to_add: Vec<String> = Vec::new();
    for p in patterns {
        let trimmed = p.trim();
        if trimmed.is_empty()
            || present.contains(trimmed)
            || to_add.iter().any(|a| a == trimmed)
        {
            continue;
        }
        to_add.push(trimmed.to_string());
    }
    if to_add.is_empty() {
        return Ok(());
    }

    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for p in to_add {
        out.push_str(&p);
        out.push('\n');
    }
    std::fs::write(&path, out).map_err(|e| format!("write .gitignore: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn format_commit_message(
    summary: String,
    description: String,
    co_authors: Vec<String>,
) -> String {
    let mut parts: Vec<String> = vec![summary];

    if !description.is_empty() || !co_authors.is_empty() {
        parts.push(String::new());
    }

    if !description.is_empty() {
        parts.push(description.clone());
    }

    if !co_authors.is_empty() {
        if !description.is_empty() {
            parts.push(String::new());
        }
        for author in &co_authors {
            parts.push(format!("Co-authored-by: {}", author));
        }
    }

    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Fetch / pull / push / ahead-behind / remote
// ---------------------------------------------------------------------------

/// Background sync for the repo picker's pull/push badges. Optionally fetches
/// the repo's first remote (best-effort — network errors are swallowed so a
/// stale-but-known ahead/behind still comes back), then computes the current
/// branch's ahead/behind. Deliberately lighter than `get_status`: it skips the
/// untracked-file scan (`-uno`) and never lists files, since the picker only
/// needs the two counts for many repos at a time.
#[tauri::command(async)]
pub fn repo_sync_status(repo_path: String, do_fetch: bool) -> Result<RepoSync, String> {
    let remote = first_remote(&repo_path);

    // Best-effort, time-boxed fetch. A failure (offline, auth, timeout) must not
    // blank the badge — we fall through and report ahead/behind from whatever
    // refs we already have. `--prune` keeps deleted remote branches from
    // lingering. `fetched` records whether we actually reached the remote so the
    // frontend's circuit breaker can back off after a run of failures.
    let mut fetched = true;
    if do_fetch {
        if let Some(remote) = remote.as_deref() {
            fetched = run_git_net(
                Some(&repo_path),
                &[
                    "fetch",
                    "--prune",
                    "--recurse-submodules=on-demand",
                    remote,
                ],
                NET_BG_CONNECT_SECS,
                NET_BG_STALL_SECS,
                NET_BG_TIMEOUT,
            )
            .is_ok_and(|(ok, _)| ok);
        }
    }

    let mut sync = RepoSync {
        ahead: 0,
        behind: 0,
        has_remote: remote.is_some(),
        fetched,
    };

    // Headers only: branch.head, branch.upstream, branch.ab. `-uno` skips the
    // potentially expensive untracked-file walk we don't need here.
    let bytes = run_git_raw(
        &repo_path,
        &[
            "--no-optional-locks",
            "status",
            "--untracked-files=no",
            "--branch",
            "--porcelain=2",
            "-z",
        ],
    )?;

    // Under `-z` the header records are NUL-terminated (no newlines in the
    // output), so walk them by splitting on NUL — same as get_status.
    let mut branch = String::new();
    let mut has_upstream = false;
    let mut rest: &[u8] = &bytes;
    while let Some(sep) = rest.iter().position(|&b| b == b'\0') {
        let record = &rest[..sep];
        if !record.starts_with(b"# ") {
            break;
        }
        let line_str = String::from_utf8_lossy(record);
        if let Some(rem) = line_str.strip_prefix("# branch.head ") {
            let val = rem.trim();
            branch = if val == "(detached)" {
                String::new()
            } else {
                val.to_string()
            };
        } else if line_str.starts_with("# branch.upstream ") {
            has_upstream = true;
        } else if let Some(rem) = line_str.strip_prefix("# branch.ab ") {
            let parts: Vec<&str> = rem.split_whitespace().collect();
            if parts.len() == 2 {
                sync.ahead = parts[0].trim_start_matches('+').parse().unwrap_or(0);
                sync.behind = parts[1].trim_start_matches('-').parse().unwrap_or(0);
            }
        }
        rest = &rest[sep + 1..];
    }

    // Same no-upstream fallback as get_status: a branch never `push -u`'d emits
    // no `# branch.ab`, so compare against the remote-tracking ref directly.
    if !has_upstream && !branch.is_empty() {
        if let Some(remote) = remote.as_deref() {
            if let Some((ahead, behind, _)) =
                remote_tracking_ahead_behind(&repo_path, remote, &branch)
            {
                sync.ahead = ahead;
                sync.behind = behind;
            }
        }
    }

    Ok(sync)
}

#[tauri::command(async)]
pub fn fetch(repo_path: String, remote: String) -> Result<(), String> {
    let (ok, combined) = run_git_net(
        Some(&repo_path),
        &[
            "fetch",
            "--prune",
            "--recurse-submodules=on-demand",
            &remote,
        ],
        NET_UI_CONNECT_SECS,
        NET_UI_STALL_SECS,
        NET_UI_TIMEOUT,
    )?;
    if !ok {
        return Err(format!("git fetch failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn pull(repo_path: String, remote: String) -> Result<(), String> {
    let (ok, combined) = run_git_net(
        Some(&repo_path),
        &["pull", "--ff", "--recurse-submodules", &remote],
        NET_UI_CONNECT_SECS,
        NET_UI_STALL_SECS,
        NET_UI_TIMEOUT,
    )?;
    if !ok {
        return Err(format!("git pull failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn push(
    repo_path: String,
    remote: String,
    branch: String,
    set_upstream: bool,
    force_with_lease: bool,
) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["push", "--progress"];
    if set_upstream {
        args.push("--set-upstream");
    }
    if force_with_lease {
        args.push("--force-with-lease");
    }
    args.push(&remote);
    args.push(&branch);

    let (ok, combined) = run_git_net(
        Some(&repo_path),
        &args,
        NET_UI_CONNECT_SECS,
        NET_UI_STALL_SECS,
        NET_UI_TIMEOUT,
    )?;
    if !ok {
        return Err(format!("git push failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn get_ahead_behind(repo_path: String, upstream: String) -> Result<AheadBehind, String> {
    if upstream.is_empty() {
        return Ok(AheadBehind { ahead: 0, behind: 0 });
    }
    let range = format!("HEAD...{}", upstream);
    let output = run_git(
        &repo_path,
        &["rev-list", "--left-right", "--count", &range, "--"],
    )?;

    // Output: "<ahead>\t<behind>\n" — left side is HEAD, right side is upstream
    // (this is the inverse of what the old code assumed). See Go reference:
    // ahead = HEAD-only count, behind = upstream-only count.
    let parts: Vec<&str> = output.split_whitespace().collect();
    if parts.len() == 2 {
        let ahead: i32 = parts[0].parse().unwrap_or(0);
        let behind: i32 = parts[1].parse().unwrap_or(0);
        Ok(AheadBehind { ahead, behind })
    } else {
        Ok(AheadBehind { ahead: 0, behind: 0 })
    }
}

#[tauri::command]
pub fn get_remote(repo_path: String) -> Result<String, String> {
    // Return the NAME of the first remote, not the URL.
    let out = run_git(&repo_path, &["remote"])?;
    if let Some(first) = out.lines().next() {
        let first = first.trim();
        if !first.is_empty() {
            return Ok(first.to_string());
        }
    }
    // If no remotes are configured but there's exactly one "origin"-shaped default,
    // fall back to "origin".
    Ok("origin".to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoIdentifier {
    pub owner: String,
    pub name: String,
}

/// Parse `owner/repo` out of a typical git remote URL. Handles SSH
/// (`git@host:owner/repo`), HTTPS (`https://host/owner/repo`), and
/// scheme://user@host/owner/repo forms, all with optional `.git` suffix.
/// Strips `.git` and discards any extra path segments — we take the LAST
/// two non-empty path parts.
fn parse_owner_repo(url: &str) -> Option<RepoIdentifier> {
    let u = url.trim();
    if u.is_empty() {
        return None;
    }
    let u = u.strip_suffix(".git").unwrap_or(u);
    let u = u.strip_suffix('/').unwrap_or(u);

    // SCP-style SSH: `git@github.com:owner/repo`
    if !u.contains("://") {
        if let Some((_user_host, path)) = u.split_once(':') {
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                return Some(RepoIdentifier {
                    owner: parts[parts.len() - 2].to_string(),
                    name: parts[parts.len() - 1].to_string(),
                });
            }
        }
    }

    // scheme://[user@]host[:port]/owner/repo
    let after_scheme = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let after_user = after_scheme
        .split_once('@')
        .map(|(_, r)| r)
        .unwrap_or(after_scheme);
    let (_host, path) = after_user.split_once('/')?;
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        return Some(RepoIdentifier {
            owner: parts[parts.len() - 2].to_string(),
            name: parts[parts.len() - 1].to_string(),
        });
    }
    None
}

/// Returns the owner/name pair parsed from `remote.origin.url`, or null when
/// the repo has no `origin` remote or the URL can't be parsed as `owner/repo`.
/// Falls back through the first available remote if `origin` is missing.
#[tauri::command]
pub fn get_repo_identifier(repo_path: String) -> Option<RepoIdentifier> {
    // Try origin first — that's the convention.
    if let Ok(url) = run_git(&repo_path, &["config", "--get", "remote.origin.url"]) {
        if let Some(id) = parse_owner_repo(&url) {
            return Some(id);
        }
    }
    // Fall back to the first remote available.
    if let Ok(remotes) = run_git(&repo_path, &["remote"]) {
        for r in remotes.lines() {
            let r = r.trim();
            if r.is_empty() {
                continue;
            }
            let key = format!("remote.{}.url", r);
            if let Ok(url) = run_git(&repo_path, &["config", "--get", &key]) {
                if let Some(id) = parse_owner_repo(&url) {
                    return Some(id);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// Collect unique paths from `git ls-files --unmerged`. Output format is:
///   <mode> <sha> <stage>\t<path>
/// We split on tab and take the trailing column 4 (path).
fn ls_files_unmerged(repo_path: &str) -> Vec<String> {
    let output = match run_git(repo_path, &["ls-files", "--unmerged"]) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<String> = Vec::new();
    for line in output.lines() {
        if let Some(tab) = line.find('\t') {
            let path = line[tab + 1..].trim().to_string();
            if !path.is_empty() && seen.insert(path.clone()) {
                ordered.push(path);
            }
        }
    }
    ordered
}

#[tauri::command]
pub fn merge_branch(repo_path: String, branch: String) -> Result<MergeResult, String> {
    let (ok, combined) = run_git_combined(&repo_path, &["merge", "--no-edit", &branch])?;
    if ok {
        let ff = combined.contains("Fast-forward") || combined.contains("fast-forward");
        return Ok(MergeResult {
            success: true,
            fast_forward: ff,
            conflicts: Vec::new(),
            error_message: None,
        });
    }

    let conflicts = ls_files_unmerged(&repo_path);
    Ok(MergeResult {
        success: false,
        fast_forward: false,
        conflicts,
        error_message: Some(combined.trim().to_string()),
    })
}

#[tauri::command]
pub fn merge_squash(repo_path: String, branch: String) -> Result<MergeResult, String> {
    let (ok, combined) = run_git_combined(&repo_path, &["merge", "--squash", &branch])?;
    if ok {
        return Ok(MergeResult {
            success: true,
            fast_forward: false,
            conflicts: Vec::new(),
            error_message: None,
        });
    }
    let conflicts = ls_files_unmerged(&repo_path);
    Ok(MergeResult {
        success: false,
        fast_forward: false,
        conflicts,
        error_message: Some(combined.trim().to_string()),
    })
}

#[tauri::command]
pub fn commit_squash_merge(repo_path: String) -> Result<(), String> {
    // --no-edit keeps the auto-generated MERGE_MSG (preserves the "Squashed commit
    // of the following:" body); --cleanup=strip trims trailing whitespace.
    let (ok, combined) =
        run_git_combined(&repo_path, &["commit", "--no-edit", "--cleanup=strip"])?;
    if !ok {
        return Err(format!("git commit failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn merge_abort(repo_path: String) -> Result<(), String> {
    let (ok, combined) = run_git_combined(&repo_path, &["merge", "--abort"])?;
    if !ok {
        return Err(format!("git merge --abort failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn is_merging(repo_path: String) -> Result<bool, String> {
    // Use rev-parse --git-dir to handle both regular .git directories AND
    // .git files used by worktrees (where .git is a text file pointing to
    // the real git dir).
    let git_dir = match run_git(&repo_path, &["rev-parse", "--git-dir"]) {
        Ok(d) => d,
        Err(_) => return Ok(false),
    };

    let git_dir_path = if Path::new(&git_dir).is_absolute() {
        PathBuf::from(&git_dir)
    } else {
        Path::new(&repo_path).join(&git_dir)
    };

    Ok(git_dir_path.join("MERGE_HEAD").exists())
}

#[tauri::command]
pub fn count_commits_to_merge(repo_path: String, target_branch: String) -> Result<i32, String> {
    let base = run_git(&repo_path, &["merge-base", "HEAD", &target_branch])?;
    let range = format!("{}..{}", base.trim(), target_branch);
    let output = run_git(&repo_path, &["rev-list", "--count", &range])?;
    output
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Failed to parse commit count: {}", e))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if !path.starts_with('~') {
        return PathBuf::from(path);
    }
    let home = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return PathBuf::from(path),
    };
    if path == "~" {
        return home;
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Resolve a clone destination: expand a leading `~`, reject a path that
/// already exists (git/gh would fail anyway, but we give a friendlier error),
/// and create the parent folder so the clone has somewhere to land. Returns the
/// absolute target path the clone should write to. Shared by `clone_repo`
/// (URL clones) and `gh::gh_clone` (GitHub clones) so both behave identically.
pub fn prepare_clone_target(target_path: &str) -> Result<String, String> {
    let target = expand_tilde(target_path);
    if target.exists() {
        return Err(format!("\"{}\" already exists.", target.display()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create destination folder: {e}"))?;
    }
    Ok(target.to_string_lossy().to_string())
}

/// Clone an arbitrary git URL into `target_path`. The URL tab of the Clone
/// dialog uses this; `GIT_TERMINAL_PROMPT=0` keeps a private/unauthenticated
/// clone from hanging on a credential prompt and instead surfaces the error.
/// Returns the absolute path of the freshly cloned repo so the UI can open it.
#[tauri::command(async)]
pub fn clone_repo(url: String, target_path: String) -> Result<String, String> {
    let target = prepare_clone_target(&target_path)?;
    // No `current_dir` — the repo doesn't exist yet; clone writes to `target`.
    let (ok, combined) = run_git_net(
        None,
        &["clone", "--progress", &url, &target],
        NET_UI_CONNECT_SECS,
        NET_UI_STALL_SECS,
        NET_UI_TIMEOUT,
    )?;
    if !ok {
        return Err(combined.trim().to_string());
    }
    Ok(target)
}

/// Unix timestamp (seconds) of a repo's most recent commit, or 0 when it has
/// none / isn't readable. Powers the repo picker's "recently modified" sort;
/// returns 0 rather than erroring so one bad repo never breaks the sort.
#[tauri::command]
#[must_use]
pub fn get_last_commit_timestamp(repo_path: String) -> i64 {
    run_git(&repo_path, &["log", "-1", "--format=%ct"])
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

fn scan_for_repos(
    dir: &Path,
    root: &Path,
    max_depth: u32,
    seen: &mut HashSet<String>,
    repos: &mut Vec<String>,
) {
    let depth = match dir.strip_prefix(root) {
        Ok(rel) => {
            if rel.as_os_str().is_empty() {
                0
            } else {
                rel.components().count() as u32
            }
        }
        Err(_) => 0,
    };
    if depth > max_depth {
        return;
    }

    if depth > 0 && is_git_repo_path(dir) {
        if let Ok(abs) = std::fs::canonicalize(dir) {
            let abs_str = abs.to_string_lossy().to_string();
            if seen.insert(abs_str.clone()) {
                repos.push(abs_str);
            }
        } else if let Some(s) = dir.to_str() {
            let owned = s.to_string();
            if seen.insert(owned.clone()) {
                repos.push(owned);
            }
        }
        return; // do not descend into a repo
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip hidden directories.
        if name_str.starts_with('.') {
            continue;
        }
        let full = entry.path();
        // fs::metadata follows symlinks (unlike symlink_metadata).
        let meta = match std::fs::metadata(&full) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_dir() {
            continue;
        }
        scan_for_repos(&full, root, max_depth, seen, repos);
    }
}

#[tauri::command]
pub fn discover_repos(scan_paths: Vec<String>, max_depth: u32) -> Result<Vec<String>, String> {
    let mut repos: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for scan_path in scan_paths {
        let expanded = expand_tilde(&scan_path);
        let abs = match std::fs::canonicalize(&expanded) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let meta = match std::fs::metadata(&abs) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_dir() {
            continue;
        }
        // If the scan path itself is a git repo, include it.
        if is_git_repo_path(&abs) {
            let abs_str = abs.to_string_lossy().to_string();
            if seen.insert(abs_str.clone()) {
                repos.push(abs_str);
            }
            continue;
        }
        scan_for_repos(&abs, &abs, max_depth, &mut seen, &mut repos);
    }

    repos.sort();
    Ok(repos)
}

pub fn is_git_repo_path(path: &Path) -> bool {
    let dotgit = path.join(".git");
    // .git can be a directory (normal repo) or a regular file (worktree).
    match std::fs::metadata(&dotgit) {
        Ok(_) => true,
        Err(_) => false,
    }
}

#[tauri::command]
pub fn is_git_repo(path: &str) -> bool {
    is_git_repo_path(Path::new(path))
}

#[tauri::command]
pub fn get_repo_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Initialise a throwaway repo with a committer identity. Local config
    /// disables commit signing so the tests don't depend on the developer's
    /// global git setup.
    fn init_repo(dir: &Path) {
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .current_dir(dir)
                .args(args)
                .status()
                .expect("spawn git")
                .success();
            assert!(ok, "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test User"]);
        git(&["config", "commit.gpgsign", "false"]);
    }

    fn new_file(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            orig_path: None,
            status: FileStatus::New,
            xy: "A.".to_string(),
            display_name: path.to_string(),
            display_dir: String::new(),
            embedded: false,
            submodule_dirty: false,
        }
    }

    fn default_log_opts() -> LogOptions {
        LogOptions {
            max_count: 50,
            skip: 0,
        }
    }

    /// Regression: the first commit on a fresh repo (unborn HEAD) must succeed.
    /// It used to fail because the index reset ran `git reset HEAD`, which errors
    /// when HEAD doesn't exist yet. Also verifies the selective-commit guarantee
    /// still holds — only the file the user selected is committed.
    #[test]
    fn commit_succeeds_on_fresh_repo_and_only_commits_selected_files() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        fs::write(repo.join("README.md"), "hello\n").expect("write README");
        fs::write(repo.join(".gitignore"), "node_modules\n").expect("write gitignore");
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        commit(
            repo_path.clone(),
            "Create initial script".to_string(),
            vec![new_file("README.md")],
            None,
        )
        .expect("first commit on a fresh repo should succeed");

        // Only README.md was committed; .gitignore stays untracked.
        let tracked = run_git(&repo_path, &["ls-files"]).expect("ls-files");
        assert_eq!(tracked.lines().collect::<Vec<_>>(), vec!["README.md"]);
    }

    /// Regression: an embedded git repository (a nested repo with its own
    /// `.git`) is reported by `get_status` as a single `embedded` directory
    /// entry, and committing it stages a gitlink (mode 160000) instead of
    /// failing with "staging produced no changes". The failure came from
    /// `update-index --add` silently ignoring the directory ("Ignoring path …/");
    /// additions now go through `git add`, which creates the gitlink.
    #[test]
    fn commits_embedded_repo_as_gitlink() {
        let tmp = tempdir().expect("tempdir");
        let outer = tmp.path();
        init_repo(outer);
        let outer_path = outer.to_str().expect("utf-8 path").to_string();

        // Born HEAD on the outer repo keeps the test focused on the gitlink path.
        fs::write(outer.join("README.md"), "outer\n").expect("write README");
        commit(
            outer_path.clone(),
            "init".to_string(),
            vec![new_file("README.md")],
            None,
        )
        .expect("outer initial commit");

        // A nested repo with its own commit, so the gitlink has a target.
        let nested = outer.join("nested");
        fs::create_dir(&nested).expect("mkdir nested");
        init_repo(&nested);
        fs::write(nested.join("inner.txt"), "inner\n").expect("write inner");
        let nested_path = nested.to_str().expect("utf-8 path").to_string();
        run_git(&nested_path, &["add", "inner.txt"]).expect("stage inner");
        run_git(&nested_path, &["commit", "-q", "-m", "inner"]).expect("commit inner");

        // get_status flags it embedded and keeps the trailing slash.
        let st = get_status(outer_path.clone()).expect("status");
        let entry = st
            .files
            .iter()
            .find(|f| f.path.starts_with("nested"))
            .expect("nested entry present in status");
        assert!(entry.embedded, "nested repo should be flagged embedded");
        assert!(entry.path.ends_with('/'), "embedded path keeps trailing slash");

        // Committing it must succeed and produce a gitlink (mode 160000).
        commit(
            outer_path.clone(),
            "Add nested".to_string(),
            vec![entry.clone()],
            None,
        )
        .expect("committing an embedded repo should stage a gitlink, not fail");

        let staged = run_git(&outer_path, &["ls-files", "--stage", "nested"]).expect("ls-files");
        assert!(
            staged.starts_with("160000"),
            "nested should be committed as a gitlink, got: {staged}"
        );
    }

    /// Regression: opening History on a fresh repo (unborn HEAD) must not error.
    /// `git log` exits 128 there ("does not have any commits yet"); `get_log`
    /// should treat that as an empty history.
    /// `is_dirty_submodule` must fire only for a submodule that is dirty inside
    /// with no pointer move — the one state the parent repo can't stage. A moved
    /// pointer (`SC..`) and plain files (`N...`) stay committable (false).
    #[test]
    fn classifies_only_unstageable_dirty_submodules() {
        // Dirty inside, pointer unmoved → not stageable from the parent.
        assert!(is_dirty_submodule("S.M."), "modified tracked content");
        assert!(is_dirty_submodule("S..U"), "untracked content");
        assert!(is_dirty_submodule("S.MU"), "both modified and untracked");
        // Committable or irrelevant → false.
        assert!(!is_dirty_submodule("SC.."), "pointer moved — stage the gitlink");
        assert!(!is_dirty_submodule("SCMU"), "pointer moved, also dirty");
        assert!(!is_dirty_submodule("S..."), "submodule with nothing changed");
        assert!(!is_dirty_submodule("N..."), "not a submodule");
        assert!(!is_dirty_submodule(""), "empty field");
    }

    /// A porcelain-v2 ordinary entry for a dirty-but-unmoved submodule must
    /// parse into an entry flagged `submodule_dirty`, while a normal file must
    /// not. The `sub` field is the 3rd token (`S.M.` vs `N...`).
    #[test]
    fn parses_dirty_submodule_flag_from_ordinary_entry() {
        let sub = parse_ordinary_entry(
            "1 .M S.M. 160000 160000 160000 abc123 abc123 vendor/lib",
        )
        .expect("ordinary entry parses");
        assert!(sub.submodule_dirty, "dirty submodule must be flagged");

        let file = parse_ordinary_entry(
            "1 .M N... 100644 100644 100644 abc123 def456 src/main.rs",
        )
        .expect("ordinary entry parses");
        assert!(!file.submodule_dirty, "a normal file is never flagged");
    }

    #[test]
    fn get_log_returns_empty_on_fresh_repo() {
        let tmp = tempdir().expect("tempdir");
        init_repo(tmp.path());
        let repo_path = tmp.path().to_str().expect("utf-8 path").to_string();

        let log = get_log(repo_path, default_log_opts())
            .expect("get_log on an empty repo should be Ok, not an error");
        assert!(log.is_empty(), "fresh repo should have no commits");
    }

    /// After the first commit, `get_log` reports it and `has_commits` flips true.
    #[test]
    fn has_commits_and_log_reflect_first_commit() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        assert!(!has_commits(&repo_path), "fresh repo has no commits");

        fs::write(repo.join("a.txt"), "x\n").expect("write file");
        commit(
            repo_path.clone(),
            "First".to_string(),
            vec![new_file("a.txt")],
            None,
        )
        .expect("commit should succeed");

        assert!(has_commits(&repo_path), "repo has a commit after committing");
        let log = get_log(repo_path, default_log_opts()).expect("get_log");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].summary, "First");
    }

    /// Regression: viewing a staged file's diff on a fresh repo (unborn HEAD)
    /// must not error. It used to fail with "git diff failed: fatal: bad
    /// revision 'HEAD'" because the diff was anchored at `HEAD`, which doesn't
    /// exist yet. We now diff against the empty tree, so the file shows fully
    /// added.
    #[test]
    fn diff_on_fresh_repo_shows_staged_file_as_added() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("README.md"), "hello\nworld\n").expect("write README");
        run_git(&repo_path, &["add", "README.md"]).expect("stage README");

        let diff = get_diff(repo_path, new_file("README.md"))
            .expect("diff on a fresh repo should be Ok, not an error");
        assert!(diff.contains("+hello"), "added line missing: {diff}");
        assert!(diff.contains("+world"), "added line missing: {diff}");
    }

    /// `get_status.has_remote` reflects whether any remote is configured. The UI
    /// uses it to offer "Publish to GitHub" instead of Push when it's false.
    #[test]
    fn status_reports_has_remote() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("a.txt"), "x\n").expect("write file");
        commit(
            repo_path.clone(),
            "First".to_string(),
            vec![new_file("a.txt")],
            None,
        )
        .expect("commit should succeed");

        let before = get_status(repo_path.clone()).expect("get_status");
        assert!(!before.has_remote, "no remote configured yet");

        run_git(
            &repo_path,
            &["remote", "add", "origin", "https://example.com/x/y.git"],
        )
        .expect("add remote");

        let after = get_status(repo_path).expect("get_status");
        assert!(after.has_remote, "remote is now configured");
    }

    /// Regression: porcelain v2 `-z` output is NUL-terminated end to end — the
    /// `# branch.*` headers carry no newlines — so the header parser must split
    /// on NUL. The old newline-based loop parsed zero headers: `has_upstream`
    /// stayed false (masked in get_status by the rev-parse + remote-tracking
    /// fallbacks) and `repo_sync_status`, which has no branch fallback, reported
    /// ahead=behind=0 for every repo, so the picker badges never appeared.
    #[test]
    fn status_parses_upstream_and_ahead_behind_from_porcelain_headers() {
        let tmp = tempdir().expect("tempdir");
        let work = tmp.path().join("work");
        let remote = tmp.path().join("remote.git");
        fs::create_dir_all(&work).expect("mkdir work");
        init_repo(&work);
        let work_path = work.to_str().expect("utf-8 path").to_string();

        fs::write(work.join("a.txt"), "1\n").expect("write file");
        commit(
            work_path.clone(),
            "first".to_string(),
            vec![new_file("a.txt")],
            None,
        )
        .expect("commit");

        // Bare remote + upstream tracking, entirely local (no network).
        let bare = remote.to_str().expect("utf-8 path");
        run_git(&work_path, &["init", "--bare", bare]).expect("init bare");
        run_git(&work_path, &["remote", "add", "origin", bare]).expect("add remote");
        run_git(&work_path, &["push", "-u", "origin", "HEAD"]).expect("push -u");

        // One local commit that hasn't been pushed → ahead by 1.
        fs::write(work.join("b.txt"), "2\n").expect("write file");
        commit(
            work_path.clone(),
            "second".to_string(),
            vec![new_file("b.txt")],
            None,
        )
        .expect("commit");

        let st = get_status(work_path.clone()).expect("get_status");
        assert!(st.has_upstream, "branch.upstream header must be parsed");
        assert!(
            st.upstream.starts_with("origin/") && !st.upstream.contains("inferred"),
            "real upstream, not the no-upstream fallback's '(inferred)' label: {}",
            st.upstream
        );
        assert_eq!(st.ahead, 1, "one unpushed local commit");
        assert_eq!(st.behind, 0);

        // repo_sync_status must derive the same counts from the headers; with the
        // old newline parser `branch` stayed empty, the fallback was skipped, and
        // this returned 0/0.
        let sync = repo_sync_status(work_path, false).expect("repo_sync_status");
        assert_eq!(sync.ahead, 1, "repo_sync_status must parse ahead from header");
        assert_eq!(sync.behind, 0);
        assert!(sync.has_remote);
    }

    /// Regression: on a branch with no upstream — cloned a base, branched off,
    /// then committed — the History view must still mark the new local commit as
    /// unpushed (the up-arrow), while leaving the shared base (on `origin/<def>`)
    /// unmarked. `ahead` stays 0 without an upstream, so the unpushed list falls
    /// back to `HEAD --not --remotes`. Mirrors GitHub Desktop; previously leogit
    /// computed nothing here and showed no arrows at all.
    #[test]
    fn unpushed_shas_marks_local_commits_on_unpublished_branch() {
        let tmp = tempdir().expect("tempdir");
        let work = tmp.path().join("work");
        let remote = tmp.path().join("remote.git");
        fs::create_dir_all(&work).expect("mkdir work");
        init_repo(&work);
        let work_path = work.to_str().expect("utf-8 path").to_string();

        // Base commit, published to origin so a remote-tracking ref exists.
        fs::write(work.join("a.txt"), "1\n").expect("write file");
        commit(work_path.clone(), "base".into(), vec![new_file("a.txt")], None).expect("commit base");
        let base_sha = run_git(&work_path, &["rev-parse", "HEAD"])
            .expect("rev-parse base")
            .trim()
            .to_string();

        let bare = remote.to_str().expect("utf-8 path");
        run_git(&work_path, &["init", "--bare", bare]).expect("init bare");
        run_git(&work_path, &["remote", "add", "origin", bare]).expect("add remote");
        run_git(&work_path, &["push", "-u", "origin", "HEAD"]).expect("push -u");

        // A new branch with NO upstream + a local-only commit on top of the base.
        run_git(&work_path, &["checkout", "-b", "feature"]).expect("checkout -b");
        fs::write(work.join("b.txt"), "2\n").expect("write file");
        commit(work_path.clone(), "local".into(), vec![new_file("b.txt")], None)
            .expect("commit local");
        let local_sha = run_git(&work_path, &["rev-parse", "HEAD"])
            .expect("rev-parse local")
            .trim()
            .to_string();

        let st = get_status(work_path).expect("get_status");
        assert!(!st.has_upstream, "the new branch has no upstream");
        assert!(
            st.unpushed_shas.contains(&local_sha),
            "the local-only commit must be marked unpushed: {:?}",
            st.unpushed_shas
        );
        assert!(
            !st.unpushed_shas.contains(&base_sha),
            "the shared base (on origin) must NOT be marked unpushed: {:?}",
            st.unpushed_shas
        );
    }

    /// A repo with no remote has nowhere to push, so no commit is "unpushed" —
    /// the History view shows no arrows. Pins the `has_remote` gate on the
    /// no-upstream fallback (without it, `--not --remotes` would mark every
    /// commit on a purely local repo).
    #[test]
    fn unpushed_shas_empty_without_a_remote() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("a.txt"), "1\n").expect("write file");
        commit(repo_path.clone(), "only".into(), vec![new_file("a.txt")], None).expect("commit");

        let st = get_status(repo_path).expect("get_status");
        assert!(!st.has_remote, "no remote configured");
        assert!(
            st.unpushed_shas.is_empty(),
            "no remote → nothing is 'unpushed': {:?}",
            st.unpushed_shas
        );
    }

    /// A remote is configured but nothing was ever pushed/fetched, so there are
    /// zero `refs/remotes/*`. Every local commit is genuinely not on the remote,
    /// so all are marked unpushed. Pins this deliberate behavior: we do NOT gate
    /// the fallback on remote-tracking refs existing, because that would wrongly
    /// hide arrows for a freshly `remote add`ed repo whose commits really are
    /// unpushed.
    #[test]
    fn unpushed_shas_marks_all_commits_when_remote_has_no_tracking_refs() {
        let tmp = tempdir().expect("tempdir");
        let work = tmp.path().join("work");
        let remote = tmp.path().join("remote.git");
        fs::create_dir_all(&work).expect("mkdir work");
        init_repo(&work);
        let work_path = work.to_str().expect("utf-8 path").to_string();

        fs::write(work.join("a.txt"), "1\n").expect("write file");
        commit(work_path.clone(), "c1".into(), vec![new_file("a.txt")], None).expect("commit");
        let c1 = run_git(&work_path, &["rev-parse", "HEAD"])
            .expect("rev-parse")
            .trim()
            .to_string();

        // Remote configured but never pushed/fetched → no refs/remotes/* exist.
        let bare = remote.to_str().expect("utf-8 path");
        run_git(&work_path, &["init", "--bare", bare]).expect("init bare");
        run_git(&work_path, &["remote", "add", "origin", bare]).expect("add remote");

        let st = get_status(work_path).expect("get_status");
        assert!(st.has_remote, "remote is configured");
        assert!(!st.has_upstream, "no upstream yet");
        assert!(
            st.unpushed_shas.contains(&c1),
            "a commit that's on no remote must be marked unpushed: {:?}",
            st.unpushed_shas
        );
    }

    fn modified_file(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            orig_path: None,
            status: FileStatus::Modified,
            xy: ".M".to_string(),
            display_name: path.to_string(),
            display_dir: String::new(),
            embedded: false,
            submodule_dirty: false,
        }
    }

    fn deleted_file(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            orig_path: None,
            status: FileStatus::Deleted,
            xy: ".D".to_string(),
            display_name: path.to_string(),
            display_dir: String::new(),
            embedded: false,
            submodule_dirty: false,
        }
    }

    /// `head_paths` is the classifier `discard_files` relies on: a committed
    /// path is in HEAD (→ restore from HEAD), a never-committed one is not (→
    /// trash + unstage). Keeps the trash-touching branch of discard untested by
    /// design so the suite never moves real files to the OS trash.
    #[test]
    fn head_paths_distinguishes_committed_from_new() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("tracked.txt"), "x\n").expect("write");
        commit(
            repo_path.clone(),
            "add".to_string(),
            vec![new_file("tracked.txt")],
            None,
        )
        .expect("commit");

        let in_head = head_paths(
            &repo_path,
            &["tracked.txt".to_string(), "never.txt".to_string()],
        );
        assert!(in_head.contains("tracked.txt"), "committed file is in HEAD");
        assert!(!in_head.contains("never.txt"), "uncommitted file is not");
    }

    /// An unborn HEAD (fresh repo, no commits) has no tree, so nothing classifies
    /// as tracked — every changed file is then a never-committed one.
    #[test]
    fn head_paths_empty_on_unborn_head() {
        let tmp = tempdir().expect("tempdir");
        init_repo(tmp.path());
        let repo_path = tmp.path().to_str().expect("utf-8 path").to_string();
        assert!(head_paths(&repo_path, &["whatever.txt".to_string()]).is_empty());
    }

    /// Discarding a modified tracked file reverts it to its committed content
    /// and leaves the working tree clean.
    #[test]
    fn discard_reverts_a_modified_tracked_file() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("a.txt"), "v1\n").expect("write");
        commit(
            repo_path.clone(),
            "add a".to_string(),
            vec![new_file("a.txt")],
            None,
        )
        .expect("commit");

        fs::write(repo.join("a.txt"), "v2-uncommitted\n").expect("modify");
        discard_files(&repo_path, vec![modified_file("a.txt")]).expect("discard");

        assert_eq!(
            fs::read_to_string(repo.join("a.txt")).expect("read"),
            "v1\n",
            "modified file must be reverted to HEAD"
        );
        let st = get_status(repo_path).expect("status");
        assert!(st.files.is_empty(), "working tree should be clean: {:?}", st.files);
    }

    /// Discarding a deleted tracked file restores it from HEAD.
    #[test]
    fn discard_restores_a_deleted_tracked_file() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        fs::write(repo.join("a.txt"), "keep\n").expect("write");
        commit(
            repo_path.clone(),
            "add a".to_string(),
            vec![new_file("a.txt")],
            None,
        )
        .expect("commit");

        fs::remove_file(repo.join("a.txt")).expect("delete");
        discard_files(&repo_path, vec![deleted_file("a.txt")]).expect("discard");

        assert_eq!(
            fs::read_to_string(repo.join("a.txt")).expect("read"),
            "keep\n",
            "deleted file must be restored from HEAD"
        );
    }

    /// Creates the file when absent, appends each pattern on its own line, and
    /// never writes a pattern that's already present.
    #[test]
    fn append_to_gitignore_creates_and_dedupes() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        append_to_gitignore(&repo_path, vec!["*.log".into(), "secret.txt".into()])
            .expect("append");
        let content = fs::read_to_string(repo.join(".gitignore")).expect("read");
        assert!(content.contains("*.log"), "missing pattern: {content}");
        assert!(content.contains("secret.txt"), "missing pattern: {content}");
        assert!(content.ends_with('\n'), "must end with a newline: {content:?}");

        append_to_gitignore(&repo_path, vec!["*.log".into()]).expect("append again");
        let content = fs::read_to_string(repo.join(".gitignore")).expect("read");
        assert_eq!(content.matches("*.log").count(), 1, "no duplicate: {content}");
    }

    /// A `.gitignore` whose last line has no trailing newline must not get the
    /// new pattern joined onto it.
    #[test]
    fn append_to_gitignore_inserts_newline_before_appending() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        let repo_path = repo.to_str().expect("utf-8 path").to_string();
        fs::write(repo.join(".gitignore"), "foo").expect("seed");

        append_to_gitignore(&repo_path, vec!["bar".into()]).expect("append");
        assert_eq!(
            fs::read_to_string(repo.join(".gitignore")).expect("read"),
            "foo\nbar\n",
            "must not join onto the last rule"
        );
    }

    /// Regression: selecting a branch from the dropdown's *Remote Branches*
    /// section (`origin/<name>`) must check out a local **tracking** branch, not
    /// detach HEAD. The old `git checkout origin/<name> --` treated the ref as a
    /// commit-ish (the trailing `--`), landing the user in detached HEAD. Also
    /// covers the collision path: re-selecting it once the local branch exists
    /// must just switch, never recreate-and-fail.
    #[test]
    fn switch_to_remote_branch_creates_local_tracking_branch() {
        let tmp = tempdir().expect("tempdir");

        // Upstream repo with a `feature` branch the clone won't have locally.
        let upstream = tmp.path().join("upstream");
        fs::create_dir(&upstream).expect("mkdir upstream");
        init_repo(&upstream);
        let upstream_path = upstream.to_str().expect("utf-8 path").to_string();
        fs::write(upstream.join("README.md"), "hello\n").expect("write README");
        commit(
            upstream_path.clone(),
            "init".to_string(),
            vec![new_file("README.md")],
            None,
        )
        .expect("seed commit");
        // Capture the default branch name (could be main or master depending on
        // the host's init.defaultBranch) so we can return to it before cloning.
        let default_branch =
            run_git(&upstream_path, &["symbolic-ref", "--short", "HEAD"]).expect("default branch");
        run_git(&upstream_path, &["checkout", "-q", "-b", "feature"]).expect("create feature");
        fs::write(upstream.join("feature.txt"), "f\n").expect("write feature file");
        commit(
            upstream_path.clone(),
            "feat".to_string(),
            vec![new_file("feature.txt")],
            None,
        )
        .expect("feature commit");
        // Leave upstream on its default branch so the clone checks *that* out and
        // `feature` exists only as a remote-tracking ref.
        run_git(&upstream_path, &["checkout", "-q", &default_branch]).expect("back to default");

        // Clone it: clone has refs/remotes/origin/feature but no local `feature`.
        let clone = tmp.path().join("clone");
        let cloned = Command::new("git")
            .args(["clone", "-q", &upstream_path, clone.to_str().expect("utf-8 path")])
            .status()
            .expect("spawn git clone")
            .success();
        assert!(cloned, "git clone failed");
        let clone_path = clone.to_str().expect("utf-8 path").to_string();

        // Selecting origin/feature with no local branch yet -> tracking branch.
        switch_branch(clone_path.clone(), "origin/feature".to_string())
            .expect("switching to a remote branch should succeed");
        let head = run_git(&clone_path, &["symbolic-ref", "--short", "HEAD"])
            .expect("HEAD must be a branch, not detached");
        assert_eq!(head, "feature", "should land on a local 'feature' branch");
        let tracked = run_git(
            &clone_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .expect("upstream ref");
        assert_eq!(tracked, "origin/feature", "local branch must track origin/feature");

        // Re-selecting it once the local branch exists must switch, not fail.
        run_git(&clone_path, &["checkout", "-q", &default_branch]).expect("hop away");
        switch_branch(clone_path.clone(), "origin/feature".to_string())
            .expect("re-selecting an already-tracked remote branch must not fail");
        let head_again =
            run_git(&clone_path, &["symbolic-ref", "--short", "HEAD"]).expect("HEAD ref");
        assert_eq!(head_again, "feature");
    }
}
