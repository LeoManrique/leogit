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
| Frontend bundler | Vite 8 | `terser` for minified release builds |
| Type system | TypeScript 5.9 strict | `$lib/*` alias points at `src/lib/*` |
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
│   │       ├── actions/             # Svelte use: actions (autofocus)
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

### Command-line repo opening

The `leogit [dir]` shell command (installed by `install.sh`, see *Release pipeline*) opens a repo straight from a terminal. All the app-side logic lives in [commands/launch.rs](tauri-app/src-tauri/src/commands/launch.rs):

- **Argv → repo path.** `resolve_repo_arg` takes the first non-flag argument, resolves it against the cwd, **canonicalizes** it (so it de-dupes against the canonical paths from `discover_repos`), and keeps it only if it's a git repo. A bare `leogit` or a non-repo arg resolves to `None`, so the app just launches/focuses.
- **Cold start** (app not running): `main.rs` calls `resolve_repo_arg` *before* the builder and stashes the result in a process-global via `set_pending_open_repo`. The frontend claims it once on mount through the `take_pending_open_repo` command (`appApi.takePendingOpenRepo`), which clears it so a reload won't re-open. In `App.svelte` this wins over the remembered `last_opened_repo` and is added to the repo list even if it lives outside the scan paths.
- **Warm start** (app already running): `tauri-plugin-single-instance` — registered **first** in `main.rs`, as the plugin requires — detects the second launch, hands its argv/cwd to `handle_second_instance`, which focuses the window and emits an `open-repo` event with the resolved path. The plugin keys on the app identifier via a `/tmp/<identifier>_si.sock` Unix socket (the second process connects, forwards, and `exit(0)`s). The frontend handles the event by phase: `MainLayout` (in `main`) reuses `handleSwitchRepo` via `openExternalRepo`; `App.svelte` handles the pre-`main` phases. The two listeners are mutually exclusive — `App` ignores the event while phase is `main`, and `MainLayout` is only mounted then.

### Windows console suppression

Release builds set `windows_subsystem = "windows"` (in `main.rs`), so the app runs with no attached console. On Windows a console-less process that spawns a console subprocess gets a **new console window allocated and briefly flashed** for each call — and because the UI polls `git status` every 2s, that would mean a `cmd` box flickering on screen continuously, plus one on every fetch/commit/diff. Every subprocess spawn therefore routes through [commands/process.rs](tauri-app/src-tauri/src/commands/process.rs): `hide_console` (std `Command`) and `hide_console_async` (tokio `Command`) set the `CREATE_NO_WINDOW` creation flag; both are no-ops off Windows. Call sites: `git_cmd` / `git_net_cmd` (git.rs), `apply_patch` (diff.rs), `check_auth` / `gh_repo_list` / `gh_clone` / `gh_publish_repo` (gh.rs), and both `claude` spawns (ai.rs). The PTY shell in terminal.rs is intentionally exempt — ConPTY is a pseudo-terminal, not a console subprocess, so it never flashes a window.

### Network resilience (offline / flaky)

Every remote-touching command is engineered so an unreachable or flaky network degrades a badge — it never freezes the app. Three layers:

1. **Off the main thread.** All network commands (`fetch`, `pull`, `push`, `clone_repo`, `repo_sync_status`, `delete_remote_branch`, and the four `gh` commands) are declared `#[tauri::command(async)]`. A plain synchronous Tauri command runs on the **main thread** — a blocking `git fetch` there freezes the window; `(async)` runs it on a worker thread instead, so the 2s status poll and repo switches keep flowing while a fetch is in flight.
2. **Time-boxed subprocesses.** `process::run_timed(cmd, label, timeout)` is the single chokepoint: it spawns the child, drains both pipes on helper threads (so a chatty `git --progress` can't pipe-buffer-deadlock), and **kills the child** if it outlives `timeout`, returning a `… timed out …` error. `git_net_cmd` additionally bakes transport timeouts into the command — `GIT_SSH_COMMAND="ssh -o ConnectTimeout=N -o BatchMode=yes"` (SSH connect cap + no interactive prompts) and `-c http.lowSpeedLimit=1000 -c http.lowSpeedTime=N` (abort an HTTP transfer that stalls). Budgets: **background** badge fetches are short (8s connect/stall, 12s hard kill — fail fast, keep last-known counts); **user-initiated** transfers are generous (15/30s, 600s hard kill — never kill a real large transfer, only a wedged one). Unit-tested in `process::tests` (`run_timed_kills_a_hung_child_promptly`).
3. **Don't keep firing when down.** [services/connectivity.ts](tauri-app/src/lib/services/connectivity.ts) gates *automatic/background* fetches (the auto-fetch timer, the tiered scheduler, the refocus/cold-open resync) on `navigator.onLine` plus a consecutive-failure circuit breaker: after 2 failures it opens with an exponential backoff window (30s → 5min cap), suppressing background fetches until the window lapses, when exactly one probe is allowed through. `repo_sync_status` returns a `fetched` flag so the breaker can tell a real fetch failure from a no-remote repo. User-initiated actions (Pull/Push/switch) always attempt (still bounded by the backend timeout) and their outcome feeds the breaker, so a successful manual pull — or the OS `online` event — re-opens background syncing immediately and triggers a resync.

## IPC contract

The frontend never touches Tauri's raw `invoke` API directly; every backend call goes through a typed wrapper in [src/lib/api/commands.ts](tauri-app/src/lib/api/commands.ts). The wrappers are grouped into namespaces matching the backend modules:

| Namespace | Commands | Backend file |
|---|---|---|
| `configApi` | `loadConfig`, `saveConfig`, `loadState`, `saveState` | `commands/config.rs` |
| `gitApi` | `getStatus`, `getHeadSha`, `getDiff`, `getDiffWhitespaceIgnored`, `getCommitDiff`, `getSelectedDiff`, `getLog`, `getCommitFiles`, `listBranches`, `createBranch`, `switchBranch`, `deleteBranch`, `deleteRemoteBranch`, `renameBranch`, `commit`, `hasStagedChanges`, `discardFiles`, `appendToGitignore`, `formatCommitMessage`, `repoSyncStatus`, `fetch`, `pull`, `push`, `getAheadBehind`, `getRemote`, `mergeBranch`, `mergeSquash`, `commitSquashMerge`, `mergeAbort`, `isMerging`, `countCommitsToMerge`, `discoverRepos`, `isGitRepo`, `getRepoName`, `cloneRepo`, `getLastCommitTimestamp` | `commands/git.rs` |
| `diffApi` | `parseDiff`, `generatePatch`, `generateInversePatch` | `commands/diff.rs` |
| `osApi` | `revealPath`, `openPath` | `commands/os.rs` |
| `ghApi` | `checkAuth`, `repoList`, `clone` | `commands/gh.rs` |
| `aiApi` | `generateCommitMessage`, `checkProviderAvailable` | `commands/ai.rs` |
| `appApi` | `takePendingOpenRepo` | `commands/launch.rs` |
| Terminal | `start_terminal`, `write_terminal`, `resize_terminal`, `close_terminal` (called via `invoke` directly from `Terminal.svelte`) | `commands/terminal.rs` |

Every command is registered in [src-tauri/src/main.rs](tauri-app/src-tauri/src/main.rs) via `tauri::generate_handler![…]`. **Adding a new command requires three edits**: implement it in `commands/<module>.rs`, register it in `main.rs`, wrap it in `api/commands.ts`.

## State management (frontend)

The three core writable Svelte stores, all in [src/lib/stores](tauri-app/src/lib/stores):

- **`appState`** — top-level phase machine (`loading` / `repo-picker` / `main` / `error`), the discovered repo list, the chosen repo path, and whether `gh` is authenticated.
- **`repoState`** — everything tied to the currently open repo: status (branch, upstream, ahead/behind, files, isMerging), log pagination, branches, the user's selection sets (`selectedFiles`, `userDeselected`), per-file diff selection (`Map<path, DiffSelection>`), active file/diff, active commit/files/diff, loading flags, last error.
- **`config`** — the live Config object. `refreshConfig()` reloads from disk and also calls `applyTheme()` which flips `document.documentElement.dataset.theme`.

Alongside these are smaller purpose-built stores: **`repoIdentifiers`** and **`repoActivity`** lazily cache each repo's GitHub identifier and last-commit timestamp (module-level maps that re-publish on each fetch, so reopening the repo picker is free), **`repoSync`** caches each repo's ahead/behind counts for the picker's pull/push badges (`setRepoSync` records counts the active poll already computed; `syncRepo` fetches + recomputes one repo, with per-path in-flight de-duplication), and **`reposState`** owns the persisted `repos-state.json` document — the `repoSortMode` / `cloneSortMode` / `recentRepos` writables plus `patchReposState` (the single read-modify-write writer — every patch is chained through one promise tail so concurrent writes can't interleave and lose a field, since the load+save spans two unlocked IPC calls), `recordRecentRepo` (promote a repo to the front of the recents list, capped at 50; returns the persistence promise), and `hydrateReposState` (startup seed).

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

- **Status poll** — every 2000 ms. Runs `get_status` silently + `get_head_sha`. If the HEAD SHA changed, the commit log is refreshed in place keeping the same loaded count so the user doesn't lose scroll position. Each run also pushes the active repo's ahead/behind into the `repoSync` store (via `setRepoSync`) so the picker badge for the open repo stays live without a dedicated fetch.
- **Auto-fetch** — every `fetch_interval_ms` (default 30 000). Skipped if the user is currently typing in an input/textarea or the window is hidden. Calls `fetchActiveRemote` (`git fetch --prune --recurse-submodules=on-demand` against the first remote) then a silent `get_status`. `fetchActiveRemote` self-skips when offline / backing off and reports its outcome to the connectivity breaker (see *Network resilience*).
- **Tiered repo-sync scheduler** ([repoSyncScheduler.ts](tauri-app/src/lib/services/repoSyncScheduler.ts)) — three intervals (2 / 5 / 10 min) plus staggered startup kicks. Each tick slices the `recentRepos` list (active excluded) into tiers — next 4, next 5, next 10 — and refreshes each via `repo_sync_status` sequentially. Tier syncs are tagged `background`, so the whole scheduler goes quiet while offline (each `syncRepo` consults the breaker) instead of grinding through dead fetches. Started in `initialize` (after `hydrateReposState` resolves, so recents are seeded) and stopped on unmount.

On regaining focus (`window` `focus`) or visibility (`visibilitychange`), `MainLayout` runs a one-shot **resync** — `fetchActiveRemote` (so a moved upstream surfaces immediately, unlike before), silent `get_status`, HEAD poll, a *forced* re-fetch of the diff for the file open in the changes pane (`loadDiffForFile(file, { force: true })`), and `repoSyncScheduler.refocusSync()` (the throttled top-tier refresh). A `resyncing` guard collapses the focus+visibility double-fire (common under tiling WMs) into a single run. All listeners, intervals, and scheduler timers clear on unmount.

## Git layer

All git operations go through `std::process::Command::new("git")` with these defaults set in `git_cmd`:

- `current_dir(repo_path)` — never relies on the cwd of the host process.
- `TERM=dumb` — suppresses pagers and color codes.
- `GIT_TERMINAL_PROMPT=0` — prevents a credential prompt from blocking the process indefinitely.

Helpers shape the output:

- `run_git_raw` — returns the raw bytes (used for NUL-delimited or binary-ish output, e.g. `status --porcelain=v2 -z`).
- `run_git` — returns trimmed UTF-8 (most line-oriented commands).
- `run_git_combined` — returns `(success, stdout+stderr)` regardless of exit code (used for local commands like `merge` where the error message is the value).
- `run_git_net` — the **network** runner (`fetch`/`pull`/`push`/`clone`/badge fetch). Builds the command via `git_net_cmd` (SSH/HTTP transport timeouts) and runs it through `process::run_timed` (hard kill-timeout). See *Network resilience* for the full rationale and budgets.

### Status parsing

`get_status` uses `status --untracked-files=all --branch --porcelain=v2 -z`. Under `-z` the **whole** stream is NUL-terminated — `# branch.*` headers included (there are no newlines anywhere) — so the parser walks the leading `# `-prefixed records by splitting on NUL, stops at the first non-header record, then splits the remainder on NUL for the file entries. Type-2 (rename) entries occupy two NUL segments — the second holds the original path — so the loop manually advances `i` when it sees `2 `. (Splitting headers on `\n` instead of NUL was a long-standing latent bug: it parsed zero headers, so `has_upstream` silently stayed false and ahead/behind survived only via the rev-parse + remote-tracking fallbacks — `repo_sync_status`, lacking those fallbacks, returned 0/0. Regression-tested in `status_parses_upstream_and_ahead_behind_from_porcelain_headers`.)

Branch fallback: if `# branch.head` is absent (empty repo) the code calls `git rev-parse --abbrev-ref HEAD` to fill in.

Ahead/behind: `# branch.ab` is only emitted when the branch has a tracking upstream. For a branch that was never `push -u`'d but has a matching `refs/remotes/<remote>/<branch>`, the shared `remote_tracking_ahead_behind` helper computes the counts with `git rev-list --left-right --count HEAD...<ref>` (left = ahead, right = behind) without flipping `has_upstream` (which still gates whether the next push needs `--set-upstream`). `repo_sync_status` — the lighter sibling powering the picker badges — reuses both `first_remote` and that helper but runs `status --untracked-files=no` (headers only, no file scan) and optionally fetches first. Its fetch is best-effort and **time-boxed** (`run_git_net`, background budget): a failure/timeout swallows so a stale-but-known count still comes back, and the outcome is surfaced as the `fetched` flag for the frontend's connectivity breaker.

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

`append_to_gitignore(repo_path, patterns)` appends ready-to-write lines to the repo-root `.gitignore`, ensuring a trailing newline first and skipping any pattern already present (compared trimmed, de-duped within the batch). The frontend builds the patterns: "Ignore File" escapes the path's glob metacharacters (`[ ] ! * # ?`, via `fileActions.escapeGitignorePath`) so it matches verbatim; "Ignore All .ext" writes a raw `*.ext` glob.

### Log parsing

`get_log` uses a custom format with `%x01` field separators and `%x00` record separators:

```
%H%x01%h%x01%s%x01%b%x01%an%x01%ae%x01%ad%x01%cn%x01%ce%x01%cd%x01%P%x01%(trailers:unfold,only)%x01%D%x00
```

Dates come back in `--date=raw` form (`<unix> <tz>`). To avoid pulling in `chrono`, the code implements `civil_from_unix` using Howard Hinnant's proleptic Gregorian algorithm and emits ISO-8601 strings manually.

The trailing `%D` captures every symbolic ref pointing at the commit (branches, `HEAD`, tags); it's split on commas into `CommitInfo.refs`. The commit list derives tag pills from this by keeping entries prefixed `tag: ` and stripping that prefix — no extra git call needed.

On a fresh repo with an unborn HEAD, `get_log` short-circuits to an empty list rather than letting `git log` fail with exit 128 ("does not have any commits yet"). It uses the shared `has_commits` helper (`git rev-parse --verify --quiet HEAD`, exit 0 only when HEAD resolves to a commit) — a precise check that, unlike blindly accepting exit 128, never masks a genuinely broken repo. The History tab then renders its "No commits yet" empty state instead of an error.

The same `has_commits` check guards diffs. `run_diff` anchors a tracked file's diff at `HEAD`, but on a fresh repo `git diff HEAD -- <file>` fails with "fatal: bad revision 'HEAD'". When there are no commits, it substitutes git's canonical empty-tree SHA (`EMPTY_TREE_SHA = 4b825dc642cb6eb9a060e54bf8d69288fbee4904`, which git always recognizes) so the staged/working file shows as fully added. Untracked files are unaffected — they already diff against `/dev/null` via `--no-index`.

### Discovery

`discover_repos` expands `~`, canonicalizes each scan root, and recursively walks up to `max_depth` levels. A directory is a repo if it contains a `.git` file or directory (handles worktrees). Hidden directories are skipped, and the scan does not descend into a discovered repo.

## AI layer

Both providers share `build_prompt` — a strict JSON-only instruction with rules for imperative-mood, 50-char-or-less titles. Responses are parsed by `parse_commit_message_text` which:

1. Strips markdown code fences (```json … ```).
2. Tries `serde_json::from_str` and accepts any of `title`/`summary`/`subject`/`message` and `description`/`body`/`details`.
3. Falls back to "first line = title, rest = description" if JSON parsing fails entirely.

**Claude** spawns `claude --print --output-format json --model <model>` and pipes the prompt to stdin. `parse_claude_envelope` reads the CLI's JSON envelope `{"type":"result","subtype":…,"is_error":bool,"result":"<text>", "api_error_status":…}`:

- **`is_error == true`** (e.g. a transient `529 Overloaded`) → the CLI's own message is surfaced verbatim as an `Err`. Critically, the CLI exits **0** in this case, so the `is_error` flag — not the exit code — is what distinguishes a failure; without this check the error text would be parsed straight into the commit title.
- otherwise the model's reply lives in `result` and is parsed as the commit message (falling back to parsing raw stdout when the text isn't the envelope).

Two robustness measures around the spawn: the prompt is streamed on a separate task (so a large diff can't deadlock against the child filling its stdout pipe before we drain it), and the child is `kill_on_drop` so a slow CLI doesn't outlive a timeout as an orphan. The spawn also sets `CLAUDE_CODE_MAX_RETRIES = 2`: by default the CLI retries a transient overload with backoff for *minutes*, far past our timeout, so the user only ever saw "timed out"; the cap makes it fail in seconds with the real error while still riding out a single blip.

**Ollama** posts to `<base_url>/api/generate` with `{model, prompt, stream: false, format: "json"}`. A 404 is translated to a friendly `ollama pull <model>` hint.

Both providers run with a 120 s timeout (`DEFAULT_TIMEOUT_SECS`). Diff caps: 20 MB (Claude) / 50 MB (Ollama).

`check_provider_available` lets the UI gate features without surfacing raw errors: `claude --version` for Claude, `GET /api/tags` for Ollama with a 5 s timeout.

## Terminal layer

`PtySession` holds the master PTY, the writer half, and the child process. Sessions are stored in a global `Mutex<HashMap<u32, Arc<Mutex<PtySession>>>>` keyed by a monotonic `AtomicU32`.

`start_terminal`:
1. Opens a PTY at 24×80 via `portable-pty`.
2. Picks the shell via `default_shell()`: on Unix `$SHELL` (`/bin/zsh` fallback); on Windows `$SHELL` is ignored (it may hold a non-resolvable POSIX path when launched from Git Bash) and the first of `pwsh.exe` → `powershell.exe` → `cmd.exe` found on `PATH` is used, mirroring Windows Terminal's default shell.
3. Forwards the parent env plus `TERM=xterm-256color`.
4. Spawns the shell with cwd = repo path.
5. Stores the session, then spawns a reader thread that loops on `read()` and emits `terminal-output-<pid>` events with the UTF-8 lossy payload. On EOF the session is removed and `terminal-closed-<pid>` is emitted.

`write_terminal` / `resize_terminal` / `close_terminal` lock the session map, clone the `Arc`, drop the map lock, then operate on the session. `close_terminal` calls `child.kill()` and removes the entry.

The frontend mounts `<Terminal>` keyed by `${repoPath}:${terminalSessionId}` so swapping repos or hitting "New session" forces a fresh component, which in turn dispatches a new `start_terminal`. The previous component's cleanup invokes `close_terminal` on its tracked pid.

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
- `repos-state.json` — `last_opened_repo`, `last_clone_dir`, and the two sort-toggle preferences (`repo_sort_mode`, `clone_sort_mode`). Every field is `Option`/`#[serde(default)]` so older state files load fine. JSON instead of TOML to keep it cheap to extend. The frontend treats this file as a single document: the `reposState` store's `patchReposState` does read-modify-write so one writer never clobbers another field, and `hydrateReposState` seeds the reactive sort-mode stores at startup.
- All save commands write atomically via `fs::write`.

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

The Tauri dev command uses `beforeDevCommand: pnpm run dev:vite` (per `tauri.conf.json`) so the Vite dev server starts in-process. Release builds use `beforeBuildCommand: pnpm run build:frontend` which writes static assets to `tauri-app/dist`, then `frontendDist: "../dist"` points the bundle at them.

Bundle targets: `app` + `dmg` (macOS), `deb` + `appimage` (Linux), `msi` (Windows).

### Release pipeline (`scripts/`)

`deploy_releases.sh` runs per-platform and uploads to one shared GitHub Release; run it once on each OS to publish a complete release. It validates prerequisites, then guards against shipping behind the live release — it queries GitHub's `/releases/latest` (the same endpoint `install.sh` installs from) and aborts if the version it's about to ship is older than that tag, since a stale local tree would otherwise clobber artifacts onto a superseded release. It then bumps/commits the version across `tauri.conf.json` / `Cargo.toml` / `package.json`, tags, then calls `bundle.sh` and packages the result:

- **macOS** — `bundle.sh` builds `leogit.app` (`--bundles app`) and ad-hoc signs it; the deploy script zips it with `ditto` into `LeoGit-<ver>-macOS-<arch>.zip`.
- **Linux** — `bundle.sh` builds an AppImage (`--bundles appimage`, no signing); the deploy script copies it to `LeoGit-<ver>-linux-<arch>.AppImage`.

`install.sh` is the curlable installer and auto-detects the platform: on macOS it unpacks into `/Applications`, strips quarantine, and re-registers with Launch Services; on Linux it drops the AppImage at `~/.local/bin/leogit.AppImage` behind a `~/.local/bin/leogit` wrapper, extracts the bundled icon, and writes a `~/.local/share/applications/leogit.desktop` launcher (warning if FUSE 2 is absent, since Arch ships only FUSE 3). The wrapper exports `WEBKIT_DISABLE_DMABUF_RENDERER=1` at launch when `/dev/nvidia0` is present (the proprietary NVIDIA driver's DMABUF/GBM path crashes WebKitGTK with "Failed to create GBM buffer" errors); it's detected per-launch rather than at install time because the active GPU is a runtime property, stays inert on AMD/Intel/nouveau, and honors a pre-set value. The desktop environment (GNOME, COSMIC, …) is irrelevant — both run the same WebKitGTK/GTK runtime — so one AppImage serves every Arch machine. As a final step it installs the `leogit [dir]` shell command into the user's login shell: it detects `$SHELL` (which survives `curl … | bash`, being inherited from the parent) and writes a `leogit()` function — into `~/.zshrc` (zsh), `~/.bashrc` on Linux / `~/.bash_profile` on macOS (bash), or an autoloaded `~/.config/fish/functions/leogit.fish` (fish); an unknown shell gets the snippet printed for manual setup. For zsh/bash the function lives inside an idempotent `# >>> leogit >>>` … `# <<< leogit <<<` marker block that re-installs replace rather than stack. The function resolves the directory and opens it (macOS `open -n --args`; Linux the PATH wrapper) — see *Command-line repo opening* for the app side.

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

The `highlight_diff` command (in [src-tauri/src/commands/highlight.rs](tauri-app/src-tauri/src/commands/highlight.rs)) tokenises every `Context | Add | Delete` row of a `FileDiff` using `syntect::parsing::ParseState` against `two-face::syntax::extra_newlines()`. It returns `Vec<Vec<Token>>` (one inner vec per flattened diff line), where each `Token` is `{ start: u32, end: u32, class: u8 }`. Indices are **code points**, matching `IntraLineRange`. Class is a `#[repr(u8)]` enum index — the wire format is whatever serde+serde_repr produces, so reordering the `TokenClass` enum is a breaking change.

The frontend ([DiffViewer.svelte:renderTokenLine](tauri-app/src/lib/components/DiffViewer.svelte)) maps each `class` to a `.syn-*` class name and emits one span per token, layering the intra-line backplate on the overlapping slice. Theme swap is pure CSS — `--syn-*` variables in [app.css](tauri-app/src/app.css) flip with `:root[data-theme]`.

Long-line guard: lines over `MAX_HIGHLIGHT_LINE_LEN = 1024` chars get an empty `TokenLine` so the parser never burns time on minified blobs. Mirrors the `MAX_INTRA_LINE_LEN` guard in `commands/diff.rs`.

## Accessibility patterns

The frontend builds warning-free (`pnpm check` and `vite build` both report 0 a11y warnings). These conventions keep it that way — Svelte's compiler enforces them:

- **Overlays close via backdrop target-check, not `stopPropagation`.** Every modal/dropdown backdrop is `role="presentation"` with `onclick={(e) => { if (e.target === e.currentTarget) close() }}`. The inner dialog is `role="dialog" aria-modal="true" tabindex="-1"` with **no** click handler. The old pattern (inner `onclick={e => e.stopPropagation()}`) tripped both "click handler needs a keyboard handler" and "dialog role needs a tabindex". Affects [ErrorModal](tauri-app/src/lib/components/ErrorModal.svelte), [ForcePushConfirm](tauri-app/src/lib/components/ForcePushConfirm.svelte), [SettingsOverlay](tauri-app/src/lib/views/SettingsOverlay.svelte), [HelpOverlay](tauri-app/src/lib/views/HelpOverlay.svelte), and the repo/branch overlays in [MainLayout](tauri-app/src/lib/views/MainLayout.svelte).
- **Resize handles are `role="slider"`, not `role="separator"`.** A focusable separator (the ARIA "window splitter") is flagged by Svelte either way — the mouse listener warns on a non-interactive role, and adding `tabindex` warns again (`a11y_no_noninteractive_tabindex`). `slider` is the interactive role Svelte accepts, and it fits: each handle has `tabindex=0`, `aria-orientation`, `aria-valuenow/min/max`, and an `onkeydown` (Arrow keys nudge by `RESIZE_STEP` = 16px, Home/End jump to min/max). The keyboard handlers share one `splitterKey()` helper in MainLayout.
- **`use:autofocus`, never the `autofocus` attribute.** The attribute is flagged (`a11y_autofocus`) and is unreliable for inputs that mount inside `{#if}` blocks. The [autofocus action](tauri-app/src/lib/actions/autofocus.ts) calls `node.focus()` on mount instead.
- **Keyboard shortcuts attach to the interactive field, not the container.** The commit composer's Cmd+Enter / Cmd+G handler lives on the summary `<input>` and description `<textarea>`, because Svelte treats `<div>` and `<form>` as non-interactive and warns on listeners attached to them. The container stays a plain `role="form"` landmark. Truly global shortcuts (Cmd+P push/publish in [Header](tauri-app/src/lib/components/Header.svelte), Cmd+R / Cmd+L in [MainLayout](tauri-app/src/lib/views/MainLayout.svelte)) bind a `window` `keydown` listener in `onMount` instead, so they fire regardless of focus.

## Notable invariants

These are easy to break and hard to debug; respect them when touching the relevant area.

- **Hunk lines include the `@@` header.** `hunks[i].lines[0]` is the hunk header itself. The flat line index used by `DiffSelection.diverging_lines` is `sum(prev_hunk.lines.length) + line_idx_in_current`, and this sum *includes* every header line. Both the Rust patch builder and the Svelte diff viewer rely on this.
- **`selectedFiles` is derived from status, not stored.** It's recomputed on every status refresh from `present − userDeselected`. Never persist `selectedFiles` directly — persist `userDeselected` (and we don't even do that across sessions today).
- **`git status` uses porcelain v2 `-z` (NUL-delimited).** Plain `--porcelain` will silently corrupt paths with spaces or unicode. If you change the args, make sure the parser stays NUL-aware.
- **The remote name is the NAME, not the URL.** `get_remote` returns the first line of `git remote` (typically `origin`), not the fetch URL. The Pull/Push/Fetch commands feed this directly to `git`. Note `get_remote` falls back to the literal `"origin"` when there are **no** remotes, so it can't be used to detect "no remote" — `RepoStatus.has_remote` (computed in `get_status` from a single `git remote` call, reused by the ahead/behind fallback) is the real signal, and the Header switches Push → "Publish to GitHub" on it.
- **Publishing uses `gh repo create`, not our own API.** `gh_publish_repo` shells out to `gh repo create <name> --source <repo_path> --remote origin --push [--private|--public] [--description ...]`, inheriting the user's `gh` auth. It's the one-shot equivalent of GitHub Desktop's "Publish Repository": creates the remote repo, adds `origin`, and pushes. `gh`'s stderr (missing auth, name collision) is surfaced verbatim to the error modal.
- **Terminal sessions die with the repo.** When `appState.repoPath` changes, `MainLayout`'s effect resets `terminalSessionId = 0`, which keys the `<Terminal>` component to unmount and call `close_terminal`. Don't try to "carry" a session across repos.
- **Diff content `\n` round-trip.** Empty diff lines come through as `""` from `String::split('\n')` but in real unified diff format are ` ` (a single space). `parse_diff` reconstructs the leading space so the patch builder generates valid unified diffs.
