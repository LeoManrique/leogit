# leogit — Technical Architecture

Functional behavior lives in [DESIGN.md](DESIGN.md). Visual design language lives in [STYLE.md](STYLE.md); the frontend contract shared by the Tauri and (planned) SwiftUI clients lives in [FRONTEND.md](FRONTEND.md). This document covers **how the code is organized** and the decisions that pin it together.

## Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri 2.11 | Native window, IPC, no Node runtime in production |
| Native dialogs | `tauri-plugin-dialog` 2.7 + `@tauri-apps/plugin-dialog` | Folder picker for the Clone dialog's destination (`dialog:allow-open` capability) |
| Single instance | `tauri-plugin-single-instance` 2.4 | Forwards a second `leogit <dir>` launch to the running window instead of duplicating it (see *Command-line repo opening*) |
| Window frame | `tauri-plugin-window-state` 2.4 | Saves size and position on exit, restores them at launch; `tauri.conf.json`'s 1280×800 is the first-run default only. No frontend surface — the plugin is registered and never invoked |
| Clipboard | `tauri-plugin-clipboard-manager` 2.3 + `@tauri-apps/plugin-clipboard-manager` | The terminal's OSC 52 writes only (`clipboard-manager:allow-write-text`). The app's own Copy actions stay on `navigator.clipboard`, which is fine for them: they are all raised by a click, and WebKit gates a programmatic write on exactly that — a shell asking for the clipboard has no click behind it |
| Backend language | Rust 2024 | Async via tokio (`features = ["full"]`); logic in the framework-free `leogit-core` crate |
| Frontend framework | Svelte 5 (runes) | `$state`, `$derived`, `$effect`, `$props` |
| Frontend bundler | Vite 8 (rolldown) | `terser` for minified release builds; `@xterm/*` split into its own chunk (rolldown `codeSplitting` group) to keep every chunk under the 500 kB warning |
| Type system | TypeScript 7 strict (native `tsc`) | `typescript` is npm-aliased to `@typescript/typescript6` — svelte-check/editors need the JS API until TS 7.1 ships the programmatic API. `$lib/*` alias points at `src/lib/*` |
| Diff syntax | syntect 5.3 + two-face 0.5 (Rust) | Class-based output; theme colours live in `--syn-*` CSS variables |
| Terminal UI (Tauri) | xterm.js 6 + FitAddon + WebLinksAddon | Black background, 12 px monospace; OSC 52 is hand-registered on `parser` (xterm ships no clipboard support) |
| Terminal UI (native) | SwiftTerm 1.18 (SPM) | Same black 12 px monospace; fed by the core PTY over a UniFFI callback |
| PTY | `portable-pty` 0.9 | Spawns user `$SHELL`, falls back to `/bin/zsh` / `cmd.exe` |
| HTTP | `reqwest` 0.13 | Used only for Ollama |
| Config | `toml` 1.1 + `directories` 6 + `serde_json` | `~/.config/leogit/{config.toml,repos-state.json}` |
| Recoverable delete | `trash` 5 | "Discard" sends never-committed files to the OS trash instead of unlinking |
| Native macOS client | SwiftUI, Swift 6 language mode | `apps/swift-ui-app`; macOS 26 deployment target, built with Xcode 26 |
| Swift ↔ Rust bridge | UniFFI 0.32 | Static lib linked into the app bundle; core's types cross via `#[uniffi::remote]` |
| macOS project generation | XcodeGen 2.46 | `project.yml` is the source of truth; the `.xcodeproj` is generated and gitignored |
| Build tool | `just` | Wraps `pnpm tauri …` and the macOS `xcodegen`/`xcodebuild` recipes |

## Repository layout

A **Cargo workspace**: all logic lives once in `core/` (the `leogit-core` crate), and each
client is a thin shell over it. The Tauri host embeds it directly; the SwiftUI client links the
same crate through UniFFI. The only place a UI framework leaks into the core is
`events::EventSink` (see *Core / host split* below).

```
leogit/
├── Cargo.toml                       # Workspace root — target/ and Cargo.lock live here
├── core/                            # leogit-core (lib leogit_core) — Tauri-free logic
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                   # Module map; the crate every client embeds
│       ├── events.rs                # EventSink seam: CoreEvent + trait (git progress, PTY out)
│       ├── config.rs                # load/save Config + ReposState
│       ├── git.rs                   # git operations (status, log, branch, discard, ignore, …)
│       ├── exclusions.rs            # the commit composer's opt-out grace window
│       ├── launch.rs                # `leogit <dir>` resolution + pending-target state
│       ├── diff.rs                  # parse_diff + build/apply patches (structured FileDiff)
│       ├── render.rs                # structured diff → pre-escaped HTML (the web-host renderer)
│       ├── highlight.rs             # syntect diff tokenizer (Token / TokenClass)
│       ├── gh.rs                    # GitHub CLI bridge (auth check, repo list, clone)
│       ├── ai.rs                    # Claude CLI + Ollama HTTP
│       ├── terminal.rs              # portable-pty session pool
│       ├── shell.rs                 # shell discovery for the terminal
│       ├── os.rs                    # reveal-in-file-manager + open-with-default-app
│       ├── paths.rs                 # the app's canonicalizer (never verbatim)
│       ├── process.rs               # CREATE_NO_WINDOW spawn + run_timed / run_blocking
│       ├── progress.rs              # git --progress step/weight parser
│       └── update.rs                # GitHub-release update check
├── apps/
│   ├── tauri-app/
│   │   ├── src/                     # Svelte 5 frontend (untouched by the core extraction)
│   │   │   ├── App.svelte           # Startup phases (loading → picker / main / error)
│   │   │   ├── main.ts              # Mounts App
│   │   │   ├── app.css              # Theme tokens + base element styles
│   │   │   └── lib/
│   │   │       ├── api/commands.ts  # Typed wrappers over every Tauri command
│   │   │       ├── actions/         # Svelte use: actions (autofocus, listNavigation, overlayStack)
│   │   │       ├── services/        # backgroundPolicy, pacedLoop, repoSyncScheduler, connectivity, …
│   │   │       ├── utils/           # path (basename), keyboard (event origin), focus, platform
│   │   │       ├── stores/          # appState, repoState, config (Svelte writables)
│   │   │       ├── components/      # Header, TabBar, FileList, CommitList, DiffViewer, …
│   │   │       └── views/           # MainLayout, RepoPicker, CloneOverlay, BranchDropdown, …
│   │   ├── src-tauri/
│   │   │   ├── src/
│   │   │   │   ├── main.rs          # PATH fix + plugins + invoke_handler registry
│   │   │   │   ├── lib.rs           # Declares shims / event_sink / launch_glue
│   │   │   │   ├── event_sink.rs    # ProgressSink (→ window emit) + TerminalChannelSink
│   │   │   │   ├── launch_glue.rs   # Window-focusing half of `leogit <dir>`
│   │   │   │   └── shims/           # One #[tauri::command] per core fn (config.rs, git.rs, …)
│   │   │   ├── capabilities/default.json
│   │   │   ├── tauri.conf.json
│   │   │   └── Cargo.toml           # tauri + plugins + leogit-core (path dep)
│   │   ├── package.json             # pnpm scripts (dev, build, check, lint)
│   │   ├── vite.config.ts           # $lib alias, port 5173
│   │   └── tsconfig.json
│   └── swift-ui-app/                # Native macOS client (SwiftUI)
│       ├── project.yml              # XcodeGen spec — the .xcodeproj is generated, gitignored
│       ├── scripts/build-rust.sh    # cargo build + uniffi-bindgen (Xcode pre-build phase)
│       ├── ffi/                     # leogit-ffi: UniFFI bridge crate over leogit-core
│       │   ├── Cargo.toml           # crate-type lib + staticlib; `bindgen` feature for the CLI
│       │   ├── uniffi.toml          # Swift module name + immutable records
│       │   ├── src/lib.rs           # #[uniffi::export] fns + #[uniffi::remote] type mirrors
│       │   ├── src/bin/…            # uniffi-bindgen-swift entry point
│       │   └── generated/           # GENERATED: LeoGitCore.swift + header + modulemap
│       └── Sources/LeoGit/
│           ├── App/                 # @main App + scene setup
│           ├── IPC/GitBridge.swift  # The only place Swift calls Rust (@concurrent wrappers)
│           ├── Stores/RepoStore.swift  # @MainActor @Observable state for the open repo
│           ├── Screens/             # ContentView, WelcomeView, the tab panes (Changes/History × Sidebar/DetailPane)
│           └── Design/              # Date formatting, FileStatus, path + shared file list
├── justfile                         # install / dev / build / check / format / mac-*
└── DESIGN.md / TECHNICAL.md / STYLE.md / FRONTEND.md / ROADMAP.md / README.md
```

### Core / host split

`leogit-core` compiles without `tauri` — it is plain `Result<T, String>` functions plus the
data types the UI serializes. The Tauri host (`apps/tauri-app/src-tauri`) adds only glue:

- **`shims/<module>.rs`** — one `#[tauri::command]` per core function, a 1:1 delegation
  (`leogit_core::git::get_status(repo_path)`). The command macro has to annotate the definition
  site because `generate_handler!` wires up what it generates, so the wrappers live on the host
  rather than the core function being registered directly. **Adding a command is four edits:**
  implement it in `core/src/<module>.rs`, add the shim in `shims/<module>.rs`, register it in
  `main.rs`, wrap it in `api/commands.ts`.
- **`event_sink.rs`** — the streaming seam. Core can't reach for an `AppHandle`, so the four
  event-emitting operations (`git::{pull,push,clone_repo}`, `terminal::start_terminal`) take an
  `Arc<dyn leogit_core::events::EventSink>` and hand it `CoreEvent`s. There are **two** sinks
  here, and each ignores the variants its operation cannot emit — the same split the UniFFI
  bridge makes, for the same reason: `ProgressSink` broadcasts `git-progress` on the app-wide
  event bus, while `TerminalChannelSink` writes one session's output and close into the
  `tauri::ipc::Channel` the frontend handed to `start_terminal` (see *Terminal transport*).
- **`launch_glue.rs`** — the window-focusing half of `leogit <dir>`; the pure argv→target
  resolution stays in `core/src/launch.rs`.

`render.rs` (structured diff → HTML) stays in core because the Svelte host is its only
consumer and the payload must stay byte-identical; the SwiftUI client consumes the structured
layer directly — `highlight::tokenize_diff` (public precisely for this) hands it the same
`Token` / `TokenClass` runs that `render.rs` collapses into `<span>`s, and
`Design/DiffLineText.swift` maps them onto `AttributedString`. `IntraLineRange` uses `u32`
indices to match `Token` and stay UniFFI-representable.

### Swift host (UniFFI)

`apps/swift-ui-app/ffi` (`leogit-ffi`) is the macOS equivalent of the Tauri shim layer: glue
only, one `#[uniffi::export]` per core function. Two decisions shape it:

- **Core stays UniFFI-free.** Types cross via `#[uniffi::remote(Record)]` / `(Enum)`
  declarations that restate core's structs in the bridge crate, so `leogit-core` gains no
  `uniffi` dependency — exactly as it gained no `tauri` one. A remote declaration must mirror
  the real type field-for-field, so drift in core surfaces as a **compile error in the bridge**
  rather than a silent wire mismatch. The cost is restating a struct when you expose it.
- **Blocking calls must leave the main actor.** Every core function shells out to `git` and
  waits. `GitBridge` marks each wrapper `@concurrent` (SE-0461); without it a `nonisolated
  async` function would inherit the caller's executor and freeze the UI.

The exported surface tracks the ported flows and stays 1:1 with core, with one deliberate
exception: the parsed diff crosses as a purpose-built `DiffPayload` record rather than
mirroring core's `ParsedDiff`, whose HTML array and side-by-side pairs are `WebView`
presentation the native client should not even be able to read. It asks for neither
(`DiffOptions { html: false, side_by_side: false }`), so core builds neither. The diff view
keeps the two-phase shape, now over one call: `get_parsed_diff` reads and parses in a single
crossing and paints the structure immediately; `tokenize_diff`, blob-backed so multi-line
constructs highlight correctly, recolours in place.
Reloads are seamless: `DiffStore` never clears what's on screen when a load starts — it
tracks a `phase` (`idle` / `loading(slow:)` / `failed`) beside the published
`payload`/`rows`/`tokens`, escalates to a spinner only when the load outlives
`slowLoadThreshold` (150 ms, Tauri's `SLOW_DIFF_THRESHOLD_MS`, via an unstructured timer task
racing the load under the same `generation` guard — `.task(id:)` cancelling the load must not
cancel the escalation for a blocking FFI call still running), and publishes nothing when the
fresh parse equals what's shown (`DiffPayload: Equatable`), so rows, scroll position, and
tokens survive an epoch bump untouched; tokens still refresh in the background on an equal
payload — context lines can recolour when blob content changed without the diff text changing
— and swap in only when different. `DiffView`'s content rule mirrors it: last-shown state
stays during a reload, and a fast first load stays blank rather than flashing a
sub-threshold spinner. Crossing the threshold is a **modifier on that content, not a
branch beside it** — `.opacity(0.45)` plus a top-aligned `ProgressView` overlay — because a
branch swaps the view SwiftUI builds and so destroys the `ScrollView`'s identity: the store
kept the payload, the view threw the scroll position away anyway, and the equality skip
above was preserving something nothing could see. `content`'s first branch is the dirty
submodule, decided before the read: `git diff` answers a submodule that changed inside with
a bare `Subproject commit <sha>-dirty` line, so the pane explains and the subprocess is
never spawned (the `.task` guards on the same flag).
Token `start`/`end` and `IntraLineRange` are code-point indices, which in Swift is the
`AttributedString.unicodeScalars` view — never `characters`, whose grapheme clusters can span
several code points.
The diff settings feed this path from `AppConfigStore` (below): `DiffStore.load` takes
`hideWhitespace` — working-tree targets pick the newly exported
`get_diff_whitespace_ignored` (`git diff -w`); commit diffs have no such variant in either
client — and `highlight`, whose off state skips the tokenize phase and drops any tokens on
screen (the Tauri `if (!sh) return` after the plain render). Both flags live in `DiffView`'s
`LoadKey`, so a Settings toggle re-keys the open diff through the seamless path above, where
the equality skip keeps scroll when nothing textual changed. `tab_size` is presentation:
SwiftUI `Text` honours no paragraph-style attributes, so `DiffLineText` expands tabs to
spaces with CSS `tab-size` stop math (next multiple of N columns) and remaps every token and
intra-line range through the expansion — a no-tab line pays one `contains` scan and nothing
else. Long lines always wrap, in both clients — the GitHub Desktop model.

Committing crosses as the same two calls the Svelte client makes — `format_commit_message`,
then `commit` — and needed no new type mirrors: core's `commit` owns the whole staging story
(it resets the index and re-stages exactly the files it was handed), so there is no separate
stage step to expose. Neither client uses `diff::generate_patch`/`DiffSelection` yet;
per-hunk staging is future work for both. Its Svelte scaffolding stays in place but stops
costing anything while unwired: `DiffViewer` renders the hunk header as a control only when
`showSelection` is set, so today's band is plain text — selectable like the rest of the diff,
and no longer one focusable no-op button per hunk on an unvirtualized list.

Branches and merge cross 1:1 as well (`list_branches`, `create_branch`, `switch_branch`,
`delete_branch`; `merge_branch`, `merge_squash`, `commit_squash_merge`, `merge_abort`,
`count_commits_to_merge`), with two flat mirrors (`BranchInfo`, `MergeResult`)
and none of the `EventSink` machinery — every branch/merge function in core is a plain
synchronous `Result<T, String>`. The multi-call sequences belong to the clients:
"New Branch" is `create_branch` then `switch_branch`, squash is `merge_squash` then
`commit_squash_merge` on success, and a *failed merge is data, not an error* —
`MergeResult { success: false }` with git's text and the conflicted paths, never a thrown
`GitError`, which is why each client decides for itself whether a refusal is a modal or a
line under a field. `rename_branch` and `delete_remote_branch` exist in core but stay unexported —
no client has UI for them yet, and the bridge doesn't carry dead surface.

Sync (`get_remote`, `fetch`, `pull`, `push`) is the first flow to cross **async** and the
first to cross the **callback seam**. The network functions are exported with
`async_runtime = "tokio"` — core runs them through `tokio::spawn_blocking`, which panics
without a live runtime context; the attribute wraps the future in `async_compat` so
UniFFI-driven polls enter one (this is why the `uniffi` dependency gains its `tokio`
feature) — and they surface in Swift as native `async throws` functions. Progress streams
back through core's `EventSink` seam: the bridge declares `SyncProgressListener` with
`#[uniffi::export(foreign)]` (a Swift-implemented protocol, the UniFFI analogue of
`TauriEventSink`), and a private `ProgressSink` adapter implements `EventSink`, translating
core's `GitProgress` — whose `op` label is a `&'static str` that cannot cross the FFI — into
a flat `SyncProgress` record: the core-computed aggregate percent (0–100, weighted per step
in `core/src/progress.rs`) plus git's raw progress line. Ticks are invoked on core's
stderr-reader thread, so the Swift side hops to the main actor itself and drops stragglers
by generation; there is no completion event — an operation is over when its `await` returns.
`get_ahead_behind` stays unexported: the native client reads ahead/behind from
`get_status` (`repo_sync_status` later joined the surface with the repo switcher — see
below, as did `clone_repo` and the gh clone surface with the Clone sheet).

AI commit-message generation crosses as the Tauri composer's own two-step pipeline —
`get_selected_diff` (the checked files' combined diff) feeding `generate_commit_message`,
which is async for a different reason than sync: no `spawn_blocking`, but core drives the
`claude` CLI through `tokio::process` and Ollama through async `reqwest`, so it carries the
same `async_runtime = "tokio"`. No `EventSink`, no streaming, no cancel — plain
request/response returning a pre-split `CommitMessage { title, description }` (mirrored 1:1,
as is `AiProviderConfig`). Assembling `AiProviderConfig` from the shared `config.toml` is
core's job (`ai::provider_config`), exported as `load_ai_config` and called by both clients
before every generate — the mapping used to exist twice, in Rust and in TypeScript, pinned
against each other by a test that could only assert one side. The composer's provider picker
persists through `patch_config { ai_provider }`, a patch that cannot touch any other
setting. Core's `check_provider_status` is what both composers probe, returning
`ProviderStatus { ready, reason, fix_command }` rather than a boolean: "installed" and "will
answer" are different questions, and asking only the first let an installed Claude CLI with an
dead session pass every check and fail every generate. The Claude probe therefore runs
`claude --version` *and* `claude auth status` — reading `loggedIn` out of the latter's JSON
payload, since a field that explicit would be pointless if the exit code carried the answer —
and the Ollama probe offers `ollama serve` as its fix only when the configured address is
loopback, because against a server the fix is on that server. **The probe alone is not
enough**, which is the mistake this replaced: signing out deletes the credentials, so a probe
sees it, but a session that *expires* leaves them on disk, so `claude auth status` still
reports a signed-in CLI and only a real request discovers the refresh failed. So
`provider_status_from_failure` reads a failed request for the same states and returns the
same shape, matching the CLI's wording in the one place that already interprets this CLI's
output — the two real messages (`Not logged in · Please run /login`, `Failed to authenticate:
OAuth session expired…`) are pinned by tests. It may only ever *raise* a block, never clear
one, since an unrecognized failure says nothing about the provider. Every probe failure is an
answer, not an error: only an unknown provider name is `Err`, and an answer core cannot parse
counts as ready, so a CLI changing its output format cannot lock a user out of Generate.

Both clients hold the result the same way: **only the blocked case is stored, tagged with the
provider it describes**, so `null` covers "ready" and "not asked yet" alike — the same thing to
a gate that refuses to lock anyone out on "not known" — and switching provider drops its
predecessor's block through a comparison rather than a clearing step someone can forget. The
answer is assigned only once it is in hand, never cleared on the way *into* a probe: since the
re-ask fires on regaining focus and asking Claude costs two process spawns, clearing first made
the remedy visibly blink out and back every time. A probe that *throws* changes nothing and is
logged — it is a wiring failure, not an answer, and must not clear a block a real failed
request proved; the log is what keeps a host predating the command ("no such command") from
reading as a probe that found nothing wrong. Both re-ask on provider change and on regaining
focus while blocked, since every remedy leaves the app: the native client runs that ahead of
the guards in `resyncOnActivate`, which exist to protect a running network operation and have
nothing to say about a probe. Because core spawns `claude` from `PATH`, the
Finder-launch environment matters for the first time: the Tauri host's hand-rolled PATH
repair moved into core as `process::fix_path_env` (spawn the login shell once, replace
`PATH`; the edition-2024 `unsafe set_var` contract — call before any other thread exists —
travels with it), the Tauri `main` now calls it from there, and the bridge exports it for
`LeoGitApp.init` to run as the process's first Rust call.

The repo switcher and background refresh cross as the picker/scheduler surface the Tauri
client already had: `load_config` / `patch_config` / `config_bounds` (full `Config`,
`ConfigPatch` and `ConfigBounds` mirrors — `theme` and `side_by_side_diff` are the recorded
native exemptions (FRONTEND.md §8) and simply go unpatched, which is what a patch makes
expressible), `load_state`/`patch_state`/`record_recent_repo` over
`ReposState`/`ReposStatePatch` mirrors — the shared `repos-state.json`, where
`last_opened_repo` is patched on every switch and restored at launch by *either* client,
and the MRU list feeds the tiered badge scheduler — `known_repos`/`effective_scan_paths`
(pure filesystem walk, no git subprocesses), and `repo_sync_status` returning a `RepoSync`
mirror, the per-repo dirty/behind/ahead badge summary. The last two are async-over-sync
exports: core's functions are synchronous, but `repo_sync_status`'s opted-in fetch can hold
its thread for the full 12 s background timeout and `known_repos` stats whole directory
trees, so both are wrapped in `tokio::task::spawn_blocking` under `async_runtime = "tokio"`
— the same worker-thread hop `#[tauri::command(async)]` performs implicitly — keeping
Swift's cooperative pool (one thread per core) unparked. On the Swift side, all refresh
timing is structured concurrency instead
of timers: `ContentView` owns `.task(id: repoPath)` loops — the status poll, the
config-driven auto-fetch, and `RepoDirectoryStore`'s tier scheduler (Tauri's cadence
number-for-number: 2/5/10 min over the MRU with 1.5/4/8 s launch kicks, sequential, active
repo excluded) — so a repo switch or close cancels and restarts them structurally, the
teardown the Tauri client does with `clearInterval` bookkeeping. Those loops start when
`repoPath` is *published*, which `open()` does before it has a status — so anything whose
decision depends on the repository's status (the warm-up fetch's no-remote gate) first
awaits `RepoStore.awaitLoadSettled()`, a main-actor continuation list released when the
last explicit load exits, success or failure. The claim is a *depth count*, not the
`isLoading` Bool the progress bar reads: `refresh()` can nest inside `open()` (a branch
action's `onWorkingTreeChanged`, a clone handing its path straight to `open`), and a Bool
would let the inner one's exit release everyone while the outer load still has no status.
Reading a `nil` status and guessing is how a gate silently stops applying whenever the
load happens to be slower than its caller. The poll's
`RepoStore.refreshQuietly` never touches `isLoading`, refetches history only when
`head_sha` moved, and bumps `workingTreeEpoch` (the diff-reload key; one meaning — "the
working tree may differ from what any derived view shows, re-derive if you care") when the
status value changed — plus unconditionally on app re-activation
(`NSApplication.didBecomeActiveNotification`, the native `resyncOnActive`). Content edits
count as a status change: porcelain v2 carries no worktree hash, so a same-row edit
(modified → still modified) would be invisible to the comparison and leave the open diff
stale until reselect — core's `FileEntry.stat_stamp` (an opaque mtime+size string,
`get_status`-only, `None` off-disk; pinned by `stat_stamp_sees_content_edits_and_absence`)
is what makes the comparison see them. The epoch is deliberately not narrower than that:
`RepoStore` signals possibility, and `DiffStore`'s equality skip is where reality is
checked, so a bump for an unchanged file costs one subprocess and zero repaints. A `ConnectivityBreaker` (Tauri's
numbers: 2 failures → 30 s backoff doubling to a 5 min cap) composes with
`Services/NetworkPathObserver.swift` — one `NWPathMonitor` publishing `isOnline`, the
analogue of the Tauri `navigator.onLine` half the breaker originally shipped without — as
`RepoDirectoryStore.shouldAttemptBackground` (`isOnline && breaker.shouldAttempt`, the
`connectivity.ts` `shouldAttemptBackground()` shape): every background fetch gates on it,
fed only by real attempts against real remotes, and while offline badge syncs degrade to
fetch-less local recomputes without burning failures into the breaker first. "Every"
includes the warm-up fetch on repo open, which is the same automatic fetch as any other:
gated on the breaker *and* on `status.hasRemote`, and reporting its outcome back — an
ungated one runs up to 15 s of a blocking-pool thread per repo open, and against a
remote-less repo (where `get_remote` answers `"origin"` regardless) it can only fail, which
would then hold every *other* repo's background sync closed. "Real attempts" is enforced by
`SyncStore.silentFetch` returning `Bool?`: `nil` when no fetch ran at all — the transfer
slot was taken, or `git remote` failed locally — so its four callers report only the two
outcomes that describe the network. A local failure counted as an unreachable remote is
the same poisoning, one layer up. The
offline→online edge fires the observer's `onRecover` kick (`ContentView.resyncOnReconnect`,
Tauri's `initConnectivity` recovery): `breaker.reset()` — a named method, because
`record(success: true)` where nothing was fetched would fabricate a success report — then a
silent fetch + quiet refresh of the active repo under `canAutoFetch` and the throttled
tier-0 sweep under `canRunRepoSweeps`, so recovery obeys the same predicates as the loops
it shortcuts. Whether each loop may run *right now* is
one type's answer: `Services/BackgroundSchedulingPolicy.swift` (@MainActor @Observable)
owns the inputs — the network-op slot (mirrored in by `SyncStore` from a `didSet` on
`activeOperation`), app activation (`NSApplication.didBecomeActive/didResignActive`), and
the repo window's occlusion (`NSWindow.didChangeOcclusionStateNotification` on the window a
zero-sized `NSViewRepresentable` accessor reports from `viewDidMoveToWindow` — deliberately
not `NSApp.keyWindow`, which is nil exactly while the app is inactive) — and exposes named
predicates each guard cites: `canPollStatus`/`canAutoFetch` (block only on the network op)
and `canRunRepoSweeps` (also requires active + visible; the deferrable multi-repo fan-out,
caught up by the wake-up resync). The active repo's work never stops: the status poll runs
a cadence ladder (2 s frontmost / 10 s visible-unfocused / 30 s hidden,
`statusPollInterval`, re-read per tick) and auto-fetch stretches its configured interval ×3
while the window is hidden (`autoFetchInterval(configured:)`) — fresher than pausing,
cheaper than GH Desktop's flat always-on interval. The Tauri client runs the same table
from `backgroundPolicy.ts` (see *Background cadence*), and both carry a once-per-session
0–30 s skew on the first automatic fetch so two windows over the same repositories don't
contend for `index.lock` on the same beat forever. The platform constraint that makes any
of this real: `Services/AppNapSuppressor.swift` holds a
`ProcessInfo.beginActivity(.background)` assertion exactly while (a repo is open) ∧ (some
background work is allowed), driven from the policy's input transitions — without it App
Nap coalesces an unfocused app's `Task.sleep` timers and the ladder silently never fires
(`.background` doesn't block idle system sleep, so the Mac still sleeps normally). Both
observer registrations use the classic block API with `MainActor.assumeIsolated` (the typed
`NotificationCenter` message types are macOS 27-beta; the app targets 26) and are removed
in `isolated deinit`s — a plain deinit is nonisolated under Swift 6 and cannot touch the
non-Sendable tokens.

The sync toolbar consuming all of this is one adaptive control (`SyncControls`), GitHub
Desktop's model: the precedence ladder — loading → detached → publish repository → publish
branch → pull → push → fetch — arrives as `RepoStatus.proposal` from core's
`sync_proposal`, and this view renders a plain button for the no-menu states and a split
button for the rest, whose chevron always offers Fetch and,
only while diverged, force push with lease (the ladder makes divergence reachable only in
the pull state, so the item lands exactly where GitHub Desktop puts it). Only the words
and the shapes are Swift's: an `extension SyncProposal` supplies `title` and
`isActionable`, and the local enum that used to re-derive the ladder from a `RepoStatus?`
is gone — `status?.proposal ?? .loading` is the whole derivation, with the `nil` case the
one thing the view still decides, because "no status yet" is a fact about its own load
rather than about the repository. The old toolbar
Refresh button is gone; its jobs are split between View ▸ Refresh (⌘R), which posts
`leogitRefreshRequested` for `ContentView` to perform the visible reload — a
scene-to-window notification, because commands live on the scene while the stores live
in the window — and the automatics: `refreshQuietly` now counts consecutive status failures and surfaces
the error banner after three, with a flag marking the message poll-owned so only the
poll's own recovery clears it (an explicit action's failure text is never swept away by a
background tick), and the branch list stays fresh because `BranchMenu`'s content reloads
on open (menu content is built when the menu opens) while the status poll reloads it whenever
`head_sha` moved — both through `BranchStore.load`, which never touches `isBusy`, so an
open menu doesn't flicker. The split button is the stock `Menu`+`primaryAction` control
with `.labelStyle(.titleAndIcon)` (macOS toolbars render labels icon-only by default), and
it deliberately carries **no count pill**: macOS bridges a toolbar menu's or button's
label to a system control that renders only its text and icon, silently dropping any other
view, no system API badges a macOS toolbar item (the 26 SDKs' toolbar `.badge` is
iOS-only), and a hand-built imitation of the control never matches the real one's chrome
or hover behavior — that route was tried and reverted. The pending counts render instead
as standalone `↑N ↓N` text in their own toolbar item declared just before (so rendered
just left of) the sync button, with `sharedBackgroundVisibility(.hidden)` removing the
item's glass capsule so it reads as status rather than a control; the spelled-out counts
ride its tooltip. Toolbar layout follows the macOS 26 grouping model: capsule sharing is
decided by `ToolbarSpacer` boundaries — adjacent items with no spacer between them form
one logical grouping drawn with a shared glass background — so the repo switcher and
branch menu are two plain adjacent `ToolbarItem`s at the leading edge, and a
`ToolbarSpacer(.flexible)` after them pushes the counts + sync cluster to the trailing
edge (necessary once the title is removed: no title area separates leading from trailing
otherwise). Neither `ControlGroup` in one item nor `ToolbarItemGroup` merges backgrounds
here, and the `.navigation` placement isolates each item in its own capsule — all three
were tried and rendered separate chips. Both controls carry `.labelStyle(.titleAndIcon)`
so repo name and branch name sit on their faces (toolbar labels render icon-only by
default), and the branch menu hides its indicator (`.menuIndicator(.hidden)`) so the
pair reads consistently — the repo chip's popover has no chevron either. The sync
control carries `.fontWeight(.regular)`: the trailing action region emphasizes its
label (bold text, heavier symbol) while the leading chips render regular, and the
control belongs to the same chip family. With the repo name on the chip
the toolbar title became duplication, so `.toolbar(removing: .title)` hides it
(`navigationTitle` still names the window for Mission Control and the Window menu), the
`navigationSubtitle` is gone entirely, and its exceptional states moved into
`BranchMenu.menuLabel` ("Detached at <sha7>", "<branch> · merging"): repo name, branch,
and counts each appear exactly once in the toolbar.

The ladder also reaches the menu bar. Its states live in `SyncProposal`, a pure type over
`RepoStatus` (ladder, title, actionability) that two views read: `SyncControls` renders it
and runs every state through one `perform()` — the button face and the split button's
primary action both call it, so no state is reachable by one and not the others — and the
repository screen republishes it as a `SyncCommand` (title, enabled, closure) through
`focusedSceneValue(\.syncCommand)`. Publishing from the window content is load-bearing: a
focused scene value set on a toolbar-hosted view never propagates to the scene (toolbar
items render in their own hosting hierarchy — tried first, and the menu item sat
permanently disabled with a nil value). `RepositoryCommands` reads it back with
`@FocusedValue` and renders Repository ▸ *action* under **⌘P**, renaming and disabling the
item with the button (Publish, Publish Branch, Pull, Push, Fetch); its closure posts
`leogitSyncActionRequested` back to `SyncControls`, whose sheet, alert, and busy guard
live with the button, so ⌘P runs the exact click path. The title is why this is a focused
value and not just a notification like ⌘R: a notification can fire an action but cannot
label it, and the menu item's title has to track repository state or it lies about what
⌘P does.

`RepoDirectoryStore.refreshDirectory` owns the row list both native pickers render, and is
deliberately not lazy: a `.task` on the repository screen primes it when that screen
appears, so the walk overlaps opening the repo instead of starting when the popover first
opens (which left the first open showing only the active repo until the scan landed, and
looking correct only on a second open). It is unkeyed — the walk spans every repo, so it
belongs to the screen's lifetime rather than a repo's, and the popover still re-runs it on
open so freshly cloned repos appear. One pass publishes twice: the shared MRU first (a
small JSON read, and the repos the user actually cycles between), then the walk's result,
with the previous walk's output retained so republishing can't momentarily drop discovered
rows. Concurrent callers (prime and popover, or the tier scheduler bootstrapping an empty
MRU) await the single in-flight `Task` rather than walking the tree twice, and the
persisted MRU is unioned with the local one on adoption, so a refresh racing `noteOpened`'s
not-yet-landed write cannot drop the repo that was just opened. `isRefreshing` and
`hasSearched` together decide what an empty list means: "no repositories found" is news
only once a pass has actually finished, and before the first one is even *started* —
which includes the whole of launch resolution, while the picker is already on screen —
the honest answer is that nothing has looked yet. (`isRefreshing` alone is false then,
so the screen greeted every launch by telling the user their scan paths were wrong.)
A walk that *fails*
sets `discoveryError`, which the list renders as one inline row with a Retry above whatever
the last successful pass found — the rows are still openable, so an error screen in their
place would take the repositories away along with the bad news. The same pass hydrates the
persisted sort mode, once per launch: the toggle's write is a background task, so
re-reading the file on a later walk could put the old value back over a choice the user had
just made.

**Discovery re-runs where the setting changes, not where a window closes.** A Settings
save whose patch moves `scan_paths` or `scan_depth` posts `leogitScanPathsChanged`, which
the root view answers with a walk — a notification because Settings is its own scene and
cannot reach `RepoDirectoryStore` directly, and on the *root* rather than the repository
screen because the picker is the surface that sends the user to that setting and must not
still be saying "No repositories found" when they come back from changing it. The
switcher's own re-walk on open covers the repository screen; the picker has no switcher to
re-open, which is exactly why the hook cannot hang off a dialog dismissal.

**The two native pickers are one view.** `RepoPickerList` is the Welcome screen's body and
the toolbar popover's content, parameterized only by what genuinely differs — how tall the
rows may run, whether a repository is already open, and whether a transfer is holding
switching back. Row *labels* come from `RepoIdentifierStore`, a process-lifetime cache of
each repository's `owner`/`name` parsed from its remote. The cache is three-valued on
purpose: a missing key is "not looked up", a stored `nil` is "looked up, no parseable
remote" (the row keeps its folder name and is never asked again), and collapsing those two
would re-spawn a `git config` per remote-less repository on every rebuild. Lookups drain
four at a time; the list asks for every row rather than the visible ones because the labels
are *searchable*, so a query has to reach a repository the user has never scrolled to, and
the pool is what keeps that affordable. `nil` stored through `updateValue`, not a subscript
assignment — assigning `nil` through the subscript of an optional-valued dictionary removes
the key, which would turn every "no remote" answer back into "not looked up" and re-queue
it forever.

The keyboard cursor is an index the *filter field* moves, via `onKeyPress` on the field
itself, rather than a `List(selection:)`: a list only moves a cursor when it is first
responder, and taking focus from the field would end the typing that produced the rows.
`ListNavigation.nextIndex` is the index arithmetic (wrapping, with a negative index meaning
"before the first row"), matching the Tauri client's `nextActiveIndex` so an arrow key
answers identically in both. Scrolling uses `ScrollViewProxy.scrollTo(_:anchor: nil)`, which
moves the least amount that reveals the row — the native form of `block: 'nearest'`, so an
already-visible cursor never jumps the list under a mouse user.

A transfer disables the *rows*, not the control that opens them: switching is what would
reset the sync UI out from under a running operation, while browsing and cloning claim no
network slot. In both clients the blocked rows are dimmed and inert rather than `disabled`,
because a disabled control takes no pointer events — the tooltip explaining why the row
can't be picked is the only reason it is still on screen rather than hidden.

**Launch resolution** (`ContentView.resolveLaunchRepo`, once per launch) takes
`last_opened_repo` first and does *not* wait on discovery: the two clients hand the working
repository to each other through the shared state file and `open` validates the path
itself, so queueing the common launch behind a filesystem crawl would only delay it. Only
when there is nothing to restore does the walk run — for the list Welcome shows, and for
the count behind the auto-open rule: exactly one discovered repository opens itself, since
there is no choice to present. That rule is confined to launch, so a later scan-path edit
that happens to narrow the list to one cannot pull the user out of the picker they are
standing in.

**Repo search** is one rule in one place: `core::repos::match_repo`, with the batch
`filter_repos` both hosts actually call. It replaces two hand-written implementations that
had drifted — on the set of labels they searched (one matched a single basename while the
rows it drew were owner-qualified, so typing what was on screen found nothing), on whether
the path prefix comparison was case- and separator-normalized (it wasn't natively, which
silently made the whole absolute path searchable), and on which roots counted. The batch
form is deliberate: one crossing per keystroke rather than one per row is what makes a
shared rule affordable for a list that re-filters as the user types. The rule replaces the
original one — the query as a subsequence of the name, the `owner/name` label, **or the
full path** — whose last clause is fatal once every row shares an ancestry: under
`/Users/leo/Dev/LeoManrique/Desktop`, `llm` matched all fourteen repositories, satisfied by
the `l` of `leo`, the `l` of `LeoManrique`, and that word's `m` before any repo name was
consulted, so the field appeared not to filter at all. The fix separates the two halves of a
path by how much signal each carries: a name keeps the scattered-subsequence match a fuzzy
finder is expected to have, while the path must contain the query *contiguously* and is
first trimmed to what lies below the deepest root containing it — a scan folder, or the home
directory — since everything above that is common to every row. `match_repo` returns the strongest of six ordered cases rather
than a `Bool` — exact name, name prefix, name substring, name initials (`gpm` →
`git-projects-manager`), name subsequence, path substring — and every caller sorts on that
before anything else, falling back to its own order (active/MRU/alphabetical natively, the
persisted recent/name sort in Tauri, discovery order in the picker) only within a tier. The
ranking is load-bearing rather than polish: both clients act on the first row — Return
natively, the keyboard cursor in Tauri — and the old orders put the already-open repository
there, so Enter re-opened what was already on screen instead of the match. The two
implementations were diffed against each other over the real repo list before shipping,
identical across twenty queries including case, whitespace, and the ancestry queries
(`users`, `desktop`) that must now match nothing; `llm` went 15 → 1 and `leo` 15 → 5 with
the three `leo*` repos leading. They differ in exactly one documented place: when a repo
lies outside every scan folder the Swift side falls back to trimming the home directory,
which the frontend can't resolve, so it searches the whole path.

Feeding the Tauri half needed the expanded scan folders in the frontend, which only
`App.svelte` had — fetched twice, and only on the paths that reach the picker.
`stores/config.ts` now owns them: `refreshConfig` resolves `effective_scan_paths` alongside
the config it derives from, so both pickers and the picker's "Searched these folders" empty
state read one store that re-resolves whenever settings change.

`Design/PathText.swift` is a straight port of the Tauri client's `PathText.svelte`, algorithm
included: `PathTruncation` restates the rule (the directory collapses to a trailing `…/`
bridge, never below a first-letter hint, and the filename middle-truncates only when even that
won't fit), and its output was diffed against the original's across every path/budget pair
before shipping. SwiftUI's own `.truncationMode` can't express it — it has no idea which half
of a path carries the file's identity — so the view binary-searches the largest budget whose
rendered width fits, measuring with the very `NSFont` it draws with, mirroring the hidden
measuring span the Svelte component uses. It is greedy horizontally (the component's
`flex: 1 1 0`), so its width never depends on its own text and the measurement cannot feed
back into layout; the fit is recomputed from `onGeometryChange`, which only fires when the
width actually changes.

The repository screen is one `HSplitView` hosted by `ContentView` *above* the tabs — the
Tauri two-column grid: the sidebar column (`RepoTabBar`, then `ChangesSidebar` or
`HistorySidebar`, framed 280/320/640 like the Tauri sidebar) beside the main-content column
(`ChangesDetailPane` or `HistoryDetailPane` over `TerminalDock`, min 380). Each column is a
stable `VStack` whose *content* switches on the tab, so the split — and its divider position
— is never rebuilt by a tab change or an empty list. The previous shape, a per-tab
`HSplitView` swapped out wholesale and replaced by a full-width empty state on a clean tree,
is what made the composer vanish and the terminal span the window. Each tab's selection
(`selectedPath`, `selectedSha`) is `@State` in `ContentView` because the list and its detail
now sit on opposite sides of the split; the sidebars re-seed it on list change and the detail
panes only read it. The composer's height is `@AppStorage("commitComposerHeight")` in
`ChangesSidebar` (220 pt default, 180–600 — Tauri's `commitHeight` bounds), applied as a
fixed `.frame(height:)` with the description `TextEditor` on `maxHeight: .infinity`, so extra
height becomes description; `Design/RowResizeHandle.swift` turns a drag on the padded
divider above it into that value (start − translation, clamped, `.pointerStyle(.rowResize)`).
Both the drag range and the rendered height are capped at the pane's measured height minus an
80 pt list floor (`onGeometryChange`), so a tall stored height can't overflow a short window
and grows back when the window does — and capping the *drag* keeps the stored value within
reach, so a drag back down moves at once instead of first spending an invisible surplus.
`Design/EmptyListPlaceholder.swift` is the sidebars' faint centred line; the pane-sized
`ContentUnavailableView`s live in the detail panes, each claiming its slot so the dock stays
pinned.

The commit composer's two fields carry the Tauri pair's behaviors.
`Design/WheelScrollableTextField.swift` is the summary input: SwiftUI's `TextField` (an
`NSTextField` underneath) moves overflowing text only with the caret, so — as the Tauri
client maps wheel deltas onto the input's `scrollLeft` — the wrapped field forwards
`scrollWheel` to the field editor's clip view (dominant axis; the event passes to ancestors
when nothing overflows) and overrides `intrinsicContentSize` so a long summary cannot demand
width from the split. The description is a `TextEditor` — the only text control that is a
real scroll view, so it grows a scrollbar once the text outgrows whatever height the
sidebar's resize handle leaves it — wearing a
hand-drawn bezel and prompt, which it lacks natively. The single-file auto-summary lives in
`CommitStore.autoSummary(for:)` and is used twice: as the summary field's prompt, and as
`commit`'s fallback message when nothing was typed — recomputed from the embedded-repo
confirmation's file snapshot, not the live list, so the message describes what the commit
actually contains.

The History detail crosses as two reads: `get_commit_detail` on selecting a commit —
metadata never loads, it rides in the `CommitInfo` the list already holds — and per selected
file `get_parsed_commit_diff`, which feeds the *same* pipeline as the working tree. That reuse is structural on the Swift side: `DiffStore`/`DiffView` take a
`DiffTarget` (`.workingTree(epoch:)` or `.commit(sha:)`) that picks both the raw-diff read and
the tokenizer's `BlobSource` in one value, so the two can never disagree, and the commit case
reads blobs at the commit's own trees — a file later rewritten still colours as it was then.
`CommitDetailStore` gets the file list and the +/− totals from that one read, so the header
and the list can never describe different commits, guarded by the same generation counter as
`DiffStore` against superseded loads. The changed-file
rows themselves are `Design/ChangedFileList.swift`, extracted from the Changes tab so both tabs
share one row implementation (the Tauri `FileList.svelte` arrangement) — the Changes tab injects
its checkbox through a `@ViewBuilder` leading slot, History injects nothing. All three core
functions use `--first-parent`, so a merge commit shows its first-parent changes instead of
`diff-tree`'s empty output, and `--root` (without it, a `log.showRoot=false` user got a
populated file list whose every diff was empty on the repository's first commit — pinned by
`commit_diff_covers_the_root_commit_regardless_of_show_root`).
The history list itself pages: `get_log`'s `skip` appends a page whenever the last row
materialises, and quiet refreshes re-fetch at the scrolled depth capped at 500 (both clients'
`MAX_COMMITS` / `historyRefreshCap`), with appends de-duplicated by sha against a poll's
concurrent reload. Both clients now run that same append-only model — see the Tauri section
below for what it replaced and why.

**Row context menus** are the per-item actions both lists hang off a right-click. Their seven
core functions had shipped in the Tauri host for months without a bridge export — `discard_files`,
`ignore_paths`, `append_to_gitignore`, `reveal_path`, `open_path`, `checkout_commit`,
`undo_last_commit` — and now cross as plain sync exports; none needs a new type mirror, and none
belongs in the `spawn_blocking` category (`repo_sync_status`, `known_repos`), since each is a
short local operation and the Swift wrapper's `@concurrent` hop already keeps it off the main
actor. The menus themselves use `contextMenu(forSelectionType:)` on the *list* rather than one
`.contextMenu` per row: it makes the right-clicked row the selection before building the menu, so
the menu and the diff pane always describe the same item — the re-select the Tauri `FileList`
performs by hand — and returning nothing from its builder deactivates it, which is how the History
detail's file list keeps its rows menu-less while sharing `ChangedFileList` with the Changes tab.
Whether a menu exists at all is a stored flag rather than an empty builder, so the modifier is
never attached where there is nothing to show.

Amend mode lives in `CommitStore` (`amendTarget` plus the commit's `co_authors`, which the
composer has no field for and simply re-attaches to the next message). Two behaviours are
load-bearing: re-entering amend on the same sha is a no-op, so right-clicking Amend twice can't
wipe edits in progress, and leaving it clears the seeded draft so an amended message can't be
re-submitted as a new commit. `commit` then relaxes its empty-file guard, because
`git commit --amend` with nothing staged *is* the message-only edit. This forced one structural
fix: `CommitStore` moved from the Changes pane (now `ChangesSidebar`) to `ContentView`, since the tab bar swaps panes by
rebuilding them — which discarded any in-progress message on every tab switch, and would have
made amend impossible, as amend is started from the History tab and finished in the Changes tab.
Which commit is `HEAD` comes from `status.head_sha`, not from the row's index, in both clients —
an index into a loaded page is only HEAD for as long as nothing moves the page's top. Undo's own
gate is shared too: offered only when the commit is provably unpushed, or when no upstream
resolved at all and nothing can prove it was pushed either.

The embedded terminal crosses as the bridge's second foreign callback trait:
`TerminalEventListener { on_output(pid, data), on_closed(pid, exit) }` — `exit` is a
`TerminalExit { exit_code, signal }` mirror of the reaped child's status — adapted to core's
`EventSink` by a private `TerminalSink` — its own trait rather than variants bolted onto
`SyncProgressListener`, because the two event shapes share nothing and each sink is scoped
to what its operation can emit. Everything is synchronous (core's PTY sessions run on plain
OS threads; no tokio): `start_terminal` — whose session reader thread holds the listener
until the child exits — plus `write_terminal`, `resize_terminal`, `close_terminal` and a
`StartedTerminal` mirror. `terminal_pty_info` (Windows-ConPTY metadata xterm.js needs
before construction; all-`None` on macOS) stays unexported. On the
Swift side, SwiftTerm renders the emulator (an SPM package pinned in `project.yml`, since
the generated project's `Package.resolved` is gitignored). A `TerminalController` bridges
one `TerminalView` to one session: keystrokes and grid sizes go out on a serial I/O queue —
ordering the blocking writes and keeping them off the main actor — and output comes back
through a coalescing relay: chunks arrive once per 4 KiB PTY read on the Rust reader
thread, append to a locked buffer, and at most one main-actor drain is ever in flight, so
floods batch into a single `feed` — the port plan's byte-stream-throughput question,
answered. Session lifecycle rides structural identity: `TerminalSessionView` mounts under
`.id("repoPath:generation")` while `TerminalStore.generation > 0`, so New Session and repo
switches unmount the old view (whose `onDisappear` kills the PTY) — the native `{#key}`
remount — and a shell exit nulls the pid *before* anything else, so unmount never
double-closes a session core already dropped; a *clean* exit then collapses the dock,
while a non-zero code or fatal signal instead prints `[Process exited with code N]` in
red and keeps the dead terminal on screen for reading (✕ and ＋ still work — the pid is
nil, so their teardown is a no-op against core). Collapsing protects the *shell's* view of the terminal but not the drawn one: the dock
applies a zero **height** at full width, so SwiftTerm still lays out and reflows its grid to
that degenerate row count, and only the PTY resize is refused (`cols >= 2, rows >= 2` in
`TerminalController.resize`) — the child keeps its 80×24-or-whatever geometry, but
re-expanding does not restore the exact prompt. The Tauri client's collapse is
`display: none` on the terminal container, which genuinely changes nothing: a hidden element
reports no box, the debounced `fit()` is a no-op, and re-expanding paints what was there.
Terminal focus (SwiftTerm's view as first responder) suppresses auto-fetch exactly like the
field-editor check. Both docks toggle on **⌃`** — VS Code's binding, and deliberately not ⌘`,
which macOS owns for cycling an app's windows — and both give the *emulator* 280 pt with the
header strip on top of that, so the same setting hands the shell the same number of rows in
either client. Focus is a *request*, not a call:
the dock asks as the panel opens, which on the first expand is before AppKit has attached the
emulator to a window, and `makeFirstResponder` on a windowless view is a silent no-op — so
`TerminalController` holds the request and replays it from the host view's
`viewDidMoveToWindow`, then makes it first responder one main-actor hop later, after SwiftUI
has settled its own responder for the pass. Collapsing releases it (the window becomes first
responder again), because a collapsed panel is zero-height but still mounted and would
otherwise keep typing out of sight.

The dock's header strip is a stock **accessory bar**: `.buttonStyle(.accessoryBar)` over the
whole row, which AppKit draws as `NSBezelStyleAccessoryBar` — the renamed *recessed* bezel
behind every scope bar in the system — so hover, press, and on-states come from the control
class instead of being painted here. The strip has to be hand-assembled because
`ToolbarItemPlacement.bottomBar` is `@available(macOS, unavailable)` in the macOS 26 SDK
(`.status` is the top toolbar's centre, and `.accessoryBar(id:)` sits *below the title bar* —
both the wrong end of the window); the accessory-bar *style*, however, is reusable in plain
window content, which is what makes the strip native without imitation. The shell label is a
`Toggle(isOn:).toggleStyle(.button)` over a binding whose setter calls `store.toggle()`
rather than assigning, so the lazy first PTY spawn still happens on the way up, and the
right-hand buttons are `Label`s under `.labelStyle(.iconOnly)` — icon-only on screen, titled
for VoiceOver. Deliberately *no* `glassEffect`: the HIG restricts Liquid Glass to the
navigation layer, and this bar lives in content, so it keeps the `.bar` material. The dock
is the last child of the main-content column's `VStack` — under the diff, beside the sidebar,
and outside the tab switch so the shell survives a tab change — rather than a
`safeAreaInset`, since nothing should scroll under an opaque 280 pt panel.

Cloning and the Settings window crossed together. `clone_repo` is exported like `pull`
(async over tokio, progress through the same `SyncProgressListener` seam — `ProgressSink`
needed no change, since core aggregates clone's phase weights before emitting) and the gh
pair joins it: `gh_repo_list` (a `GhRepo` mirror; async over `spawn_blocking` like
`repo_sync_status`, since the 20 s `gh` query must not park a cooperative thread) and
`gh_clone` (no listener — `gh repo clone` reports nothing parseable, so the sheet shows an
indeterminate bar; the Tauri dialog shows no bar at all in that case, only its `Cloning…`
button state, because its bar is gated on a `git-progress` event a gh clone never emits). The destination contract is core's
`prepare_clone_target`, shared by both paths: the caller passes the *full* target path,
core expands `~`, refuses an existing path, and creates the parent. What the folder is
*called* is `core::repos::derive_clone_target` / `clone_target_path`, which both dialogs
call, so the destination preview and the Clone button's enable condition are the same
answer and cannot disagree about whether the app is about to succeed. The sheet (reachable
from either picker's footer) seeds its destination from the shared `last_clone_dir` → first
scan path → `~/Dev`, persists the parent folder back on success, shares the GitHub tab's
`clone_sort_mode` with the Tauri dialog through `repos-state.json`, and disables every exit
while cloning — there is no cancel; a dismissed sheet would orphan the clone. Success hands
the fresh path to `RepoStore.open`, so the normal `.task(id:)` chain records it as recent
and runs the warm-up fetch.

`CloneStore` is owned by `ContentView`, not by the sheet: the GitHub list is a
once-per-run cache, and a store created with each presentation re-ran the 20 s `gh` query
every time it opened. `reopen()` is therefore what each presentation resets — the URL,
the filter, the selection and the error — while the tab, the list and the sort mode
survive, because which tab you clone from is a preference and re-fetching an unchanged list
is exactly what the cache exists to avoid. A repository created since launch is reached by
the Refresh button beside the filter; `hasLoadedList` is set only on success, so a failed
load retries on the next open rather than reopening onto a stale error with no list behind
it. `visibleRepos` is recomputed when the filter, the sort mode or the list changes rather
than per layout pass, and it filters *before* it sorts — the query is what shrinks the
list, so ordering rows it is about to discard is work thrown away, 200 rows at a time, per
keystroke. Both name orders go through `NameCollation`, which is case- **and**
diacritic-blind (the Tauri lists' `sensitivity: 'base'`), with an explicit tiebreak after
it: Swift's sort is not stable, so two same-second pushes would otherwise swap places
between passes. The
Settings scene (`Settings { }` in `LeoGitApp`, which is what binds ⌘, and the app-menu
item) exposes only fields with native consumers — auto-fetch cadence, the Diff section
(hide whitespace, syntax highlighting, tab size 1–16), scan paths/depth, the terminal
shell (via the newly exported `list_shells` + `ShellOption` mirror, with a
stored-but-uninstalled id rendering as Automatic), and the AI knobs. Exactly two fields
cross untouched as documented exemptions (FRONTEND.md §8): `theme` permanently — the
native app follows the system appearance — and `side_by_side_diff` until the split
layout gets its own design pass (ROADMAP) — both simply go unnamed by the patch, which is
what a patch makes expressible. `SettingsStore.save()` sends a `ConfigPatch` of the fields
this window owns; the load-fresh-then-overlay discipline it used to hand-roll now lives
inside core, under a lock. Discrete controls save through a 300 ms debounce; text fields
commit on focus-loss/Return, and travel blank (core reads blank as absent). Closing the
window is neither a focus-loss nor a Return, so `flushPendingSave()` handles both ways an
edit can be pending: it fires a debounce still counting down, and — for a field the user
typed into and never left — compares the current patch against `lastPersisted`, writing
only if they differ. Without the second case ⌘W silently dropped the typed value, in the one
surface whose premise is that you never press Save. A debounce that runs to completion also
clears `pendingSave` (guarded by a generation counter so it can't clear a newer one), or the
first toggle of a session would leave it set and make every subsequent close save
unconditionally. The numeric controls' ranges come from `config_bounds()`, so the form
cannot offer a value the writer clamps away. Config
consumption has one native owner: `Stores/AppConfigStore.swift` (@MainActor @Observable,
created in `LeoGitApp` and put in the environment of both the main window and the
Settings scene) holds the shared `Config` and reloads it at exactly three sites — launch,
every successful Settings save (`SettingsStore` calls `reload()` after `patch_config`
lands, which is how an edit reaches the open diff and the auto-fetch loop live), and the
activation resync (edits made from the Tauri client arrive on return to the app). The
auto-fetch loop reads the store each tick, so interval and toggle changes apply within
one interval — the live re-arm the
Tauri client still lacks (its ROADMAP entry) — and `DiffView`'s `LoadKey` reads it so the
diff toggles re-key the open diff the moment a save lands.

Publish closed the gh surface. `gh_publish_repo` crossed like `gh_clone` (already
core-async over the blocking pool; no listener — `gh repo create` streams nothing parseable,
so the sheet and the toolbar banner stay indeterminate), and `SyncStore` gained a `.publish`
case in the same single `activeOperation` slot as push/pull — Tauri's `'publish'` network op
— so every background loop pauses for its duration. `check_auth` remains unexported: every gh
call's own error text already distinguishes "gh missing" from "not authenticated".
There is deliberately no pull-request surface in either client — PRs are a GitHub
feature, not a git one, and the web UI serves them.

`scripts/build-rust.sh` builds the static lib and regenerates the bindings, and Xcode runs it as
a pre-build phase — so the Swift API can never be stale relative to the Rust it calls.
`ffi/generated/` is gitignored for the same reason. Three things there are load-bearing and
guarded by assertions in that script:

| Concern | Why it bites |
|---|---|
| `module_name` (`uniffi.toml`) ↔ `--module-name` ↔ `SWIFT_INCLUDE_PATHS` | The bindings import the C shim as `#if canImport(LeoGitCoreFFI)`. A mismatch does not error — the guard just goes false and every FFI symbol fails to resolve. |
| `--link-frameworks SystemConfiguration` | Core's `reqwest` → `hyper-util` chain reads system proxy settings. Declaring the framework in the modulemap keeps the requirement with the library instead of in each consumer's build settings. |
| `SWIFT_DEFAULT_ACTOR_ISOLATION: nonisolated` | Xcode 26 defaults new app targets to `MainActor`, which **breaks** the generated bindings (raw pointers, `deinit`, sync C interop cannot be main-actor isolated — mozilla/uniffi-rs#2818). UI types opt into `@MainActor` explicitly instead. |

## Process model

There are two processes at runtime:

1. **Tauri host (Rust)** — owns the window, runs the invoke handler, owns the PTY session pool, shells out to `git` / `gh` / `claude`.
2. **WebView (Svelte)** — renders the UI, dispatches commands via `invoke(...)`, subscribes to terminal output via Tauri events.

All work flows through Tauri's IPC. There are no HTTP servers, no sidecars, and no Node runtime in production.

### Startup PATH fix

`main.rs::fix_path_env` runs once before the Tauri builder. On macOS/Linux it spawns `$SHELL -ilc 'echo -n "$PATH"'` and replaces the process PATH with the result. Without this, apps launched from Finder or a `.desktop` entry inherit a minimal PATH (e.g. `/usr/bin:/bin:/usr/sbin:/sbin`) and miss user-installed tools like `claude`, `gh`, or Homebrew binaries. No-op on Windows.

### Path normalisation

[core/src/paths.rs](core/src/paths.rs) owns the form every path takes once it's inside the app, and **`std::fs::canonicalize` must not be called anywhere else**. On Windows it always answers with a *verbatim* (extended-length) path — `\\?\C:\Users\Leo\Dev\leogit` — which names the right folder but leaks everywhere: the repo tooltip and the picker's empty state rendered the prefix literally, and PowerShell, unable to map a verbatim path onto a `PSDrive`, dropped to a provider-qualified prompt (`PS Microsoft.PowerShell.Core\FileSystem::\\?\C:\…`) that also breaks any script doing string work on `$PWD`.

- `paths::canonicalize` replaces `fs::canonicalize` at every site: `discover_repos` (scan roots and each hit), `repo_root`, `init_repo`, `resolve_launch_target`. Backed by [`dunce`](https://lib.rs/crates/dunce), which strips the prefix only when the legacy namespace can express the same path — never over `MAX_PATH`, never for a reserved DOS name (`CON`, `COM1`), never for a network share — so nothing that genuinely needs the prefix loses it.
- `paths::simplify_str` does the same conversion without touching the filesystem, for paths that arrive already-absolute from elsewhere: `config::normalize_repo_paths` (see below) and `start_terminal`'s cwd, the boundary where a third-party shell reads the path.
- **macOS and Linux are untouched by construction**, not by a platform branch of ours: `dunce`'s strip check is a `const fn` returning `false` off Windows and its `canonicalize` is a re-export of `std::fs::canonicalize` there.
- `paths::expand_tilde` turns a leading `~` into the home directory, and takes that directory from the **OS** (`directories::BaseDirs`, already a dependency) rather than from `$HOME`. That variable is not part of the environment Windows hands a program — Explorer and the Start menu don't set it, only POSIX-flavoured shells (Git Bash, MSYS) do — so reading it directly made `~` resolve or fail *depending on how the app was launched*: scan paths worked under `tauri dev` started from such a shell and matched nothing in an installed build, where discovery skipped every root it couldn't resolve and the picker just said "no repositories found". `BaseDirs` reads `FOLDERID_Profile` on Windows, which no environment can misreport, and `$HOME` on macOS and Linux exactly as before (falling back to the passwd entry), so the fix is Windows-only in effect without a `cfg`. Expansion also accepts either separator and emits the platform's own, so `~\Dev` works and the result never comes out half `\` and half `/` — a form that compares unequal to the same folder discovered any other way.
- The frontend has the matching rule in [lib/utils/path.ts](apps/tauri-app/src/lib/utils/path.ts): `basename` splits on **either** separator. Four components had their own `/`-only copy, so on Windows the whole path counted as one segment and a repo without a GitHub remote to name it was labelled `C:\Users\Leo\Dev\ryubing\Ryubing`. Deliberately not used for git's own paths (`PathText`, `fileActions`) — git reports forward slashes on every platform, and a separator-agnostic split there would cut a filename that legitimately contains a backslash on Linux or macOS.

### Command-line repo opening

The `leogit [dir]` shell command (installed by `install.sh`, see *Release pipeline*) opens a repo straight from a terminal. All the app-side logic lives in [core/src/launch.rs](core/src/launch.rs):

- **Argv → `LaunchTarget`.** `resolve_launch_target` takes the first non-flag argument, resolves it against the cwd, and **canonicalizes** it (so it de-dupes against the canonical paths from `discover_repos`). It returns `{ path, is_repo }` rather than an optional repo path: an existing directory always produces a target, and `is_repo` decides whether the frontend opens it or offers to create a repository there. `is_repo` comes from `git::repo_root`, which shells out to `rev-parse --show-toplevel` so a **subdirectory resolves to its repo root** (`leogit src/` opens the repo, and can't be mistaken for a fresh folder), falling back to the `.git` probe when the toplevel is unreadable so an existing repo is never offered a nested `git init`. Only a missing path, a non-directory, or a bare `leogit` resolves to `None` — those just launch/focus the window.
- **Cold start** (app not running): `main.rs` calls `resolve_launch_target` *before* the builder and stashes the result in a process-global via `set_pending_launch_target`. The frontend claims it once on mount through the `take_pending_launch_target` command (`appApi.takePendingLaunchTarget`), which clears it so a reload won't re-open. In `App.svelte` a repo target wins over the remembered `last_opened_repo` and is added to the repo list even if it lives outside the scan paths; a non-repo target raises the init prompt and lets normal resolution continue behind it.
- **Warm start** (app already running): `tauri-plugin-single-instance` — registered **first** in `main.rs`, as the plugin requires — detects the second launch, hands its argv/cwd to `handle_second_instance`, which focuses the window and emits an `open-repo` event carrying the `LaunchTarget`. The plugin keys on the app identifier via a `/tmp/<identifier>_si.sock` Unix socket (the second process connects, forwards, and `exit(0)`s). The frontend splits the event three ways: a **non-repo** target always goes to `App.svelte` (the prompt isn't scoped to the open repo, so `MainLayout` ignores those); a **repo** target goes to `MainLayout` in phase `main` (reusing `handleSwitchRepo` via `openExternalRepo`) and to `App.svelte` in the pre-`main` phases. The repo listeners stay mutually exclusive — `App` ignores repo targets while phase is `main`, and `MainLayout` is only mounted then.
- **Initialising a non-repo folder.** `App.svelte` owns the prompt ([InitRepoConfirm.svelte](apps/tauri-app/src/lib/components/InitRepoConfirm.svelte)) in every phase, so it can render over the picker, over another open repo, or at first launch. Confirming calls `git::init_repo`, which creates the folder if needed, runs `git init` (naming the branch `main` unless the user configured `init.defaultBranch`), and returns the path to open. It is **idempotent** — a folder already inside a repo returns that repo's root instead of nesting a new one — so a double-confirm, or confirming after the user ran `git init` themselves, opens the repo rather than failing. `App` then routes the result: `MainLayout.openExternalRepo` (bound via `bind:this`) when one is mounted, since only it can reset the open repo's view state; otherwise it moves the app into `main` itself. Unborn HEAD is already handled downstream, so the fresh repo renders immediately.

### Repo-less phases

`loading`, `repo-picker` and `error` render `<Header>` above the phase content. `Header` derives `hasRepo` from `$appState.repoPath` rather than taking a prop — `repoPath` is the single source of truth and a separate flag could disagree with it — and hides the repo chip, branch chip, status area and the sync button when it is false. Settings, Help and the update chip remain, so they are reachable in every phase. `onOpenRepos`/`onOpenBranches` are therefore optional props: only the hidden chips call them. Header's effects already no-op without a repo (each guards on `repoPath`), so mounting it costs nothing.

Two consequences worth knowing:

- `RepoPicker`'s overlay is `position: absolute; inset: 0` inside `.pre-main-body`, **not** `position: fixed`. A viewport-fixed overlay would sit on top of the header and swallow the only controls that can rescue an empty picker.
- `App.svelte` binds `Escape` / `Ctrl+,` / `?` only while the phase isn't `main`; `MainLayout` owns those keys otherwise, and binding in both would double-handle them. `?` is ignored while a text field has focus, since the picker autofocuses its search box. Only the *binding* is split — both handlers pass `Escape` to the one overlay stack, which is why a dialog that can appear in every phase (the `leogit <dir>` init prompt) needs no rule about which host is listening.

`effective_scan_paths` (git.rs) reports the folders discovery would walk, so the empty state can list them. It and `discover_repos` both route through `resolve_scan_paths`, the sole owner of the "empty config → stock defaults" rule, so the folders shown can't drift from the folders searched — pinned by `effective_scan_paths_matches_the_resolution_discovery_uses`.

### Windows console suppression

Release builds set `windows_subsystem = "windows"` (in `main.rs`), so the app runs with no attached console. On Windows a console-less process that spawns a console subprocess gets a **new console window allocated and briefly flashed** for each call — and because the UI polls `git status` every 2s, that would mean a `cmd` box flickering on screen continuously, plus one on every fetch/commit/diff. Every subprocess spawn therefore routes through [core/src/process.rs](core/src/process.rs): `hide_console` (std `Command`) and `hide_console_async` (tokio `Command`) set the `CREATE_NO_WINDOW` creation flag; both are no-ops off Windows. Call sites: `git_cmd` / `git_net_cmd` (git.rs), `apply_patch` (diff.rs), `check_auth` / `gh_repo_list` / `gh_clone` / `gh_publish_repo` (gh.rs), and both `claude` spawns (ai.rs). The PTY shell in terminal.rs is intentionally exempt — ConPTY is a pseudo-terminal, not a console subprocess, so it never flashes a window.

### Network resilience (offline / flaky)

Every remote-touching command is engineered so an unreachable or flaky network degrades a badge — it never freezes the app. Three layers:

1. **Off the main thread.** Every command that spawns a subprocess or touches the filesystem — the whole of `git.rs` (except the pure `format_commit_message`), all of `diff.rs`, and the four `gh` commands — is declared `#[tauri::command(async)]`. A plain synchronous Tauri command runs inline on the **main thread**: a blocking `git` spawn there freezes the window, and the failure mode is sneaky — commands that are normally instant (`get_status`, `rev-parse`) turn slow exactly when a big push/pull saturates the repo's disk, so a synchronous status poll would stall the UI thread every tick for the whole transfer. `(async)` runs them on tokio worker threads instead. One refinement on top: a `(async)` sync fn still pins one of the ~num-cpus *core* workers for its whole duration, so the commands that can legitimately run for minutes (`fetch`, `pull`, `push`, `clone_repo`, `delete_remote_branch`, `gh_publish_repo`, `gh_clone`) are `async fn`s delegating to `process::run_blocking` (tokio's dedicated blocking pool) — a 10-minute push can never starve the worker pool on a low-core machine.
2. **Time-boxed subprocesses.** `process::run_timed(cmd, label, timeout)` is the single chokepoint: it spawns the child, drains both pipes on helper threads (so a chatty `git --progress` can't pipe-buffer-deadlock), and **kills the child** if it outlives `timeout`, returning a `… timed out …` error. `run_timed_streaming` is the same runner with an incremental stderr reader — each `\r`/`\n`-terminated line is handed to a callback as it arrives (git repaints its meter with bare `\r`), which is how live `--progress` output reaches the UI. `git_net_cmd` additionally bakes transport timeouts into the command — `GIT_SSH_COMMAND="ssh -o ConnectTimeout=N -o BatchMode=yes"` (SSH connect cap + no interactive prompts) and `-c http.lowSpeedLimit=1000 -c http.lowSpeedTime=N` (abort an HTTP transfer that stalls). Budgets: **background** badge fetches are short (8s connect/stall, 12s hard kill — fail fast, keep last-known counts); **user-initiated** transfers are generous (15/30s, 600s hard kill — never kill a real large transfer, only a wedged one). Unit-tested in `process::tests` (`run_timed_kills_a_hung_child_promptly`, `run_timed_streaming_splits_stderr_on_cr_and_lf`).
3. **Don't keep firing when down.** [services/connectivity.ts](apps/tauri-app/src/lib/services/connectivity.ts) gates *automatic/background* fetches (the auto-fetch timer, the tiered scheduler, the refocus/cold-open resync) on `navigator.onLine` plus a consecutive-failure circuit breaker: after 2 failures it opens with an exponential backoff window (30s → 5min cap), suppressing background fetches until the window lapses, when exactly one probe is allowed through. `repo_sync_status` returns a `fetched` flag so the breaker can tell a real fetch failure from a no-remote repo. User-initiated actions (fetch/pull/push/switch) always attempt (still bounded by the backend timeout) and their outcome feeds the breaker, so a successful manual pull — or the OS `online` event — re-opens background syncing immediately and triggers a resync.
4. **One transfer at a time.** The [stores/networkOps.ts](apps/tauri-app/src/lib/stores/networkOps.ts) `activeNetworkOp` store marks a user fetch/push/pull/publish as in flight — the *automatic* fetches deliberately claim no slot, since nobody waits on them and taking it would disable the action cluster on a timer. All handlers guard on it (mutual exclusion), and the status poll, auto-fetch, wake-up resync, and the tiered scheduler pause while it's set — polling mid-transfer only spawns git processes that contend with the transfer for the repo's disk, locks, and bandwidth; the op's own completion refresh covers what they would have found.

### Transfer progress (`git-progress` events)

`push`, `pull`, and `clone_repo` run with `--progress` through `run_git_net_streaming`, feeding each stderr line to [core/src/progress.rs](core/src/progress.rs) — a port of GitHub Desktop's step/weight model. Each op has an ordered phase table (push: `Compressing objects` 0.2 / `Writing objects` 0.7 / `remote: Resolving deltas` 0.1; pull: `remote: Compressing objects` 0.1 / `Receiving objects` 0.7 / `Resolving deltas` 0.15 / checkout 0.15; clone: 0.1 / 0.6 / 0.1 / 0.2 — the checkout phase matches both `Updating files` (git ≥2.25) and the older `Checking out files`, a divergence from GitHub Desktop's table, which only knows the legacy label and tops out early on modern git). A line matching phase *i* contributes the full weight of every earlier phase plus `weight × value/total`; unknown lines (e.g. `Enumerating objects`) are *context* — they update the display text but not the bar; the aggregate fraction is monotonic, so out-of-order output can never rewind it. `progress_forwarder` (`core/src/git.rs`) throttles emission — a whole-percent move, 150 ms elapsed, or the finishing 100 % frame — and hands each tick to the host `EventSink` as a `CoreEvent::GitProgress {op, path, percent, text}`; `TauriEventSink` re-emits it as the `git-progress` window event (from the stderr reader thread; same seam as the terminal PTY output). Frontend consumers: `Header` mirrors push/pull events for the active repo into the `networkProgress` store (in-button fill + the raw git line in the header's status area); `CloneOverlay` consumes `clone` events (URL-tab clones only — the GitHub tab clones through `gh`, which reports nothing, so its bar simply never appears). Background fetches deliberately emit nothing.

## IPC contract

The frontend never touches Tauri's raw `invoke` API directly; every backend call goes through a typed wrapper in [src/lib/api/commands.ts](apps/tauri-app/src/lib/api/commands.ts). The wrappers are grouped into namespaces matching the backend modules:

| Namespace | Commands | Backend file |
|---|---|---|
| `configApi` | `loadConfig`, `patchConfig`, `configBounds`, `loadState`, `patchState`, `recordRecentRepo` | `core/src/config.rs` |
| `gitApi` | `getStatus`, `fileStatusStyles`, `getSelectedDiff`, `getLog`, `getCommitDetail`, `listBranches`, `createBranch`, `switchBranch`, `checkoutCommit`, `deleteBranch`, `deleteRemoteBranch`, `renameBranch`, `commit`, `undoLastCommit`, `hasStagedChanges`, `classifyDiscard`, `discardFiles`, `appendToGitignore`, `ignorePaths`, `formatCommitMessage`, `repoSyncStatus`, `fetch`, `pull`, `push`, `getAheadBehind`, `getRemote`, `getRepoIdentifier`, `mergeBranch`, `mergeSquash`, `commitSquashMerge`, `mergeAbort`, `countCommitsToMerge`, `effectiveScanPaths`, `isGitRepo`, `initRepo`, `getRepoName`, `cloneRepo` | `core/src/git.rs` |
| `reposApi` | `knownRepos`, `filterRepos`, `deriveCloneTarget`, `cloneTargetPath` | `core/src/repos.rs` |
| `exclusionsApi` | `reconcile` | `core/src/exclusions.rs` |
| `diffApi` | `getParsedDiff`, `getParsedCommitDiff`, `copyDiffText`, `generatePatch`, `generateInversePatch` | `core/src/diff.rs` |
| `highlightApi` | `highlightDiff` | `core/src/highlight.rs` |
| `updateApi` | `checkForUpdate` | `core/src/update.rs` |
| `osApi` | `revealPath`, `openPath`, `openUrl` | `core/src/os.rs` |
| `ghApi` | `checkAuth`, `repoList`, `clone`, `publishRepo` | `core/src/gh.rs` |
| `aiApi` | `loadAiConfig`, `generateCommitMessage`, `checkProviderStatus`, `providerStatusFromFailure` | `core/src/ai.rs` |
| `appApi` | `takePendingLaunchTarget` | `core/src/launch.rs` |
| `terminalApi` | `listShells`, `ptyInfo`, `start`, `write`, `resize`, `close` | `core/src/terminal.rs`, `core/src/shell.rs` |

Every command is registered in [src-tauri/src/main.rs](apps/tauri-app/src-tauri/src/main.rs) via `tauri::generate_handler![…]`. **Adding a new command is four edits** (see *Core / host split*): implement it in `core/src/<module>.rs`, add the `#[tauri::command]` shim in `shims/<module>.rs`, register it in `main.rs`, wrap it in `api/commands.ts`.

## State management (frontend)

The three core writable Svelte stores, all in [src/lib/stores](apps/tauri-app/src/lib/stores):

- **`appState`** — top-level phase machine (`loading` / `repo-picker` / `main` / `error`), the discovered repo list, the chosen repo path, and whether `gh` is authenticated. `App.svelte` renders `MainLayout` for `main` and, for every other phase, a `.pre-main` column of `<Header>` + the phase's content — so app-level chrome exists in all of them (see *Repo-less phases*).
- **`repoState`** — everything tied to the currently open repo: status (branch, upstream, ahead/behind, files, isMerging), log pagination, branches, the user's selection sets (`selectedFiles`, `userDeselected`), per-file diff selection (`Map<path, DiffSelection>`), active file/diff, active commit/files/diff, loading flags, a `statusLoaded` flag (default status ≠ loaded status — anything that *skips* work on a status field has to know the difference, since `hasRemote` defaults to false and a switch resets it), and **three failure fields with different owners**, written through `reportActionError` / `reportNotice` in the store rather than by each call site building its own `repoState.update` — the classification is then a choice of function, not a shape copied from the site next door, which is how reveal-in-Finder came to seize the window. `error` (with an optional `errorRetry` bound to the same attempt) is an operation the user was waiting on and goes to the blocking `ErrorModal`; `notice` is an OS hand-off that didn't take and goes to a dismissible strip, dismissible because nothing else can ever disprove it; `pollError` is written only by refreshes the app started itself — set after three consecutive failures, cleared by any successful read, rendered in the same strip without a ✕ because its own recovery retires it. `refreshStatus` takes `{silent, background}` as two separate opts for exactly this: *silent* means "don't write `error`", which a user action's follow-up refresh also wants, while *background* means "this one is mine" and is what feeds the streak. Conflating them let three `index.lock` races behind a commit and two discards accuse a healthy repository of having vanished. Keeping the three apart is what lets a repository that really has gone away say so without a modal the user must dismiss every two seconds. A **fourth** kind never reaches any of them: `activeFileDiffError` / `activeCommitFileDiffError` are per-pane and render inline where the diff would have been, always alongside a cleared payload. A failed read of one file blocks nothing (the rest of the repository is on screen and correct) and is the user's own task (so the strip above the content is the wrong place), which puts it outside §6.13's two classes entirely. Re-selecting the row is the retry, and works because the payload is gone: the loader's "already open" short-circuit tests it.
- **`config`** — the live Config object, and the only copy in the client: Settings renders from it rather than loading a second one, so the form and the diff viewers cannot disagree about what is set. Publishing goes through `applyConfig(cfg)` — the store, `applyTheme()` (which flips `document.documentElement.dataset.theme`), and the resolved `scanFolders`, that last one re-asked only when `scan_paths` actually moved. `refreshConfig()` is `load_config` plus that; Settings hands it `patch_config`'s return value directly, since the patch already answers with the whole normalized config and a second read would only re-ask what it was just told.

Alongside these are smaller purpose-built stores: **`networkOps`** holds the user-initiated network op in flight (`activeNetworkOp` — the poll/auto-fetch/scheduler pause on it and the sync handlers use it for mutual exclusion) and its live transfer progress (`networkProgress`, fed from `git-progress` events), **`repoIdentifiers`** lazily caches each repo's GitHub identifier (a module-level map that re-publishes on each fetch, so reopening the repo picker is free), **`repoSync`** caches each repo's ahead/behind counts and working-tree `dirty` flag for the picker's pull/push badges and dirty dot (`setRepoSync` records values the active poll already computed; `syncRepo` fetches + recomputes one repo, with per-path in-flight de-duplication; its change-equality guard compares every field, so a new one must be added there too or its transitions get swallowed), and **`reposState`** mirrors the persisted `repos-state.json` document — the `repoSortMode` / `cloneSortMode` / `recentRepos` writables plus thin wrappers over the backend's atomic writers: `patchReposState` → the `patch_state` command (one field-wise read-modify-write under a process-wide lock, so a patch can never clobber another writer's field), `recordRecentRepo` → the `record_recent_repo` command (backend owns the MRU move-to-front/de-dupe/cap and returns the authoritative list, which reseeds the `recentRepos` store), and `hydrateReposState` (startup seed). Both wrappers log-and-swallow failures, so callers never need a rejection path for lost preferences.

`MainLayout.svelte` is the orchestrator: it owns the polling intervals, focus listeners, and most of the cross-cutting handlers (commit, switch branch, merge, etc.). Components stay dumb — they receive props and emit callbacks; they don't read or write the stores directly when avoidable.

**Which surface owns `Escape` is a registration, not a list** ([actions/overlayStack.ts](apps/tauri-app/src/lib/actions/overlayStack.ts)). Every dismissable surface carries `use:dismissOnEscape={…}` on its root, which pushes its dismissal onto a module-level LIFO for the element's lifetime; the two window key handlers (`App.svelte` pre-main, `MainLayout.svelte` in `main`) call `dismissTopOverlay()` and do nothing else with the key. Registration order *is* stacking order, because a dialog opened from a popover mounts after it. What it replaced was two hand-written lists of overlay flags — one per host, already drifted — that closed **every** overlay at once, so dismissing a confirmation took the popover that raised it as well; the surfaces that worked around this by answering `Escape` on their own element and calling `stopPropagation()` no longer need to, and the context menu, which did neither, used to close itself *and* the popover underneath. A surface mid-operation registers a dismissal that declines, so `Escape` is consumed rather than falling through to something that would close. The same stack publishes `overlayDepth`, which is what the app's chords read before firing: the old hand-kept `modalOpen` had never been told about four of the dialogs, so ⌘↩ fired a commit straight through the embedded-repo confirmation asking about that commit.

Both diff panes — the Changes tab's and the commit detail's — render through **`SeamlessDiffPane`**, a wrapper whose only job is the loading treatment: it dims its children and lays a spinner over them once the load outlives the slow threshold, and never unmounts them. The panes are the same pane twice and the rule had been written twice; the old shape swapped the whole thing for a "Loading diff…" line, discarding the rendered rows, their syntax tokens and the scroll position on every slow load, including the ones that came back identical. The loader stopped nulling the payload at the threshold to match.

### Selection bookkeeping

The file checkbox state is **opt-out**: every change reported by `git status` is staged unless the user explicitly deselected it. `repoState.userDeselected` is the source of truth for "things the user un-checked", and every status refresh rebuilds `selectedFiles` as `present − userDeselected`. This is what keeps polling unobtrusive: a status refresh never silently re-selects something the user just un-checked.

**An opt-out outlives its path, for 30 seconds and at least two consecutive misses.** Core owns that rule ([core/src/exclusions.rs](core/src/exclusions.rs), `reconcile_exclusions`) and both clients call it, because it was written twice and had already drifted into two opposite answers. Pruning the opt-out the tick its path disappears re-includes a file the user deliberately unchecked — a formatter rewrote it between two reads — and the next commit takes it: a commit nobody meant to make and never saw happen. Never pruning it grows a set of paths that will never be seen again. The grace window bounds the set without reopening the hole, and the asymmetry is why it errs long: an over-long opt-out costs one visible checkbox click.

**The window has two terms, and each covers what the other cannot.** It is **wall-clock**, not a count of ticks, because the poll's cadence is 2 s or 30 s depending on where the window is and "fifteen ticks" would mean anything from half a minute to seven; the caller passes the time actually elapsed, so a tick skipped for a transfer neither shortens nor extends anything. But elapsed time alone fails at the other end of that same ladder: at the 30 s rung a single read is charged the whole window, and a transfer can hand over minutes in one lump, so a purely time-based rule would drop the opt-out on the **first** look — which is exactly the look that can land in the half-second a formatter has the file renamed away. Hence `EXCLUSION_GRACE_READS`: two consecutive misses cannot be one unlucky read, at any cadence. Both counters advance on **every** tick, outside the equality gate, because a path is dropped for having been *absent* long enough — which an unchanged file list keeps being true of — and a single reappearance **resets** both, so a file edited repeatedly through a formatter never accumulates its way out of the set.

Both clients keep the clocks beside their exclusion set rather than inside it — every reader of that set asks "is this path excluded?", and a map of clocks would make each of them carry the answer to a different question — and rebuild both wholesale from core's answer, so they cannot drift apart. The crossing is skipped entirely while nothing is excluded, which is the usual state of the app, so the poll pays for this only once the user has actually unchecked something. `resetPollState()` clears the clocks with the rest of the per-repository poll memory on a switch; natively `CommitStore.reset()` does.

### Large changesets

[FileList.svelte](apps/tauri-app/src/lib/components/FileList.svelte) virtualizes its rows: only the slice currently in the viewport (plus an 8-row buffer above and below) is mounted in the DOM. A spacer div carries the full virtual height (`files.length * 24px`) so the scrollbar represents the real list, and each rendered row is absolutely positioned at `top: index * 24px`. Without this, a 1000-file changeset would block the main thread for hundreds of milliseconds every time the Changes tab pane goes from `display: none` to `display: flex`. Arrow-key navigation updates `scrollTop` synchronously, awaits `tick()`, then focuses the now-rendered target — so Home/End/↑/↓ work even when the target row is far outside the rendered window.

[CommitList.svelte](apps/tauri-app/src/lib/components/CommitList.svelte) virtualizes the same way (50px rows, spacer of `commits.length * 50px`). It measures the viewport height with a **ResizeObserver**, not a one-shot `clientHeight` read: the History pane is `display: none` while the Changes tab is active, so a single measurement at mount can capture 0 and strand the rendered range at `ceil(0 / ROW_HEIGHT) + buffer` ≈ 5 rows. The observer re-measures when the pane gains size, so the full window of commits renders once History is shown.

The **log is append-only and rooted at HEAD**: `commits[0]` is the repository's HEAD, always. Paging adds older rows to the end (deduplicated by sha against a concurrent poll reload) and drops nothing, so every visible row keeps its position and there is no scroll compensation to get right. The only move that changes what row 0 *is* is a re-read from HEAD — a commit, a checkout, an undo, a different repository — which `log.resetSeq` marks and the list answers by scrolling to row 0.

It replaced a bidirectional sliding window that tracked the absolute index of row 0 (`windowStartOffset`) and compensated `scrollTop` by `delta × ROW_HEIGHT` on every slide. The window worked, and cost more than it bought: row 0 stopped being HEAD the moment it advanced, which is what let Amend and Undo be offered on the wrong commit; a replacement had to be told apart from a slide by a second signal or the list scrolled to the end of the fresh page and paged again; and the HEAD-moved test it needed could only fire past offset 0, so a commit made while parked at the top of the list moved nothing at all. Re-reading from offset 0 makes `commits[0] === HEAD` true by construction, and none of those cases exist to be handled.

Paging keeps `log.isPaging` rather than the repo-wide `isLoading` it used to set — reading further into the past has nothing to do with whether a commit can be made, and it was disabling the Commit button on the other tab. A failed page goes to the non-blocking banner (`reportNotice`): the history already on screen is still correct, and scrolling again re-asks.

Selection is a `$derived` key read by an `untrack`ed `$effect`, the shape the changes pane's auto-select uses: a string of the loaded shas, gated on History being the visible tab so the pane behind Changes doesn't spend a `git log` and a diff read on a selection nobody is looking at. The effect re-seats on native's two conditions — nothing selected, or the selected sha is no longer in the list, which is what an amend or an undo does to it. Its leading `#` is load-bearing: it keeps "History is showing an empty log" (clear the selection) distinct from "History isn't showing" (do nothing), which a bare empty string cannot.

### Background cadence

Three background loops run while a repository is open, and they share one policy and one loop primitive.

**The policy** ([services/backgroundPolicy.ts](apps/tauri-app/src/lib/services/backgroundPolicy.ts)) is the single answer to "may this run now, and how often?", ported from the native `BackgroundSchedulingPolicy`. It reduces the window to one of three states — `active` (visible and focused), `inactive` (visible, not focused), `hidden` (`document.hidden`) — from `visibilitychange`, `focus` and `blur`, attached once for the whole app, and exposes a named predicate per loop rather than a boolean each loop composes for itself. Status poll: 2 s / 10 s / 30 s across the ladder, never stopping. Auto-fetch: the configured interval, ×3 while hidden, never stopping. Multi-repo sweeps: paused unless the window is `active`. The asymmetry is deliberate — a visible-but-unfocused window that has quietly gone stale is the failure the ladder exists to prevent, and hidden work is made cheap rather than absent so returning to the window shows a current screen instead of a catch-up. Only badges for repositories nobody is looking at are genuinely deferrable.

**The loop** ([services/pacedLoop.ts](apps/tauri-app/src/lib/services/pacedLoop.ts)) is a self-scheduling `setTimeout` chain: it asks when the next run is due *after* each run settles. `setInterval` cannot express a cadence that changes, and three properties fall out of re-deciding late — runs can never overlap (so no in-flight guard), a state change applies immediately via `reschedule()` rather than after the current period drains, and a `dueAt` of `Infinity` parks the loop entirely, which is what "auto-fetch is off" means. A 250 ms floor keeps a deadline already in the past from spinning.

- **Status poll** — the ladder above. Every status write in this client goes through the one `refreshStatus` in `MainLayout`, including the header's post-transfer reload (which takes it as a prop) and `Ctrl+R`, because a status write is more than storing what `get_status` returns: it ages the exclusion set, drops the open diff when its file leaves the working tree, and feeds `repoSync`'s badge for this repo. A second implementation forgets some of those, which is how the `MERGING` chip used to outlive an abort — the chip now reads `RepoStatus.merging`, so that particular omission is no longer expressible. HEAD is compared against `RepoStatus.head_sha` from the read that just landed; when it moved, the commit log is refreshed in place keeping the same loaded count (so the user doesn't lose scroll position) and the branch list reloads with it, since a checkout made in the terminal moves the menu's checkmark too. Each publishing run also pushes the active repo's ahead/behind + dirty flag into `repoSync` via `setRepoSync`, so the picker badges and dot for the open repo stay live without a dedicated fetch. The poll pauses entirely while `activeNetworkOp` is set.
  - **A tick that changes nothing publishes nothing.** `refreshStatus` compares `JSON.stringify(status)` against the previous reply and returns without touching the store when they match — whole-value, like the native client's `Equatable` compare, rather than a hand-picked list of fields that a later field would silently fall out of. `stat_stamp` rides inside it, which is the only reason an idle repository is recognizable at all: porcelain v2 carries no worktree hash, so a file that was modified and still is reads identically without it. Only *silent* refreshes take the shortcut — an explicit one also clears the error modal on success, which is a change even when the status is not — and a standing poll banner still retires on a skipped tick, because a successful read is what disproves it.
  - **The open diff reloads from the same stamp.** A `$derived` key of the active file's `path`, `xy` and `stat_stamp` feeds an `untrack`ed effect that re-fetches the diff when the *same* path's stamp moves; a different path means the selection changed and its own load is already in flight. Per-file, which is narrower than the native client's whole-status epoch — an unrelated edit elsewhere in the tree re-tokenizes nothing.
- **Auto-fetch** — `fetch_interval_ms` (default 30 000) through the policy's multiplier, rescheduled by an effect watching those two config fields, so a Settings change applies at once and one made in the native client applies on the next wake-up (which re-reads the config). Switched off, the loop parks rather than idling; switching it on reschedules against the last fetch, so it runs as soon as the configured interval says it is due. The first run carries a once-per-session 0–30 s skew so two windows started together don't stay in lockstep on the same repositories. Skipped while a network op is in flight or **text has the keyboard** — asked at the tick through `isTextInputFocused()` rather than latched, since a latch set from `focusin` is only cleared by another one and closing a focused terminal panel used to strand it at `true`, leaving auto-fetch dead for the session. That predicate is `document.hasFocus() && isTextInputElement(document.activeElement)`, and the first half is not decoration: `activeElement` does **not** clear when the window loses focus, so asking about the element alone would answer "still typing" for the entire time the user was away in another app and hold back every fetch the ladder is stretching rather than pausing. The native client gets the same answer for free — `NSApp.keyWindow` is nil while the app is inactive, so its first responder is nil. Calls `fetchActiveRemote` (`git fetch --prune --recurse-submodules=on-demand` against the first remote) then a silent `get_status`. `fetchActiveRemote` self-skips when offline / backing off and reports its outcome to the connectivity breaker (see *Network resilience*).
- **Tiered repo-sync scheduler** ([repoSyncScheduler.ts](apps/tauri-app/src/lib/services/repoSyncScheduler.ts)) — **one loop, three deadlines** (2 / 5 / 10 min) with staggered startup kicks, sleeping until the nearest. Each pass slices the `recentRepos` list (active excluded) into tiers — next 4, next 5, next 10 — and refreshes the due ones via `repo_sync_status` sequentially, across tiers as well as within them. Three independent timers came due together on their common multiple and walked all three lists at once, which is precisely what the sequential fan-out is meant to avoid. Tier syncs are tagged `background`, so while offline / backing off each `syncRepo` consults the breaker and downgrades to a **fetch-less local recompute** instead of grinding through dead fetches — the network goes quiet but the dirty dot keeps tracking local edits, since it needs no remote. `canRunRepoSweeps()` is re-checked before every repository, so a transfer starting or the window going away abandons the rest of the tier; the deadline is already re-armed, so it simply retries. The loop parks on the *activity* half of that predicate only — a network op has nothing that would un-park it, and a tick during one costs nothing. The tiers cover only the ~19 most recent repos, so the dropdown additionally calls `syncVisibleRepos` whenever its list is on screen: a sequential fetch-less sweep that always fills rows with no cached entry and, at most once per 30 s, re-checks the whole visible list. Started in `initialize` (after `hydrateReposState` resolves, so recents are seeded) and stopped on unmount.

When the window **wakes up** — a rise in the activity ladder, so focus returning or the window coming back on screen — `MainLayout` runs a one-shot **resync**: `refreshConfig` *first*, then `fetchActiveRemote` (so a moved upstream surfaces immediately), a silent `get_status`, the HEAD comparison, and `repoSyncScheduler.kickTopTier()` (the throttled top-tier catch-up, since the tier loop was parked while we were away). The config re-read leads because `config.toml` is shared with the native client and editable outside this process: without it a save made anywhere else never reached a running window, so theme, diff settings, provider and auto-fetch stayed at their launch values for the lifetime of the app. Reading it before the refreshes means they already see the new values. A `resyncing` guard collapses a two-step wake-up (hidden → visible → focused, common under tiling WMs) into a single run. Every loop and listener tears down on unmount.

The open diff is deliberately **not** re-read here. It used to be, unconditionally, because there was no way to tell whether the file had changed while the window was away; the per-file stamp effect above is that way, and it fires off the very status read the resync awaits. Keeping both meant two `git diff`s of the same file racing on every activation with nothing deciding which answer won — and one wasted read on every activation where nothing had changed at all. The native client still re-reads unconditionally (`refreshQuietly(forceDiffReload: true)`), which is E-9 and is the parity plan's WS-P.

**The one thing this cannot enforce is the hidden rung.** A WebView is free to throttle timers in a backgrounded document, so 30 s is a floor rather than a guarantee — which only ever makes hidden work cheaper, and the wake-up resync is what actually guarantees a current screen. The native client pins the same ladder exactly by holding an App Nap assertion; pinning it here would mean moving the tick to a Rust-side ticker and adding a backend event nothing else needs.

**Repo discovery is re-run, not cached for the session** ([services/repoDiscovery.ts](apps/tauri-app/src/lib/services/repoDiscovery.ts)). `rediscoverRepos()` re-walks the scan paths through `known_repos` and republishes `appState.repos`; it runs whenever a list is about to be looked at (the main-view dropdown on open) and whenever the thing it walks changes — a `scan_paths` or `scan_depth` patch, called by Settings itself, so the setting takes effect where it was made rather than on a later dismissal, identically in both phases. A single in-flight pass is shared rather than duplicated, `discoveringRepos` distinguishes "still walking" from "found nothing" for the empty state, and the currently-open repo is re-added if a walk that raced the fire-and-forget MRU write would otherwise drop its row. Failures log and leave the previous list standing — a stale list beats an empty one.

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

Merge state: `RepoStatus.merging` is `MERGE_HEAD`'s existence in the repo's git
directory, resolved by `git_dir` — a **filesystem** answer, not a git one.
`<repo>/.git` is either the directory itself or, for a linked worktree or a
submodule, a one-line `gitdir: <path>` pointer; reading it costs no subprocess,
which is what lets the status poll carry the flag for free. `git rev-parse
--git-dir` stays as the fallback for shapes that file can't describe (a bare
repo, or a path somewhere inside the work tree rather than at its root). Folding
it into the status is what removed a per-tick subprocess *and* the class of bug
where one refresh path forgot to ask; there is deliberately no separate
"is merging" command, because a second route to the same answer is how they
diverge. Covered by `status_reports_a_merge_in_progress_and_its_end` and
`merge_state_resolves_through_a_worktree_git_file`.

HEAD identity: `# branch.head` reading `(detached)` sets `RepoStatus.detached` (and leaves `branch` empty) so the UI can distinguish a detached HEAD from a still-loading status; `# branch.oid` yields `head_sha` for free (no extra `rev-parse`), or stays empty for an unborn branch (`(initial)`). The Header shows `On <short-sha>` + a `DETACHED HEAD` marker and the sync button disables itself while detached; the History "Checkout commit" item is disabled on the current HEAD. Covered by `get_status_reports_branch_and_head_sha`.

Sync proposal: `RepoStatus.proposal` is `sync_proposal(&status)` — the precedence
ladder (detached → publish repository → publish branch → pull → push → fetch) as a
**total function** of the status, so the combinations three independent booleans
could hold at once are unrepresentable rather than merely unhandled. That is what
retires the "push offered on a diverged branch" class: pull outranks push, so the
rejected state has no way to be proposed. `get_status` is a thin wrapper around
`read_status` that fills the field once, on the single exit — the read itself has
three returns, and an early one that forgot is exactly the bug the field exists to
remove. It rides the status rather than answering a separate command for the same
reason `merging` does, plus one the Tauri host feels directly: a command would be
an IPC crossing per poll tick, carrying the whole file list up, to run six
comparisons. `SyncProposal::Loading` is what an unfilled status maps to (empty
branch, not detached), which is also what each client holds before its first read —
so "nothing known yet" reads the same whether the host has no status or an empty
one. Covered by `sync_ladder_follows_its_precedence`,
`sync_ladder_waits_for_a_real_status`,
`sync_ladder_offers_publish_for_an_unborn_repository` and
`get_status_carries_the_proposal`.

Ahead/behind: `# branch.ab` is only emitted when the branch has a tracking upstream. For a branch that was never `push -u`'d but has a matching `refs/remotes/<remote>/<branch>`, the shared `remote_tracking_ahead_behind` helper computes the counts with `git rev-list --left-right --count HEAD...<ref>` (left = ahead, right = behind) without flipping `has_upstream` (which still gates whether the next push needs `--set-upstream`). `repo_sync_status` — the lighter sibling powering the picker badges and dirty dot — reuses both `first_remote` and that helper but runs `status --untracked-files=normal` (an untracked directory stays a single `dir/` record instead of being enumerated, which answers "any change at all?" identically to `-uall`) and optionally fetches first. Besides the branch headers it reports `dirty`: whether any `? `/`1 `/`2 `/`u ` record with a UTF-8-decodable path follows them (`is_change_record`) — precisely the records `get_status` turns into Changes-tab rows (it skips non-UTF-8 paths, so the dot must too). The active repo's `dirty` never comes from here: the status poll writes `status.files.length > 0` into the store, so the dot and the visible Changes tab agree by construction. Its fetch is best-effort and **time-boxed** (`run_git_net`, background budget): a failure/timeout swallows so a stale-but-known count still comes back, and the outcome is surfaced as the `fetched` flag for the frontend's connectivity breaker.

Unpushed markers (`unpushed_shas`, the History view's up-arrow): when the branch has a resolved upstream (real or inferred) and is ahead, the set is `git rev-list HEAD ^<upstream>`. When there's **no** resolvable upstream — a new local branch never pushed, with no same-named remote ref (the cloned-`main`-then-branched case) — but the repo has remote-tracking refs, it falls back to `git rev-list HEAD --not --remotes`: local commits not reachable from **any** remote branch. That marks the new commits while leaving the shared base (on `origin/main`) unmarked, matching GitHub Desktop; without the fallback `ahead` stays 0 there and the History view showed no arrows at all. `--remotes` (every remote) is chosen deliberately over scoping to a single push remote: it's conservative — it can only ever *under*-mark (miss an arrow on a commit that also lives on some unrelated remote ref), never draw a *false* arrow on an already-pushed commit, which a wrong-remote guess would. The one accepted divergence from GitHub Desktop is a multi-remote/fork repo where a commit was pushed only to a non-default remote. A repo with a remote but no `refs/remotes/*` yet (just `remote add`, never pushed) correctly marks every commit. Both forms are skipped when there's nothing to compute (in-sync upstream branch, or a repo with no remotes) so the status poll stays cheap. Covered by `unpushed_shas_marks_local_commits_on_unpublished_branch`, `unpushed_shas_empty_without_a_remote`, and `unpushed_shas_marks_all_commits_when_remote_has_no_tracking_refs`.
Branch switching: `switch_branch` takes the short name as the UI shows it. `list_branches` surfaces remote branches with their prefix (`origin/feature`), so a naive `git checkout origin/feature --` would treat the ref as a commit-ish and detach HEAD. Instead `switch_branch` probes with `git show-ref --verify --quiet` (which exits non-zero — i.e. `run_git` returns `Err` — when the ref is missing): if the name isn't a local branch (`refs/heads/<name>`) but is a remote one (`refs/remotes/<name>`), it routes through `checkout_tracking_branch`, which drops the first path segment to get the local name (`origin/team/x` → `team/x`, matching `git switch`'s DWIM) and runs `git checkout -b <local> --track <remote>`. The local-branch-first guard means a local branch whose name legitimately contains a slash is never misread as remote, and if the derived local name already exists it's switched to as-is rather than recreated (so a second remote's same-named branch reuses the existing local branch).

Checkout commit (detached HEAD): `checkout_commit` runs a plain `git checkout <sha>` (full SHA from the History list, so no ref/path ambiguity), landing the user on a detached HEAD — mirroring GitHub Desktop's "Checkout commit". It uses `run_git_combined` and surfaces git's message verbatim on failure (most commonly "local changes would be overwritten"), so a refused checkout never silently loses work and leaves HEAD attached. Reattaching is just `switch_branch` to any branch. Covered by `checkout_commit_detaches_then_branch_reattaches` and `checkout_commit_fails_when_local_changes_would_be_overwritten`.

### Commit detail

`get_commit_detail` returns a commit's changed files **and** its `+`/`−` totals
from one `git log <sha> -1 --first-parent --format= --raw --numstat --root
--no-color -z`. `--name-status` and `--numstat` do *not* combine — git honours
only the former — but `--raw` and `--numstat` do, and `--raw` carries the same
status letter plus both paths of a rename. `--first-parent` (rather than
`diff-tree`) is what makes a merge commit report its files at all.

The two sections are told apart by **shape**, not by position: a `--raw` record
opens with `:`, a `--numstat` record with its two counts, so the parse is
correct whichever order git emits them in. Under `-z` a rename's raw record is
followed by *two* path segments and its numstat record leaves the inline path
column empty and puts both paths in following segments, so the walk advances by
a variable number of segments — the arithmetic every bug here would live in.
Binary files appear in the file list and contribute nothing to the totals
(`--numstat` prints `-` for them, which fails to parse and is skipped rather
than counted as zero). Covered by
`commit_detail_reports_files_and_totals_in_one_pass` and
`commit_detail_lists_binary_files_without_counting_their_lines`.

### Diff read + parse

`get_parsed_diff` / `get_parsed_commit_diff` read the raw patch and parse it in
one call. Fusing them removed a full round trip per file selection from each
client and — more importantly — gave the *whitespace-only* answer somewhere to
be computed: when `hide_whitespace` leaves nothing to render, core re-reads the
unfiltered diff and asks whether *that* has anything to render. The question is
not "is the unfiltered patch non-empty" — a pure rename's header is non-empty
and has no lines either — so the comparison is on the parse, not the string. The
second `git diff` runs only on the path where the pane would otherwise be blank.

`parse_diff_with` is a hand-rolled unified-diff parser (no `regex` crate). It captures the full file header (`diff --git`, `index ...`, `--- a/...`, `+++ b/...`) into `file_header` because `git apply` requires it for new/deleted/renamed files. Each hunk stores its own `@@` header line as the first entry in `lines` so flat/global line indexing stays consistent across the frontend and backend. `DiffLine.text` — the raw patch line — is filled only for `Hunk` and `NoNewline` rows, the only two whose meaning *is* their text; every other row's `text` duplicated `content` byte for byte, once per line of every diff, on both wires.

`DiffOptions` decides what is built alongside the parse. The phase-1 HTML array and the side-by-side pairing exist for a `WebView` host; the native host renders from the line model and asks for neither, so neither is built, marshalled, or dropped at the bridge. `show_anyway` is the escape from the size guard below.

**Size guard.** A patch over 4 MiB, or one containing a line over 5 000 bytes, is *withheld* rather than parsed: `ParsedDiff.size_guard` carries the measurements and the viewer offers to render it anyway. The long-line limit earns its place separately from the byte total — a minified bundle or a base64 blob is slow at a size the total waves through. This withholds, it never refuses; the escape re-asks with `show_anyway` and applies to that one request, so moving to another file gets the guard back.

**Empty is not one thing.** `EmptyDiffReason` separates `NoChanges` (the file matches its committed state), `WhitespaceOnly` (the change is there and the setting is hiding it), and `NoTextualChanges` (a mode change or a pure rename — a header with zero hunks). A *failed* read is none of these: it is an `Err`, which is what lets a viewer clear a stale diff instead of captioning it.

**Clipboard.** `copy_text` rebuilds a flat line range from the model — `content` for ordinary rows, `text` for the two that have it — so a copy carries the file's own lines with real tabs, immune to gutters, `+`/`−` prefixes, side-by-side filler cells and a viewer's tab expansion. Out-of-range indices clamp rather than panic: a viewer's selection and the model can briefly disagree while a new diff loads.

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

**Embedded-repo detection.** `get_status` flags an entry `embedded: true` when git reports it as an untracked entry whose path keeps a **trailing slash**. Under `--untracked-files=all` git expands every ordinary untracked folder into individual files, so the only entry that stays a directory is an embedded git repo (git never recurses into one). The frontend uses the flag to (a) render a distinct ↪ link badge instead of the green `A` in [FileList.svelte](apps/tauri-app/src/lib/components/FileList.svelte), and (b) pop a confirm modal ([EmbeddedRepoConfirm.svelte](apps/tauri-app/src/lib/components/EmbeddedRepoConfirm.svelte)) before committing, since committing a gitlink — rather than the folder's files — is a surprising outcome. Covered by the `commits_embedded_repo_as_gitlink` test.

**Dirty-submodule detection.** A tracked submodule whose working tree is dirty *inside* but whose recorded commit hasn't moved has nothing the parent repo can stage — `git add` is a no-op, so a commit would dead-end with `staging produced no changes`. `get_status` flags these `submodule_dirty: true` by reading the porcelain-v2 **`sub` field** (the 3rd token of a changed entry): `S<c><m><u>`, where the entry is flagged only when it's a submodule (`S`) with the commit-pointer char `c == '.'` (unmoved) and at least one of `m`/`u` set (`is_dirty_submodule`). A moved pointer (`c == 'C'`) stays committable — the gitlink change stages normally. The frontend treats a flagged entry as non-selectable: every writer to `selectedFiles` ([MainLayout.svelte](apps/tauri-app/src/lib/views/MainLayout.svelte) — refresh seed, select-all, range-toggle, single-toggle) skips it via `isCommittable`, the row's checkbox is `disabled` with an explanatory tooltip, and the diff pane shows a "Submodule changes" message instead of the opaque `Subproject commit …-dirty` line. The user can therefore never reach the failing commit path. Covered by the `classifies_only_unstageable_dirty_submodules` and `parses_dirty_submodule_flag_from_ordinary_entry` tests.

### Discard & ignore

`discard_files` powers the Changes-tab "Discard" menu. It classifies each target by **HEAD membership**, not by the porcelain status code, via `head_paths` — a single `git ls-tree -r -z --name-only HEAD -- <paths>` that returns which of the targets exist as committed blobs (empty on an unborn HEAD). That sidesteps the ambiguity in the status code (an `AA` add/add conflict has no HEAD blob; a rename's new path doesn't either) and needs no per-file `cat-file`. The classification is `classify_discard`, which both the action and the **confirmation dialog** call, so the promise and the outcome come from one decision — a status letter gets three cases wrong (a staged re-add of a path that exists in HEAD is restorable, a rename whose original is *not* in HEAD is not, and under an unborn HEAD nothing is). Then:

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

`discover_repos` expands `~` and canonicalizes each scan root through `paths` (never `fs::canonicalize` — see [Path normalisation](#path-normalisation)), then recursively walks up to `max_depth` levels. An empty `scan_paths` list (config cleared, or config load failed upstream) falls back to `config::default_scan_paths()` — the same stock folders a fresh config gets — so the frontend passes the configured list through verbatim with no path resolution of its own. A directory is a repo if it contains a `.git` file or directory (handles worktrees). Hidden directories are skipped, and the scan does not descend into a discovered repo.

What a client actually lists is `repos::known_repos`, which unions that walk with the persisted MRU and drops any entry no longer on disk. Discovery alone forgets everything the user reached another way — a clone, a CLI open, a folder picked outside the scan paths — on every restart, even though the MRU that remembers them is already on disk; the MRU alone goes stale, and where it also feeds the background sweep, a dead entry costs a time-boxed fetch per tier interval.

A root that doesn't resolve is skipped rather than failing the scan, so every run logs one `[discover]` line with the counts and, when anything was skipped, a second naming those folders **as expanded**. A `~` still visible there means the home lookup came up empty, which is a different fault from a folder that isn't on disk; without the line, both looked identical from the outside — an empty picker. `effective_scan_paths`, which backs the picker's "searched these folders" list, expands for the same reason: that list exists to be checked against the disk.

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

**Provider resolution.** `ai::provider_config` maps the user's `Config` onto the
`AiProviderConfig` a request needs, and `load_ai_config` is what each client
calls before every generate — read fresh, never cached, so an edit in either
client applies on the next click. The mapping lives here rather than in each
host because two copies had drifted over which config read the provider came
from; resolving it in one place is what guarantees the model, the server URL and
the timeout all belong to the provider actually about to run. Each provider
keeps its **own** model (`[claude]` / `[ollama]` in the config): one shared
field meant a model set for Claude was handed to Ollama, which has never heard
of it, and Generate failed with nothing on screen explaining why. The timeout is
per provider too, and is read — a settings control that persisted a value
nothing consumed was worse than no control, because the user believed the
timeout was set.

Both providers run with the timeout from their own config section (120 s by default, clamped by `config_bounds().ai_timeout_secs`). Diff caps: 20 MB (Claude) / 50 MB (Ollama).

`check_provider_status` lets the UI gate features without surfacing raw errors, and answers with a reason and a fix command rather than a bare boolean: `claude --version` then `claude auth status` for Claude, `GET /api/tags` for Ollama with a 5 s timeout. `provider_status_from_failure` answers the same question from a request that already failed — the only route to the expired-session state, which leaves its credentials on disk and so reads as signed in.

## Terminal layer

`PtySession` holds the master PTY, the writer half, and the child process. Sessions are stored in a global `Mutex<HashMap<u32, Arc<Mutex<PtySession>>>>` keyed by a monotonic `AtomicU32`.

`start_terminal`:
1. Opens a PTY at 24×80 via `portable-pty`.
2. Resolves the shell via `shell::resolve(shell_id)` — see *Shell discovery* below.
3. Spawns it with cwd = repo path, adding only `TERM=xterm-256color`, `COLORTERM=truecolor`, and (Git Bash only) `CHERE_INVOKING=1`.
4. Stores the session, then spawns **two** threads (see *Output coalescing and flow control* below): a reader that loops on `read()`, feeds bytes through a `Utf8Decoder` and hands them to a bounded channel, and an emitter that drains that channel into the sink as `TerminalOutput`. When the reader hits EOF it drops its end of the channel; the emitter flushes whatever is left, removes the session, **reaps the child with `child.wait()`** — safe to block there: post-EOF the child is already dead (or its status was cached by a kill's internal `try_wait`), and the session is out of the map so nothing else can want its mutex — and emits `TerminalClosed` carrying a `TerminalExit { exit_code, signal }`. Flushing before the close event is what keeps "session over" from arriving ahead of the text that preceded it — a dying shell's last words are exactly the ones worth keeping. Both clients key off the exit VS Code-style: a clean exit closes the panel; a non-zero code or a fatal signal keeps the dead terminal on screen with `[Process exited with code N]` rather than flashing it away. The wait is also what stops each session leaving a zombie behind, which the old drop-without-wait teardown did.

It returns `StartedTerminal { pid, shell_id, shell_label }` — the label is resolved backend-side because the stored preference may name an uninstalled shell, and the panel header shows what actually launched.

`write_terminal` / `resize_terminal` / `close_terminal` go through `session_for(pid)`, which locks the session map, clones the `Arc`, and drops the map lock before the caller touches the session. `close_terminal` calls `child.kill()` (portable-pty's escalation: SIGHUP → a short `try_wait` grace loop → SIGKILL) but deliberately does **not** remove the entry — the emitter thread owns teardown, and it needs the child handle still in the session to collect the exit status after the kill (which then reports the fatal signal rather than a clean exit).

`resize_terminal` **ignores a grid smaller than 2×2** rather than passing it on. A collapsed or not-yet-laid-out panel legitimately measures 0 or 1 cells, and pushing that through costs real damage: the emulator reflows its whole scrollback to one row and the shell gets a `SIGWINCH` announcing a window nobody has. There is no new size to report, so it returns `Ok`. Enforcing it here rather than per client is the point — one client had an explicit guard and the other relied on its layout library's internals.

### Output coalescing and flow control

The reader thread does not emit; it `send`s to a `sync_channel` bounded at 64 chunks (~256 KiB in flight), and a second thread drains it. Both properties come from that bound:

- **Flow control.** A full queue blocks the reader, which stops draining the PTY, which fills its buffer, which makes the *shell* wait. Slowing a runaway `cat` down is the correct answer; the alternatives are an unbounded buffer that grows until something dies, or discarding output the user asked for.
- **Coalescing, driven by back-pressure rather than a fixed window.** A delivery takes whatever is *already* queued behind its first chunk; if nothing is, it goes out immediately. So an echoed keystroke or a prompt costs no added latency, while a flood — where the queue is always backed up — arrives in a few dozen large deliveries instead of one per 4 KiB read (each of which was a JSON IPC message on one host and a main-actor hop on the other). Once gathering, a delivery stops at 256 KiB or 8 ms, whichever comes first. A fixed byte threshold would have been strictly worse: at 8 KiB against 4 KiB reads it can only ever halve the count, and it holds a small reply hostage waiting for company that never arrives.

### Session transport (Tauri)

A session's output does not go on the event bus. `start_terminal` takes a
`tauri::ipc::Channel<TerminalEvent>` as an argument, and `TerminalChannelSink` writes
the session's two message kinds into it — `{event:"output", data}` and
`{event:"closed", exit}`, an internally-tagged serde enum the frontend reads as a
discriminated union. Two tests in `event_sink.rs` pin that JSON, because it is
hand-written TypeScript on the other side and nothing between the two languages
would notice a rename.

**The argument is the whole point.** A Tauri event emitted with no listener attached
is dropped, not queued, and a `listen()` is itself an async round trip — so
registering after `start_terminal` returned left a two-round-trip window in which
the reader thread was already emitting. A fast-printing shell lost its first prompt;
a shell that died instantly (the broken `.zshrc` case) could lose its error output
*and* its exit notice, leaving the panel to close on nothing. A `Channel`'s id is
minted in its JS constructor, before any IPC, so the handler exists before Rust can
possibly hold the channel: the race isn't narrowed, it is unrepresentable. That also
makes this seam the same shape as the bridge's — a listener handed to the spawn —
rather than two different answers to one problem.

Two consequences the frontend has to honour. **`closed` can overtake
`start_terminal`'s own return**, since they travel as independent messages, so
`Terminal.svelte` records that the session ended and refuses to adopt the pid
afterwards — a handle the backend has already dropped would make the panel's ✕ kill
a session that is gone. And the channel's ordering guarantee is the transport's, not
the frontend's: Tauri stamps each send with a monotonic index and the JS side
reassembles on it, which it needs because deliveries fork by size (a JSON payload
under 8 KiB is eval'd directly; anything larger is fetched by a second async invoke).

`start`, `resize` and `close` are `#[tauri::command(async)]`, so they run off the main
thread — `close_terminal` alone can block ~250 ms in portable-pty's kill escalation.
**`write_terminal` is deliberately still synchronous**: Tauri runs a sync command
inline in the order its IPC messages arrive, and that arrival order *is* the
keystroke-ordering guarantee. Spawning each write onto the async runtime would let
two keystrokes race, which is the one thing a terminal may never do.

### Key ownership between the app and the shell (Tauri)

Every key the app binds is `Ctrl`-or-`Cmd`, and on Windows and Linux `Ctrl` is also the shell's modifier — so inside the terminal a single keystroke had two owners. Two mechanisms, one rule (FRONTEND.md §6.11): **while the terminal has focus, the shell gets everything except the chord that toggles the panel.**

- `Terminal.svelte` installs an `attachCustomKeyEventHandler` that returns `false` only for the toggle. Returning `false` means xterm neither handles the event nor calls `preventDefault`, so it bubbles to the window listener that owns it; returning `true` for everything else keeps `Ctrl+P`, `Ctrl+R` and `Escape` with readline and vim. The handler runs for `keyup` and `keypress` as well as `keydown`, so it matches on the phase too — releasing an event xterm had already begun processing is not the same as declining it.
- The window-level handlers (`MainLayout.handleKeyDown`, `Header.handleGlobalKeyDown`) re-check the event's origin through `utils/keyboard.ts`'s `isFromTerminal`, which tests for an `.xterm` ancestor — the class xterm puts on the element it was opened into. An `instanceof HTMLTextAreaElement` test cannot do this job in either direction: xterm's input sink *is* a textarea, so the old `if (inField) return` swallowed the toggle exactly where it was most wanted, while the push shortcut, deliberately ungated, fired straight through it.

`.xterm` rather than the dock's `.terminal-section` is deliberate: the dock header's buttons are ordinary app chrome and must keep the app's shortcuts.

The toggle itself is **`Ctrl` only**, on every platform. It is the one chord in this client that does not also answer to `Cmd`: ⌘` is macOS's window cycling, and a Tauri build that answered to it took a system gesture away from the user. The narrowing has to be made in both places at once — the xterm handler and `MainLayout`'s — or the chord stops working from inside the panel.

`MainLayout`'s test sits **above** the `if (inField) return` bail, with the composer's chords and `Ctrl/Cmd+R`. The bail exists to keep app shortcuts out of what someone is typing, and `Ctrl` + `` ` `` is not that; the native client never had the question, since a SwiftUI `keyboardShortcut` is a key equivalent and fires ahead of the first responder. So the handler's order is load-bearing in three steps: terminal origin first, then the chords that must work inside a field, then the bail, then everything else.

### Pointer and clipboard (Tauri)

`WebLinksAddon` takes our own activation handler, which returns unless the platform's modifier is held (⌘ on macOS, `Ctrl` elsewhere) and then hands the URL to `os::open_url` rather than `window.open`. The addon has no modifier option and no way to make its decorations conditional, so a gated link still underlines on a bare hover — which is why the affordance is not optional: `options.hover` builds a small "Follow link (⌘ + click)" tooltip inside `Terminal.element`, carrying xterm's own `xterm-hover` class so xterm's mouse tracking stops at it and the link cannot flicker out from under the pointer. The class has no styles of its own; it is purely that marker.

OSC 52 is registered by hand — xterm implements idents 0/1/2/4/8/10/11/12/104/110/111/112 and nothing for the clipboard. `parser.registerOscHandler(52, …)` receives the payload with `52;` already stripped, so what arrives is `Pc;Pd`: a selection *set* (`c` clipboard, `s` select, `p` primary, digits for cut buffers; empty means the spec's default) and base64 data. **Only writes are honoured.** `Pd === "?"` is a read request, which would type the clipboard back down the TTY — an exfiltration route for anything that can print to the terminal — so it is swallowed with `true` rather than declined, which also stops a fallback handler answering it. The write goes through `tauri-plugin-clipboard-manager`, not `navigator.clipboard`: WebKit refuses a programmatic clipboard write without a recent user gesture, and a shell asking for the clipboard has none.

### Child environment — do not forward the parent env

`CommandBuilder::new` already assembles the right environment. On Windows it seeds from the current process, then overlays `HKLM\…\Session Manager\Environment` and merges `HKCU\Environment`, so `PATH` becomes the same system+user merge Explorer and Windows Terminal hand their children.

`start_terminal` deliberately does not copy `std::env::vars()` over the top of that — doing so breaks the terminal two ways on Windows:

- **Launched from Git Bash** (the `leogit` shell function), the inherited `PATH` is MSYS-style (`/usr/bin`, `/c/Program Files/...`). Win32 cannot resolve a single entry, so essentially every command fails.
- **Launched from Explorer**, the inherited `PATH` is a login-time snapshot that misses anything installed since.

Only deliberate additions go on top. `session_env` is a pure function returning those additions, and `session_env_never_overrides_path` is the regression test.

### UTF-8 across read boundaries

PTY reads split wherever the 4 KiB buffer fills, so multi-byte characters routinely straddle two reads. Decoding each chunk with `String::from_utf8_lossy` turned every such split into a permanent U+FFFD — stray marks through box-drawing, accented text and emoji. `Utf8Decoder` holds the truncated tail (bounded at 3 bytes) until its continuation bytes arrive, which is what conhost does. Invalid bytes still collapse to one replacement character and decoding resumes.

### Shell discovery

[core/src/shell.rs](core/src/shell.rs) probes for shells rather than naming them, so the picker can never offer one that fails to spawn. Windows, best-first: **Git Bash** (install root from `HKLM\SOFTWARE\GitForWindows\InstallPath`, falling back to the default dirs then `git.exe`'s grandparent; launched `--login -i` so `/etc/profile` populates the MSYS `PATH`), **PowerShell** (`pwsh.exe`), **Windows PowerShell** (`powershell.exe`), **Command Prompt**. Git Bash leads because it is the shell git workflows assume; pwsh beats 5.1, whose in-box PSReadLine 2.0 repaints badly under ConPTY. Unix: `$SHELL` first, then zsh/bash/fish/sh de-duplicated by path.

`resolve` falls back to the best available shell when the stored id is unknown or uninstalled — the terminal opening with the wrong shell beats it not opening. `available()` is total and never empty, so neither function can panic.

The frontend mounts `<Terminal>` keyed by `${repoPath}:${terminalSessionId}` so swapping repos or hitting "New session" forces a fresh component, which in turn dispatches a new `start_terminal`. The previous component's cleanup invokes `close_terminal` on its tracked pid. Changing the shell preference applies to new sessions, not running ones — and the preference the next session reads is fresh, because the config store is re-read on window activation, so a shell chosen in the native client's Settings is already in hand.

The component also puts the caret back when the app becomes active: WebKit leaves xterm *painted* as focused while the hidden textarea behind it holds nothing, so keystrokes vanish until the user clicks. The condition is read at that moment — `container.contains(document.activeElement)` — rather than latched on the way out. A flag set from `focusin` is only ever cleared by another `focusin`, and clicking a plain element raises none, so it would strand `true` and the shell would take the caret back from wherever the user had actually gone. AppKit does this for the native client on its own.

`terminal_pty_info` reports `{backend, build_number}` and must be called *before* the xterm instance is constructed — xterm reads `windowsPty` when it builds its buffer, so setting it afterwards does nothing. Declaring ConPTY on a build ≥ 21376 is what enables reflow on resize; without it xterm assumes any line whose last cell is non-blank is wrapped, which is what smears a resized prompt. The `ResizeObserver` is debounced 80 ms because an undebounced panel drag pushes a `ResizePseudoConsole` per frame and PSReadLine repaints its whole edit buffer on each one.

## GitHub layer

Everything in `gh.rs` shells out to the `gh` CLI, time-boxed through `process::run_timed` (20 s for metadata queries, 600 s for transfers), with errors as gh's own stderr verbatim (a `stderr_or` fallback covers the rare silent failure) and spawn failures mapped to "GitHub CLI (gh) is not installed.":

- `check_auth` → `gh auth status` (exit code only; total — missing, unauthenticated, and timed out all collapse to `false`). The Tauri client runs it once at launch into a field nothing reads; kept for future gh-backed gating.
- `gh_repo_list` → `gh repo list --no-archived --json <fields>` (the Clone dialog's GitHub tab).
- `gh_publish_repo` → `gh repo create <name> --source <repo> --remote origin --push [--private|--public]` (see the bullet under *Design decisions*).
- `gh_clone` → `gh repo clone <owner/name> <target>` through the same `prepare_clone_target` guard as `git clone`.

## OS integration layer

[core/src/os.rs](core/src/os.rs) holds the two file-manager hand-offs behind the Changes-tab menu. Both take a repo-relative path and join it onto the repo path **in Rust** (`PathBuf::from(repo_path).join(rel_path)`) so git's forward-slash paths never clash with Windows separators, then spawn a platform launcher:

- `reveal_path` — macOS `open -R`, Windows `explorer /select,<path>`, Linux `xdg-open <parent dir>` (no portable "select file" there).
- `open_path` — macOS `open`, Windows `cmd /c start "" <path>`, Linux `xdg-open <path>`.

They're `#[tauri::command(async)]` (worker thread) and routed through `process::run_timed` (15 s cap, so a wedged file manager can't hang a thread) with `hide_console` for the Windows no-flash guarantee. The launchers are treated as fire-and-forget: a completed run is success regardless of exit code, because some launchers (notably `explorer /select,`) return non-zero even on success — only a spawn failure (e.g. `xdg-open` absent) or a timeout surfaces as an error. The frontend side (clipboard copy, label selection) lives in [services/fileActions.ts](apps/tauri-app/src/lib/services/fileActions.ts); the menu is built in `FileList.svelte` and the destructive-discard confirmation in [DiscardConfirm.svelte](apps/tauri-app/src/lib/components/DiscardConfirm.svelte).

## Config & persistence

Defined in [core/src/config.rs](core/src/config.rs).

- Config dir is resolved via `directories::BaseDirs::config_dir().join("leogit")` (`~/.config/leogit` on Linux, `~/Library/Application Support/leogit` on macOS, `%APPDATA%\leogit` on Windows). It's created if missing.
- `config.toml` — every field on the `Config` struct, ending with the `[claude]` and `[ollama]` tables. **Field order is load-bearing**: `toml` serializes in declaration order and a table swallows every key after it, so nothing scalar may be declared below those two — a file that writes cleanly and reads back wrong is the failure mode, pinned by `config_round_trips_through_toml_with_its_tables_last`. New fields carry `#[serde(default = "…")]` so users on older configs keep working, and unknown keys — a retired field still sitting in an older file — parse as ignored, never an error (`config_ignores_retired_keys` pins it, guarding against a future `deny_unknown_fields` invalidating files already on disk). Defaults are written to disk on first run so the file is discoverable.
- **`patch_config` is the only writer**, and it reads-edits-normalizes-writes under a process-wide `CONFIG_LOCK`. Two clients share this file and each runs its commands concurrently, so the whole-object write it replaces was a lost update waiting to happen: a save posted the entire config as it looked when a dialog *opened*, silently reverting whatever the other client had written since. A patch names only the fields its surface owns. Clearing an optional field is patching it to `""` — the config's standing blank-means-absent rule, rather than a second `Option` layer every host would have to model.
- **`Config::normalized()` runs on the way in and on the way out.** Numbers clamp to `config_bounds()` — landing on the nearest bound, not the default, so an out-of-range entry keeps the user's intent ("as big as allowed"); blank-after-trim strings become absent; an unrecognized `ai_provider` folds onto `claude`; blank and duplicate scan paths drop (a blank one would expand to the whole home directory). Applying it on the *read* is what heals a file another client already poisoned — `Some("")` is not `None`, so an emptied model box used to run `claude --model ""` and an emptied server URL made Ollama POST to a hostless path. `config_bounds()` is also what a settings control's `min`/`max` are built from, so a form cannot offer a value the writer then clamps away; the three hand-copied bound tables (two of them in different units, one disagreeing with its own control's starting value) are gone.
- `repos-state.json` — `last_opened_repo`, `last_clone_dir`, the two sort-toggle preferences (`repo_sort_mode`, `clone_sort_mode`), and `recent_repos` (MRU order, capped at `MAX_RECENT_REPOS = 50`). Every field is `Option`/`#[serde(default)]` so older state files load fine. JSON instead of TOML to keep it cheap to extend.
- Every read runs `normalize_repo_paths`, which converts the stored paths (see [Path normalisation](#path-normalisation)) and de-dupes `recent_repos` afterwards. A file written before that change holds Windows verbatim paths, which no longer match anything `discover_repos` returns — `last_opened_repo` would silently stop resolving (the app forgets the open repo and lands in the picker) and the MRU list would grow a second entry per folder. It runs on every read rather than as a one-shot migration because it's idempotent and the next write persists it, so the file heals itself with no schema version to carry.
- Writes go through two commands that each run one read-modify-write under a process-wide `STATE_LOCK` (Tauri runs commands concurrently; two interleaved load+save cycles would drop the slower writer's fields): `patch_state(ReposStatePatch)` merges the supplied fields (`None` = leave as-is; `recent_repos` is deliberately not patchable), and `record_recent_repo(path)` owns the MRU move-to-front/de-dupe/cap. Both return the resulting state so the frontend reseeds from the authoritative copy. A corrupt state file self-heals inside `update_state`: it logs, starts from defaults, and lets the save rewrite it, instead of wedging every future patch on the same parse error. Covered by the `prepend_recent_*` / `apply_patch_*` tests.

## Tauri capabilities

[capabilities/default.json](apps/tauri-app/src-tauri/capabilities/default.json) is intentionally minimal:

```json
"permissions": [
  "core:default",
  "core:path:default",
  "core:event:default",
  "core:window:default",
  "core:window:allow-set-title",
  "core:app:default",
  "dialog:allow-open",
  "clipboard-manager:allow-write-text"
]
```

No filesystem, shell, or HTTP plugins are exposed to the WebView. All side effects route through our explicit `#[tauri::command]` functions, which is a deliberate security boundary. The three entries beyond the default sets are each one capability, not a set: `allow-set-title` because the window is named after the open repository, `dialog:allow-open` for the Clone dialog's destination picker — the folder chooser is the only native dialog the frontend raises — and `clipboard-manager:allow-write-text` for the terminal's OSC 52 writes. **Write, deliberately without read**: the sequence's read form is the one an untrusted shell could use to lift the clipboard, and a capability that isn't granted cannot be reached by mistake.

## Build / dev

```bash
# From the project root
just install         # pnpm install inside apps/tauri-app
just dev             # pnpm tauri dev   (Vite on :5173 + Tauri host)
just build           # pnpm tauri build (debug bundle)
just build-release   # pnpm tauri build --release with RUST_BACKTRACE=1
just check           # pnpm svelte-check + cargo check --workspace
just format          # prettier + cargo fmt --all
just clean           # nuke dist/, target/, node_modules, generated macOS artifacts

# Native macOS client (needs Xcode + `brew install xcodegen`)
just mac-generate    # project.yml → LeoGit.xcodeproj
just mac-bindings    # cargo build -p leogit-ffi + regenerate Swift bindings
just mac-build       # xcodebuild (runs mac-bindings first, via a pre-build phase)
just mac-run         # build, then launch LeoGit.app
just mac-run --no-build  # relaunch the last build, skipping xcodebuild
```

`just check` covers the whole Cargo workspace, so `leogit-ffi` type-checks alongside core and
the Tauri host. The macOS app is not part of it — it needs Xcode, so it stays behind `mac-*`.

Inside `tauri-app`, `pnpm run check:native` runs the same tsconfig through the TypeScript 7 native compiler (`tsc --noEmit`, ~0.2 s full check) for fast feedback on `.ts` files; `pnpm check` (svelte-check, on the TS 6 JS line) stays authoritative because the native compiler doesn't see `.svelte` files. `src/vite-env.d.ts` (`vite/client` types) declares the CSS side-effect imports that TS 6/7's stricter resolution (TS2882) would otherwise reject.

The Tauri dev command uses `beforeDevCommand: pnpm run dev:vite` (per `tauri.conf.json`) so the Vite dev server starts in-process. Release builds use `beforeBuildCommand: pnpm run build:frontend` which writes static assets to `tauri-app/dist`, then `frontendDist: "../dist"` points the bundle at them.

Bundle targets: `app` + `dmg` (macOS), `deb` + `appimage` (Linux), `msi` (Windows).

### Release pipeline (`scripts/`)

`deploy_releases.sh` runs per-platform and uploads to one shared GitHub Release; run it once on each OS to publish a complete release. It validates prerequisites, then guards against shipping behind the live release — it queries GitHub's `/releases/latest` (the same endpoint `install.sh` installs from) and aborts if the version it's about to ship is older than that tag, since a stale local tree would otherwise clobber artifacts onto a superseded release. It then bumps/commits the version across `tauri.conf.json` / `Cargo.toml` / `package.json`, tags, then calls `bundle.sh` and packages the result:

- **macOS** — `bundle.sh` builds `leogit.app` (`--bundles app`) and ad-hoc signs it; the deploy script zips it with `ditto` into `LeoGit-<ver>-macOS-<arch>.zip`.
- **Linux** — `bundle.sh` builds an AppImage (`--bundles appimage`, no signing); the deploy script copies it to `LeoGit-<ver>-linux-<arch>.AppImage`.

`install.sh` is the curlable installer and auto-detects the platform: on macOS it unpacks into `/Applications`, strips quarantine, and re-registers with Launch Services; on Linux it drops the AppImage at `~/.local/bin/leogit.AppImage` behind a `~/.local/bin/leogit` wrapper, extracts the bundled icon, and writes a `~/.local/share/applications/leogit.desktop` launcher (warning if FUSE 2 is absent, since Arch ships only FUSE 3). The wrapper exports `WEBKIT_DISABLE_DMABUF_RENDERER=1` at launch when `/dev/nvidia0` is present (the proprietary NVIDIA driver's DMABUF/GBM path crashes WebKitGTK with "Failed to create GBM buffer" errors); it's detected per-launch rather than at install time because the active GPU is a runtime property, stays inert on AMD/Intel/nouveau, and honors a pre-set value. The desktop environment (GNOME, COSMIC, …) is irrelevant — both run the same WebKitGTK/GTK runtime — so one AppImage serves every Arch machine. As a final step it installs the `leogit [dir]` shell command into the user's login shell: it detects `$SHELL` (which survives `curl … | bash`, being inherited from the parent) and writes a `leogit()` function — into `~/.zshrc` (zsh), `~/.bashrc` on Linux / `~/.bash_profile` on macOS (bash), or an autoloaded `~/.config/fish/functions/leogit.fish` (fish); an unknown shell gets the snippet printed for manual setup. For zsh/bash the function lives inside an idempotent `# >>> leogit >>>` … `# <<< leogit <<<` marker block that re-installs replace rather than stack. The function resolves the directory and opens it (macOS `open -n --args`; Linux the PATH wrapper) — see *Command-line repo opening* for the app side.

### In-app update check

[core/src/update.rs](core/src/update.rs) closes the loop on that pipeline: `check_for_update` issues one unauthenticated `GET /repos/LeoManrique/leogit/releases/latest` (10 s timeout, `User-Agent: leogit/<ver>` — GitHub 403s without one) and compares the `v`-stripped `tag_name` against `env!("CARGO_PKG_VERSION")`. The compare is a three-part numeric tuple, not `semver` or a string compare — it matches the `sort -V` ordering `deploy_releases.sh` already applies to the same tags. The parse is **strict**: anything that isn't exactly three numeric parts (`0.2.0-beta.1`, `0.1.28+build.5`, `1.2.3.4`) yields `None` and means "no update". Coercing those instead is wrong in *both* directions — a lenient parse reads `0.2.0-beta.1` as `(0, 2, 0)` and announces a phantom update over `0.1.27`, and reads `0.1.28+build.5` as `(0, 1, 0)` and hides a real one — and it's reachable, since `deploy_releases.sh` only regex-validates the version when one is passed as an argument.

A version match alone isn't enough to announce, though: `deploy_releases.sh` runs **once per platform onto one shared release**, so the first platform to finish publishes a release the others aren't in yet. `check_for_update` therefore also requires an asset named exactly `LeoGit-<ver>-<platform>-<arch>.<ext>` (`-setup.exe` on Windows) — the same string `install.sh` resolves — and stays quiet otherwise. Without that gate a Windows user gets sent to a page holding only a macOS zip, and on macOS/Linux it's worse: `install.sh` kills the running app at step 2 and only discovers the missing artifact at step 4, leaving the user with no app *and* no update. `artifact_name` is pinned to those literal strings by a golden test, since a drifted name would silently hide every update rather than fail loudly.

`Ok(None)` means current; `Err` means the check itself failed and is a retry signal, never a user-facing error. There is no auto-download, no signed feed, and no `tauri-plugin-updater` — the payload's `install_command` carries the `install.sh` one-liner on macOS/Linux and is `None` on Windows, where the release page's installer is the path instead. In debug builds only, `LEOGIT_FAKE_UPDATE=<ver>` short-circuits the whole request (artifact gate included) so the UI can be exercised without publishing a release.

The frontend runs it once per session from `App.svelte` (not `MainLayout`, so it also covers the repo-picker phase) via [services/updateChecker.ts](apps/tauri-app/src/lib/services/updateChecker.ts): gated on `shouldAttemptBackground()`, it retries every 30 min *until one check completes*, then stops for good — plus an `online` listener so launching offline (a plane, a captive portal) retries the moment connectivity returns instead of waiting out the window. Its outcome deliberately does **not** feed `recordResult` — a rate-limited GitHub API says nothing about the git remotes the connectivity breaker guards. The result lands in [stores/update.ts](apps/tauri-app/src/lib/stores/update.ts) (`availableUpdate` plus a session-only `updateDismissed`; neither is persisted, so a skipped release resurfaces next launch). Opening the release page uses the new `os::open_url`, a sibling of `open_path` reusing the same `open` / `cmd /c start` / `xdg-open` hand-off — no opener/shell plugin was added. It rejects non-`https` URLs and any URL containing whitespace or ``&^<>|"'` `` — syntax to `cmd`'s parser even unquoted — plus `%`, since `cmd` expands `%VAR%` *before* that check's characters are handled and could smuggle them back in. That also rules out percent-encoding and (via `&`) query strings, which is fine for the `https://github.com/...` paths we open and keeps the door deliberately narrow.

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

The `highlight_diff` command (in [src-tauri/src/commands/highlight.rs](core/src/highlight.rs)) tokenises every `Context | Add | Delete` row of a `FileDiff` using `syntect` against `two-face::syntax::extra_newlines()`, then returns **render-ready HTML strings** (one per flattened diff line) via [core/src/render.rs](core/src/render.rs). Tokens (`{ start, end, class }`, code-point indices matching `IntraLineRange`) are internal to the backend now — `TokenClass` never crosses the wire, so it carries no `repr`/serde contract.

**Tokenise the file, never the diff.** syntect is a stateful, line-sequential parser: a line's classification depends on every line above it. A diff supplies neither a line-1 start nor contiguous lines, so parsing the diff's own rows leaves the parser in whatever context a fresh `ScopeStack` begins in. For a `.svelte` file that context is top-level **markup**, so a hunk inside `<script lang="ts">` got tokenised as markup — a `listen<string>` generic came back as an HTML *tag*, and comments got no `Comment` class at all. A markup hunk looked perfect and a script hunk looked broken purely by where the hunk landed, which read as "highlighting is inconsistent".

`highlight_diff` therefore takes an optional `BlobSource` (`workingTree { repoPath }` for uncommitted changes, `commit { repoPath, sha }` for a committed diff — the frontend states *what it is looking at*, Rust owns the rev-specs). `highlight_from_blobs` reads each side's full blob via `git::read_blob` / `git::read_working_tree_file`, parses each from line 1 in `tokenize_file`, and maps tokens onto rows by line number: **`Delete` rows take the old blob, `Add`/`Context` the new**. The two sides never share a `ParseState`, so a deleted line's unterminated string or comment can no longer bleed into the rows below it. Parsing is the cost and recording is nearly free, so the `wanted` line set bounds the payload, not the work.

When `source` is absent or a blob can't be read (added/deleted file paths, an oversized file), it falls back to `highlight_from_diff_lines` — the legacy diff-only parse — so highlighting degrades rather than disappears.

**Scope resolution walks the stack, leaf-first.** `scope_to_class` descends `ScopeStack` rather than reading only the top scope, because a delimiter carries its container beneath it: `//` is `[source.ts, comment.line, punctuation.definition.comment]`. Reading just the leaf classified that as `Punctuation` (which renders with no class), so **every line comment was two-tone** — an uncoloured `//` before an italic body. Punctuation is treated as *transparent*: remembered as a fallback while the walk continues, so `//` inherits `Comment` from below. Leaf-first order keeps genuine nesting correct — the `name` in `` `hi ${name}` `` has `variable.other` above `string.template`, so it stays `Variable`. Matching uses `Scope::is_prefix_of` against a `LazyLock` table (most-specific first), which is allocation-free in what is the module's hottest loop.

**Markup languages need their own scope family.** The `TokenClass` table maps the *code* scopes (`keyword`, `string`, `entity.name.function`, …) that programming languages emit. Markdown — and reStructuredText, AsciiDoc, Textile — emit almost none of those; they tag their text with a `markup.*` family (`markup.heading`, `markup.bold`, `markup.italic`, `markup.strikethrough`, `markup.raw.inline`, `markup.quote`) plus `markup.underline.link` / `meta.link` / `meta.image` for links. With none of those in the table, `scope_to_class` fell through to `Plain` for every token, so a `.md` diff rendered as flat text while every code language highlighted. Those scopes now map to the `Heading | Strong | Emphasis | Strikethrough | Link | Raw | Quote` classes. The same leaf-first descent that colours `//` with its comment also colours a heading's `#`, a bold span's `**`, and a link's brackets with their construct, since each delimiter's `punctuation.*` scope sits above its `markup.*` container. Covered by `markdown_constructs_get_markup_classes`.

**A single tilde must not strike text through.** Sublime's Markdown grammar opens a strikethrough on a run of *one or two* tildes; GitHub (cmark-gfm) and markdown-it — VS Code's preview, which refuses a delimiter run shorter than two outright — both need two. Opened by one tilde, the grammar then closes on the next stray `~`, or, finding none, strikes the rest of the paragraph, so ordinary prose (`~25 min · ~2 h`, `~/leogit`) rendered muted and struck through. The syntax set ships pre-compiled, so the grammar is not ours to correct; its delimiter scopes are. [`single_tilde_strikes`](core/src/highlight.rs) measures each `punctuation.definition.strikethrough.begin|end` run against the following op's offset and returns the byte ranges of every single-tilde run, which `drop_strikethrough` puts back to `Plain` — leaving nested constructs (a bold span inside the run) on their own classes. Runs are tracked on **every** line, recorded or not, since a `~~` run may legitimately open above the recorded window and span lines; `~~` runs are never touched. Pinned by `single_tilde_does_not_strike_through` and `double_tilde_run_survives_across_lines`.

**Fenced code blocks are re-highlighted by their info string.** A ```` ```lang ```` fence *should* highlight its body as `lang`, but the Markdown grammar only **embeds** a fixed subset of languages: `rust`, `python`, `js`/`ts`, `json`, `c`, `java`, `ruby`, `bash` come back with real `source.*` scopes, while `go`, `yaml`, `html`, `shell` (and many more) come back as opaque `markup.raw.code-fence` — a whole block of one scope. Relying on embedding therefore leaves *most* real-world code blocks flat (Go blocks were the tell). So `tokenize_file` resolves the fence itself: [`fence_role`](core/src/highlight.rs) reads syntect's own fence scopes (`meta.code-fence.definition.begin|end`, `constant.other.language-name`) to find each fence's boundaries and info string — no hand-rolled CommonMark scanner — and [`resolve_fence_language`](core/src/highlight.rs) maps that info string to a syntax via `find_syntax_by_token` (which matches names *and* extensions, so `go`, `ts`, `c++` all resolve). The body is then tokenized with **that language's own `ParseState`**, run in parallel with the Markdown parser (which keeps advancing so it still detects the closing fence). Every labelled fence highlights uniformly, embedded or not; an unlabelled or unknown-language fence (`mermaid`, `text`) has nothing to resolve and stays plain. This is why the table maps only `markup.raw.inline` (inline `` `code` ``) and *not* bare `markup.raw` — a code-fence body must never take the flat `Raw` tint. The fence path is gated to Markdown (`text.html.markdown`) since only it emits those scopes. Pinned by `markdown_fenced_code_block_highlights_by_info_string` (Go **and** Python) and `markdown_unlabelled_fence_body_stays_plain`.

**HTML emission lives in [core/src/render.rs](core/src/render.rs), shared by both render phases.** `parse_diff` calls `plain_html` (escaped text + intra-line backplate) so phase 1 ships inside the parse payload; `highlight_diff` calls `highlighted_html`, which lays one `.syn-*`-classed span per token over the *same* backplate — one implementation, so the phases can't drift. `render_line` clamps malformed token bounds, fills inter-token gaps as plain text, escapes only `&`/`<`/`>` (element content, never attribute values), and splits spans around the intra-line range, merging classes on the overlap. Theme swap is pure CSS — `--syn-*` variables in [app.css](apps/tauri-app/src/app.css) flip with `:root[data-theme]`. `Plain`/`Variable`/`Punctuation` deliberately map to no class and inherit `--text-primary`. The markup classes carry font styling as well as colour — `.syn-strong` is bold, `.syn-emphasis`/`.syn-quote` italic, `.syn-strike` struck, `.syn-link` underlined — so a Markdown diff reads the way the rendered document would. Pinned by the `render::tests` (escaping, class merging, code-point indexing, bound clamping).

Guards: lines over `MAX_HIGHLIGHT_LINE_LEN = 1024` chars are still *parsed* (state below them depends on it) but not recorded, mirroring `MAX_INTRA_LINE_LEN` in `core/src/diff.rs`; files over `MAX_HIGHLIGHT_FILE_LINES = 20_000` bail to the fallback.

**`highlight_diff` must stay `#[tauri::command(async)]`.** A plain `#[tauri::command]` runs on the **main thread**. Tokenising whole blobs is ~20× more expensive than the old diff-only parse — the repo's largest file (`git.rs`, 3286 lines) measures **~52 ms release / ~284 ms debug** vs ~2 ms / ~14 ms diff-only — which is enough to beachball the cursor on every file switch. The old parse was cheap enough to hide that the command was on the UI thread at all. The sibling diff commands (`parse_diff`, `get_diff`, `get_commit_diff`, `generate_patch`) are now `(async)` too, along with every other subprocess/filesystem command (see *Network resilience*, layer 1).

DiffViewer's debounced (80 ms) phase 2 and its `lastDiff` guard (which keeps the status poll from re-tokenising) mean each file switch costs one tokenise, off the UI thread, after plain text has already painted — so there is no token cache yet.

**`parse_diff` returns a `ParsedDiff` wrapper, not a bare `FileDiff`.** Alongside `file_diff` it carries everything else the viewer would otherwise re-derive per render: `html` (the phase-1 lines above), `sbs_pairs` (the side-by-side pairing — context/header rows spanning both columns, each delete run zipped against the following add run, `NoNewline` markers rowless), and `additions`/`deletions` for the header badge. The pairs reference lines by **flat/global index** — the same indexing the per-line HTML and the selection map use — and the viewer resolves them through a trivial `flatLines` flatten, so the pairing algorithm itself lives only in [diff.rs:build_sbs_pairs](core/src/diff.rs). `FileDiff` itself deliberately stays lean and wire-identical to before, because the frontend round-trips it back into `highlight_diff` / `generate_patch` — putting the derived artifacts on it would echo them over IPC on every highlight. Covered by the `diff::tests` (run zipping, `NoNewline`, backplate HTML).

## Accessibility patterns

The frontend builds warning-free (`pnpm check` and `vite build` both report 0 a11y warnings). These conventions keep it that way — Svelte's compiler enforces them:

- **Overlays close via backdrop target-check, not `stopPropagation`.** Every modal/dropdown backdrop is `role="presentation"` with `onclick={(e) => { if (e.target === e.currentTarget) close() }}`. The inner dialog is `role="dialog" aria-modal="true" tabindex="-1"` with **no** click handler. The old pattern (inner `onclick={e => e.stopPropagation()}`) tripped both "click handler needs a keyboard handler" and "dialog role needs a tabindex". Affects [ErrorModal](apps/tauri-app/src/lib/components/ErrorModal.svelte), [ForcePushConfirm](apps/tauri-app/src/lib/components/ForcePushConfirm.svelte), [SettingsOverlay](apps/tauri-app/src/lib/views/SettingsOverlay.svelte), [HelpOverlay](apps/tauri-app/src/lib/views/HelpOverlay.svelte), and the repo/branch overlays in [MainLayout](apps/tauri-app/src/lib/views/MainLayout.svelte).
- **Resize handles are `role="slider"`, not `role="separator"`.** A focusable separator (the ARIA "window splitter") is flagged by Svelte either way — the mouse listener warns on a non-interactive role, and adding `tabindex` warns again (`a11y_no_noninteractive_tabindex`). `slider` is the interactive role Svelte accepts, and it fits: each handle has `tabindex=0`, `aria-orientation`, `aria-valuenow/min/max`, and an `onkeydown` (Arrow keys nudge by `RESIZE_STEP` = 16px, Home/End jump to min/max). The keyboard handlers share one `splitterKey()` helper in MainLayout. The composer's handle additionally clamps against measured geometry, mirroring native's `ChangesSidebar`: a wrapper around both tab panes carries `bind:clientHeight` (the Changes pane itself is `display: none` on History and would report zero), `commitMax` is that height less an 80px list floor, and both the drag and the rendered height obey it. The stored `leogit:commitHeight` is left alone, so a window that grows gives the height back without a fresh drag — while capping the *drag* too keeps the divider moving on the first pixel instead of spending an invisible surplus.
- **`use:autofocus`, never the `autofocus` attribute.** The attribute is flagged (`a11y_autofocus`) and is unreliable for inputs that mount inside `{#if}` blocks. The [autofocus action](apps/tauri-app/src/lib/actions/autofocus.ts) calls `node.focus()` on mount instead.
- **Autocorrect is disabled once, at the root.** `<html>` in [index.html](apps/tauri-app/index.html) carries `autocorrect="off" autocapitalize="off" spellcheck="false"`. All three are inheritable HTML attributes, so every descendant input/textarea/contenteditable inherits them — no field opts out individually, and WebKit's macOS autocorrect pills, inline predictions, and spell squiggles stay off app-wide. Only add these attributes to a specific field if it needs to *re-enable* the behavior.
- **The app hands a fix to its own terminal rather than running it.** `Terminal.svelte` exports `runCommand`, which writes `<command>\r` to the PTY and queues when the shell is still starting — so a caller need not know whether the panel is warm, since it may have been created by the very click calling it (`MainLayout.runInTerminal` starts and expands the panel, `await tick()`s for the mount, then hands the command over). The queue is flushed *after* the output listener is registered, or the command would run with its echo dropped into D-4's window and the user would see a bare prompt. This exists because the one command worth offering — `claude auth login` — opens a browser and then blocks on stdin for a pasted code: driving that ourselves would mean rebuilding a terminal beside the one we ship, and asking for an auth code in app chrome is a habit worth not teaching. Core supplies the command string (`ProviderStatus.fix_command`), the client supplies the shell, and the button spells the command out because the app is about to type into the user's shell.
- **A shortcut is either window-level or it doesn't exist.** Svelte treats `<div>` and `<form>` as non-interactive and warns on listeners attached to them, so a chord has only two homes: one interactive field, or a `window` `keydown` listener bound in `onMount`. The field is a trap for anything the user might want *before* clicking in — the composer's Cmd+Enter / Cmd+G lived there and were unreachable until you were already typing — so the window is the default and the composer's container stays a plain `role="form"` landmark. `MainLayout`'s single handler owns them all (Cmd+R, Cmd+L, Cmd+B, Escape, the terminal toggle, and now the composer's two), reaching the composer through `bind:this` and two exported functions that gate exactly as its buttons do. Ordering inside that handler is load-bearing: terminal-origin events return first (`utils/keyboard.ts`), then Escape, then the composer's chords — which sit deliberately *above* the "a field has focus, leave it alone" bail, since they are for the fields — and everything else below it. Cmd+P is the exception, still in [Header](apps/tauri-app/src/lib/components/Header.svelte) with the transfer state it acts on.
- **Searchable repo lists share one keyboard-nav helper** (and, for the two that search repo *paths*, one match rule — see *Repo search* above). The startup picker ([RepoPicker](apps/tauri-app/src/lib/views/RepoPicker.svelte)), header switcher ([RepoDropdown](apps/tauri-app/src/lib/views/RepoDropdown.svelte)), and Clone dialog ([CloneOverlay](apps/tauri-app/src/lib/views/CloneOverlay.svelte)) all let you type-then-arrow: ↑/↓ move a keyboard cursor (`activeIndex`, reset to the top match whenever the query changes) and Enter picks the highlighted row (opens it, or in Clone sets the clone target). The two reusable pieces live in [listNavigation.ts](apps/tauri-app/src/lib/actions/listNavigation.ts) — `nextActiveIndex()` (wrapping index math) and the `scrollIntoViewWhenActive` action (`block: 'nearest'`, so already-visible rows never jump). The active row shows a `--border-active` inset ring, distinct from hover/selected fills. MainLayout's global `keydown` never interferes because it early-returns when focus is in a field and only handles Escape + meta-combos. The native side keeps the same two pieces under the same names — `ListNavigation.nextIndex` and `scrollTo(_:anchor: nil)` — so an arrow key answers identically in both clients.
- **Both repo pickers read one set of label rules.** `repoLabel` / `repoFullLabel` / `repoSearchLabels` / `collidingRepoLabels` live in [repoIdentifiers.ts](apps/tauri-app/src/lib/stores/repoIdentifiers.ts), beside the cache they read, and the native `RepoIdentifierStore` carries the same four. They are what keeps a row's *displayed* label and its *searched* labels the same set: a row is found by what it shows, including the owner-qualified form, and the `owner/` prefix appears only where another row shares the name and an identifier exists to disambiguate with.
- **The Clone dialog list is one tab stop, not one-per-row.** Its repo rows are `role="option" tabindex="-1"` inside a `role="listbox" tabindex="0"` container, so Tab flows filter input → sort button → list → Local path → Browse → Cancel/Clone (rows are reached by arrows, not Tab). The filter input is a `role="combobox"` with `aria-controls`/`aria-activedescendant` pointing at the listbox and its active option, and `handleListKeyDown` is shared by the input and the listbox so arrows/Enter work from either.

## Notable invariants

These are easy to break and hard to debug; respect them when touching the relevant area.

- **Hunk lines include the `@@` header.** `hunks[i].lines[0]` is the hunk header itself. The flat line index used by `DiffSelection.diverging_lines` is `sum(prev_hunk.lines.length) + line_idx_in_current`, and this sum *includes* every header line. Both the Rust patch builder and the Svelte diff viewer rely on this.
- **File inclusion is derived from status, not stored.** It's recomputed on every status refresh from `present − excluded`. Never persist the inclusion set — persist the *exclusions* (and we don't even do that across sessions today). How long an exclusion outlives its path leaving the list still differs between the clients; the convergence is filed as CH-7/H-20 in the parity plan.
- **`git status` uses porcelain v2 `-z` (NUL-delimited).** Plain `--porcelain` will silently corrupt paths with spaces or unicode. If you change the args, make sure the parser stays NUL-aware.
- **The remote name is the NAME, not the URL.** `get_remote` returns the first line of `git remote` (typically `origin`), not the fetch URL, and **`None` when the repo has no remote**. It used to invent `"origin"` there, which made every "skip when there's no remote" guard unfireable: a doomed `git fetch` ran on every tick and its failures were read as the network being down, opening the connectivity breaker against every *other* repo. `RepoStatus.has_remote` remains the cheap signal (`get_status` computes it from the same `git remote` call it needs for the ahead/behind fallback), and it is one of the six fields core's `sync_proposal` reads to decide the header's face. The one place a remote may be named before one exists is a publish, which is what creates it — `git::DEFAULT_PUBLISH_REMOTE`, used by `gh_publish_repo` and nowhere else.
- **An automatic fetch uses the background budget.** `fetch(.., background: true)` runs under 8/8/12 s, the same as the badge sweep; a fetch the user asked for keeps 15/30/600 s. Nobody is waiting on the automatic one, and there is a single global network slot — an unreachable remote holding it for ten minutes stalls every other repo's refresh behind it.
- **Publishing uses `gh repo create`, not our own API.** `gh_publish_repo` shells out to `gh repo create <name> --source <repo_path> --remote origin --push [--private|--public] [--description ...]`, inheriting the user's `gh` auth. It's the one-shot equivalent of GitHub Desktop's "Publish Repository": creates the remote repo, adds `origin`, and pushes. `gh`'s stderr (missing auth, name collision) is surfaced verbatim to the error modal.
- **A repo path is whatever `paths::canonicalize` returns.** Discovery, `repo_root`, `init_repo` and `resolve_launch_target` must all produce the identical string for the same folder — they feed one de-dupe set, the `last_opened_repo` comparison, the persisted MRU that orders the switcher, and the `repoIdentifiers` / `repoSync` cache keys, so a path only one of them can produce shows up as a duplicate repo with no badges, sorted into the tail. Calling `fs::canonicalize` directly re-introduces the Windows verbatim prefix and breaks exactly that. Pinned by `repo_paths_are_ordinary_and_agree_across_producers`.
- **Terminal sessions die with the repo.** When `appState.repoPath` changes, `MainLayout`'s effect resets `terminalSessionId = 0`, which keys the `<Terminal>` component to unmount and call `close_terminal`. Don't try to "carry" a session across repos.
- **Diff content `\n` round-trip.** Empty diff lines come through as `""` from `String::split('\n')` but in real unified diff format are ` ` (a single space). `parse_diff` reconstructs the leading space so the patch builder generates valid unified diffs.
