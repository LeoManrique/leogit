//! Cross-platform subprocess spawning helpers.
//!
//! [`prepare_child`] is the single hook every command we spawn passes through,
//! and it applies the corrections a child needs on the way out:
//!
//! **No console window (Windows).** Release builds run as a GUI app
//! (`windows_subsystem = "windows"`), which has no attached console. When such
//! a process spawns a console subprocess (`git`, `gh`, `claude`), Windows
//! allocates and briefly flashes a new console window for each call. Because
//! the UI polls `git status` every 2s, that means a `cmd` box flickering on
//! screen continuously; the `CREATE_NO_WINDOW` creation flag suppresses it.
//!
//! **No `AppImage` environment (Linux).** The Linux client runs from an `AppImage`
//! whose `AppRun` points a dozen variables at a temporary mount that vanishes
//! when we quit. Those belong to us, never to our children — see
//! [`crate::appimage`] for what that breaks and how it is undone.
//!
//! **A re-probed `PATH` (macOS/Linux).** Startup usually installs a *cached*
//! login `PATH` rather than waiting ~430 ms to ask the shell — see
//! [`fix_path_env`]. When it does, [`spawn_path_reprobe`] asks for real once the
//! UI is up, and anything it learns is applied here, per child. That is what
//! makes the cache safe to trust: a wrong cached `PATH` is corrected within
//! seconds of launch instead of on the next one.
//!
//! Every correction is a no-op on the platforms it doesn't apply to, so call
//! sites stay platform-agnostic and there is one place to add the next one.
//!
//! This module also owns [`run_timed`], the bounded subprocess runner every
//! network-touching git/gh command goes through so an offline or flaky
//! connection can never wedge a worker thread indefinitely — with
//! [`run_timed_uncaptured`] for the hand-off launchers, which want the bound
//! and none of the output — and [`fix_path_env`], which repairs the minimal
//! `PATH` a GUI launch inherits so user-installed subprocesses can be found at
//! all.
//!
//! The bound is a real one, which took three pieces: the child gets a process
//! group (a job object on Windows) of its own so the kill reaches the `ssh` or
//! `git-remote-https` that `git fetch` actually talks to the network through;
//! its exit arrives over a channel from a waiter thread, so the budget is a
//! `recv_timeout` rather than a poll; and the pipe readers are waited for with
//! a grace window and detached if it expires, because a reader whose pipe a
//! survivor still holds cannot be woken by anything. See [`KillScope`] for why
//! the process group is a per-call decision rather than something
//! [`prepare_child`] does to every child.
//!
//! Because every child funnels through one of the `prepare_child*` hooks, they
//! are also the one place that can count them: see [`spawn_count`]. Spawn count
//! is the unit the I/O-efficiency work is denominated in — a git invocation is
//! mostly fork/exec, so removing a command is worth far more than making one
//! cheaper — and `cargo run --example bench` reads that counter to attribute
//! spawns to the operation that caused them.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, mpsc};
use std::time::{Duration, Instant};

use super::appimage;
#[cfg(not(target_os = "windows"))]
use super::path_cache;

/// Number of children prepared since process start (see [`spawn_count`]).
static SPAWN_COUNT: AtomicU64 = AtomicU64::new(0);

/// A login `PATH` learned *after* startup installed one, or `None` while the
/// two agree (which is the overwhelmingly common case, and every case on
/// Windows).
///
/// Written at most once per run, by [`spawn_path_reprobe`]'s worker; read by
/// every `prepare_child*` call. A `RwLock` rather than an `OnceLock` because
/// the tests need to put it back, and because "no fresh value" has to be a
/// state the readers can see rather than a value they have to interpret.
static FRESH_PATH: RwLock<Option<String>> = RwLock::new(None);

/// Whether [`fix_path_env`] installed a *cached* `PATH` at startup — the one
/// condition under which a background re-probe has anything to add.
static STARTUP_USED_CACHE: AtomicBool = AtomicBool::new(false);

/// The `PATH` a later re-probe found, when it found a different one.
fn fresh_path() -> Option<String> {
    FRESH_PATH
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// How many child processes this process has prepared so far.
///
/// Counted in the `prepare_child*` hooks rather than at the `spawn`/`output`
/// call, because those hooks are the only chokepoint every child shares — a
/// counter at each spawn site is a counter that the next spawn site forgets.
/// Every call site in core prepares a command and then runs it exactly once, so
/// the two are the same number in practice; a command that is built and then
/// dropped unrun would be counted anyway.
///
/// `Relaxed` throughout: the value is a diagnostic total, and nothing orders
/// other memory by it.
#[must_use]
pub fn spawn_count() -> u64 {
    SPAWN_COUNT.load(Ordering::Relaxed)
}

/// CREATE_NO_WINDOW process creation flag (Win32 `winbase.h`).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the standard child-process corrections to a std `Command` (see module
/// docs). Returns the command for chaining.
pub fn prepare_child(cmd: &mut std::process::Command) -> &mut std::process::Command {
    SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    appimage::sanitize(cmd);
    // After the sanitising, never before: the order of the corrections is what
    // decides the child's `PATH`, and a re-probed one is the most recent
    // answer we have. It is already free of the `AppImage`'s entries — the
    // probe shell was itself prepared here, so it inherited a sanitised `PATH`
    // and appended to that. Std resolves a bare program name against the
    // child's own `PATH` on unix, so a tool only the fresh `PATH` knows about
    // is found by the very next child, not the next launch.
    if let Some(path) = fresh_path() {
        cmd.env("PATH", path);
    }
    cmd
}

/// [`prepare_child`] for a tokio `Command`.
pub fn prepare_child_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    appimage::sanitize(cmd);
    if let Some(path) = fresh_path() {
        cmd.env("PATH", path);
    }
    cmd
}

/// [`prepare_child`] for a `portable-pty` `CommandBuilder` — the terminal's
/// shell, the one child that is not started from a std/tokio `Command`.
///
/// It carries no console-window correction because it cannot need one: a PTY
/// child is attached to the pseudo-console the pty system allocated for it, so
/// Windows never opens a console of its own for it. What it does share with the
/// other two hooks is the `AppImage` sanitising and the spawn count, and going
/// through here rather than calling [`appimage::sanitize`] directly is what
/// keeps "every child we spawn passes through one hook" true of the terminal as
/// well — so the counter has no blind spot.
pub fn prepare_child_pty(cmd: &mut portable_pty::CommandBuilder) {
    SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
    appimage::sanitize(cmd);
    // The terminal's shell rebuilds its own `PATH` from the user's rc files
    // anyway, so this only decides what it starts from — but starting from the
    // stale value is how a `PATH` the user just fixed still looks broken in the
    // panel, and this is the same correction every other child gets.
    //
    // Not the parent's `PATH`, which [`crate::terminal`] forbids overwriting:
    // this value exists only where there is a login shell to have probed it,
    // and `spawn_path_reprobe` never runs on Windows — so the registry-merged
    // `PATH` `CommandBuilder` assembles there is never touched.
    if let Some(path) = fresh_path() {
        cmd.env("PATH", path);
    }
}

/// How long the login shell gets to answer before the probe is abandoned and
/// the inherited `PATH` is kept.
///
/// The shell runs the user's own rc files, which can do anything — stat a
/// disconnected network mount, let a prompt framework download itself on first
/// run. Ten seconds is well past the ~430 ms a healthy shell takes and short
/// enough that a wedged one is a slow launch rather than an app that never
/// opens. It is also the budget VS Code gives the identical probe.
#[cfg(not(target_os = "windows"))]
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the login `PATH` this process is using came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSource {
    /// Read from the on-disk cache: no shell was spawned, so startup paid
    /// microseconds instead of ~430 ms.
    Cached,
    /// Freshly asked of the login shell, because there was no cache or it no
    /// longer described the machine.
    Probed,
}

/// Ask the user's login shell what `PATH` their terminal would have, returning
/// it verbatim (`None` when there is no `SHELL`, the shell fails or times out,
/// or it printed nothing usable).
///
/// Split out of [`fix_path_env`] so that reading the `PATH` and *installing* it
/// are separate acts: this half is an ordinary subprocess anyone may call at any
/// time, while the install half carries a hard "before any other thread exists"
/// contract. The measurement harness times this one on its own for exactly that
/// reason — startup cost is what it is measuring, and calling `fix_path_env`
/// to measure it would mean writing the environment of a running process.
///
/// The probe shell goes through [`prepare_child`] like any other child, which
/// matters for more than tidiness: a login shell appends to the `PATH` it
/// inherits, so probing with the `AppImage`'s `PATH` would report the `AppImage`'s
/// entries back and bake a temporary mount into the `PATH` every later child
/// starts from.
///
/// ## Why it asks `env` between two markers
///
/// The obvious probe — `echo -n "$PATH"` — is wrong in two ways that both fail
/// *silently*:
///
/// - **fish does not have a `PATH` string.** `$PATH` there is a list, and
///   interpolating a list inside double quotes joins it with **spaces**. A fish
///   user's probe therefore returns one space-separated blob, and every later
///   lookup against it fails. Asking `env` instead reads the exported variable,
///   which is colon-joined in every shell by definition.
/// - **rc files print things.** A greeting, a `fortune`, a version-manager
///   warning — anything an rc file writes to stdout lands in the same capture
///   and corrupts the value. Bracketing the payload with a per-call marker
///   makes that noise identifiable rather than indistinguishable.
///
/// The marker carries the nanosecond clock so it cannot collide with anything
/// the shell might print of its own accord.
///
/// Only the `PATH=` line is read; the rest of the captured environment is
/// dropped with the buffer that held it. It routinely contains API tokens and
/// session secrets, so nothing here logs, stores or returns any of it.
#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn probe_login_path() -> Option<String> {
    let shell = match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => s,
        _ => return None,
    };
    probe_login_path_with(&shell)
}

/// [`probe_login_path`] against a named shell, so a test can hand it a script
/// that behaves like a chatty login shell without the machine having one.
#[cfg(not(target_os = "windows"))]
fn probe_login_path_with(shell: &str) -> Option<String> {
    probe_login_path_bounded(shell, PROBE_TIMEOUT)
}

#[cfg(not(target_os = "windows"))]
fn probe_login_path_bounded(shell: &str, timeout: Duration) -> Option<String> {
    let marker = probe_marker();
    let mut cmd = Command::new(shell);
    cmd.arg("-ilc")
        .arg(format!("echo -n \"{marker}\"; env; echo -n \"{marker}\""))
        // The convention VS Code established for exactly this probe, so an rc
        // file that already guards its slow half for one editor guards it here
        // too, for free.
        .env("LEOGIT_RESOLVING_ENVIRONMENT", "1")
        // `-i` makes this an *interactive* shell. Handed a terminal on stdin it
        // may try to take it over, and this can run from a test or a CLI launch
        // where there is one.
        .stdin(Stdio::null());
    prepare_child(&mut cmd);

    // `Group`, because a login shell is exactly the case the scope exists for:
    // an rc file that starts a daemon hands our stdout to a process that will
    // still hold it long after the shell we spawned has gone.
    match run_timed(cmd, "login shell PATH probe", timeout, KillScope::Group) {
        Ok(out) if out.status.success() => {
            let found = path_from_probe_output(&String::from_utf8_lossy(&out.stdout), &marker);
            if found.is_none() {
                eprintln!("[path] {shell} printed no PATH between the probe markers");
            }
            found
        }
        Ok(out) => {
            eprintln!("[path] {shell} exited with {}", out.status);
            None
        }
        Err(err) => {
            eprintln!("[path] {err}");
            None
        }
    }
}

/// A marker no shell would print by accident: this process, this instant.
#[cfg(not(target_os = "windows"))]
fn probe_marker() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("LEOGIT-{}-{nanos}", std::process::id())
}

/// The exported `PATH` from an `env` dump bracketed by `marker`, ignoring
/// everything outside the two markers and every variable but `PATH`.
///
/// A missing closing marker means the shell died mid-dump, which is exactly the
/// case where a naive parse would return a truncated `PATH` — so it returns
/// `None` rather than a value that looks plausible.
#[cfg(not(target_os = "windows"))]
fn path_from_probe_output(stdout: &str, marker: &str) -> Option<String> {
    let payload = stdout.split_once(marker)?.1.split_once(marker)?.0;
    payload
        .lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

/// No-op on Windows — GUI launches inherit the full user environment, so there
/// is no login shell to ask.
#[cfg(target_os = "windows")]
#[must_use]
pub fn probe_login_path() -> Option<String> {
    None
}

/// The user's login `PATH` and where it came from — the cache when it still
/// describes this machine, a fresh probe otherwise.
///
/// This is the function that takes ~430 ms off every launch, and the whole
/// question it answers is *when the cached value is wrong*; `path_cache` owns
/// that judgement and its reasoning. A miss re-probes and rewrites the cache,
/// so the cost is paid by the launch after the user edits their `.zshrc`, not
/// by every launch forever.
#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn resolve_login_path() -> Option<(String, PathSource)> {
    let shell = match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            eprintln!("[path] no $SHELL to ask; keeping the inherited PATH");
            return None;
        }
    };
    resolve_login_path_with(&shell)
}

/// [`resolve_login_path`] against a named shell — the seam the tests drive,
/// because the alternative is writing `$SHELL` into a running process.
#[cfg(not(target_os = "windows"))]
fn resolve_login_path_with(shell: &str) -> Option<(String, PathSource)> {
    match path_cache::load(shell) {
        Ok(hit) => {
            println!(
                "[path] cached login PATH ({} entries, probed {})",
                hit.path.split(':').count(),
                describe_age(hit.age)
            );
            return Some((hit.path, PathSource::Cached));
        }
        // One line, naming the *first* reason the cache was rejected: with a
        // ~430 ms probe on the other side of it, "why did this launch take
        // half a second" has to be answerable from the log.
        Err(reason) => println!("[path] probing {shell}: {reason}"),
    }

    let path = probe_login_path_with(shell)?;
    if let Err(err) = path_cache::store(shell, &path) {
        // Best effort by design: a cache we could not write costs the next
        // launch ~430 ms and nothing else, so it must never fail a startup.
        eprintln!("[path] could not cache the probed PATH: {err}");
    }
    Some((path, PathSource::Probed))
}

/// No login shell on Windows, so nothing to cache and nothing to resolve.
#[cfg(target_os = "windows")]
#[must_use]
pub fn resolve_login_path() -> Option<(String, PathSource)> {
    None
}

/// A cache age in the coarsest unit that still says something useful.
#[cfg(not(target_os = "windows"))]
fn describe_age(age: Duration) -> String {
    let secs = age.as_secs();
    match secs {
        0..=89 => format!("{secs}s ago"),
        90..=5399 => format!("{}m ago", secs / 60),
        5400..=172_799 => format!("{}h ago", secs / 3_600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// Replace this process's `PATH` with the user's interactive login `PATH`.
///
/// macOS/Linux apps launched from Finder or a `.desktop` entry inherit a
/// minimal `PATH` (e.g. `/usr/bin:/bin:/usr/sbin:/sbin`) and miss
/// user-installed binaries like `claude`, `gh`, or homebrew tools.
/// [`resolve_login_path`] answers what their terminal would have — from the
/// cache when it still holds, and only otherwise by spawning their shell.
/// No-op on Windows, where GUI launches inherit the full user environment
/// already.
///
/// Call this once at the very top of the host's startup — before the UI
/// framework, the tokio runtime, or any worker thread exists — because it
/// writes the process environment, which is only sound while nothing else
/// can be reading it concurrently. Both hosts do exactly that: the Tauri app
/// at the top of `main`, the `SwiftUI` app in its `App.init`.
///
/// The one qualification: a probe that had to ask the shell leaves
/// [`run_timed`]'s helper threads winding down behind it, and a reader whose
/// pipe a survivor still holds may outlive the call outright. What the contract
/// actually needs is that no *other* thread touch the environment, and those
/// threads never do — they read a pipe, take a mutex, and end.
///
/// It stays synchronous for that reason and no other. The cache is what took
/// the wait out of it; moving the work off the critical path would mean
/// writing the environment from a thread, which this contract forbids. What
/// runs later instead is [`spawn_path_reprobe`], which never touches the
/// environment at all.
pub fn fix_path_env() {
    if let Some((path, source)) = resolve_login_path() {
        // Read by `spawn_path_reprobe`: a launch that already asked the shell
        // has the truth, and re-asking it would be a pure waste of a spawn.
        STARTUP_USED_CACHE.store(source == PathSource::Cached, Ordering::Relaxed);
        // SAFETY: per this function's contract, callers invoke it before
        // any other thread exists, so nothing else can be reading the
        // environment concurrently. Edition 2024 marks `set_var` unsafe
        // precisely to guard against that data race.
        unsafe {
            std::env::set_var("PATH", path);
        }
    }
}

/// Ask the login shell for real, in the background, when startup trusted the
/// cache — and hand anything new to the children spawned from now on.
///
/// Hosts call this once the UI exists; ~430 ms on a worker thread is invisible
/// there, and it is the other half of what makes the cache safe. The cache's
/// staleness key is good but not omniscient (a version manager can change what
/// an unchanged `.zshrc` evaluates to), so this is the check that closes the
/// gap — within seconds of launch rather than on the next one.
///
/// A no-op when startup probed the shell itself: that value *is* the fresh one.
///
/// It deliberately never calls `set_var`. Writing the process environment is
/// sound only while no other thread can read it, and by the time this runs the
/// whole app is running — so the correction is delivered to children through
/// [`prepare_child`] instead, which is the only place a `PATH` was ever used.
/// The soundness contract belongs to [`fix_path_env`] alone.
#[cfg(not(target_os = "windows"))]
pub fn spawn_path_reprobe() {
    /// Second and later calls are no-ops: a host that grows another startup
    /// path should not get another probe.
    static STARTED: std::sync::Once = std::sync::Once::new();

    if !STARTUP_USED_CACHE.load(Ordering::Relaxed) {
        return;
    }
    STARTED.call_once(|| {
        // Named, because a thread that spawns a shell shows up in every sampler
        // and crash report the user might send us.
        let started = std::thread::Builder::new()
            .name("leogit-path-reprobe".to_string())
            .spawn(reprobe_login_path);
        if let Err(err) = started {
            eprintln!("[path] could not start the background re-probe: {err}");
        }
    });
}

/// The background worker: probe, rewrite the cache, and publish the result to
/// later children only if it actually differs from what startup installed.
#[cfg(not(target_os = "windows"))]
fn reprobe_login_path() {
    let Ok(shell) = std::env::var("SHELL") else {
        return;
    };
    let Some(fresh) = probe_login_path_with(&shell) else {
        return;
    };
    if let Err(err) = path_cache::store(&shell, &fresh) {
        eprintln!("[path] could not refresh the PATH cache: {err}");
    }
    // Reading the environment is fine from here; only writing it is not.
    if fresh == std::env::var("PATH").unwrap_or_default() {
        println!("[path] re-probe agrees with the cached PATH");
        return;
    }
    println!(
        "[path] re-probe found a different PATH ({} entries); applying it to new children",
        fresh.split(':').count()
    );
    set_fresh_path(Some(fresh));
}

/// The one writer of [`FRESH_PATH`], so the lock discipline lives in a single
/// place and a test can undo what it set.
#[cfg(not(target_os = "windows"))]
fn set_fresh_path(path: Option<String>) {
    *FRESH_PATH.write().unwrap_or_else(PoisonError::into_inner) = path;
}

/// No login shell to re-ask on Windows.
#[cfg(target_os = "windows")]
pub fn spawn_path_reprobe() {}

/// How long a pipe reader gets to finish after the child itself is gone.
///
/// Nothing can *unblock* a reader whose pipe is still held open by a process we
/// did not kill: `read` on a pipe with a live writer blocks by definition, and
/// closing the descriptor behind `std`'s back is a double-close hazard rather
/// than a wake-up. So the only sound bound is to stop waiting for it — after
/// this window the reader thread is detached, and whatever it had captured by
/// then is what the caller gets.
///
/// Detached, not left running: it is also told to stop, so it ends at its next
/// read rather than following a chatty survivor for that survivor's lifetime.
/// See [`drain`] for what "next read" costs.
const READER_GRACE: Duration = Duration::from_secs(2);

/// Callback fed each stderr line as it arrives (see [`run_timed_streaming`]).
type StderrLineFn = Box<dyn FnMut(&str) + Send>;

/// What a [`run_timed`] timeout is allowed to kill.
///
/// The distinction is the whole of finding F13. `git fetch` does not talk to
/// the network itself — it spawns `ssh` or `git-remote-https`, and those
/// grandchildren inherit our pipe write ends. Killing only the process we
/// started therefore leaves the transport running *and* the stderr reader
/// blocked on a pipe that never closes, so the hard timeout that exists
/// precisely as the backstop bounded nothing in the one case it was written
/// for: it burned a worker thread instead, and for a user-initiated pull the
/// spinner never stopped.
///
/// It cannot simply be the default either, which is why this is a parameter and
/// not a change to [`prepare_child`]. A hand-off launcher (`open`, `xdg-open`,
/// `cmd /c start`) exists to start something that outlives it — often the
/// user's browser, in the same process group — so a group kill there would
/// close the window it had just opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillScope {
    /// Give the child a process group of its own (a job object on Windows) and
    /// let the timeout kill all of it. Every network command wants this.
    Group,
    /// Kill only the process we spawned, leaving anything it started alone.
    /// For the hand-off launchers, whose children are the user's, not ours.
    Child,
}

/// Whether a timed run reads the child's output at all.
///
/// A separate axis from [`KillScope`] on purpose: what a timeout may kill and
/// whether the caller wants the bytes are independent questions, and folding
/// one into the other is how a launcher ends up paying a reader grace it has no
/// use for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capture {
    /// Pipe stdout and stderr and drain both — everything that reads a result.
    Both,
    /// Send both streams to `/dev/null` (`NUL`). Nothing is piped, so no
    /// descendant can hold a pipe of ours open and there is no reader to wait
    /// for; the returned [`Output`] carries the status and two empty buffers.
    None,
}

/// Run a fully blocking task on tokio's dedicated blocking pool and await it.
///
/// A `#[tauri::command(async)]` sync fn runs inline inside a spawned future on
/// a tokio *core* worker (one of ~num-cpus) — fine for a quick subprocess, but
/// a transfer that can legitimately run for minutes (push, pull, clone) would
/// pin a core worker for its whole duration and, on a low-core machine, starve
/// every other command. The blocking pool exists precisely for such tasks, so
/// the long-running network commands are `async fn`s that delegate here.
///
/// # Errors
/// When the blocking task panics or the runtime is shutting down.
pub async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Run `cmd` to completion or until `timeout` elapses, whichever comes first,
/// returning its [`Output`] (status + captured stdout/stderr).
///
/// This is the single chokepoint that keeps a network subprocess (`git fetch`,
/// `gh repo list`, …) from blocking a worker thread forever when the machine is
/// offline or the connection is flaky: if the child outlives `timeout` it is
/// killed — together with everything it started, under [`KillScope::Group`] —
/// and an `Err` is returned, tagged with `label` so the caller can treat it as
/// a transient/offline condition.
///
/// Three helper threads make that bound real. Both pipes are drained on their
/// own so a chatty child (git progress on stderr) can never deadlock by filling
/// a pipe buffer while we wait for it, and the child is waited on by a third,
/// which reports its exit over a channel — so `timeout` is a `recv_timeout` and
/// a fetch that finishes in 120 ms is observed at 120 ms. The readers are then
/// given [`READER_GRACE`] and *detached* if they need longer, which is what
/// makes this function's return unconditional: no descendant can hold it open
/// by holding a pipe.
///
/// # Errors
/// Returns `Err` if the process fails to spawn, can't be waited on, or exceeds
/// `timeout` (in which case it is killed first). A non-zero exit is *not* an
/// error — inspect [`Output::status`] for that.
pub fn run_timed(
    cmd: Command,
    label: &str,
    timeout: Duration,
    scope: KillScope,
) -> Result<Output, String> {
    run_timed_inner(cmd, label, timeout, scope, Capture::Both, None)
}

/// [`run_timed`] for a caller that throws the output away: both streams are
/// sent to `/dev/null` (`NUL`) instead of being piped, so no reader thread is
/// started and no [`READER_GRACE`] is ever paid.
///
/// That grace is the whole reason this exists. A hand-off launcher is *done*
/// the instant it has handed off, but `xdg-open` execs the registered handler,
/// which daemonises still holding our stdout and stderr — so no EOF arrives,
/// both readers have to be detached, and "Reveal in file manager" resolves two
/// seconds after the work it reports on finished. With nothing piped there is
/// nothing for the survivor to hold, and the call costs what the launcher
/// costs.
///
/// Only for a caller that genuinely ignores the bytes: the [`Output`] comes
/// back with its status and two empty buffers, so a message built from `stderr`
/// would be built from nothing.
///
/// # Errors
/// Same contract as [`run_timed`].
pub fn run_timed_uncaptured(
    cmd: Command,
    label: &str,
    timeout: Duration,
    scope: KillScope,
) -> Result<Output, String> {
    run_timed_inner(cmd, label, timeout, scope, Capture::None, None)
}

/// [`run_timed`], but each line the child writes to stderr is also handed to
/// `on_stderr_line` as it arrives — the transport for live `git --progress`
/// output. Git repaints its progress meter with carriage returns, so "lines"
/// are split on `\r` as well as `\n`; the full stderr is still captured in the
/// returned [`Output`] for error reporting.
///
/// The callback runs on the stderr reader thread, so it must not block: a slow
/// callback would stall the pipe drain this function exists to guarantee.
///
/// # Errors
/// Same contract as [`run_timed`].
pub fn run_timed_streaming(
    cmd: Command,
    label: &str,
    timeout: Duration,
    scope: KillScope,
    on_stderr_line: impl FnMut(&str) + Send + 'static,
) -> Result<Output, String> {
    run_timed_inner(
        cmd,
        label,
        timeout,
        scope,
        Capture::Both,
        Some(Box::new(on_stderr_line)),
    )
}

fn run_timed_inner(
    mut cmd: Command,
    label: &str,
    timeout: Duration,
    scope: KillScope,
    capture: Capture,
    on_stderr_line: Option<StderrLineFn>,
) -> Result<Output, String> {
    let (out, err) = match capture {
        Capture::Both => (Stdio::piped(), Stdio::piped()),
        Capture::None => (Stdio::null(), Stdio::null()),
    };
    cmd.stdout(out).stderr(err);
    if scope == KillScope::Group {
        #[cfg(unix)]
        {
            // A group of its own is what lets the timeout reach `ssh` and
            // `git-remote-https`. Never combined with `setsid`: a session
            // leader is more than is needed here, and `process_group` is the
            // stable spelling of the part that matters.
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        // A background process group that reads the terminal is *stopped* with
        // SIGTTIN, which would look exactly like the hang this scope exists to
        // prevent. Nothing run this way is interactive — `GIT_TERMINAL_PROMPT=0`
        // and ssh's `BatchMode=yes` say so — so an empty stdin is the honest
        // one. (Windows has no such signal; it is set there for symmetry, and
        // because a timed command that reads stdin is a bug either way.)
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn().map_err(|e| format!("{label}: {e}"))?;

    // Taken now, because the `Child` moves onto the waiter thread below and
    // this is the last moment anything else can see it.
    let killer = match Killer::attach(&child, scope) {
        Ok(killer) => killer,
        Err(err) => {
            // A child the timeout could never reach is worse than no child at
            // all, so this fails loudly rather than running one unbounded.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{label}: {err}"));
        }
    };

    // Move the pipe ends onto reader threads so buffering can't stall the
    // child. They append into shared buffers instead of returning one at the
    // end, so a reader that has to be abandoned still leaves behind everything
    // it did manage to read.
    let stdout_sink = Arc::new(Mutex::new(Vec::new()));
    let stderr_sink = Arc::new(Mutex::new(Vec::new()));
    // Raised once the collector below has taken what it wants and stopped
    // looking, so a detached reader stops appending into a buffer nobody will
    // ever read again. Without it a chatty survivor could grow that buffer for
    // as long as it held the pipe — unbounded memory behind a call that has
    // already returned.
    let abandoned = Arc::new(AtomicBool::new(false));
    let (drained_tx, drained_rx) = mpsc::channel::<()>();
    // `Capture::None` pipes nothing, so there is no reader to start and nothing
    // to wait for; the collection loop below is then a no-op by construction.
    let readers = if capture == Capture::Both {
        let mut out_pipe = child.stdout.take();
        let out_sink = Arc::clone(&stdout_sink);
        let out_abandoned = Arc::clone(&abandoned);
        let out_drained = drained_tx.clone();
        // Deliberately never joined — see `READER_GRACE`.
        let _out_reader = std::thread::spawn(move || {
            if let Some(pipe) = out_pipe.as_mut() {
                drain(pipe, &out_sink, &out_abandoned, None);
            }
            let _ = out_drained.send(());
        });
        let mut err_pipe = child.stderr.take();
        let err_sink = Arc::clone(&stderr_sink);
        let err_abandoned = Arc::clone(&abandoned);
        let _err_reader = std::thread::spawn(move || {
            if let Some(pipe) = err_pipe.as_mut() {
                drain(pipe, &err_sink, &err_abandoned, on_stderr_line);
            }
            let _ = drained_tx.send(());
        });
        2
    } else {
        0
    };

    // The child is waited on by a thread of its own and its exit arrives over a
    // channel, so `recv_timeout` *is* the budget. The poll this replaced woke
    // every 50 ms and quantised every network operation onto that grid (F29).
    let (exited_tx, exited_rx) = mpsc::channel();
    let _waiter = std::thread::spawn(move || {
        let _ = exited_tx.send(child.wait());
    });

    let (status, deadline) = match exited_rx.recv_timeout(timeout) {
        // The child is gone, so the grace starts here: what is left to wait for
        // is whatever still holds a copy of its pipes.
        Ok(Ok(status)) => (Some(status), Instant::now() + READER_GRACE),
        // `wait` itself failed: the child is unaccounted for, so kill it on the
        // way out rather than leaving it behind with our pipes.
        Ok(Err(e)) => {
            killer.kill();
            return Err(format!("{label}: {e}"));
        }
        Err(_) => {
            killer.kill();
            // One deadline for the reap *and* the readers. SIGKILL is not
            // refusable so the reap lands almost at once, and whatever is left
            // of the window is what the readers get — a fresh one for each
            // would let a timed-out call overrun its budget by twice the grace
            // this file documents. The reaped status is the killed one, which
            // is not the answer the caller gets; the timeout is.
            let deadline = Instant::now() + READER_GRACE;
            drop(exited_rx.recv_timeout(READER_GRACE));
            (None, deadline)
        }
    };

    // One grace window shared by both readers rather than one each: the bound
    // belongs to this call, and two 2 s waits in series would be a 4 s one.
    for _ in 0..readers {
        let left = deadline.saturating_duration_since(Instant::now());
        if drained_rx.recv_timeout(left).is_err() {
            break;
        }
    }
    // Before the sinks are emptied, so the window in which a still-running
    // reader appends bytes that will never be read is as short as it can be.
    abandoned.store(true, Ordering::Relaxed);
    let stdout = take_captured(&stdout_sink);
    let stderr = take_captured(&stderr_sink);

    match status {
        // A child that exited on its own succeeded, even if a reader had to be
        // abandoned: a hook that backgrounded something without redirecting its
        // output is the child's business, not this call's failure.
        Some(status) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        None => Err(format!(
            "{label} timed out after {}s (network unreachable?)",
            timeout.as_secs()
        )),
    }
}

/// Read `pipe` to EOF, appending into `sink` as it goes and — for the streaming
/// variant — handing each finished line to `on_line`.
///
/// The sink is filled incrementally rather than returned at the end so an
/// abandoned reader still leaves its partial capture behind, and the lock is
/// never held across a `read`: the collector has to be able to take what is
/// there while this thread sits on a pipe that may never close.
///
/// `abandoned` is how that thread learns the collector has stopped looking. It
/// is checked *after* a read rather than before, because a blocked `read` is
/// not interruptible from here — so this thread may still sit in one until the
/// survivor's next chunk or its EOF, whichever comes first. That is a parked
/// thread, which costs a stack; what the flag ends is the *growth*, which is
/// what a chatty long-lived grandchild would otherwise make unbounded.
fn drain(
    pipe: &mut impl Read,
    sink: &Mutex<Vec<u8>>,
    abandoned: &AtomicBool,
    mut on_line: Option<StderrLineFn>,
) {
    let mut chunk = [0u8; 8192];
    // Only the tail after the last break is carried, so a long progress stream
    // costs one line of memory rather than a transcript.
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let read = match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => &chunk[..n],
        };
        // `Relaxed`: nothing else is ordered by this flag, and the sink it
        // guards has a mutex of its own.
        if abandoned.load(Ordering::Relaxed) {
            // No trailing-line flush either — the callback's consumer returned
            // with the call that owned it.
            return;
        }
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(read);
        let Some(on_line) = on_line.as_mut() else {
            continue;
        };
        pending.extend_from_slice(read);
        while let Some(end) = pending.iter().position(|&b| b == b'\n' || b == b'\r') {
            on_line(&String::from_utf8_lossy(&pending[..end]));
            pending.drain(..=end);
        }
    }
    // The last line need not be terminated; git's final progress repaint isn't.
    if let Some(on_line) = on_line.as_mut()
        && !pending.is_empty()
    {
        on_line(&String::from_utf8_lossy(&pending));
    }
}

/// Everything captured so far, leaving the sink empty for a reader that may
/// still be writing into it.
fn take_captured(sink: &Mutex<Vec<u8>>) -> Vec<u8> {
    std::mem::take(&mut *sink.lock().unwrap_or_else(PoisonError::into_inner))
}

/// What a timeout kills the child through, taken at spawn because the `Child`
/// itself moves onto the waiter thread and `wait` needs it exclusively.
#[cfg(unix)]
struct Killer {
    pid: libc::pid_t,
    scope: KillScope,
}

#[cfg(unix)]
impl Killer {
    /// A pid is all a signal needs, so this cannot fail in practice.
    fn attach(child: &std::process::Child, scope: KillScope) -> Result<Self, String> {
        let pid = libc::pid_t::try_from(child.id())
            .map_err(|_| "child pid does not fit in a pid_t".to_string())?;
        Ok(Self { pid, scope })
    }

    /// Signal *before* the waiter reaps. An unreaped child is a zombie and a
    /// zombie's pid stays reserved, which is what keeps the group id we signal
    /// from having been recycled onto somebody else's group in the meantime.
    ///
    /// The waiter can in principle return from `wait` in the instant between
    /// the budget expiring and this call, and there is no portable way to hold
    /// a pid without reaping it. The guards are what keep that loser harmless:
    /// never pid 1, and never our own group.
    fn kill(&self) {
        if self.pid <= 1 {
            return;
        }
        match self.scope {
            // `process_group(0)` made the child the leader of a new group, so
            // its pid is also its pgid.
            KillScope::Group => {
                // SAFETY: both calls are always safe to make; the pid is one we
                // spawned and, but for the race above, have not yet reaped.
                unsafe {
                    if self.pid != libc::getpgrp() {
                        libc::killpg(self.pid, libc::SIGKILL);
                    }
                }
            }
            // SAFETY: as above.
            KillScope::Child => unsafe {
                libc::kill(self.pid, libc::SIGKILL);
            },
        }
    }
}

/// The Windows mirror of the unix [`Killer`]: a duplicated process handle
/// stands in for the pid, and a job object stands in for the process group.
///
/// A handle names the kernel's process *object* rather than a number, so unlike
/// the unix side there is no window in which a reaped pid could have been
/// recycled — terminating a handle whose process has already gone answers
/// `ERROR_ACCESS_DENIED` instead of killing a stranger. The duplicate is taken
/// on this thread while the `Child` is still ours, and closing it later does
/// not disturb the waiter thread's own handle.
///
/// **Unverified on the machine this was written on** (no Windows toolchain
/// here): every signature is read from the pinned `windows-sys` 0.61 source,
/// and the structure deliberately mirrors the unix arm line for line so the two
/// can be reviewed against each other.
#[cfg(windows)]
struct Killer {
    process: std::os::windows::io::OwnedHandle,
    /// Present only for [`KillScope::Group`], and only where the host let us
    /// create one — a job is what makes a kill reach descendants, and giving
    /// one to a hand-off launcher would kill the application it was asked to
    /// start.
    job: Option<JobHandle>,
}

#[cfg(windows)]
impl Killer {
    fn attach(child: &std::process::Child, scope: KillScope) -> Result<Self, String> {
        /// Raised the first time a job could not be created, because the cause
        /// is a property of how this app was launched: it will fail identically
        /// for every command afterwards, and a line each would be noise.
        static REPORTED: std::sync::Once = std::sync::Once::new();

        use std::os::windows::io::AsHandle;
        let process = child
            .as_handle()
            .try_clone_to_owned()
            .map_err(|e| format!("could not duplicate the child's handle: {e}"))?;
        let job = match scope {
            // A failure here is degraded, not fatal. A host already inside a
            // job that forbids nesting cannot give the child one of its own,
            // and refusing to run would turn that into *every* network command
            // failing. Falling back to the duplicated handle above is exactly
            // [`KillScope::Child`]: the process we spawned is still killed on
            // time, and only its descendants survive — which is the behaviour
            // this platform had before job objects existed at all. The failed
            // handle is closed on the way out by [`JobHandle`]'s `Drop`.
            KillScope::Group => JobHandle::create_and_assign(child)
                .inspect_err(|err| {
                    REPORTED.call_once(|| {
                        eprintln!(
                            "[process] {err}; a timeout will reach only the process we spawned"
                        );
                    });
                })
                .ok(),
            KillScope::Child => None,
        };
        Ok(Self { process, job })
    }

    /// Terminate the job (everything the child started) or just the child,
    /// matching the scope. Both calls are asynchronous — the waiter thread's
    /// `wait` returning is the confirmation, exactly as on unix.
    fn kill(&self) {
        use std::os::windows::io::AsRawHandle;
        let outcome = match &self.job {
            Some(job) => job.terminate(),
            // SAFETY: the handle is owned by `self.process` and open for the
            // whole call; it carries `PROCESS_TERMINATE` because
            // `try_clone_to_owned` duplicates with `DUPLICATE_SAME_ACCESS`.
            None => match unsafe {
                windows_sys::Win32::System::Threading::TerminateProcess(
                    self.process.as_raw_handle(),
                    1,
                )
            } {
                0 => Err(std::io::Error::last_os_error()),
                _ => Ok(()),
            },
        };
        if let Err(err) = outcome {
            // A child that exited between the budget expiring and this call
            // answers `ERROR_ACCESS_DENIED`, which is the documented way of
            // saying "already gone" — not a failure worth reporting.
            if err.raw_os_error()
                != i32::try_from(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED).ok()
            {
                eprintln!("[process] could not kill a timed-out child: {err}");
            }
        }
    }
}

/// An unnamed job object holding one timed child, with kill-on-close so a crash
/// of *this* process cannot leave a wedged `ssh` behind holding a ref lock.
///
/// That flag is also the one place Windows behaves differently from unix: a
/// process the child deliberately backgrounded is ended when the job closes,
/// where unix leaves it running. It is the trade the flag exists to make, and
/// it only reaches processes a bounded network command started.
#[cfg(windows)]
struct JobHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl JobHandle {
    fn create_and_assign(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };

        // SAFETY: both arguments are optional; null means a default security
        // descriptor and an unnamed job.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(format!(
                "could not create a job object: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Owned from here, so every failure below closes it.
        let owned = Self(job);

        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };
        let len = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| "the job limit struct does not fit a DWORD".to_string())?;
        // SAFETY: `info` is a live, correctly sized value of exactly the
        // information class named by the second argument.
        let set = unsafe {
            SetInformationJobObject(
                owned.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                len,
            )
        };
        if set == 0 {
            return Err(format!(
                "could not set the job's limits: {}",
                std::io::Error::last_os_error()
            ));
        }

        // SAFETY: `child` is borrowed for the call, so its handle is open; it
        // stays owned by `child` and is not closed here.
        if unsafe { AssignProcessToJobObject(owned.0, child.as_raw_handle()) } == 0 {
            return Err(format!(
                "could not put the child in its job: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(owned)
    }

    fn terminate(&self) -> std::io::Result<()> {
        // SAFETY: `self.0` is an open job handle owned by `self`.
        if unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is owned by `self` and closed exactly once here.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a login shell, written to disk by [`fake_shell`].
    ///
    /// It behaves like the shells that break the naive probe: it greets the
    /// user on stdout, sets a `PATH` of its own, runs whatever `-ilc` handed
    /// it, and signs off afterwards. `$1` after the `shift` is the script,
    /// because the real invocation is `<shell> -ilc '<script>'`.
    #[cfg(not(target_os = "windows"))]
    const CHATTY_SHELL: &str = "#!/bin/sh\n\
         echo 'Welcome! You have mail.'\n\
         PATH=/fake/leogit/bin:/usr/bin\n\
         export PATH\n\
         shift\n\
         /bin/sh -c \"$1\"\n\
         echo\n\
         echo 'PATH=/decoy-printed-after-the-marker'\n";

    /// A shell that never answers, for the timeout path.
    ///
    /// A plain child rather than an `exec`, deliberately: the hanging process
    /// is a *descendant* of the one we spawned, holding the same inherited
    /// pipes. That used to be the shape nothing bounded — killing the shell
    /// left the `sleep` with our stdout and the reader blocked on it for the
    /// full 30 s — and it is the shape [`KillScope::Group`] exists to bound.
    #[cfg(not(target_os = "windows"))]
    const SLEEPY_SHELL: &str = "#!/bin/sh\nsleep 30\n";

    /// Write `body` into `dir` as an executable file and return its path.
    #[cfg(not(target_os = "windows"))]
    fn fake_shell(dir: &std::path::Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write the fake shell");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod +x");
        path.to_string_lossy().into_owned()
    }

    /// The parse is the whole defence against an rc file that prints: only
    /// what lies between the two markers counts, and only its `PATH=` line.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn only_the_path_between_the_markers_is_read() {
        let marker = "LEOGIT-1-2";
        let output = format!(
            "you have mail\nPATH=/before-the-marker\n\
             {marker}SHELL=/bin/zsh\nPATH=/opt/homebrew/bin:/usr/bin\nHOME=/Users/x\n\
             {marker}\ngoodbye\nPATH=/after-the-marker\n"
        );
        assert_eq!(
            path_from_probe_output(&output, marker).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );

        // No markers at all: the shell failed before running our script.
        assert_eq!(path_from_probe_output("PATH=/usr/bin\n", marker), None);
        // Opening marker only: the dump was truncated, so the `PATH` we can
        // see may be too. Refusing beats returning a plausible half.
        assert_eq!(
            path_from_probe_output(&format!("{marker}PATH=/usr/bin"), marker),
            None
        );
        // Markers, but nothing we asked for between them.
        assert_eq!(
            path_from_probe_output(&format!("{marker}HOME=/Users/x\n{marker}"), marker),
            None
        );
        // An exported-but-empty PATH is not an answer either.
        assert_eq!(
            path_from_probe_output(&format!("{marker}PATH=\n{marker}"), marker),
            None
        );
    }

    /// End to end against a shell that prints before *and* after the payload:
    /// the value that comes back is the one the shell exported, not its
    /// chatter — and not the decoy `PATH=` line it prints once we are done.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_probe_reads_past_a_chatty_login_shell() {
        let dir = tempfile::tempdir().expect("tempdir");
        let shell = fake_shell(dir.path(), "chatty", CHATTY_SHELL);
        let probed = probe_login_path_bounded(&shell, Duration::from_secs(10));
        assert_eq!(probed.as_deref(), Some("/fake/leogit/bin:/usr/bin"));
    }

    /// A login shell that hangs must cost the launch its budget and no more.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_hanging_login_shell_times_out_instead_of_wedging_startup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let shell = fake_shell(dir.path(), "sleepy", SLEEPY_SHELL);
        let started = Instant::now();
        assert_eq!(
            probe_login_path_bounded(&shell, Duration::from_millis(300)),
            None
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the probe outlived its budget: {:?}",
            started.elapsed()
        );
    }

    /// The point of the whole exercise: the first launch pays for a shell, the
    /// next one reads a file.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_second_resolve_answers_from_the_cache() {
        let home = crate::config::ConfigHome::new();
        let dir = tempfile::tempdir().expect("tempdir");
        let shell = fake_shell(dir.path(), "chatty", CHATTY_SHELL);

        let (probed, source) = resolve_login_path_with(&shell).expect("first resolve");
        assert_eq!(source, PathSource::Probed);
        assert_eq!(probed, "/fake/leogit/bin:/usr/bin");
        assert!(
            home.path().join(path_cache::CACHE_FILE_NAME).exists(),
            "a probe must leave a cache behind"
        );

        let (cached, source) = resolve_login_path_with(&shell).expect("second resolve");
        assert_eq!(source, PathSource::Cached);
        assert_eq!(cached, probed);
    }

    /// What the background re-probe buys: a child prepared after it runs
    /// carries the new `PATH`, without anyone having written the environment.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_reprobed_path_reaches_the_next_child() {
        // The static is process-wide and other tests spawn `git` while this
        // one runs, so the value installed here has to remain a *usable*
        // `PATH`: a marker prepended to the real one, never a replacement.
        let inherited = std::env::var("PATH").unwrap_or_default();
        let reprobed = format!("/leogit-test-reprobed-entry:{inherited}");
        set_fresh_path(Some(reprobed.clone()));

        let mut cmd = Command::new("git");
        prepare_child(&mut cmd);
        let applied = cmd
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());

        set_fresh_path(None);
        assert_eq!(applied.as_deref(), Some(reprobed.as_str()));
        assert_eq!(
            fresh_path(),
            None,
            "the static must be left as it was found"
        );
    }

    #[test]
    fn run_timed_captures_a_quick_command() {
        let mut cmd = Command::new("git");
        cmd.arg("--version");
        let out = run_timed(
            cmd,
            "git --version",
            Duration::from_secs(10),
            KillScope::Group,
        )
        .expect("git --version");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("git version"));
    }

    // The spawn counter is the unit every I/O-efficiency claim is denominated
    // in, so it has to be exact: one prepared child, one tick — never two for a
    // command that also gets a timeout, which is the miscount that would make
    // every network op in the bench look twice as expensive as it is.
    //
    // The counter is process-wide and the harness runs other spawning tests on
    // other threads, so even a window as narrow as the `prepare_child` call
    // itself can catch somebody else's atomic add. Every concurrent spawn can
    // only *increase* the delta, so the smallest of several readings is the
    // uncontended one — and it would still read 2 if `prepare_child` ticked
    // twice, which is the miscount that would make every network op in the
    // bench look twice as expensive as it is.
    #[test]
    fn preparing_a_child_advances_the_spawn_counter_by_one() {
        let delta = (0..16)
            .map(|_| {
                let mut cmd = Command::new("git");
                cmd.arg("--version");
                let before = spawn_count();
                prepare_child(&mut cmd);
                spawn_count() - before
            })
            .min()
            .expect("sixteen readings");
        assert_eq!(delta, 1, "one prepared child must tick the counter once");

        // And the counted command still runs: a counter that only ever agreed
        // with itself would say nothing about the commands it attributes.
        let mut cmd = Command::new("git");
        cmd.arg("--version");
        prepare_child(&mut cmd);
        let out = run_timed(
            cmd,
            "git --version",
            Duration::from_secs(10),
            KillScope::Group,
        )
        .expect("git --version");
        assert!(out.status.success());
    }

    // Git repaints its progress meter with bare carriage returns, so the
    // streaming reader must treat `\r` as a line break, deliver the trailing
    // unterminated chunk, and still hand back the byte-exact stderr capture.
    #[cfg(unix)]
    #[test]
    fn run_timed_streaming_splits_stderr_on_cr_and_lf() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'a\\rb\\nc' 1>&2"]);
        let (tx, rx) = mpsc::channel();
        let out = run_timed_streaming(
            cmd,
            "sh",
            Duration::from_secs(10),
            KillScope::Group,
            move |line| {
                let _ = tx.send(line.to_string());
            },
        )
        .expect("sh must run");
        assert!(out.status.success());
        let lines: Vec<String> = rx.try_iter().collect();
        assert_eq!(lines, ["a", "b", "c"]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), "a\rb\nc");
    }

    // The whole point of run_timed: a child that outlives its budget is killed
    // and reported as a timeout, promptly — not waited out. `sleep` keeps this
    // portable across Unix; Windows lacks an equivalent stock binary.
    #[cfg(unix)]
    #[test]
    fn run_timed_kills_a_hung_child_promptly() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let started = Instant::now();
        let res = run_timed(cmd, "sleep", Duration::from_millis(300), KillScope::Group);
        let err = res.expect_err("a 30s sleep must exceed a 300ms budget");
        assert!(err.contains("timed out"), "unexpected error: {err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "child was not killed promptly: {:?}",
            started.elapsed()
        );
    }

    /// Whether `pid` still exists — signal 0 checks for the process without
    /// sending anything, and `ESRCH` is how the kernel says "gone".
    #[cfg(unix)]
    fn process_is_alive(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 delivers nothing; it only performs the existence
        // and permission checks.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    // F13, the regression this whole design exists for: `git fetch` does its
    // networking through a grandchild, so a timeout that only kills the child
    // bounds nothing. The stand-in is a shell that backgrounds a `sleep`
    // holding the same inherited pipes and then hangs itself.
    //
    // Two assertions, and both matter: that the call returns on time (the old
    // code returned after 30 s, because the stderr reader was still waiting on
    // the grandchild's copy of the pipe), and that the grandchild is actually
    // dead afterwards rather than merely detached from us.
    #[cfg(unix)]
    #[test]
    fn a_timeout_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pid_file = dir.path().join("grandchild.pid");
        let mut cmd = Command::new("sh");
        // `printf` rather than `echo -n`, which prints its own `-n` under the
        // `/bin/sh` macOS ships.
        cmd.arg("-c").arg(format!(
            "sleep 30 & printf '%s' \"$!\" > '{}'; sleep 30",
            pid_file.display()
        ));

        let started = Instant::now();
        let err = run_timed(cmd, "sh", Duration::from_millis(300), KillScope::Group)
            .expect_err("a 30s sleep must exceed a 300ms budget");
        assert!(err.contains("timed out"), "unexpected error: {err}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the call was held open by the grandchild: {:?}",
            started.elapsed()
        );

        let recorded = std::fs::read_to_string(&pid_file).expect("the shell must record its child");
        let pid: libc::pid_t = recorded.trim().parse().expect("a pid");
        // SIGKILL delivery is not instantaneous and the group leader has to
        // exit before the kernel reaps it, so poll rather than assert once.
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_is_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !process_is_alive(pid),
            "the grandchild survived the group kill (pid {pid})"
        );
    }

    // The universal backstop, for the case the group kill deliberately does
    // *not* cover: the child exited by itself, cleanly, having left something
    // running that holds our pipes. Killing that is not ours to do — a hook
    // that backgrounds a process without redirecting is the child's business —
    // so the reader is abandoned instead, and the child's own success is
    // reported after the grace window rather than after the survivor's
    // lifetime.
    #[cfg(unix)]
    #[test]
    fn a_normal_exit_is_not_held_open_by_a_lingering_grandchild() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 3 & exit 0");
        let started = Instant::now();
        let out = run_timed(cmd, "sh", Duration::from_secs(30), KillScope::Group)
            .expect("the shell exits successfully");
        assert!(out.status.success());
        assert!(
            started.elapsed() < READER_GRACE + Duration::from_millis(500),
            "the abandoned reader was waited out instead of detached: {:?}",
            started.elapsed()
        );
    }

    // The other half of abandoning a reader: it has to stop *filling* the sink,
    // not merely stop being waited for. A survivor that keeps writing would
    // otherwise grow a buffer nobody will read for as long as it holds the
    // pipe, which is a leak behind a call that already returned.
    //
    // Driven against `drain` directly rather than through `run_timed`, because
    // the condition is about what the thread does after the call it served is
    // over — there is no return value left to assert on.
    #[test]
    fn an_abandoned_reader_stops_filling_its_sink() {
        use std::io::Write as _;

        let (mut pipe, mut writer) = std::io::pipe().expect("a pipe");
        let sink = Arc::new(Mutex::new(Vec::new()));
        let abandoned = Arc::new(AtomicBool::new(false));

        // The survivor: it holds the write end and keeps talking, so nothing
        // this reader does can ever produce an EOF.
        let stop = Arc::new(AtomicBool::new(false));
        let writer_stop = Arc::clone(&stop);
        let chatter = std::thread::spawn(move || {
            while !writer_stop.load(Ordering::Relaxed) {
                if writer.write_all(&[b'x'; 4096]).is_err() {
                    // The reader end closed, which is the outcome under test.
                    break;
                }
            }
        });

        let reader_sink = Arc::clone(&sink);
        let reader_flag = Arc::clone(&abandoned);
        let (done_tx, done_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            drain(&mut pipe, &reader_sink, &reader_flag, None);
            let _ = done_tx.send(());
        });

        let captured = |sink: &Mutex<Vec<u8>>| sink.lock().expect("the sink lock").len();
        // Only meaningful once the reader is actually running, so wait for it
        // to have captured something before pulling the rug.
        let deadline = Instant::now() + Duration::from_secs(5);
        while captured(&sink) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(captured(&sink) > 0, "the reader never read anything");

        abandoned.store(true, Ordering::Relaxed);
        assert!(
            done_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "an abandoned reader must end at its next read, not at EOF"
        );
        reader.join().expect("the reader thread");
        let settled = captured(&sink);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            captured(&sink),
            settled,
            "the sink kept growing after the collector gave up"
        );

        stop.store(true, Ordering::Relaxed);
        chatter.join().expect("the writer thread");
    }

    // The hand-off launchers keep the bound and want none of the bytes, and on
    // Linux that distinction is the whole latency of "Reveal in file manager":
    // `xdg-open` execs a handler that daemonises holding our stdout, so a
    // captured run has no EOF to wait for and pays the full reader grace after
    // the launcher itself has exited. With nothing piped there is nothing to
    // hold, and the stand-in below — a shell that backgrounds a sleep and
    // exits — returns at once instead of two seconds later.
    #[cfg(unix)]
    #[test]
    fn an_uncaptured_run_is_not_held_open_by_a_lingering_grandchild() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 3 & exit 0");
        let started = Instant::now();
        let out = run_timed_uncaptured(cmd, "sh", Duration::from_secs(30), KillScope::Child)
            .expect("the shell exits successfully");
        assert!(out.status.success());
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "an uncaptured run reports no output"
        );
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "the uncaptured path still paid a reader grace: {:?}",
            started.elapsed()
        );
    }

    // F29: the budget used to be a 50 ms poll, so nothing could be observed to
    // finish sooner than the first tick and every network operation was
    // quantised onto that grid. The bound here is deliberately just under one
    // tick — it is the assertion that the poll is gone, not a latency budget.
    // If it flakes on a loaded machine, lengthen the sleep, never the bound.
    #[cfg(unix)]
    #[test]
    fn completion_is_observed_without_a_poll_interval() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 0.02");
        let started = Instant::now();
        let out = run_timed(cmd, "sh", Duration::from_secs(10), KillScope::Group)
            .expect("the shell exits successfully");
        assert!(out.status.success());
        assert!(
            started.elapsed() < Duration::from_millis(48),
            "a 20 ms child took {:?} — the wait is still quantised",
            started.elapsed()
        );
    }
}
