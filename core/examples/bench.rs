//! Measurement harness for the I/O-efficiency work: wall time and spawn count
//! for the git operations the clients actually call, against a real repository.
//!
//! An *example*, not a binary, so it is compiled on demand and never ships in a
//! release bundle. Run it through `just bench <repo> [--fetch] [--scan <dir>]`.
//!
//! Every git question in `LeoGit` is a subprocess, and a subprocess costs the
//! same ~8 ms whatever it asks. So the number this prints beside each duration
//! — spawns per call — is the one that predicts what a change is worth: making
//! a git command cheaper buys almost nothing, removing one buys a fork/exec.
//! The `git --version` row is the floor to read every other row against.
//!
//! Each operation gets one untimed warm-up call and then three timed ones. The
//! warm-up is not politeness: the first `git status` on a cold repository pays
//! for populating the OS page cache with the index and the object store, which
//! is a one-off the steady-state poll never pays again, and including it would
//! report a number no user ever experiences.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use leogit_core::git::{self, FileEntry, LogOptions};
use leogit_core::process::{self, PathSource};

/// Timed calls per operation, after the warm-up. Three is enough for a median
/// to reject a single scheduler hiccup, and cheap enough that a `--fetch` run
/// stays polite to the remote.
const SAMPLES: usize = 3;
/// [`SAMPLES`] in the type the spawn totals are counted in. Written out rather
/// than cast, because a `usize`-to-`u64` cast is a lint for no gain here.
const SAMPLES_U64: u64 = 3;

/// The first page of history, in the shape both clients ask for it: 50 commits
/// from the top (`MainLayout.svelte`'s `PAGE_SIZE`).
const LOG_PAGE: LogOptions = LogOptions {
    max_count: 50,
    skip: 0,
};

/// How deep discovery walks, matching the depth the repo picker passes.
const SCAN_DEPTH: u32 = 3;

struct Args {
    repo: String,
    fetch: bool,
    scan: Option<String>,
}

/// One measured operation: the raw samples, plus the spawn delta across all of
/// them (kept as a total so per-call can stay integer arithmetic).
struct Timing {
    times_ms: Vec<f64>,
    spawns: u64,
}

/// One rendered table line. Built as strings because a skipped operation still
/// earns a row — "n/a (clean tree)" is a measurement result too, and dropping
/// the row would leave a reader wondering whether it was measured at all.
struct Row {
    operation: String,
    median: String,
    range: String,
    spawns: String,
    notes: String,
}

impl Row {
    fn measured(operation: &str, timing: &Timing, notes: &str) -> Self {
        let mut sorted = timing.times_ms.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        Self {
            operation: operation.to_string(),
            median: format!("{median:.1}"),
            range: format!("{min:.1}–{max:.1}"),
            spawns: per_call(timing.spawns),
            notes: notes.to_string(),
        }
    }

    /// A row for an operation measured exactly once, because a warm-up would
    /// change what is being measured rather than settle it.
    fn single(operation: &str, ms: f64, spawns: u64, notes: &str) -> Self {
        Self {
            operation: operation.to_string(),
            median: format!("{ms:.1}"),
            range: "one call".to_string(),
            spawns: spawns.to_string(),
            notes: notes.to_string(),
        }
    }

    fn skipped(operation: &str, notes: &str) -> Self {
        Self {
            operation: operation.to_string(),
            median: "n/a".to_string(),
            range: "n/a".to_string(),
            spawns: "n/a".to_string(),
            notes: notes.to_string(),
        }
    }
}

/// Spawns per call, to one decimal, without ever converting a count to `f64`.
/// A fractional value is a finding rather than noise: it means the operation
/// spawned a different number of children on different runs, which is what a
/// cache or an early return inside git looks like from out here.
fn per_call(total: u64) -> String {
    let tenths = total * 10 / SAMPLES_U64;
    if tenths.is_multiple_of(10) {
        (tenths / 10).to_string()
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

/// Warm up once, then time `SAMPLES` calls, returning the samples and the last
/// result so the caller can describe what the operation actually found.
fn measure<T>(label: &str, mut op: impl FnMut() -> T) -> (Timing, T) {
    eprintln!("  {label}…");
    let mut last = op();
    let before = process::spawn_count();
    let mut times_ms = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        last = op();
        times_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let timing = Timing {
        times_ms,
        spawns: process::spawn_count() - before,
    };
    (timing, last)
}

/// `git --version` built the way `git.rs` builds every git command — the same
/// env and the same [`process::prepare_child`] hook — and run with `output()`,
/// the same way `run_git_raw` runs one.
///
/// Deliberately not through `process::run_timed`, which is a floor this row
/// must not include: that runner starts three helper threads per child and
/// gives the child a process group of its own, and this row exists to measure
/// fork/exec and nothing else. It no longer *quantises* anything — the wait is
/// a channel receive rather than a 50 ms poll, which is what makes the network
/// rows below read a real duration instead of a multiple of the tick.
fn git_version_cmd(repo: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .env("TERM", "dumb")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("--version");
    process::prepare_child(&mut cmd);
    cmd
}

fn parse_args() -> Result<Args, String> {
    let mut repo = None;
    let mut fetch = false;
    let mut scan = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fetch" => fetch = true,
            "--scan" => {
                scan = Some(args.next().ok_or("--scan needs a directory")?);
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => {
                if repo.replace(other.to_string()).is_some() {
                    return Err("more than one repository path given".to_string());
                }
            }
        }
    }
    Ok(Args {
        repo: repo.ok_or("no repository path given")?,
        fetch,
        scan,
    })
}

/// The floor, the status read, and the two sync reads.
fn bench_status(args: &Args, rows: &mut Vec<Row>) -> Vec<FileEntry> {
    let (timing, ()) = measure("git --version (process floor)", || {
        let out = git_version_cmd(&args.repo)
            .output()
            .expect("git --version must run");
        assert!(out.status.success(), "git --version failed");
    });
    rows.push(Row::measured(
        "`git --version` (process floor)",
        &timing,
        "fork/exec only, no repository work",
    ));

    let (timing, status) = measure("get_status", || {
        git::get_status(args.repo.clone()).expect("get_status")
    });
    rows.push(Row::measured(
        "`get_status`",
        &timing,
        &format!(
            "{} changed files, branch `{}`",
            status.files.len(),
            status.branch
        ),
    ));

    let (timing, sync) = measure("repo_sync_status(no fetch)", || {
        git::repo_sync_status(args.repo.clone(), false).expect("repo_sync_status")
    });
    rows.push(Row::measured(
        "`repo_sync_status(fetch=false)`",
        &timing,
        &format!("ahead {}, behind {}", sync.ahead, sync.behind),
    ));

    if args.fetch {
        let (timing, sync) = measure("repo_sync_status(fetch)", || {
            git::repo_sync_status(args.repo.clone(), true).expect("repo_sync_status")
        });
        rows.push(Row::measured(
            "`repo_sync_status(fetch=true)`",
            &timing,
            &format!(
                "network; reached remote: {}",
                if sync.fetched { "yes" } else { "no" }
            ),
        ));
    } else {
        rows.push(Row::skipped(
            "`repo_sync_status(fetch=true)`",
            "not run (pass `--fetch`; it touches the network)",
        ));
    }

    status.files
}

/// History and the two diff shapes, which need the status result above.
fn bench_log_and_diff(args: &Args, files: &[FileEntry], rows: &mut Vec<Row>) {
    let (timing, commits) = measure("get_log", || {
        git::get_log(args.repo.clone(), LOG_PAGE).expect("get_log")
    });
    rows.push(Row::measured(
        "`get_log` (first page, 50)",
        &timing,
        &format!("{} commits parsed", commits.len()),
    ));

    let Some(first) = files.first() else {
        rows.push(Row::skipped("`get_diff` (first file)", "n/a (clean tree)"));
        rows.push(Row::skipped(
            "`get_selected_diff` (all)",
            "n/a (clean tree)",
        ));
        return;
    };

    let (timing, diff) = measure("get_diff", || {
        git::get_diff(args.repo.clone(), first.clone())
    });
    rows.push(Row::measured(
        "`get_diff` (first file)",
        &timing,
        &describe_patch(&diff, &format!("`{}`", first.path)),
    ));

    if files.len() < 2 {
        rows.push(Row::skipped(
            "`get_selected_diff` (all)",
            "n/a (only one changed file)",
        ));
        return;
    }
    let (timing, diff) = measure("get_selected_diff", || {
        git::get_selected_diff(args.repo.clone(), files.to_vec())
    });
    rows.push(Row::measured(
        "`get_selected_diff` (all)",
        &timing,
        &describe_patch(&diff, &format!("{} files", files.len())),
    ));
}

/// Patch size in bytes alongside `subject`, or the error git gave. A failed
/// diff still produced a timing, so it stays in the table saying why.
fn describe_patch(diff: &Result<String, String>, subject: &str) -> String {
    match diff {
        Ok(patch) => format!("{subject}, {} B of patch", patch.len()),
        Err(err) => format!("{subject}, failed: {err}"),
    }
}

fn bench_discovery(scan: &str, rows: &mut Vec<Row>) {
    let (timing, repos) = measure("discover_repos", || {
        git::discover_repos(vec![scan.to_string()], SCAN_DEPTH).expect("discover_repos")
    });
    rows.push(Row::measured(
        "`discover_repos` (depth 3)",
        &timing,
        &format!("{} repos under `{scan}`", repos.len()),
    ));
}

/// What a launch actually pays to know the user's `PATH`.
///
/// Measured with **one** call and no warm-up, unlike every other row here. A
/// warm-up would be the probe itself, and the timed calls would then all be
/// cache hits — which is the number this row exists to contrast against, not
/// the one it reports. So run the bench twice: the first run says `probed` and
/// costs a shell, every later one says `cached` and costs a file read, until
/// the user edits an rc file or the seven-day ceiling expires.
fn bench_resolved_login_path(rows: &mut Vec<Row>) {
    eprintln!("  resolve_login_path…");
    let before = process::spawn_count();
    let started = Instant::now();
    let resolved = process::resolve_login_path();
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let notes = match resolved {
        Some((path, source)) => {
            let source = match source {
                PathSource::Cached => "cached",
                PathSource::Probed => "probed",
            };
            format!("{source}, {} PATH entries", path.split(':').count())
        }
        None => "no login PATH resolved".to_string(),
    };
    rows.push(Row::single(
        "`resolve_login_path` (startup)",
        elapsed,
        process::spawn_count() - before,
        &notes,
    ));
}

/// The cost the row above avoids: the probe with no cache in front of it.
fn bench_login_path(rows: &mut Vec<Row>) {
    let (timing, path) = measure("probe_login_path", process::probe_login_path);
    let notes = path.map_or_else(
        || "no login PATH resolved".to_string(),
        |path| format!("{} PATH entries", path.split(':').count()),
    );
    rows.push(Row::measured(
        "`probe_login_path` (startup)",
        &timing,
        &notes,
    ));
}

/// Column widths are computed rather than fixed so the table stays aligned in a
/// plain terminal as well as rendering as markdown.
fn print_table(rows: &[Row]) {
    let width = |pick: fn(&Row) -> &str, header: &str| {
        rows.iter()
            .map(|row| pick(row).chars().count())
            .chain(std::iter::once(header.chars().count()))
            .max()
            .unwrap_or(0)
    };
    let w_op = width(|r| r.operation.as_str(), "operation");
    let w_med = width(|r| r.median.as_str(), "median ms");
    let w_range = width(|r| r.range.as_str(), "min–max ms");
    let w_spawn = width(|r| r.spawns.as_str(), "spawns/call");
    println!(
        "| {:<w_op$} | {:>w_med$} | {:>w_range$} | {:>w_spawn$} | notes |",
        "operation", "median ms", "min–max ms", "spawns/call"
    );
    println!(
        "|{}|{}|{}|{}|---|",
        "-".repeat(w_op + 2),
        "-".repeat(w_med + 2),
        "-".repeat(w_range + 2),
        "-".repeat(w_spawn + 2)
    );
    for row in rows {
        println!(
            "| {:<w_op$} | {:>w_med$} | {:>w_range$} | {:>w_spawn$} | {} |",
            row.operation, row.median, row.range, row.spawns, row.notes
        );
    }
}

/// Point every file `LeoGit` owns at a directory of the bench's own.
///
/// The startup row reads and rewrites the login-`PATH` cache. Left pointing at
/// the real config directory it would overwrite the entry the user's next app
/// launch reads — costing them a slow launch for having run a benchmark, and
/// reporting a timing that depended on whether they had opened the app that
/// week. A fixed name rather than a fresh directory per run, because the second
/// run reading what the first one wrote is precisely what the row measures.
fn redirect_config_dir() {
    let dir = std::env::temp_dir().join("leogit-bench-config");
    // SAFETY: the first statement of `main`, before any thread exists — the
    // same contract `process::fix_path_env` documents for writing the
    // environment of a running process.
    unsafe {
        std::env::set_var("LEOGIT_CONFIG_DIR", &dir);
    }
    eprintln!("bench: config directory redirected to {}", dir.display());
}

fn main() {
    redirect_config_dir();
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("bench: {err}");
            eprintln!("usage: bench <repo-path> [--fetch] [--scan <dir>]");
            std::process::exit(2);
        }
    };
    if !Path::new(&args.repo).join(".git").exists() {
        eprintln!("bench: {} is not a git repository", args.repo);
        std::process::exit(2);
    }

    eprintln!(
        "bench: {} ({SAMPLES} timed runs after one warm-up)",
        args.repo
    );
    let mut rows = Vec::new();
    let files = bench_status(&args, &mut rows);
    bench_log_and_diff(&args, &files, &mut rows);
    if let Some(scan) = args.scan.as_deref() {
        bench_discovery(scan, &mut rows);
    } else {
        rows.push(Row::skipped(
            "`discover_repos` (depth 3)",
            "not run (pass `--scan <dir>`)",
        ));
    }
    bench_resolved_login_path(&mut rows);
    bench_login_path(&mut rows);

    println!();
    print_table(&rows);
    println!();
    println!(
        "Total spawns for the whole run (warm-ups included): {}",
        process::spawn_count()
    );
}
