# leogit — Technical Architecture

Functional behavior lives in [DESIGN.md](DESIGN.md). Visual design language lives in [FRONTEND.md](FRONTEND.md). This document covers **how the code is organized** and the decisions that pin it together.

## Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri 2.11 | Native window, IPC, no Node runtime in production |
| Native dialogs | `tauri-plugin-dialog` 2.7 + `@tauri-apps/plugin-dialog` | Folder picker for the Clone dialog's destination (`dialog:allow-open` capability) |
| Single instance | `tauri-plugin-single-instance` 2.4 | Forwards a second `leogit <dir>` launch to the running window instead of duplicating it (see *Command-line repo opening*) |
| Backend language | Rust 2021 | Async via tokio (`features = ["full"]`) |
| Frontend framework | Svelte 5 (runes) | `$state`, `$derived`, `$effect`, `$props` |
| Frontend bundler | Vite 8 (rolldown) | `terser` for minified release builds; `@xterm/*` split into its own chunk (rolldown `codeSplitting` group) to keep every chunk under the 500 kB warning |
| Type system | TypeScript 7 strict (native `tsc`) | `typescript` is npm-aliased to `@typescript/typescript6` — svelte-check/editors need the JS API until TS 7.1 ships the programmatic API. `$lib/*` alias points at `src/lib/*` |
| Diff syntax | syntect 5.3 + two-face 0.5 (Rust) | Class-based output; theme colours live in `--syn-*` CSS variables |
| Terminal UI | xterm.js 6 + FitAddon + WebLinksAddon | Black background, 12 px monospace |
| PTY | `portable-pty` 0.9 | Spawns user `$SHELL`, falls back to `/bin/zsh` / `cmd.exe` |
| HTTP | `reqwest` 0.13 | Used only for Ollama |
| Config | `toml` 1.1 + `directories` 6 + `serde_json` | `~/.config/leogit/{config.toml,repos-state.json}` |
| Recoverable delete | `trash` 5 | "Discard" sends never-committed files to the OS trash instead of unlinking |
| Build tool | `just` | Wraps `pnpm tauri …` |

## Repository layout

```
leogit/
├── tauri-app/
│   ├── src/                         # Svelte 5 frontend
│   │   ├── App.svelte               # Startup phases (loading → picker / main / error)
│   │   ├── main.ts                  # Mounts App
│   │   ├── app.css                  # Theme tokens + base element styles
│   │   └── lib/
│   │       ├── api/commands.ts      # Typed wrappers over every Tauri command
│   │       ├── actions/             # Svelte use: actions (autofocus, listNavigation)
│   │       ├── utils/path.ts        # basename for OS paths (either separator)
│   │       ├── stores/              # appState, repoState, config (Svelte writables)
│   │       ├── components/          # Header, TabBar, FileList, CommitList,
│   │       │                        # CommitMessage, DiffViewer, Terminal,
│   │       │                        # ErrorModal, PathText
│   │       └── views/               # MainLayout (orchestrator), RepoPicker,
│   │                                # RepoDropdown, CloneOverlay, BranchDropdown,
│   │                                # SettingsOverlay, HelpOverlay, MergeOverlay,
│   │                                # CommitDetail
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── main.rs              # PATH fix + single-instance + invoke_handler registry
│   │   │   ├── lib.rs               # Re-exports commands::*
│   │   │   └── commands/
│   │   │       ├── config.rs        # load/save Config + ReposState
│   │   │       ├── git.rs           # git operations (status, log, branch, discard, ignore, …)
│   │   │       ├── launch.rs        # `leogit <dir>` repo opening (CLI arg + single-instance)
│   │   │       ├── diff.rs          # parse_diff + build/apply patches
│   │   │       ├── gh.rs            # GitHub CLI bridge (auth check, repo list, clone)
│   │   │       ├── ai.rs            # Claude CLI + Ollama HTTP
│   │   │       ├── terminal.rs      # portable-pty session pool
│   │   │       ├── highlight.rs     # syntect diff tokenizer
│   │   │       ├── os.rs            # reveal-in-file-manager + open-with-default-app
│   │   │       ├── paths.rs         # the app's canonicalizer (never verbatim)
│   │   │       └── process.rs       # CREATE_NO_WINDOW spawn + run_timed helpers
│   │   ├── capabilities/default.json
│   │   ├── tauri.conf.json
│   │   └── Cargo.toml
│   ├── package.json                 # pnpm scripts (dev, build, check, lint)
│   ├── vite.config.ts               # $lib alias, port 5173
│   └── tsconfig.json
├── justfile                         # install / dev / build / build-release / check / format
└── DESIGN.md / TECHNICAL.md / ROADMAP.md / FRONTEND.md / README.md
```

## Process model

There are two processes at runtime:

1. **Tauri host (Rust)** — owns the window, runs the invoke handler, owns the PTY session pool, shells out to `git` / `gh` / `claude`.
2. **WebView (Svelte)** — renders the UI, dispatches commands via `invoke(...)`, subscribes to terminal output via Tauri events.

All work flows through Tauri's IPC. There are no HTTP servers, no sidecars, and no Node runtime in production.

### Startup PATH fix

`main.rs::fix_path_env` runs once before the Tauri builder. On macOS/Linux it spawns `$SHELL -ilc 'echo -n "$PATH"'` and replaces the process PATH with the result. Without this, apps launched from Finder or a `.desktop` entry inherit a minimal PATH (e.g. `/usr/bin:/bin:/usr/sbin:/sbin`) and miss user-installed tools like `claude`, `gh`, or Homebrew binaries. No-op on Windows.

### Path normalisation

[commands/paths.rs](tauri-app/src-tauri/src/commands/paths.rs) owns the form every path takes once it's inside the app, and **`std::fs::canonicalize` must not be called anywhere else**. On Windows it always answers with a *verbatim* (extended-length) path — `\\?\C:\Users\Leo\Dev\leogit` — which names the right folder but leaks everywhere: the repo tooltip and the picker's empty state rendered the prefix literally, and PowerShell, unable to map a verbatim path onto a `PSDrive`, dropped to a provider-qualified prompt (`PS Microsoft.PowerShell.Core\FileSystem::\\?\C:\…`) that also breaks any script doing string work on `$PWD`.

- `paths::canonicalize` replaces `fs::canonicalize` at every site: `discover_repos` (scan roots and each hit), `repo_root`, `init_repo`, `resolve_launch_target`. Backed by [`dunce`](https://lib.rs/crates/dunce), which strips the prefix only when the legacy namespace can express the same path — never over `MAX_PATH`, never for a reserved DOS name (`CON`, `COM1`), never for a network share — so nothing that genuinely needs the prefix loses it.
- `paths::simplify_str` does the same conversion without touching the filesystem, for paths that arrive already-absolute from elsewhere: `config::normalize_repo_paths` (see below) and `start_terminal`'s cwd, the boundary where a third-party shell reads the path.
- **macOS and Linux are untouched by construction**, not by a platform branch of ours: `dunce`'s strip check is a `const fn` returning `false` off Windows and its `canonicalize` is a re-export of `std::fs::canonicalize` there.
- The frontend has the matching rule in [lib/utils/path.ts](tauri-app/src/lib/utils/path.ts): `basename` splits on **either** separator. Four components had their own `/`-only copy, so on Windows the whole path counted as one segment and a repo without a GitHub remote to name it was labelled `C:\Users\Leo\Dev\ryubing\Ryubing`. Deliberately not used for git's own paths (`PathText`, `fileActions`) — git reports forward slashes on every platform, and a separator-agnostic split there would cut a filename that legitimately contains a backslash on Linux or macOS.

### Command-line repo opening

The `leogit [dir]` shell command (installed by `install.sh`, see *Release pipeline*) opens a repo straight from a terminal. All the app-side logic lives in [commands/launch.rs](tauri-app/src-tauri/src/commands/launch.rs):

- **Argv → `LaunchTarget`.** `resolve_launch_target` takes the first non-flag argument, resolves it against the cwd, and **canonicalizes** it (so it de-dupes against the canonical paths from `discover_repos`). It returns `{ path, is_repo }` rather than an optional repo path: an existing directory always produces a target, and `is_repo` decides whether the frontend opens it or offers to create a repository there. `is_repo` comes from `git::repo_root`, which shells out to `rev-parse --show-toplevel` so a **subdirectory resolves to its repo root** (`leogit src/` opens the repo, and can't be mistaken for a fresh folder), falling back to the `.git` probe when the toplevel is unreadable so an existing repo is never offered a nested `git init`. Only a missing path, a non-directory, or a bare `leogit` resolves to `None` — those just launch/focus the window.
- **Cold start** (app not running): `main.rs` calls `resolve_launch_target` *before* the builder and stashes the result in a process-global via `set_pending_launch_target`. The frontend claims it once on mount through the `take_pending_launch_target` command (`appApi.takePendingLaunchTarget`), which clears it so a reload won't re-open. In `App.svelte` a repo target wins over the remembered `last_opened_repo` and is added to the repo list even if it lives outside the scan paths; a non-repo target raises the init prompt and lets normal resolution continue behind it.
- **Warm start** (app already running): `tauri-plugin-single-instance` — registered **first** in `main.rs`, as the plugin requires — detects the second launch, hands its argv/cwd to `handle_second_instance`, which focuses the window and emits an `open-repo` event carrying the `LaunchTarget`. The plugin keys on the app identifier via a `/tmp/<identifier>_si.sock` Unix socket (the second process connects, forwards, and `exit(0)`s). The frontend splits the event three ways: a **non-repo** target always goes to `App.svelte` (the prompt isn't scoped to the open repo, so `MainLayout` ignores those); a **repo** target goes to `MainLayout` in phase `main` (reusing `handleSwitchRepo` via `openExternalRepo`) and to `App.svelte` in the pre-`main` phases. The repo listeners stay mutually exclusive — `App` ignores repo targets while phase is `main`, and `MainLayout` is only mounted then.
- **Initialising a non-repo folder.** `App.svelte` owns the prompt ([InitRepoConfirm.svelte](tauri-app/src/lib/components/InitRepoConfirm.svelte)) in every phase, so it can render over the picker, over another open repo, or at first launch. Confirming calls `git::init_repo`, which creates the folder if needed, runs `git init` (naming the branch `main` unless the user configured `init.defaultBranch`), and returns the path to open. It is **idempotent** — a folder already inside a repo returns that repo's root instead of nesting a new one — so a double-confirm, or confirming after the user ran `git init` themselves, opens the repo rather than failing. `App` then routes the result: `MainLayout.openExternalRepo` (bound via `bind:this`) when one is mounted, since only it can reset the open repo's view state; otherwise it moves the app into `main` itself. Unborn HEAD is already handled downstream, so the fresh repo renders immediately.

### Repo-less phases

`loading`, `repo-picker` and `error` render `<Header>` above the phase content. `Header` derives `hasRepo` from `$appState.repoPath` rather than taking a prop — `repoPath` is the single source of truth and a separate flag could disagree with it — and hides the repo chip, branch chip, status area, Pull, the Push split-button and Refresh when it is false. Settings, Help and the update chip remain, so they are reachable in every phase. `onOpenRepos`/`onOpenBranches` are therefore optional props: only the hidden chips call them. Header's effects already no-op without a repo (each guards on `repoPath`), so mounting it costs nothing.

Two consequences worth knowing:

- `RepoPicker`'s overlay is `position: absolute; inset: 0` inside `.pre-main-body`, **not** `position: fixed`. A viewport-fixed overlay would sit on top of the header and swallow the only controls that can rescue an empty picker.
- `App.svelte` binds `Escape` / `Ctrl+,` / `?` only while the phase isn't `main`; `MainLayout` owns those keys otherwise, and binding in both would double-handle them. `?` is ignored while a text field has focus, since the picker autofocuses its search box.

`effective_scan_paths` (git.rs) reports the folders discovery would walk, so the empty state can list them. It and `discover_repos` both route through `resolve_scan_paths`, the sole owner of the "empty config → stock defaults" rule, so the folders shown can't drift from the folders searched — pinned by `effective_scan_paths_matches_the_resolution_discovery_uses`.

### Windows console suppression

Release builds set `windows_subsystem = "windows"` (in `main.rs`), so the app runs with no attached console. On Windows a console-less process that spawns a console subprocess gets a **new console window allocated and briefly flashed** for each call — and because the UI polls `git status` every 2s, that would mean a `cmd` box flickering on screen continuously, plus one on every fetch/commit/diff. Every subprocess spawn therefore routes through [commands/process.rs](tauri-app/src-tauri/src/commands/process.rs): `hide_console` (std `Command`) and `hide_console_async` (tokio `Command`) set the `CREATE_NO_WINDOW` creation flag; both are no-ops off Windows. Call sites: `git_cmd` / `git_net_cmd` (git.rs), `apply_patch` (diff.rs), `check_auth` / `gh_repo_list` / `gh_clone` / `gh_publish_repo` (gh.rs), and both `claude` spawns (ai.rs). The PTY shell in terminal.rs is intentionally exempt — ConPTY is a pseudo-terminal, not a console subprocess, so it never flashes a window.

### Network resilience (offline / flaky)

Every remote-touching command is engineered so an unreachable or flaky network degrades a badge — it never freezes the app. Three layers:

1. **Off the main thread.** Every command that spawns a subprocess or touches the filesystem — the whole of `git.rs` (except the pure `format_commit_message`), all of `diff.rs`, and the four `gh` commands — is declared `#[tauri::command(async)]`. A plain synchronous Tauri command runs inline on the **main thread**: a blocking `git` spawn there freezes the window, and the failure mode is sneaky — commands that are normally instant (`get_status`, `rev-parse`) turn slow exactly when a big push/pull saturates the repo's disk, so the 2 s poll used to stall the UI thread every tick for the whole transfer. `(async)` runs them on tokio worker threads instead. One refinement on top: a `(async)` sync fn still pins one of the ~num-cpus *core* workers for its whole duration, so the commands that can legitimately run for minutes (`fetch`, `pull`, `push`, `clone_repo`, `delete_remote_branch`, `gh_publish_repo`, `gh_clone`) are `async fn`s delegating to `process::run_blocking` (tokio's dedicated blocking pool) — a 10-minute push can never starve the worker pool on a low-core machine.
2. **Time-boxed subprocesses.** `process::run_timed(cmd, label, timeout)` is the single chokepoint: it spawns the child, drains both pipes on helper threads (so a chatty `git --progress` can't pipe-buffer-deadlock), and **kills the child** if it outlives `timeout`, returning a `… timed out …` error. `run_timed_streaming` is the same runner with an incremental stderr reader — each `\r`/`\n`-terminated line is handed to a callback as it arrives (git repaints its meter with bare `\r`), which is how live `--progress` output reaches the UI. `git_net_cmd` additionally bakes transport timeouts into the command — `GIT_SSH_COMMAND="ssh -o ConnectTimeout=N -o BatchMode=yes"` (SSH connect cap + no interactive prompts) and `-c http.lowSpeedLimit=1000 -c http.lowSpeedTime=N` (abort an HTTP transfer that stalls). Budgets: **background** badge fetches are short (8s connect/stall, 12s hard kill — fail fast, keep last-known counts); **user-initiated** transfers are generous (15/30s, 600s hard kill — never kill a real large transfer, only a wedged one). Unit-tested in `process::tests` (`run_timed_kills_a_hung_child_promptly`, `run_timed_streaming_splits_stderr_on_cr_and_lf`).
3. **Don't keep firing when down.** [services/connectivity.ts](tauri-app/src/lib/services/connectivity.ts) gates *automatic/background* fetches (the auto-fetch timer, the tiered scheduler, the refocus/cold-open resync) on `navigator.onLine` plus a consecutive-failure circuit breaker: after 2 failures it opens with an exponential backoff window (30s → 5min cap), suppressing background fetches until the window lapses, when exactly one probe is allowed through. `repo_sync_status` returns a `fetched` flag so the breaker can tell a real fetch failure from a no-remote repo. User-initiated actions (Pull/Push/switch) always attempt (still bounded by the backend timeout) and their outcome feeds the breaker, so a successful manual pull — or the OS `online` event — re-opens background syncing immediately and triggers a resync.
4. **One transfer at a time.** The [stores/networkOps.ts](tauri-app/src/lib/stores/networkOps.ts) `activeNetworkOp` store marks a user push/pull/publish as in flight. All handlers guard on it (mutual exclusion), and the 2 s poll, auto-fetch, refocus resync, and the tiered scheduler pause while it's set — polling mid-transfer only spawns git processes that contend with the transfer for the repo's disk, locks, and bandwidth; the op's own completion refresh covers what they would have found.

### Transfer progress (`git-progress` events)

`push`, `pull`, and `clone_repo` run with `--progress` through `run_git_net_streaming`, feeding each stderr line to [commands/progress.rs](tauri-app/src-tauri/src/commands/progress.rs) — a port of GitHub Desktop's step/weight model. Each op has an ordered phase table (push: `Compressing objects` 0.2 / `Writing objects` 0.7 / `remote: Resolving deltas` 0.1; pull: `remote: Compressing objects` 0.1 / `Receiving objects` 0.7 / `Resolving deltas` 0.15 / checkout 0.15; clone: 0.1 / 0.6 / 0.1 / 0.2 — the checkout phase matches both `Updating files` (git ≥2.25) and the older `Checking out files`, a divergence from GitHub Desktop's table, which only knows the legacy label and tops out early on modern git). A line matching phase *i* contributes the full weight of every earlier phase plus `weight × value/total`; unknown lines (e.g. `Enumerating objects`) are *context* — they update the display text but not the bar; the aggregate fraction is monotonic, so out-of-order output can never rewind it. `progress_forwarder` (git.rs) throttles emission — a whole-percent move, 150 ms elapsed, or the finishing 100 % frame — and emits the `git-progress` event `{op, path, percent, text}` via `AppHandle::emit` (from the stderr reader thread; same pattern as the terminal PTY events). Frontend consumers: `Header` mirrors push/pull events for the active repo into the `networkProgress` store (in-button fill + the raw git line in the header's status area); `CloneOverlay` consumes `clone` events (URL-tab clones only — the GitHub tab clones through `gh`, which reports nothing, so its bar simply never appears). Background fetches deliberately emit nothing.

## IPC contract

The frontend never touches Tauri's raw `invoke` API directly; every backend call goes through a typed wrapper in [src/lib/api/commands.ts](tauri-app/src/lib/api/commands.ts). The wrappers are grouped into namespaces matching the backend modules:

| Namespace | Commands | Backend file |
|---|---|---|
| `configApi` | `loadConfig`, `saveConfig`, `loadState`, `patchState`, `recordRecentRepo` | `commands/config.rs` |
| `gitApi` | `getStatus`, `getHeadSha`, `getDiff`, `getDiffWhitespaceIgnored`, `getCommitDiff`, `getSelectedDiff`, `getLog`, `getCommitFiles`, `listBranches`, `createBranch`, `switchBranch`, `deleteBranch`, `deleteRemoteBranch`, `renameBranch`, `commit`, `hasStagedChanges`, `discardFiles`, `appendToGitignore`, `ignorePaths`, `formatCommitMessage`, `repoSyncStatus`, `fetch`, `pull`, `push`, `getAheadBehind`, `getRemote`, `mergeBranch`, `mergeSquash`, `commitSquashMerge`, `mergeAbort`, `isMerging`, `countCommitsToMerge`, `discoverRepos`, `isGitRepo`, `initRepo`, `getRepoName`, `cloneRepo`, `getLastCommitTimestamp` | `commands/git.rs` |
| `diffApi` | `parseDiff`, `generatePatch`, `generateInversePatch` | `commands/diff.rs` |
| `highlightApi` | `highlightDiff` | `commands/highlight.rs` |
| `updateApi` | `checkForUpdate` | `commands/update.rs` |
| `osApi` | `revealPath`, `openPath`, `openUrl` | `commands/os.rs` |
| `ghApi` | `checkAuth`, `repoList`, `clone` | `commands/gh.rs` |
| `aiApi` | `generateCommitMessage`, `checkProviderAvailable` | `commands/ai.rs` |
| `appApi` | `takePendingLaunchTarget` | `commands/launch.rs` |
| `terminalApi` | `listShells`, `ptyInfo`, `start`, `write`, `resize`, `close` | `commands/terminal.rs`, `commands/shell.rs` |

Every command is registered in [src-tauri/src/main.rs](tauri-app/src-tauri/src/main.rs) via `tauri::generate_handler![…]`. **Adding a new command requires three edits**: implement it in `commands/<module>.rs`, register it in `main.rs`, wrap it in `api/commands.ts`.

## State management (frontend)

The three core writable Svelte stores, all in [src/lib/stores](tauri-app/src/lib/stores):

- **`appState`** — top-level phase machine (`loading` / `repo-picker` / `main` / `error`), the discovered repo list, the chosen repo path, and whether `gh` is authenticated. `App.svelte` renders `MainLayout` for `main` and, for every other phase, a `.pre-main` column of `<Header>` + the phase's content — so app-level chrome exists in all of them (see *Repo-less phases*).
- **`repoState`** — everything tied to the currently open repo: status (branch, upstream, ahead/behind, files, isMerging), log pagination, branches, the user's selection sets (`selectedFiles`, `userDeselected`), per-file diff selection (`Map<path, DiffSelection>`), active file/diff, active commit/files/diff, loading flags, last error.
- **`config`** — the live Config object. `refreshConfig()` reloads from disk and also calls `applyTheme()` which flips `document.documentElement.dataset.theme`.

Alongside these are smaller purpose-built stores: **`networkOps`** holds the user-initiated network op in flight (`activeNetworkOp` — the poll/auto-fetch/scheduler pause on it and the Push/Pull handlers use it for mutual exclusion) and its live transfer progress (`networkProgress`, fed from `git-progress` events), **`repoIdentifiers`** and **`repoActivity`** lazily cache each repo's GitHub identifier and last-commit timestamp (module-level maps that re-publish on each fetch, so reopening the repo picker is free), **`repoSync`** caches each repo's ahead/behind counts and working-tree `dirty` flag for the picker's pull/push badges and dirty dot (`setRepoSync` records values the active poll already computed; `syncRepo` fetches + recomputes one repo, with per-path in-flight de-duplication; its change-equality guard compares every field, so a new one must be added there too or its transitions get swallowed), and **`reposState`** mirrors the persisted `repos-state.json` document — the `repoSortMode` / `cloneSortMode` / `recentRepos` writables plus thin wrappers over the backend's atomic writers: `patchReposState` → the `patch_state` command (one field-wise read-modify-write under a process-wide lock, so a patch can never clobber another writer's field), `recordRecentRepo` → the `record_recent_repo` command (backend owns the MRU move-to-front/de-dupe/cap and returns the authoritative list, which reseeds the `recentRepos` store), and `hydrateReposState` (startup seed). Both wrappers log-and-swallow failures, so callers never need a rejection path for lost preferences.

`MainLayout.svelte` is the orchestrator: it owns the polling intervals, focus listeners, and most of the cross-cutting handlers (commit, switch branch, merge, etc.). Components stay dumb — they receive props and emit callbacks; they don't read or write the stores directly when avoidable.

### Selection bookkeeping

The file checkbox state is **opt-out**: every change reported by `git status` is staged unless the user explicitly deselected it. `repoState.userDeselected` is the source of truth for "things the user un-checked." On every status refresh:

1. Build `presentPaths` from the new status.
2. Rebuild `selectedFiles` as `present − userDeselected`.
3. Prune `userDeselected` of paths that no longer exist (so a deselected file that gets reverted then re-modified comes back checked).

This is what keeps polling unobtrusive: a 2 s status refresh never silently re-selects something the user just un-checked.

### Large changesets

[FileList.svelte](tauri-app/src/lib/components/FileList.svelte) virtualizes its rows: only the slice currently in the viewport (plus an 8-row buffer above and below) is mounted in the DOM. A spacer div carries the full virtual height (`files.length * 24px`) so the scrollbar represents the real list, and each rendered row is absolutely positioned at `top: index * 24px`. Without this, a 1000-file changeset would block the main thread for hundreds of milliseconds every time the Changes tab pane goes from `display: none` to `display: flex`. Arrow-key navigation updates `scrollTop` synchronously, awaits `tick()`, then focuses the now-rendered target — so Home/End/↑/↓ work even when the target row is far outside the rendered window.

[CommitList.svelte](tauri-app/src/lib/components/CommitList.svelte) virtualizes the same way (50px rows, spacer of `commits.length * 50px`). It measures the viewport height with a **ResizeObserver**, not a one-shot `clientHeight` read: the History pane is `display: none` while the Changes tab is active, so a single measurement at mount can capture 0 and strand the rendered range at `ceil(0 / ROW_HEIGHT) + buffer` ≈ 5 rows. The observer re-measures when the pane gains size, so the full window of commits renders once History is shown.

### Polling and lifecycle

`MainLayout.svelte` owns two intervals plus the tiered sync scheduler:

- **Status poll** — every 2000 ms. Runs `get_status` silently + `get_head_sha`. If the HEAD SHA changed, the commit log is refreshed in place keeping the same loaded count so the user doesn't lose scroll position. Each run also pushes the active repo's ahead/behind + dirty flag into the `repoSync` store (via `setRepoSync`) so the picker badges and dot for the open repo stay live without a dedicated fetch. An in-flight guard skips a tick while the previous cycle is still running (cycles can outlive the interval when the repo's disk is busy), and the poll pauses entirely while `activeNetworkOp` is set.
- **Auto-fetch** — every `fetch_interval_ms` (default 30 000). Skipped if the user is currently typing in an input/textarea, the window is hidden, or a network op is in flight. Calls `fetchActiveRemote` (`git fetch --prune --recurse-submodules=on-demand` against the first remote) then a silent `get_status`. `fetchActiveRemote` self-skips when offline / backing off and reports its outcome to the connectivity breaker (see *Network resilience*).
- **Tiered repo-sync scheduler** ([repoSyncScheduler.ts](tauri-app/src/lib/services/repoSyncScheduler.ts)) — three intervals (2 / 5 / 10 min) plus staggered startup kicks. Each tick slices the `recentRepos` list (active excluded) into tiers — next 4, next 5, next 10 — and refreshes each via `repo_sync_status` sequentially. Tier syncs are tagged `background`, so while offline / backing off each `syncRepo` consults the breaker and downgrades to a **fetch-less local recompute** instead of grinding through dead fetches — the network goes quiet but the dirty dot keeps tracking local edits, since it needs no remote. A tier bails between repos while a user transfer runs so badge fetches never steal its bandwidth. The tiers cover only the ~19 most recent repos, so the dropdown additionally calls `syncVisibleRepos` whenever its list is on screen: a sequential fetch-less sweep that always fills rows with no cached entry and, at most once per 30 s, re-checks the whole visible list. Started in `initialize` (after `hydrateReposState` resolves, so recents are seeded) and stopped on unmount.

On regaining focus (`window` `focus`) or visibility (`visibilitychange`), `MainLayout` runs a one-shot **resync** — `fetchActiveRemote` (so a moved upstream surfaces immediately, unlike before), silent `get_status`, HEAD poll, a *forced* re-fetch of the diff for the file open in the changes pane (`loadDiffForFile(file, { force: true })`), and `repoSyncScheduler.refocusSync()` (the throttled top-tier refresh). A `resyncing` guard collapses the focus+visibility double-fire (common under tiling WMs) into a single run. All listeners, intervals, and scheduler timers clear on unmount.

## Git layer

All git operations go through `std::process::Command::new("git")` with these defaults set in `git_cmd`:

- `current_dir(repo_path)` — never relies on the cwd of the host process.
- `TERM=dumb` — suppresses pagers and color codes.
- `GIT_TERMINAL_PROMPT=0` — prevents a credential prompt from blocking the process indefinitely.
- `GIT_OPTIONAL_LOCKS=0` (as GitHub Desktop sets globally) — keeps read-only commands (`status`, `diff`) from opportunistically refreshing the index under `index.lock`. Commands run concurrently on worker threads, so without this a poll-time `diff` holding the lock could make a simultaneous `commit` fail with "index.lock exists".

Helpers shape the output:

- `run_git_raw` — returns the raw bytes (used for NUL-delimited or binary-ish output, e.g. `status --porcelain=v2 -z`).
- `run_git` — returns trimmed UTF-8 (most line-oriented commands).
- `run_git_combined` — returns `(success, stdout+stderr)` regardless of exit code (used for local commands like `merge` where the error message is the value).
- `run_git_net` / `run_git_net_streaming` — the **network** runners (`fetch`/`pull`/`push`/`clone`/badge fetch; the `_streaming` variant additionally forwards stderr lines live). Both build the command via `git_net_cmd` (SSH/HTTP transport timeouts), run it through `process::run_timed` / `run_timed_streaming` (hard kill-timeout), and collapse `\r`-overwritten progress repaints in the combined output so a failed transfer's error message reads like the terminal rendered it, not pages of meter spew. See *Network resilience* for the full rationale and budgets.

### Status parsing

`get_status` uses `status --untracked-files=all --branch --porcelain=v2 -z`. Under `-z` the **whole** stream is NUL-terminated — `# branch.*` headers included (there are no newlines anywhere) — so the parser walks the leading `# `-prefixed records by splitting on NUL, stops at the first non-header record, then splits the remainder on NUL for the file entries. Type-2 (rename) entries occupy two NUL segments — the second holds the original path — so the loop manually advances `i` when it sees `2 `. (Splitting headers on `\n` instead of NUL was a long-standing latent bug: it parsed zero headers, so `has_upstream` silently stayed false and ahead/behind survived only via the rev-parse + remote-tracking fallbacks — `repo_sync_status`, lacking those fallbacks, returned 0/0. Regression-tested in `status_parses_upstream_and_ahead_behind_from_porcelain_headers`.)

Branch fallback: if `# branch.head` is absent (empty repo) the code calls `git rev-parse --abbrev-ref HEAD` to fill in.

HEAD identity: `# branch.head` reading `(detached)` sets `RepoStatus.detached` (and leaves `branch` empty) so the UI can distinguish a detached HEAD from a still-loading status; `# branch.oid` yields `head_sha` for free (no extra `rev-parse`), or stays empty for an unborn branch (`(initial)`). The Header shows `On <short-sha>` + a `DETACHED HEAD` marker and suppresses Push/Pull while detached; the History "Checkout commit" item is disabled on the current HEAD. Covered by `get_status_reports_branch_and_head_sha`.

Ahead/behind: `# branch.ab` is only emitted when the branch has a tracking upstream. For a branch that was never `push -u`'d but has a matching `refs/remotes/<remote>/<branch>`, the shared `remote_tracking_ahead_behind` helper computes the counts with `git rev-list --left-right --count HEAD...<ref>` (left = ahead, right = behind) without flipping `has_upstream` (which still gates whether the next push needs `--set-upstream`). `repo_sync_status` — the lighter sibling powering the picker badges and dirty dot — reuses both `first_remote` and that helper but runs `status --untracked-files=normal` (an untracked directory stays a single `dir/` record instead of being enumerated, which answers "any change at all?" identically to `-uall`) and optionally fetches first. Besides the branch headers it reports `dirty`: whether any `? `/`1 `/`2 `/`u ` record with a UTF-8-decodable path follows them (`is_change_record`) — precisely the records `get_status` turns into Changes-tab rows (it skips non-UTF-8 paths, so the dot must too). The active repo's `dirty` never comes from here: the 2 s poll writes `status.files.length > 0` into the store, so the dot and the visible Changes tab agree by construction. Its fetch is best-effort and **time-boxed** (`run_git_net`, background budget): a failure/timeout swallows so a stale-but-known count still comes back, and the outcome is surfaced as the `fetched` flag for the frontend's connectivity breaker.

Unpushed markers (`unpushed_shas`, the History view's up-arrow): when the branch has a resolved upstream (real or inferred) and is ahead, the set is `git rev-list HEAD ^<upstream>`. When there's **no** resolvable upstream — a new local branch never pushed, with no same-named remote ref (the cloned-`main`-then-branched case) — but the repo has remote-tracking refs, it falls back to `git rev-list HEAD --not --remotes`: local commits not reachable from **any** remote branch. That marks the new commits while leaving the shared base (on `origin/main`) unmarked, matching GitHub Desktop; without the fallback `ahead` stays 0 there and the History view showed no arrows at all. `--remotes` (every remote) is chosen deliberately over scoping to a single push remote: it's conservative — it can only ever *under*-mark (miss an arrow on a commit that also lives on some unrelated remote ref), never draw a *false* arrow on an already-pushed commit, which a wrong-remote guess would. The one accepted divergence from GitHub Desktop is a multi-remote/fork repo where a commit was pushed only to a non-default remote. A repo with a remote but no `refs/remotes/*` yet (just `remote add`, never pushed) correctly marks every commit. Both forms are skipped when there's nothing to compute (in-sync upstream branch, or a repo with no remotes) so the 2s status poll stays cheap. Covered by `unpushed_shas_marks_local_commits_on_unpublished_branch`, `unpushed_shas_empty_without_a_remote`, and `unpushed_shas_marks_all_commits_when_remote_has_no_tracking_refs`.
Branch switching: `switch_branch` takes the short name as the UI shows it. `list_branches` surfaces remote branches with their prefix (`origin/feature`), so a naive `git checkout origin/feature --` would treat the ref as a commit-ish and detach HEAD. Instead `switch_branch` probes with `git show-ref --verify --quiet` (which exits non-zero — i.e. `run_git` returns `Err` — when the ref is missing): if the name isn't a local branch (`refs/heads/<name>`) but is a remote one (`refs/remotes/<name>`), it routes through `checkout_tracking_branch`, which drops the first path segment to get the local name (`origin/team/x` → `team/x`, matching `git switch`'s DWIM) and runs `git checkout -b <local> --track <remote>`. The local-branch-first guard means a local branch whose name legitimately contains a slash is never misread as remote, and if the derived local name already exists it's switched to as-is rather than recreated (so a second remote's same-named branch reuses the existing local branch).

Checkout commit (detached HEAD): `checkout_commit` runs a plain `git checkout <sha>` (full SHA from the History list, so no ref/path ambiguity), landing the user on a detached HEAD — mirroring GitHub Desktop's "Checkout commit". It uses `run_git_combined` and surfaces git's message verbatim on failure (most commonly "local changes would be overwritten"), so a refused checkout never silently loses work and leaves HEAD attached. Reattaching is just `switch_branch` to any branch. Covered by `checkout_commit_detaches_then_branch_reattaches` and `checkout_commit_fails_when_local_changes_would_be_overwritten`.

### Diff parsing

`parse_diff` is a hand-rolled unified-diff parser (no `regex` crate). It captures the full file header (`diff --git`, `index ...`, `--- a/...`, `+++ b/...`) into `file_header` because `git apply` requires it for new/deleted/renamed files. Each hunk stores its own `@@` header line as the first entry in `lines` so flat/global line indexing stays consistent across the frontend and backend.

### Patch generation

`build_patch` rebuilds a unified diff from a `FileDiff` plus a per-line `DiffSelection`:

- Unselected ADDs are dropped (they don't exist in the old file).
- Unselected DELETEs become context lines (` `-prefixed) so the hunk still describes a contiguous slice of the old file.
- The `@@` header is regenerated with recomputed counts.
- `\ No newline at end of file` markers are echoed back.

`apply_patch` pipes the patch via stdin to `git apply --unidiff-zero --whitespace=nowarn` with optional `--cached` (stage) and `--reverse` (discard / inverse). `--reject` is deliberately NOT passed; it silently writes `.rej` files and would mask failures.

### Commit pipeline

`commit` is the safest path through a partial-stage commit:

1. `git reset -- .` to clear the index (only the user's selection should make it into the commit). The pathspec form is used instead of `git reset HEAD` so the same step works on a fresh repo whose HEAD is still unborn — `reset HEAD` fails there with "ambiguous argument 'HEAD'", while `reset -- .` (the repo root is the cwd, so `.` covers the whole index) succeeds and still unstages everything.
2. `stage_files` splits the selection into **removals** (rename source paths, deletions — staged via `git update-index --force-remove`, the precise tool for dropping an index entry) and **additions/modifications** (staged via `git_add`, which runs `git add --pathspec-from-file=- --pathspec-file-nul`). Additions deliberately go through porcelain `git add`, **not** `update-index --add`: the latter silently ignores any path that resolves to a directory (it prints `Ignoring path …/` and exits 0), which left an **embedded git repository** — a nested repo git reports as a single untracked directory entry — unstaged and surfaced the misleading `staging produced no changes` error. `git add` stages a directory's files normally and an embedded repo as a **gitlink** (mode 160000), matching the git CLI. Embedded-repo advice is silenced via `-c advice.addEmbeddedRepo=false` since the UI explains the gitlink up front. Both helpers pipe paths NUL-separated to dodge arg-length/quoting limits.
3. `has_staged_changes` validates the index isn't empty (`git diff --cached --quiet` returns 1 when there are staged changes).
4. The commit message is piped to `git commit -F -` via stdin (avoids arg-length and shell-quoting issues).

**Embedded-repo detection.** `get_status` flags an entry `embedded: true` when git reports it as an untracked entry whose path keeps a **trailing slash**. Under `--untracked-files=all` git expands every ordinary untracked folder into individual files, so the only entry that stays a directory is an embedded git repo (git never recurses into one). The frontend uses the flag to (a) render a distinct ↪ link badge instead of the green `A` in [FileList.svelte](tauri-app/src/lib/components/FileList.svelte), and (b) pop a confirm modal ([EmbeddedRepoConfirm.svelte](tauri-app/src/lib/components/EmbeddedRepoConfirm.svelte)) before committing, since committing a gitlink — rather than the folder's files — is a surprising outcome. Covered by the `commits_embedded_repo_as_gitlink` test.

**Dirty-submodule detection.** A tracked submodule whose working tree is dirty *inside* but whose recorded commit hasn't moved has nothing the parent repo can stage — `git add` is a no-op, so a commit would dead-end with `staging produced no changes`. `get_status` flags these `submodule_dirty: true` by reading the porcelain-v2 **`sub` field** (the 3rd token of a changed entry): `S<c><m><u>`, where the entry is flagged only when it's a submodule (`S`) with the commit-pointer char `c == '.'` (unmoved) and at least one of `m`/`u` set (`is_dirty_submodule`). A moved pointer (`c == 'C'`) stays committable — the gitlink change stages normally. The frontend treats a flagged entry as non-selectable: every writer to `selectedFiles` ([MainLayout.svelte](tauri-app/src/lib/views/MainLayout.svelte) — refresh seed, select-all, range-toggle, single-toggle) skips it via `isCommittable`, the row's checkbox is `disabled` with an explanatory tooltip, and the diff pane shows a "Submodule changes" message instead of the opaque `Subproject commit …-dirty` line. The user can therefore never reach the failing commit path. Covered by the `classifies_only_unstageable_dirty_submodules` and `parses_dirty_submodule_flag_from_ordinary_entry` tests.

### Discard & ignore

`discard_files` powers the Changes-tab "Discard" menu. It classifies each target by **HEAD membership**, not by the porcelain status code, via `head_paths` — a single `git ls-tree -r -z --name-only HEAD -- <paths>` that returns which of the targets exist as committed blobs (empty on an unborn HEAD). That sidesteps the ambiguity in the status code (an `AA` add/add conflict has no HEAD blob; a rename's new path doesn't either) and needs no per-file `cat-file`. Then:

- **In HEAD** (modified / deleted / conflicted / a rename's *original* path) → restored with `git checkout HEAD -- <paths>` (index + worktree both reset to the committed version).
- **Not in HEAD** (untracked, staged adds, a rename's *new* path) → can't be "reverted", so the working-tree file is moved to the **OS trash** (the `trash` crate — recoverable, unlike `rm`; best-effort per file, a failure is logged and skipped) and any staged entry is dropped with `git reset -- <paths>` (pathspec form, unborn-HEAD-safe like the commit reset). `discard_files` is a sync command taking `repo_path: &str` — it's local and fast, like `commit`. Covered by `discard_*` / `head_paths_*` tests.

`append_to_gitignore(repo_path, patterns)` appends ready-to-write lines to the repo-root `.gitignore`, ensuring a trailing newline first and skipping any pattern already present (compared trimmed, de-duped within the batch). "Ignore All .ext" calls it with a raw `*.ext` glob; "Ignore File" goes through its sibling `ignore_paths(repo_path, paths)`, which escapes each literal path's glob metacharacters (`[ ] ! * # ?`, GitHub Desktop's set) in Rust before delegating — so the escaping contract and its implementation live next to each other, covered by `ignore_paths_escapes_glob_metacharacters`.

### Log parsing

`get_log` uses a custom format with `%x01` field separators and `%x00` record separators:

```
%H%x01%h%x01%s%x01%b%x01%an%x01%ae%x01%ad%x01%cn%x01%ce%x01%cd%x01%P%x01%(trailers:unfold,only)%x01%D%x00
```

Dates come back in `--date=raw` form (`<unix> <tz>`). To avoid pulling in `chrono`, the code implements `civil_from_unix` using Howard Hinnant's proleptic Gregorian algorithm and emits ISO-8601 strings manually.

The trailing `%D` captures every symbolic ref pointing at the commit (branches, `HEAD`, tags). `get_log` keeps only the `tag: `-prefixed entries (`tags_from_decorations`) and ships them as `CommitInfo.tags`, which the commit list renders as pills directly — branch/HEAD decorations never cross the IPC boundary since no UI consumes them. `get_log` also pre-derives the two fields the commit composer needs when amending or restoring an undone commit: `co_authors` (the values of `Co-Authored-By:` trailers, matched case-insensitively via `co_author_value`) and `body_without_coauthors` (the body with those lines stripped) — the inverse of `format_commit_message`, so both directions of the co-author round trip live in Rust.

On a fresh repo with an unborn HEAD, `get_log` short-circuits to an empty list rather than letting `git log` fail with exit 128 ("does not have any commits yet"). It uses the shared `has_commits` helper (`git rev-parse --verify --quiet HEAD`, exit 0 only when HEAD resolves to a commit) — a precise check that, unlike blindly accepting exit 128, never masks a genuinely broken repo. The History tab then renders its "No commits yet" empty state instead of an error.

The same `has_commits` check guards diffs. `run_diff` anchors a tracked file's diff at `HEAD`, but on a fresh repo `git diff HEAD -- <file>` fails with "fatal: bad revision 'HEAD'". When there are no commits, it substitutes git's canonical empty-tree SHA (`EMPTY_TREE_SHA = 4b825dc642cb6eb9a060e54bf8d69288fbee4904`, which git always recognizes) so the staged/working file shows as fully added. Untracked files are unaffected — they already diff against `/dev/null` via `--no-index`.

### Discovery

`discover_repos` expands `~`, canonicalizes each scan root through `paths::canonicalize` (never `fs::` — see [Path normalisation](#path-normalisation)), and recursively walks up to `max_depth` levels. An empty `scan_paths` list (config cleared, or config load failed upstream) falls back to `config::default_scan_paths()` — the same stock folders a fresh config gets — so the frontend passes the configured list through verbatim with no path resolution of its own. A directory is a repo if it contains a `.git` file or directory (handles worktrees). Hidden directories are skipped, and the scan does not descend into a discovered repo.

## AI layer

Both providers share `build_prompt` — a strict JSON-only instruction with rules for imperative-mood, 50-char-or-less titles. Responses are parsed by `parse_commit_message_text` which:

1. Strips markdown code fences (```json … ```).
2. Tries `serde_json::from_str` and accepts any of `title`/`summary`/`subject`/`message` and `description`/`body`/`details`.
3. Falls back to "first line = title, rest = description" if JSON parsing fails entirely.

**Claude** spawns `claude --print --output-format json --model <model>` and pipes the prompt to stdin. `parse_claude_envelope` reads the CLI's JSON envelope `{"type":"result","subtype":…,"is_error":bool,"result":"<text>", "api_error_status":…}`:

- **`is_error == true`** (e.g. a transient `529 Overloaded`) → the CLI's own message is surfaced verbatim as an `Err`. Critically, the CLI exits **0** in this case, so the `is_error` flag — not the exit code — is what distinguishes a failure; without this check the error text would be parsed straight into the commit title.
- otherwise the model's reply lives in `result` and is parsed as the commit message (falling back to parsing raw stdout when the text isn't the envelope).

On a **non-zero exit** the CLI is inconsistent about where the failure lands: API/auth errors go into the stdout envelope (with an *empty* stderr — the cause of a bare "Claude CLI failed:" in the UI), crashes write to stderr, and a killed process may emit nothing. `claude_failure_message` therefore tries the stdout envelope's `result` (only when it's a non-empty string, so a contentless envelope can't short-circuit the chain) → trimmed stderr → trimmed stdout → the exit status itself, so the surfaced error is never blank. Error text is capped at 1000 chars on both exit paths (a Node stack trace's useful part is at the top).

Two robustness measures around the spawn: the prompt is streamed on a separate task (so a large diff can't deadlock against the child filling its stdout pipe before we drain it), and the child is `kill_on_drop` so a slow CLI doesn't outlive a timeout as an orphan. The spawn also sets `CLAUDE_CODE_MAX_RETRIES = 2`: by default the CLI retries a transient overload with backoff for *minutes*, far past our timeout, so the user only ever saw "timed out"; the cap makes it fail in seconds with the real error while still riding out a single blip.

**Ollama** posts to `<base_url>/api/generate` with `{model, prompt, stream: false, format: "json"}`. A 404 is translated to a friendly `ollama pull <model>` hint.

Both providers run with a 120 s timeout (`DEFAULT_TIMEOUT_SECS`). Diff caps: 20 MB (Claude) / 50 MB (Ollama).

`check_provider_available` lets the UI gate features without surfacing raw errors: `claude --version` for Claude, `GET /api/tags` for Ollama with a 5 s timeout.

## Terminal layer

`PtySession` holds the master PTY, the writer half, and the child process. Sessions are stored in a global `Mutex<HashMap<u32, Arc<Mutex<PtySession>>>>` keyed by a monotonic `AtomicU32`.

`start_terminal`:
1. Opens a PTY at 24×80 via `portable-pty`.
2. Resolves the shell via `shell::resolve(shell_id)` — see *Shell discovery* below.
3. Spawns it with cwd = repo path, adding only `TERM=xterm-256color`, `COLORTERM=truecolor`, and (Git Bash only) `CHERE_INVOKING=1`.
4. Stores the session, then spawns a reader thread that loops on `read()`, feeds bytes through a `Utf8Decoder`, and emits `terminal-output-<pid>` events. On EOF the session is removed and `terminal-closed-<pid>` is emitted.

It returns `StartedTerminal { pid, shell_id, shell_label }` — the label is resolved backend-side because the stored preference may name an uninstalled shell, and the panel header shows what actually launched.

`write_terminal` / `resize_terminal` / `close_terminal` go through `session_for(pid)`, which locks the session map, clones the `Arc`, and drops the map lock before the caller touches the session. `close_terminal` calls `child.kill()` and removes the entry.

### Child environment — do not forward the parent env

`CommandBuilder::new` already assembles the right environment. On Windows it seeds from the current process, then overlays `HKLM\…\Session Manager\Environment` and merges `HKCU\Environment`, so `PATH` becomes the same system+user merge Explorer and Windows Terminal hand their children.

`start_terminal` used to copy `std::env::vars()` over the top of that, which broke the terminal two ways on Windows:

- **Launched from Git Bash** (the `leogit` shell function), the inherited `PATH` is MSYS-style (`/usr/bin`, `/c/Program Files/...`). Win32 cannot resolve a single entry, so essentially every command failed.
- **Launched from Explorer**, the inherited `PATH` is a login-time snapshot that misses anything installed since.

Only deliberate additions go on top now. `session_env` is a pure function returning those additions, and `session_env_never_overrides_path` is the regression test.

### UTF-8 across read boundaries

PTY reads split wherever the 4 KiB buffer fills, so multi-byte characters routinely straddle two reads. Decoding each chunk with `String::from_utf8_lossy` turned every such split into a permanent U+FFFD — stray marks through box-drawing, accented text and emoji. `Utf8Decoder` holds the truncated tail (bounded at 3 bytes) until its continuation bytes arrive, which is what conhost does. Invalid bytes still collapse to one replacement character and decoding resumes.

### Shell discovery

[commands/shell.rs](tauri-app/src-tauri/src/commands/shell.rs) probes for shells rather than naming them, so the picker can never offer one that fails to spawn. Windows, best-first: **Git Bash** (install root from `HKLM\SOFTWARE\GitForWindows\InstallPath`, falling back to the default dirs then `git.exe`'s grandparent; launched `--login -i` so `/etc/profile` populates the MSYS `PATH`), **PowerShell** (`pwsh.exe`), **Windows PowerShell** (`powershell.exe`), **Command Prompt**. Git Bash leads because it is the shell git workflows assume; pwsh beats 5.1, whose in-box PSReadLine 2.0 repaints badly under ConPTY. Unix: `$SHELL` first, then zsh/bash/fish/sh de-duplicated by path.

`resolve` falls back to the best available shell when the stored id is unknown or uninstalled — the terminal opening with the wrong shell beats it not opening. `available()` is total and never empty, so neither function can panic.

The frontend mounts `<Terminal>` keyed by `${repoPath}:${terminalSessionId}` so swapping repos or hitting "New session" forces a fresh component, which in turn dispatches a new `start_terminal`. The previous component's cleanup invokes `close_terminal` on its tracked pid. Changing the shell preference applies to new sessions, not running ones.

`terminal_pty_info` reports `{backend, build_number}` and must be called *before* the xterm instance is constructed — xterm reads `windowsPty` when it builds its buffer, so setting it afterwards does nothing. Declaring ConPTY on a build ≥ 21376 is what enables reflow on resize; without it xterm assumes any line whose last cell is non-blank is wrapped, which is what smears a resized prompt. The `ResizeObserver` is debounced 80 ms because an undebounced panel drag pushes a `ResizePseudoConsole` per frame and PSReadLine repaints its whole edit buffer on each one.

## GitHub layer

Everything in `gh.rs` shells out to the `gh` CLI:

- `check_auth` → `gh auth status` (exit code only).
- `list_prs` → `gh pr list --json <fields> --state <s> --limit 30`.
- `get_pr_checks` → `gh pr checks <n> --json name,state,bucket,link,workflow`. Special-cased: `gh` exits non-zero when any check is pending/failing but still writes valid JSON; we parse stdout first and only treat it as a hard error if parsing also fails.
- `create_pr` / `create_pr_fill` → `gh pr create [--title --body | --fill] [--base] [--draft]`. Returns the PR URL.
- `checkout_pr` → `gh pr checkout <n>`.
- `get_current_branch_pr` → `gh pr list --head <branch> --state open` (returns the first match or `None`).

## OS integration layer

[commands/os.rs](tauri-app/src-tauri/src/commands/os.rs) holds the two file-manager hand-offs behind the Changes-tab menu. Both take a repo-relative path and join it onto the repo path **in Rust** (`PathBuf::from(repo_path).join(rel_path)`) so git's forward-slash paths never clash with Windows separators, then spawn a platform launcher:

- `reveal_path` — macOS `open -R`, Windows `explorer /select,<path>`, Linux `xdg-open <parent dir>` (no portable "select file" there).
- `open_path` — macOS `open`, Windows `cmd /c start "" <path>`, Linux `xdg-open <path>`.

They're `#[tauri::command(async)]` (worker thread) and routed through `process::run_timed` (15 s cap, so a wedged file manager can't hang a thread) with `hide_console` for the Windows no-flash guarantee. The launchers are treated as fire-and-forget: a completed run is success regardless of exit code, because some launchers (notably `explorer /select,`) return non-zero even on success — only a spawn failure (e.g. `xdg-open` absent) or a timeout surfaces as an error. The frontend side (clipboard copy, label selection) lives in [services/fileActions.ts](tauri-app/src/lib/services/fileActions.ts); the menu is built in `FileList.svelte` and the destructive-discard confirmation in [DiscardConfirm.svelte](tauri-app/src/lib/components/DiscardConfirm.svelte).

## Config & persistence

Defined in [src-tauri/src/commands/config.rs](tauri-app/src-tauri/src/commands/config.rs).

- Config dir is resolved via `directories::BaseDirs::config_dir().join("leogit")` (`~/.config/leogit` on Linux, `~/Library/Application Support/leogit` on macOS, `%APPDATA%\leogit` on Windows). It's created if missing.
- `config.toml` — every field on the `Config` struct. New fields carry `#[serde(default = "…")]` so users on older configs keep working. Defaults are written to disk on first run so the file is discoverable.
- `repos-state.json` — `last_opened_repo`, `last_clone_dir`, the two sort-toggle preferences (`repo_sort_mode`, `clone_sort_mode`), and `recent_repos` (MRU order, capped at `MAX_RECENT_REPOS = 50`). Every field is `Option`/`#[serde(default)]` so older state files load fine. JSON instead of TOML to keep it cheap to extend.
- Every read runs `normalize_repo_paths`, which converts the stored paths (see [Path normalisation](#path-normalisation)) and de-dupes `recent_repos` afterwards. A file written before that change holds Windows verbatim paths, which no longer match anything `discover_repos` returns — `last_opened_repo` would silently stop resolving (the app forgets the open repo and lands in the picker) and the MRU list would grow a second entry per folder. It runs on every read rather than as a one-shot migration because it's idempotent and the next write persists it, so the file heals itself with no schema version to carry.
- Writes go through two commands that each run one read-modify-write under a process-wide `STATE_LOCK` (Tauri runs commands concurrently; two interleaved load+save cycles would drop the slower writer's fields): `patch_state(ReposStatePatch)` merges the supplied fields (`None` = leave as-is; `recent_repos` is deliberately not patchable), and `record_recent_repo(path)` owns the MRU move-to-front/de-dupe/cap. Both return the resulting state so the frontend reseeds from the authoritative copy. A corrupt state file self-heals inside `update_state`: it logs, starts from defaults, and lets the save rewrite it, instead of wedging every future patch on the same parse error. Covered by the `prepend_recent_*` / `apply_patch_*` tests.

## Tauri capabilities

[capabilities/default.json](tauri-app/src-tauri/capabilities/default.json) is intentionally minimal:

```json
"permissions": [
  "core:default",
  "core:path:default",
  "core:event:default",
  "core:window:default",
  "core:app:default"
]
```

No filesystem, shell, or HTTP plugins are exposed to the WebView. All side effects route through our explicit `#[tauri::command]` functions, which is a deliberate security boundary.

## Build / dev

```bash
# From the project root
just install         # pnpm install inside tauri-app
just dev             # pnpm tauri dev   (Vite on :5173 + Tauri host)
just build           # pnpm tauri build (debug bundle)
just build-release   # pnpm tauri build --release with RUST_BACKTRACE=1
just check           # pnpm svelte-check + cargo check
just format          # prettier + cargo fmt
just clean           # nuke dist/, target/, node_modules
```

Inside `tauri-app`, `pnpm run check:native` runs the same tsconfig through the TypeScript 7 native compiler (`tsc --noEmit`, ~0.2 s full check) for fast feedback on `.ts` files; `pnpm check` (svelte-check, on the TS 6 JS line) stays authoritative because the native compiler doesn't see `.svelte` files. `src/vite-env.d.ts` (`vite/client` types) declares the CSS side-effect imports that TS 6/7's stricter resolution (TS2882) would otherwise reject.

The Tauri dev command uses `beforeDevCommand: pnpm run dev:vite` (per `tauri.conf.json`) so the Vite dev server starts in-process. Release builds use `beforeBuildCommand: pnpm run build:frontend` which writes static assets to `tauri-app/dist`, then `frontendDist: "../dist"` points the bundle at them.

Bundle targets: `app` + `dmg` (macOS), `deb` + `appimage` (Linux), `msi` (Windows).

### Release pipeline (`scripts/`)

`deploy_releases.sh` runs per-platform and uploads to one shared GitHub Release; run it once on each OS to publish a complete release. It validates prerequisites, then guards against shipping behind the live release — it queries GitHub's `/releases/latest` (the same endpoint `install.sh` installs from) and aborts if the version it's about to ship is older than that tag, since a stale local tree would otherwise clobber artifacts onto a superseded release. It then bumps/commits the version across `tauri.conf.json` / `Cargo.toml` / `package.json`, tags, then calls `bundle.sh` and packages the result:

- **macOS** — `bundle.sh` builds `leogit.app` (`--bundles app`) and ad-hoc signs it; the deploy script zips it with `ditto` into `LeoGit-<ver>-macOS-<arch>.zip`.
- **Linux** — `bundle.sh` builds an AppImage (`--bundles appimage`, no signing); the deploy script copies it to `LeoGit-<ver>-linux-<arch>.AppImage`.

`install.sh` is the curlable installer and auto-detects the platform: on macOS it unpacks into `/Applications`, strips quarantine, and re-registers with Launch Services; on Linux it drops the AppImage at `~/.local/bin/leogit.AppImage` behind a `~/.local/bin/leogit` wrapper, extracts the bundled icon, and writes a `~/.local/share/applications/leogit.desktop` launcher (warning if FUSE 2 is absent, since Arch ships only FUSE 3). The wrapper exports `WEBKIT_DISABLE_DMABUF_RENDERER=1` at launch when `/dev/nvidia0` is present (the proprietary NVIDIA driver's DMABUF/GBM path crashes WebKitGTK with "Failed to create GBM buffer" errors); it's detected per-launch rather than at install time because the active GPU is a runtime property, stays inert on AMD/Intel/nouveau, and honors a pre-set value. The desktop environment (GNOME, COSMIC, …) is irrelevant — both run the same WebKitGTK/GTK runtime — so one AppImage serves every Arch machine. As a final step it installs the `leogit [dir]` shell command into the user's login shell: it detects `$SHELL` (which survives `curl … | bash`, being inherited from the parent) and writes a `leogit()` function — into `~/.zshrc` (zsh), `~/.bashrc` on Linux / `~/.bash_profile` on macOS (bash), or an autoloaded `~/.config/fish/functions/leogit.fish` (fish); an unknown shell gets the snippet printed for manual setup. For zsh/bash the function lives inside an idempotent `# >>> leogit >>>` … `# <<< leogit <<<` marker block that re-installs replace rather than stack. The function resolves the directory and opens it (macOS `open -n --args`; Linux the PATH wrapper) — see *Command-line repo opening* for the app side.

### In-app update check

[commands/update.rs](tauri-app/src-tauri/src/commands/update.rs) closes the loop on that pipeline: `check_for_update` issues one unauthenticated `GET /repos/LeoManrique/leogit/releases/latest` (10 s timeout, `User-Agent: leogit/<ver>` — GitHub 403s without one) and compares the `v`-stripped `tag_name` against `env!("CARGO_PKG_VERSION")`. The compare is a three-part numeric tuple, not `semver` or a string compare — it matches the `sort -V` ordering `deploy_releases.sh` already applies to the same tags. The parse is **strict**: anything that isn't exactly three numeric parts (`0.2.0-beta.1`, `0.1.28+build.5`, `1.2.3.4`) yields `None` and means "no update". Coercing those instead is wrong in *both* directions — a lenient parse reads `0.2.0-beta.1` as `(0, 2, 0)` and announces a phantom update over `0.1.27`, and reads `0.1.28+build.5` as `(0, 1, 0)` and hides a real one — and it's reachable, since `deploy_releases.sh` only regex-validates the version when one is passed as an argument.

A version match alone isn't enough to announce, though: `deploy_releases.sh` runs **once per platform onto one shared release**, so the first platform to finish publishes a release the others aren't in yet. `check_for_update` therefore also requires an asset named exactly `LeoGit-<ver>-<platform>-<arch>.<ext>` (`-setup.exe` on Windows) — the same string `install.sh` resolves — and stays quiet otherwise. Without that gate a Windows user gets sent to a page holding only a macOS zip, and on macOS/Linux it's worse: `install.sh` kills the running app at step 2 and only discovers the missing artifact at step 4, leaving the user with no app *and* no update. `artifact_name` is pinned to those literal strings by a golden test, since a drifted name would silently hide every update rather than fail loudly.

`Ok(None)` means current; `Err` means the check itself failed and is a retry signal, never a user-facing error. There is no auto-download, no signed feed, and no `tauri-plugin-updater` — the payload's `install_command` carries the `install.sh` one-liner on macOS/Linux and is `None` on Windows, where the release page's installer is the path instead. In debug builds only, `LEOGIT_FAKE_UPDATE=<ver>` short-circuits the whole request (artifact gate included) so the UI can be exercised without publishing a release.

The frontend runs it once per session from `App.svelte` (not `MainLayout`, so it also covers the repo-picker phase) via [services/updateChecker.ts](tauri-app/src/lib/services/updateChecker.ts): gated on `shouldAttemptBackground()`, it retries every 30 min *until one check completes*, then stops for good — plus an `online` listener so launching offline (a plane, a captive portal) retries the moment connectivity returns instead of waiting out the window. Its outcome deliberately does **not** feed `recordResult` — a rate-limited GitHub API says nothing about the git remotes the connectivity breaker guards. The result lands in [stores/update.ts](tauri-app/src/lib/stores/update.ts) (`availableUpdate` plus a session-only `updateDismissed`; neither is persisted, so a skipped release resurfaces next launch). Opening the release page uses the new `os::open_url`, a sibling of `open_path` reusing the same `open` / `cmd /c start` / `xdg-open` hand-off — no opener/shell plugin was added. It rejects non-`https` URLs and any URL containing whitespace or ``&^<>|"'` `` — syntax to `cmd`'s parser even unquoted — plus `%`, since `cmd` expands `%VAR%` *before* that check's characters are handled and could smuggle them back in. That also rules out percent-encoding and (via `&`) query strings, which is fine for the `https://github.com/...` paths we open and keeps the door deliberately narrow.

**Linux build host (one-time setup).** Building the AppImage needs the Rust toolchain plus Tauri's GTK/WebKit deps and AppImage tooling:

```bash
sudo pacman -S --needed rustup base-devel webkit2gtk-4.1 librsvg \
  libappindicator-gtk3 patchelf file openssl fuse2
rustup default stable
# linuxdeploy's gtk plugin copies gdk-pixbuf's loader dir, which current Arch
# no longer creates (loaders are built into gdk-pixbuf; librsvg dropped its
# pixbuf loader). An empty dir satisfies it — LeoGit only renders PNG icons:
sudo mkdir -p /usr/lib/gdk-pixbuf-2.0/2.10.0/loaders
gdk-pixbuf-query-loaders | sudo tee /usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache >/dev/null
```

`bundle.sh` sets `NO_STRIP=true` (linuxdeploy's bundled `strip` chokes on the `.relr.dyn` section modern binutils emits) and `APPIMAGE_EXTRACT_AND_RUN=1` (run nested plugin AppImages by extracting, not FUSE-mounting) for the Linux build, and fails fast with the `mkdir` command above if the gdk-pixbuf dir is missing. `pnpm-lock.yaml` is committed (not ignored) so `bundle.sh`'s `--frozen-lockfile` install is reproducible on a fresh clone.

Frontend bundle is now under 1 MB (Shiki + grammars moved out). Vite's default `chunkSizeWarningLimit` is sufficient.

### Diff syntax highlighting

The `highlight_diff` command (in [src-tauri/src/commands/highlight.rs](tauri-app/src-tauri/src/commands/highlight.rs)) tokenises every `Context | Add | Delete` row of a `FileDiff` using `syntect` against `two-face::syntax::extra_newlines()`, then returns **render-ready HTML strings** (one per flattened diff line) via [commands/render.rs](tauri-app/src-tauri/src/commands/render.rs). Tokens (`{ start, end, class }`, code-point indices matching `IntraLineRange`) are internal to the backend now — `TokenClass` never crosses the wire, so it carries no `repr`/serde contract.

**Tokenise the file, never the diff.** syntect is a stateful, line-sequential parser: a line's classification depends on every line above it. A diff supplies neither a line-1 start nor contiguous lines, so parsing the diff's own rows leaves the parser in whatever context a fresh `ScopeStack` begins in. For a `.svelte` file that context is top-level **markup**, so a hunk inside `<script lang="ts">` got tokenised as markup — a `listen<string>` generic came back as an HTML *tag*, and comments got no `Comment` class at all. A markup hunk looked perfect and a script hunk looked broken purely by where the hunk landed, which read as "highlighting is inconsistent".

`highlight_diff` therefore takes an optional `BlobSource` (`workingTree { repoPath }` for uncommitted changes, `commit { repoPath, sha }` for a committed diff — the frontend states *what it is looking at*, Rust owns the rev-specs). `highlight_from_blobs` reads each side's full blob via `git::read_blob` / `git::read_working_tree_file`, parses each from line 1 in `tokenize_file`, and maps tokens onto rows by line number: **`Delete` rows take the old blob, `Add`/`Context` the new**. The two sides never share a `ParseState`, so a deleted line's unterminated string or comment can no longer bleed into the rows below it. Parsing is the cost and recording is nearly free, so the `wanted` line set bounds the payload, not the work.

When `source` is absent or a blob can't be read (added/deleted file paths, an oversized file), it falls back to `highlight_from_diff_lines` — the legacy diff-only parse — so highlighting degrades rather than disappears.

**Scope resolution walks the stack, leaf-first.** `scope_to_class` descends `ScopeStack` rather than reading only the top scope, because a delimiter carries its container beneath it: `//` is `[source.ts, comment.line, punctuation.definition.comment]`. Reading just the leaf classified that as `Punctuation` (which renders with no class), so **every line comment was two-tone** — an uncoloured `//` before an italic body. Punctuation is treated as *transparent*: remembered as a fallback while the walk continues, so `//` inherits `Comment` from below. Leaf-first order keeps genuine nesting correct — the `name` in `` `hi ${name}` `` has `variable.other` above `string.template`, so it stays `Variable`. Matching uses `Scope::is_prefix_of` against a `LazyLock` table (most-specific first), which is allocation-free in what is the module's hottest loop.

**Markup languages need their own scope family.** The `TokenClass` table maps the *code* scopes (`keyword`, `string`, `entity.name.function`, …) that programming languages emit. Markdown — and reStructuredText, AsciiDoc, Textile — emit almost none of those; they tag their text with a `markup.*` family (`markup.heading`, `markup.bold`, `markup.italic`, `markup.strikethrough`, `markup.raw.inline`, `markup.quote`) plus `markup.underline.link` / `meta.link` / `meta.image` for links. With none of those in the table, `scope_to_class` fell through to `Plain` for every token, so a `.md` diff rendered as flat text while every code language highlighted. Those scopes now map to the `Heading | Strong | Emphasis | Strikethrough | Link | Raw | Quote` classes. The same leaf-first descent that colours `//` with its comment also colours a heading's `#`, a bold span's `**`, and a link's brackets with their construct, since each delimiter's `punctuation.*` scope sits above its `markup.*` container. Covered by `markdown_constructs_get_markup_classes`.

**A single tilde must not strike text through.** Sublime's Markdown grammar opens a strikethrough on a run of *one or two* tildes; GitHub (cmark-gfm) and markdown-it — VS Code's preview, which refuses a delimiter run shorter than two outright — both need two. Opened by one tilde, the grammar then closes on the next stray `~`, or, finding none, strikes the rest of the paragraph, so ordinary prose (`~25 min · ~2 h`, `~/leogit`) rendered muted and struck through. The syntax set ships pre-compiled, so the grammar is not ours to correct; its delimiter scopes are. [`single_tilde_strikes`](tauri-app/src-tauri/src/commands/highlight.rs) measures each `punctuation.definition.strikethrough.begin|end` run against the following op's offset and returns the byte ranges of every single-tilde run, which `drop_strikethrough` puts back to `Plain` — leaving nested constructs (a bold span inside the run) on their own classes. Runs are tracked on **every** line, recorded or not, since a `~~` run may legitimately open above the recorded window and span lines; `~~` runs are never touched. Pinned by `single_tilde_does_not_strike_through` and `double_tilde_run_survives_across_lines`.

**Fenced code blocks are re-highlighted by their info string.** A ```` ```lang ```` fence *should* highlight its body as `lang`, but the Markdown grammar only **embeds** a fixed subset of languages: `rust`, `python`, `js`/`ts`, `json`, `c`, `java`, `ruby`, `bash` come back with real `source.*` scopes, while `go`, `yaml`, `html`, `shell` (and many more) come back as opaque `markup.raw.code-fence` — a whole block of one scope. Relying on embedding therefore leaves *most* real-world code blocks flat (Go blocks were the tell). So `tokenize_file` resolves the fence itself: [`fence_role`](tauri-app/src-tauri/src/commands/highlight.rs) reads syntect's own fence scopes (`meta.code-fence.definition.begin|end`, `constant.other.language-name`) to find each fence's boundaries and info string — no hand-rolled CommonMark scanner — and [`resolve_fence_language`](tauri-app/src-tauri/src/commands/highlight.rs) maps that info string to a syntax via `find_syntax_by_token` (which matches names *and* extensions, so `go`, `ts`, `c++` all resolve). The body is then tokenized with **that language's own `ParseState`**, run in parallel with the Markdown parser (which keeps advancing so it still detects the closing fence). Every labelled fence highlights uniformly, embedded or not; an unlabelled or unknown-language fence (`mermaid`, `text`) has nothing to resolve and stays plain. This is why the table maps only `markup.raw.inline` (inline `` `code` ``) and *not* bare `markup.raw` — a code-fence body must never take the flat `Raw` tint. The fence path is gated to Markdown (`text.html.markdown`) since only it emits those scopes. Pinned by `markdown_fenced_code_block_highlights_by_info_string` (Go **and** Python) and `markdown_unlabelled_fence_body_stays_plain`.

**HTML emission lives in [commands/render.rs](tauri-app/src-tauri/src/commands/render.rs), shared by both render phases.** `parse_diff` calls `plain_html` (escaped text + intra-line backplate) so phase 1 ships inside the parse payload; `highlight_diff` calls `highlighted_html`, which lays one `.syn-*`-classed span per token over the *same* backplate — one implementation, so the phases can't drift. `render_line` clamps malformed token bounds, fills inter-token gaps as plain text, escapes only `&`/`<`/`>` (element content, never attribute values), and splits spans around the intra-line range, merging classes on the overlap. Theme swap is pure CSS — `--syn-*` variables in [app.css](tauri-app/src/app.css) flip with `:root[data-theme]`. `Plain`/`Variable`/`Punctuation` deliberately map to no class and inherit `--text-primary`. The markup classes carry font styling as well as colour — `.syn-strong` is bold, `.syn-emphasis`/`.syn-quote` italic, `.syn-strike` struck, `.syn-link` underlined — so a Markdown diff reads the way the rendered document would. Pinned by the `render::tests` (escaping, class merging, code-point indexing, bound clamping).

Guards: lines over `MAX_HIGHLIGHT_LINE_LEN = 1024` chars are still *parsed* (state below them depends on it) but not recorded, mirroring `MAX_INTRA_LINE_LEN` in `commands/diff.rs`; files over `MAX_HIGHLIGHT_FILE_LINES = 20_000` bail to the fallback.

**`highlight_diff` must stay `#[tauri::command(async)]`.** A plain `#[tauri::command]` runs on the **main thread**. Tokenising whole blobs is ~20× more expensive than the old diff-only parse — the repo's largest file (`git.rs`, 3286 lines) measures **~52 ms release / ~284 ms debug** vs ~2 ms / ~14 ms diff-only — which is enough to beachball the cursor on every file switch. The old parse was cheap enough to hide that the command was on the UI thread at all. The sibling diff commands (`parse_diff`, `get_diff`, `get_commit_diff`, `generate_patch`) are now `(async)` too, along with every other subprocess/filesystem command (see *Network resilience*, layer 1).

DiffViewer's debounced (80 ms) phase 2 and its `lastDiff` guard (which keeps the 2 s status poll from re-tokenising) mean each file switch costs one tokenise, off the UI thread, after plain text has already painted — so there is no token cache yet.

**`parse_diff` returns a `ParsedDiff` wrapper, not a bare `FileDiff`.** Alongside `file_diff` it carries everything else the viewer would otherwise re-derive per render: `html` (the phase-1 lines above), `sbs_pairs` (the side-by-side pairing — context/header rows spanning both columns, each delete run zipped against the following add run, `NoNewline` markers rowless), and `additions`/`deletions` for the header badge. The pairs reference lines by **flat/global index** — the same indexing the per-line HTML and the selection map use — and the viewer resolves them through a trivial `flatLines` flatten, so the pairing algorithm itself lives only in [diff.rs:build_sbs_pairs](tauri-app/src-tauri/src/commands/diff.rs). `FileDiff` itself deliberately stays lean and wire-identical to before, because the frontend round-trips it back into `highlight_diff` / `generate_patch` — putting the derived artifacts on it would echo them over IPC on every highlight. Covered by the `diff::tests` (run zipping, `NoNewline`, backplate HTML).

## Accessibility patterns

The frontend builds warning-free (`pnpm check` and `vite build` both report 0 a11y warnings). These conventions keep it that way — Svelte's compiler enforces them:

- **Overlays close via backdrop target-check, not `stopPropagation`.** Every modal/dropdown backdrop is `role="presentation"` with `onclick={(e) => { if (e.target === e.currentTarget) close() }}`. The inner dialog is `role="dialog" aria-modal="true" tabindex="-1"` with **no** click handler. The old pattern (inner `onclick={e => e.stopPropagation()}`) tripped both "click handler needs a keyboard handler" and "dialog role needs a tabindex". Affects [ErrorModal](tauri-app/src/lib/components/ErrorModal.svelte), [ForcePushConfirm](tauri-app/src/lib/components/ForcePushConfirm.svelte), [SettingsOverlay](tauri-app/src/lib/views/SettingsOverlay.svelte), [HelpOverlay](tauri-app/src/lib/views/HelpOverlay.svelte), and the repo/branch overlays in [MainLayout](tauri-app/src/lib/views/MainLayout.svelte).
- **Resize handles are `role="slider"`, not `role="separator"`.** A focusable separator (the ARIA "window splitter") is flagged by Svelte either way — the mouse listener warns on a non-interactive role, and adding `tabindex` warns again (`a11y_no_noninteractive_tabindex`). `slider` is the interactive role Svelte accepts, and it fits: each handle has `tabindex=0`, `aria-orientation`, `aria-valuenow/min/max`, and an `onkeydown` (Arrow keys nudge by `RESIZE_STEP` = 16px, Home/End jump to min/max). The keyboard handlers share one `splitterKey()` helper in MainLayout.
- **`use:autofocus`, never the `autofocus` attribute.** The attribute is flagged (`a11y_autofocus`) and is unreliable for inputs that mount inside `{#if}` blocks. The [autofocus action](tauri-app/src/lib/actions/autofocus.ts) calls `node.focus()` on mount instead.
- **Autocorrect is disabled once, at the root.** `<html>` in [index.html](tauri-app/index.html) carries `autocorrect="off" autocapitalize="off" spellcheck="false"`. All three are inheritable HTML attributes, so every descendant input/textarea/contenteditable inherits them — no field opts out individually, and WebKit's macOS autocorrect pills, inline predictions, and spell squiggles stay off app-wide. Only add these attributes to a specific field if it needs to *re-enable* the behavior.
- **Keyboard shortcuts attach to the interactive field, not the container.** The commit composer's Cmd+Enter / Cmd+G handler lives on the summary `<input>` and description `<textarea>`, because Svelte treats `<div>` and `<form>` as non-interactive and warns on listeners attached to them. The container stays a plain `role="form"` landmark. Truly global shortcuts (Cmd+P push/publish in [Header](tauri-app/src/lib/components/Header.svelte), Cmd+R / Cmd+L in [MainLayout](tauri-app/src/lib/views/MainLayout.svelte)) bind a `window` `keydown` listener in `onMount` instead, so they fire regardless of focus.
- **Searchable repo lists share one keyboard-nav helper.** The startup picker ([RepoPicker](tauri-app/src/lib/views/RepoPicker.svelte)), header switcher ([RepoDropdown](tauri-app/src/lib/views/RepoDropdown.svelte)), and Clone dialog ([CloneOverlay](tauri-app/src/lib/views/CloneOverlay.svelte)) all let you type-then-arrow: ↑/↓ move a keyboard cursor (`activeIndex`, reset to the top match whenever the query changes) and Enter picks the highlighted row (opens it, or in Clone sets the clone target). The two reusable pieces live in [listNavigation.ts](tauri-app/src/lib/actions/listNavigation.ts) — `nextActiveIndex()` (wrapping index math) and the `scrollIntoViewWhenActive` action (`block: 'nearest'`, so already-visible rows never jump). The active row shows a `--border-active` inset ring, distinct from hover/selected fills. MainLayout's global `keydown` never interferes because it early-returns when focus is in a field and only handles Escape + meta-combos.
- **The Clone dialog list is one tab stop, not one-per-row.** Its repo rows are `role="option" tabindex="-1"` inside a `role="listbox" tabindex="0"` container, so Tab flows filter input → sort button → list → Local path → Browse → Cancel/Clone (rows are reached by arrows, not Tab). The filter input is a `role="combobox"` with `aria-controls`/`aria-activedescendant` pointing at the listbox and its active option, and `handleListKeyDown` is shared by the input and the listbox so arrows/Enter work from either.

## Notable invariants

These are easy to break and hard to debug; respect them when touching the relevant area.

- **Hunk lines include the `@@` header.** `hunks[i].lines[0]` is the hunk header itself. The flat line index used by `DiffSelection.diverging_lines` is `sum(prev_hunk.lines.length) + line_idx_in_current`, and this sum *includes* every header line. Both the Rust patch builder and the Svelte diff viewer rely on this.
- **`selectedFiles` is derived from status, not stored.** It's recomputed on every status refresh from `present − userDeselected`. Never persist `selectedFiles` directly — persist `userDeselected` (and we don't even do that across sessions today).
- **`git status` uses porcelain v2 `-z` (NUL-delimited).** Plain `--porcelain` will silently corrupt paths with spaces or unicode. If you change the args, make sure the parser stays NUL-aware.
- **The remote name is the NAME, not the URL.** `get_remote` returns the first line of `git remote` (typically `origin`), not the fetch URL. The Pull/Push/Fetch commands feed this directly to `git`. Note `get_remote` falls back to the literal `"origin"` when there are **no** remotes, so it can't be used to detect "no remote" — `RepoStatus.has_remote` (computed in `get_status` from a single `git remote` call, reused by the ahead/behind fallback) is the real signal, and the Header switches Push → "Publish to GitHub" on it.
- **Publishing uses `gh repo create`, not our own API.** `gh_publish_repo` shells out to `gh repo create <name> --source <repo_path> --remote origin --push [--private|--public] [--description ...]`, inheriting the user's `gh` auth. It's the one-shot equivalent of GitHub Desktop's "Publish Repository": creates the remote repo, adds `origin`, and pushes. `gh`'s stderr (missing auth, name collision) is surfaced verbatim to the error modal.
- **A repo path is whatever `paths::canonicalize` returns.** Discovery, `repo_root`, `init_repo` and `resolve_launch_target` must all produce the identical string for the same folder — they feed one de-dupe set, the `last_opened_repo` comparison, and the `repoIdentifiers` / `repoActivity` / `repoSync` cache keys, so a path only one of them can produce shows up as a duplicate repo with no badges. Calling `fs::canonicalize` directly re-introduces the Windows verbatim prefix and breaks exactly that. Pinned by `repo_paths_are_ordinary_and_agree_across_producers`.
- **Terminal sessions die with the repo.** When `appState.repoPath` changes, `MainLayout`'s effect resets `terminalSessionId = 0`, which keys the `<Terminal>` component to unmount and call `close_terminal`. Don't try to "carry" a session across repos.
- **Diff content `\n` round-trip.** Empty diff lines come through as `""` from `String::split('\n')` but in real unified diff format are ` ` (a single space). `parse_diff` reconstructs the leading space so the patch builder generates valid unified diffs.
