use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    let (display_name, display_dir) = extract_display_name_and_dir(&path);
    Some(FileEntry {
        path,
        orig_path: None,
        status,
        xy,
        display_name,
        display_dir,
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
        unpushed_shas: Vec::new(),
    };

    if bytes.is_empty() {
        return Ok(result);
    }

    // Branch headers are newline-terminated inside the porcelain output.
    // File entries are NUL-terminated. We process the bytes in order:
    // strip leading header lines (those starting with "# ") and then split
    // the remainder on NUL.
    //
    // The header section ends at the first byte that does not begin a "# " line.
    let mut rest: &[u8] = &bytes;

    // Process headers line-by-line until we hit a non-header line.
    while let Some(nl_pos) = rest.iter().position(|&b| b == b'\n') {
        let line = &rest[..nl_pos];
        if !line.starts_with(b"# ") {
            break;
        }
        let line_str = String::from_utf8_lossy(line);
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
            let ab = rem.trim();
            let parts: Vec<&str> = ab.split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].trim_start_matches('+').parse().unwrap_or(0);
                let behind = parts[1].trim_start_matches('-').parse().unwrap_or(0);
                result.ahead = ahead;
                result.behind = behind;
            }
        }
        rest = &rest[nl_pos + 1..];
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
        if let Ok(remotes) = run_git(&repo_path, &["remote"]) {
            if let Some(first_remote) = remotes.lines().next().map(str::trim) {
                if !first_remote.is_empty() {
                    let remote_ref = format!("refs/remotes/{}/{}", first_remote, result.branch);
                    let exists = git_cmd(
                        &repo_path,
                        &["rev-parse", "--verify", "--quiet", &remote_ref],
                    )
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                    if exists {
                        let range = format!("HEAD...{}", remote_ref);
                        if let Ok(out) = run_git(
                            &repo_path,
                            &["rev-list", "--left-right", "--count", &range, "--"],
                        ) {
                            let parts: Vec<&str> = out.split_whitespace().collect();
                            if parts.len() == 2 {
                                result.ahead = parts[0].parse().unwrap_or(0);
                                result.behind = parts[1].parse().unwrap_or(0);
                                // Surface the synthesised tracking name so the
                                // UI / debug logs make sense; this is purely
                                // informational and does NOT flip has_upstream.
                                result.upstream =
                                    format!("{}/{} (inferred)", first_remote, result.branch);
                                effective_upstream = Some(remote_ref);
                            }
                        }
                    }
                }
            }
        }
    }

    // List unpushed commit SHAs so the History view can mark them. Skipped
    // when there's nothing to mark — avoids an extra `git rev-list` on every
    // 2s status poll for in-sync branches.
    if result.ahead > 0 {
        if let Some(upstream_ref) = effective_upstream.as_deref() {
            let exclude = format!("^{}", upstream_ref);
            if let Ok(out) = run_git(&repo_path, &["rev-list", "HEAD", &exclude]) {
                result.unpushed_shas = out
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
            }
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
            // Untracked file
            let path = rest_seg.to_string();
            let (display_name, display_dir) = extract_display_name_and_dir(&path);
            result.files.push(FileEntry {
                path,
                orig_path: None,
                status: FileStatus::New,
                xy: "??".to_string(),
                display_name,
                display_dir,
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

fn diff_args_for_file<'a>(file: &'a FileEntry, ignore_ws: bool) -> Vec<&'a str> {
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
        args.push("HEAD");
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
    let args = diff_args_for_file(file, ignore_ws);
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

#[tauri::command]
pub fn switch_branch(repo_path: String, branch: String) -> Result<(), String> {
    run_git(&repo_path, &["checkout", &branch, "--"])?;
    Ok(())
}

#[tauri::command]
pub fn delete_branch(repo_path: String, name: String) -> Result<(), String> {
    run_git(&repo_path, &["branch", "-D", &name])?;
    Ok(())
}

#[tauri::command]
pub fn delete_remote_branch(
    repo_path: String,
    remote: String,
    branch: String,
) -> Result<(), String> {
    let refspec = format!(":{}", branch);
    run_git(&repo_path, &["push", &remote, &refspec])?;
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

    update_index(repo_path, &renamed_old, true)?;
    update_index(repo_path, &normal, false)?;
    update_index(repo_path, &deleted, true)?;
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

    // Reset the index to HEAD so only what the user selected gets committed.
    let reset = git_cmd(&repo_path, &["reset", "HEAD"])
        .output()
        .map_err(|e| format!("git reset: {}", e))?;
    if !reset.status.success() {
        return Err(format!(
            "git reset HEAD failed: {}",
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

#[tauri::command]
pub fn fetch(repo_path: String, remote: String) -> Result<(), String> {
    let (ok, combined) = run_git_combined(
        &repo_path,
        &[
            "fetch",
            "--prune",
            "--recurse-submodules=on-demand",
            &remote,
        ],
    )?;
    if !ok {
        return Err(format!("git fetch failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
pub fn pull(repo_path: String, remote: String) -> Result<(), String> {
    let (ok, combined) =
        run_git_combined(&repo_path, &["pull", "--ff", "--recurse-submodules", &remote])?;
    if !ok {
        return Err(format!("git pull failed: {}", combined.trim()));
    }
    Ok(())
}

#[tauri::command]
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

    let (ok, combined) = run_git_combined(&repo_path, &args)?;
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
#[tauri::command]
pub fn clone_repo(url: String, target_path: String) -> Result<String, String> {
    let target = prepare_clone_target(&target_path)?;
    let mut cmd = Command::new("git");
    cmd.env("TERM", "dumb")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["clone", "--progress", &url, &target]);
    super::process::hide_console(&mut cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("Could not run git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
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

fn is_git_repo_path(path: &Path) -> bool {
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
