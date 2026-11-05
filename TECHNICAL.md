# leogit: technical architecture

How the code is organized and the rules that hold it together. Behavior lives in [DESIGN.md](DESIGN.md), looks in [STYLE.md](STYLE.md), the cross-client contract in [FRONTEND.md](FRONTEND.md).

The pre-monorepo design is at tag `v0.1.32`. Recall it with `git worktree add ../leogit-classic v0.1.32`.

## Stack

| Layer | Choice | Note |
|---|---|---|
| Core | Rust 2024, tokio | Framework-free `leogit-core` |
| Windows, Linux shell | Tauri 2.11 | No Node at runtime |
| Web frontend | Svelte 5, Vite 8, TypeScript 7 | `typescript` aliases TS 6 for svelte-check |
| macOS client | SwiftUI, Swift 6 mode | macOS 26 target, Xcode 26 |
| Bridge | UniFFI 0.32 | Static lib |
| Xcode project | XcodeGen | `project.yml` is the truth |
| Highlighting | syntect 5.3, two-face 0.5 | Colors live in `--syn-*` |
| Terminal | portable-pty 0.9, xterm.js 6, SwiftTerm 1.20 | Core owns the PTY |
| HTTP | reqwest 0.13 | Ollama, update check |
| Discard | trash 5 | Recoverable |

## Repository layout

```
leogit/
├── justfile, Cargo.toml      # workspace; clippy pedantic lints
├── core/
│   ├── examples/bench.rs
│   └── src/
│       ├── git.rs            # status, log, branches, commit, sync, discard
│       ├── operation.rs      # merge/rebase/pick/revert in progress
│       ├── history_rewrite/  # preflight, cherry_pick, replay, lineage, squash, reorder, undo
│       ├── sync_ladder.rs, git_version.rs, exclusions.rs, repos.rs
│       ├── diff.rs, highlight.rs, render.rs
│       ├── config.rs, launch.rs, update.rs, progress.rs, events.rs
│       ├── ai.rs, commit_prompt.txt, gh.rs, os.rs
│       ├── terminal.rs, shell.rs
│       ├── process.rs, path_cache.rs, paths.rs, appimage.rs
│       └── test_support.rs
├── apps/
│   ├── tauri-app/
│   │   ├── src/              # App.svelte, app.css, lib/{api,actions,services,stores,utils,components,views}
│   │   ├── tests/            # pure helpers, node --test
│   │   └── src-tauri/src/    # main.rs, shims/, event_sink.rs, launch_glue.rs
│   └── swift-ui-app/
│       ├── project.yml, Resources/AppIcon.icon, scripts/build-rust.sh
│       ├── ffi/              # leogit-ffi; generated/ is gitignored
│       └── Sources/LeoGit/   # App, IPC/GitBridge, Stores, Screens, Services, Design
├── scripts/                  # install.sh, Python release tools
└── docs/plans/               # historical
```

### Core / host split

Core compiles without Tauri or UniFFI: plain `Result<T, String>` functions plus serializable types.

- A new Tauri command is four edits: `core/src/<module>.rs`, `shims/<module>.rs`, `main.rs`, `api/commands.ts`.
- Streaming goes through `events::EventSink`, taken by `pull`, `push`, `clone_repo`, `gh_clone` and `start_terminal`. Each host has a progress sink and a terminal sink.
- `render.rs` is web-only; native maps `highlight::tokenize_diff` tokens onto `AttributedString`.

### Swift host (UniFFI)

#### Bridge

- `leogit-ffi` is one `#[uniffi::export]` per core function. `#[uniffi::remote]` mirrors restate structs, so drift in core is a compile error in the bridge.
- No dead surface: `rename_branch`, `delete_remote_branch` and `get_ahead_behind` reach neither client; `check_auth` and `terminal_pty_info` stay Tauri-side.
- One non-mirror: `DiffPayload` has no HTML and carries `SbsPair` with `u32` indices.
- Every core call blocks. `GitBridge` wrappers are `@concurrent` (SE-0461), or a `nonisolated async` inherits the caller's executor and freezes the UI. Copy uses pure helpers, since it must answer inside its event.
- Network exports use `async_runtime = "tokio"`, because core's `spawn_blocking` panics without a runtime. `repo_sync_status`, `known_repos` and `gh_repo_list` hop to `spawn_blocking` to keep Swift's cooperative pool free.
- Progress ticks arrive on core's reader thread, so Swift hops to the main actor and drops stragglers by generation. A transfer ends when its `await` returns.
- `build-rust.sh` runs as a pre-build phase and asserts:
  - `module_name`, `--module-name` and `SWIFT_INCLUDE_PATHS` agree, or symbols just fail to resolve.
  - `--link-frameworks SystemConfiguration`, because reqwest reads proxy settings.
  - `SWIFT_DEFAULT_ACTOR_ISOLATION: nonisolated`, because a main-actor default breaks generated bindings.

#### Stores and races

- A blocking call can't be cancelled, so stores guard the write, not the task: writers compare the `openGeneration` captured before their first await. `loadRepoData` takes it from its caller.
- A superseded open returns `OpenOutcome.superseded`, not a failure, and callers don't read `repoPath` afterwards.
- `isBusy` is a depth count because `refresh()` nests inside `open()`. Anything gated on status awaits `awaitLoadSettled()` rather than reading nil.
- `refreshWorkingTree()` must not consume the `head_sha` edge: a moved sha hands over to the full reload, or History goes stale. It claims the load lock with no bar (`beginLoad(showsProgress:)`), or a poll tick lands a pre-action snapshot.
- `historyLoaded` is separate from `commits`, since empty means both "none" and "not read". `open()` clears commits as it publishes the path, so no stale sha reaches the new repo.
- One `RepositoryWriteGate` per window. `isBlocked` (anyone's write) disables controls; `isRunning` (this store's) drives spinners. Keep them apart.
- A refused claim is `OpOutcome.refusedBusy`: neither success nor error, so deliberately not a `Result`. `.disabled` is only a pre-await read; the claim is what's correct.
- Pull holds both slots, because a pull under `index.lock` fails its merge, and a slow commit under a pull loses `HEAD`'s lock. Automatic fetches deliberately have no "no write in flight" rule, since they only move remote-tracking refs.
- `AppConfigStore` is the one writer. It registers each patch synchronously, before any await, so order isn't the scheduler's. `generate` awaits `settle()` so it can't resolve the previous provider. Stores get it at construction, never assigned later.
- Settings controls patch only their own field. `seed` records through `patch(for:)`, so normalized values don't read as dirty. `flush()` on close writes every dirty field. An unreadable config shows the failure and no controls.
- Git's text goes in a sheet, because `.alert` reflows it into one unselectable block. `pollFailure` and `errorMessage` are two fields, so a dismissable notice can't hide a dead repo.
- `CommitStore` and `CloneStore` live on `ContentView`: tabs rebuild panes, and a sheet-owned store re-runs the 20 s `gh` query.
- `RepoIdentifierStore` stores `nil` with `updateValue`; subscript assignment removes the key and re-queues the lookup forever.

#### Scheduling

- Loops are `.task(id: repoPath)`, so a switch cancels them.
- `BackgroundSchedulingPolicy` exposes `canPollStatus`, `canAutoFetch`, `canRunRepoSweeps` and `canTickRelativeDates`. Sweeps re-ask before every repo. Wake-up runs on any rise in `wakefulness`.
- Occlusion comes from a zero-size window accessor, not `NSApp.keyWindow`, which is nil while inactive.
- `AppNapSuppressor` holds `beginActivity(.background)` while a repo is open, or App Nap coalesces `Task.sleep` and the ladder never fires.
- `silentFetch` returns `Bool?`; `nil` means no fetch ran, so only real attempts feed the breaker. A remote-less warm-up fetch would close the breaker for every other repo.

#### Diff pane

- `get_parsed_diff` paints first; `tokenize_diff` recolors after the debounce.
- Reloads never clear. Slow loads dim through a modifier, because a branch destroys `ScrollView` identity. Split and unified are branches inside one `ScrollView`, chosen by the loaded pairing, not the setting. Svelte follows both rules.
- `LoadKey` is path, target, `stat_stamp`, `xy`, `head_sha` and three read settings, not tab size. `head_sha` catches a `--mixed` reset, which leaves bytes and letters alone.
- `modelMoved` and `pairingMoved` compare separately, or a layout change drops color.
- Token indices are code points: use `unicodeScalars`, never `characters`.
- Scroll resets on a new `DiffIdentity`, set outside the equality skip because two files can parse equal.
- Copy is a range over the row model. A SwiftUI selection can't leave its `Text`, so multi-line copy goes through the gutter.

#### AppKit traps

- `NSToolbarView` eats right-clicks before `.contextMenu`. Use `.toolbarContextMenu`: one `NSEvent` monitor plus an anchor per chip, skipping sheets and modals itself. The anchor also sets `allowsDisplayModeCustomization = false`, because SwiftUI installs a new `NSToolbar` when Welcome is replaced.
- `focusedSceneValue` from a toolbar-hosted view never reaches the scene. Publish from window content.
- Menu key equivalents match before the responder chain, so ⌃\` lives on a menu item or SwiftTerm swallows it. Declare each chord in the menu-bar copy only.
- Toolbar items render only text and icon, so `↑N ↓N` is its own item.
- `Toggle(sources:isOn:)` is the only mixed checkbox.
- `ListSelection.activeID` derives the shown row in list order, never `Set.first`. Keeping a selection is an `onChange` write-back, since AppKit has already drawn the deselection.
- `RowResizeHandle` is `.focusable(interactions: .activate)`; plain `.focusable()` takes initial focus on a clean tree.
- `onKeyPress` beating `List` in reorder mode is measured, not documented. Re-check on OS bumps.

#### Native terminal

- Collapse pins the emulator's frame inside a clipping frame. A zero height reflows scrollback and sends `SIGWINCH`.
- Focus is a request replayed from `viewDidMoveToWindow`, since `makeFirstResponder` on a windowless view does nothing.
- A shell exit nulls the pid first, so unmount can't double-close.
- SwiftTerm reads options once, so scrollback goes through `TerminalOptions`.

## Process model

Tauri is two processes, Rust host and WebView, joined only by IPC. Native is one process with core linked in. No servers or sidecars.

### Startup PATH fix

- `process::fix_path_env` runs first in both hosts, before any other thread, as the only `set_var` caller (unsafe in edition 2024). Finder and `.desktop` launches otherwise get a minimal PATH.
- It reads `shell-path-cache.json`. `$SHELL -ilc` costs ~400 ms, and `-i` is required because `~/.zshrc` is where tools append.
- `path_cache.rs` keys on every startup file whether or not it exists, `/etc/paths.d`, version-manager state and env values, with a 7-day ceiling.
- `spawn_path_reprobe` refreshes later and only feeds the child hooks, never the process env.
- The probe reads `PATH=` from `env` between markers. Never `"$PATH"` (fish joins with spaces) or a bare echo (rc files print). It goes through `prepare_child`, or AppImage entries get baked in.

### Path normalisation

- `paths.rs` owns the form. **Never call `fs::canonicalize` elsewhere**: on Windows it returns `\\?\` paths that break tooltips, de-dupe and PowerShell prompts. `dunce` strips the prefix only when safe.
- `expand_tilde` asks the OS (`BaseDirs`), not `$HOME`, which Windows sets only under POSIX shells.
- Frontend `basename` splits on either separator. Don't use it on git's paths: git writes `/`, and a backslash can be in a filename.

### Command-line repo opening

- `launch.rs` resolves argv to `LaunchTarget { path, is_repo }`; a subdirectory opens its repo.
- Tauri cold start stashes the target for `take_pending_launch_target`. Warm start goes through `tauri-plugin-single-instance`, registered first, which emits `open-repo`.
- macOS declares `public.folder` (Viewer, Alternate), so `open -a <path> <dir>` reaches `application(_:open:)` cold and warm.
- `LaunchStore` holds `pending` and `latest`. `resolveLaunchRepo` claims `pending` itself, since `.onChange` can't see a value set before it was installed.
- `init_repo` is idempotent: a folder inside a repo returns that root.

### Repo-less phases

`RepoPicker`'s overlay is `absolute`, never `fixed`, or it covers the header controls that rescue an empty picker. `App.svelte` binds keys only outside `main`.

### Child-process preparation

Every spawn passes a `prepare_child*` hook, which also ticks `spawn_count()`.

- Windows: `CREATE_NO_WINDOW`, or the poll flashes a console. The PTY is exempt.
- AppImage: children lose everything under `$APPDIR`, derived from values, not a name list. `PATH` is filtered, never deleted. The app's own env stays, since bundled WebKitGTK needs it. Recomputed per spawn because `fix_path_env` changes PATH.

### Network resilience (offline / flaky)

- **Threads.** Subprocess commands are `#[tauri::command(async)]`, since plain commands run on the main thread. Anything that can block 100 ms or more is an `async fn` over `run_blocking`, or a long push starves the workers.
- **Time boxes.** `run_timed` is the chokepoint. `KillScope::Group` kills the whole group before reaping, since `ssh` inherits our pipes. `Child` is for OS launchers, because `xdg-open` runs the browser in-group. Readers get 2 s, then detach.
- **Budgets.** Connect/stall/kill: background 8/8/12 s, user 15/30/600 s. Plus `BatchMode=yes`, `ServerAliveInterval=10`, `ServerAliveCountMax=3`, `http.lowSpeedLimit=1000`.
- **Breaker.** It gates automatic fetches only, and only their results feed it. User actions always try and never report to it.
- **One transfer.** Poll and loops pause during a user transfer. Automatic fetches claim no slot.
- **Silent fetches.** One in flight per repo, because a ref-lock loser would be charged as a network failure. Catch-up and tier fetches within 60 s of the last success downgrade to a local recompute, never a skip. The auto-fetch tick and the user's Fetch and Pull ignore that window. A pull's fetch stamps it; a push doesn't.

### Transfer progress (`git-progress` events)

`progress.rs` has a step/weight table per op, and the checkout phase matches both `Updating files` and `Checking out files`. Unknown lines change text only; the fraction is monotonic. Background fetches emit nothing.

## IPC contract

The frontend never calls raw `invoke`; it uses typed namespaces in `lib/api/commands.ts` that match core modules.

## State management (frontend)

- `appState` is the phase machine, `repoState` the open repo, `config` the only config copy. `repoSync`'s equality guard compares every field, so add new ones there.
- Failure owners: `error` (modal), `notice` (dismissible), `pollError` (three straight failures, retired by a good read), and inline per-pane diff errors.
- `refreshStatus` takes `silent` and `background` separately, or `index.lock` races accuse a healthy repo.
- `statusLoaded` exists because `hasRemote` defaults to false.
- `repoWrite` is one write per window: `beginRepoWrite(kind)`, return if refused, release in `finally`.
- Status read ordering lives in `statusReadsIssued` and `newestStatusLanded`; natively `issueStatusRead` and `landStatusRead`.
- Escape is a registration: `use:dismissOnEscape`, a LIFO in `overlayStack.ts`. `overlayDepth` gates app chords.
- `SeamlessDiffPane` dims and never unmounts. `showSplit` derives from `sbs_pairs`, not the prop.

### Selection bookkeeping

Inclusion is `present − userDeselected`, rebuilt each refresh, never persisted.

### Row selection (Tauri)

`utils/listSelection.ts` holds pure rules keyed by path or sha, never index. Functions return the same object when nothing changed, so `FileList` uses `$state.raw` and `selectCommits` tests identity before touching the store.

### Pane empty states

`PaneEmptyState` copies `ContentUnavailableView` metrics read from the SwiftUI binary, so they can drift. `.diff-empty-hold` holds the pane during the slow-load threshold. Natively, `ToolbarStatus` keeps the old face across a switch, but anything actionable reads live status.

### Large changesets

- File rows and commit rows virtualize. `--row-height` comes from the same constant.
- `CommitList` uses `ResizeObserver`, since a hidden pane measures 0.
- `commits[0]` is always `HEAD`. Paging appends and dedupes by sha; only `log.resetSeq` scrolls to the top.

### Background cadence

The schedule itself is in [FRONTEND.md](FRONTEND.md) §6.1.

- `pacedLoop.ts` is a self-scheduling `setTimeout`, so runs can't overlap and `dueAt = Infinity` parks it.
- Every status write goes through one `refreshStatus`. An identical silent tick publishes nothing, and `stat_stamp` is what makes edits visible, since porcelain v2 has no worktree hash.
- The wake-up deliberately doesn't re-read the open diff; the per-file key does it.
- `isTextInputFocused` asks at the tick and includes `document.hasFocus()`, because `activeElement` survives window blur.
- Wake-up runs `refreshConfig` first, since `config.toml` is shared.
- Discovery re-walks when a list opens or scan settings change, with one pass in flight.

## Git layer

Commands use `git_cmd` or `git_net_cmd`: explicit cwd, `TERM=dumb`, `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0` (or a poll's `diff` holds `index.lock` against a commit), `-c core.quotepath=false`.

Judge outcomes by repository state, not git's translated stderr.

### Path quoting

- `-z` paths are raw; printed paths are C-quoted. `core.quotepath=false` is on both builders, because `pull` prints names into refusals. `"`, `\`, TAB and LF stay quoted anyway, so `git::unquote_path` decodes every printed path.
- `strip_path_prefix`: split on the real TAB, decode, then strip `a/` or `b/`, which sits inside the quotes.
- `build_patch` replays `file_header` verbatim.

### Status parsing

- `status --untracked-files=all --branch --porcelain=v2 -z`. Everything is NUL-terminated, headers included. Type-2 rows take two segments. Plain `--porcelain` corrupts paths.
- The poll is one subprocess. `has_remote`, `has_commits` and `operation` come from disk, falling back to a spawn for shapes files can't answer (`GIT_CONFIG*`, `include`, `extensions.worktreeConfig`, reftable, a global remote). Tests assert which path answered.
- `(detached)` sets `detached` and leaves `branch` empty; `head_sha` is empty when unborn.
- With no upstream, `remote_tracking_ahead_behind` measures against `fallback_remote`, leaving `has_upstream` false.
- `repo_sync_status` uses `--untracked-files=normal` and, after a fetch, measures the remote it fetched. The active repo's dirty dot comes from `status.files`.
- `unpushed_shas` falls back to `rev-list HEAD --not --remotes`: it may under-mark, never a false arrow.
- `embedded` is an untracked entry with a trailing slash. `submodule_dirty` is `S` with `c == '.'` and `m` or `u` set; `git add` would no-op, so the row isn't selectable.

Operation probe order, first match wins:

| Probe | Answer | Why here |
|---|---|---|
| `rebase-merge/` | Rebase | a stopped `--rebase-merges` also has `MERGE_HEAD` |
| `rebase-apply/` | Rebase, or none with `applying` | that's `git am` |
| `sequencer/todo` first word | Revert or CherryPick | shared sequencer; the todo outlives `CHERRY_PICK_HEAD` |
| `REVERT_HEAD`, `CHERRY_PICK_HEAD` | Revert, CherryPick | single commits have no sequencer |
| `MERGE_HEAD` | Merge | nothing else can also be |

`AUTO_MERGE` is never a signal: a `stash pop` conflict leaves it.

### Continue and abort

`continue_operation`, with both refusals before the first write:

1. `--literal-pathspecs diff --check` on unmerged paths, 500 per batch. Only `<<<<<<<` and `>>>>>>>` count, since `=======` is also a Markdown underline.
2. For a rebase, refuse unstaged edits, because git's refusal names the wrong cause.
3. Stage exactly `ls_files_unmerged`, never `add -u`.
4. `<op> --continue` under `GIT_EDITOR=:`, or `--skip` when a pick's index equals `HEAD`.

- A rebase in that position drops the commit silently; `rebase_stopped_on_a_pick` lets `skipped` report it.
- `started_at` reads `orig-head` or `sequencer/head` before running, else `HEAD`. Never `ORIG_HEAD`.
- Core never resets over commits the user made.
- `git_add` is literal (`--literal-pathspecs`), or `weird[1].txt` globs.

### History rewrites

- Floor: git 2.45 for `cherry-pick --empty=keep`, with no degraded mode.
- Preflight names up to ten tracked changes. Untracked files pass.
- Cherry-pick passes `-m 1`, so a merge commit can be picked against its first parent.
- Replays refuse a shallow boundary because that's where `rebase --root` succeeds and disconnects history. `records_a_parent` reads the commit object, since the walk hides those parents.
- Cherry-pick: `Ok(success)`, `Ok(!success)` only for a conflict, or `Err` with the repo restored. `git switch --no-guess` is judged by where `HEAD` lands, because a failing `post-checkout` exits non-zero after switching.
- A stop is a conflict only with unmerged paths. A dead signer or failing hook has nothing to resolve, so core aborts, then switches back, in that order. Never `--quit`.
- Replays are a todo this app writes, via `GIT_SEQUENCE_EDITOR`, with paths in env vars and a unique tempdir: `git rebase -i --no-autostash --no-update-refs --no-rebase-merges --no-autosquash --reschedule-failed-exec --empty=keep`. Full ids, no comments.
- Squash's message lives at `rebase-merge/leogit-message`, so git owns its lifetime. The `exec git commit --amend --no-verify --cleanup=whitespace` sits right after the last `fixup`; at the end it would reword the wrong commit.
- `Lineage::of` finds the oldest with `merge-base --octopus` and ignores input order.
- Reorder judges a no-op by the resulting order and runs no git for it, since even a null rebase writes reflog entries.
- Undo requires the branch tip to equal `after_sha`. Checked out: `update-index --refresh`, then `reset --keep`. Not `--hard` (deletes untracked), `--merge` (drops staged) or `switch -C` (hook exit codes lie). The refresh matters because `GIT_OPTIONAL_LOCKS=0` leaves stat data stale.
- Not checked out: `branch -f`, never `update-ref`, which moves a branch another worktree holds.
- A half-failed reset is put back with `read-tree -m -u <before> <after>`.
- Undo names moved submodules instead of updating them, since `git submodule update` may hit the network. It probes where it came to rest (`diff-index --raw -z`, both modes `160000`).
- `UndoOffer` and `OpenAction` are client memory, set after the reload because their rules read status. Continue remembers the open action before asking core, since a poll can forget it mid-flight. Only cherry-pick is held to its branch; a stopped rebase reports `detached`.

### Sync ladder

- `sync_ladder::propose` is total over the status. It rides `RepoStatus` to save a crossing per tick.
- `upstream_was_rewritten_away` runs only on the diverged rung, from the reflog (`rev-list -g`, then `rev-list -1 --stdin`). Every doubt is `Pull`. Never cached, because `reflog expire` changes the answer.
- Force push is `--force-with-lease=refs/heads/<b>:<sha>`, pinned after `reflog_holds`. A bare lease compares against a ref automatic fetches keep moving. Not `--force-if-includes`: it misreads a fresh clone's missing reflog.

### Commit detail

`git log <sha> -1 --first-parent --format= --raw --numstat --root --no-color -z`. `--name-status` doesn't combine with `--numstat`. Sections are told apart by shape. `--first-parent` shows merges; `--root` beats `log.showRoot=false`.

### Diff read + parse

- Hunks store `@@` as `lines[0]`.
- `WhitespaceOnly` compares parses, since a rename header isn't empty.
- Size guard: clients remember which diff was revealed, not per request.
- A failed read is `Err`, never an `EmptyDiffReason`.
- `copy_text` clamps indices and drops `\ No newline`, which has no row.

### Patch generation

`build_patch`: unselected adds drop, unselected deletes become context, `@@` recomputed. `git apply --unidiff-zero --whitespace=nowarn`, never `--reject`, which hides failures in `.rej` files.

### Commit pipeline

1. `reset -- .`, because `reset HEAD` fails when unborn.
2. Removals via `update-index --force-remove`; additions via `git add`, because `update-index --add` skips embedded repos.
3. `git diff --cached --quiet` checks the index.
4. `git commit -F -` on stdin.

### Discard & ignore

`classify_discard` uses HEAD membership (`ls-tree -r -z`), not status letters, for both dialog and action. In HEAD: `checkout HEAD --`. Otherwise: OS trash plus `reset --`. `ignore_paths` escapes `[ ] ! * # ?`.

### Log parsing

`%x01` fields, `%x00` records, `--date=raw`. Unborn HEAD returns an empty log, and diffs use `EMPTY_TREE_SHA`. `switch_branch` routes `origin/x` through `checkout -b --track`, or it detaches.

### Discovery

`discover_repos` de-dupes roots by file identity (`~/Dev` and `~/dev` on a case-insensitive volume) and stats only symlinks. `resolve_scan_paths` owns the defaults, so what's shown is what's searched. `known_repos` unions the walk with the MRU.

`repos::match_repo` ranks exact, prefix, substring, initials, subsequence, path substring. Path matching is contiguous and trimmed below the deepest root, or `llm` matches every shared ancestry.

## AI layer

- `commit_prompt.txt` is a plain file so outside tools send the same prompt.
- Claude: check `is_error`, because the CLI exits 0 on failures. `CLAUDE_CODE_MAX_RETRIES=2`, or retries outlast our timeout.
- Ollama: `num_ctx` from `ollama_num_ctx`, about 2.5 bytes per token plus 4096, power of two, 4096 to 32768. Longer prompts lose their start silently.
- `provider_config` resolves per generate, never cached.
- `check_provider_status` counts unparseable answers as ready. `provider_status_from_failure` catches expired sessions, which still read as signed in, and only ever raises a block.

## Terminal layer

- The emitter thread owns teardown: flush, remove, `child.wait()`, emit `TerminalClosed`. So the last output precedes close and no zombies stay. `close_terminal` kills but doesn't remove.
- `child.kill()` (SIGHUP, grace, SIGKILL), never `clone_killer()`, which only sends SIGHUP.
- `resize_terminal` ignores grids under 2×2, which reflow scrollback and send a false `SIGWINCH`. Both clients debounce 80 ms.
- `session_for` drops the map lock before touching a session.

### Output coalescing and flow control

`sync_channel` of 64 chunks at 4 KiB; a full queue blocks the reader, which makes the shell wait. A delivery takes what's queued, up to 256 KiB or 8 ms, and goes immediately when alone, so keystrokes add no latency.

### Session transport (Tauri)

- Output uses a `tauri::ipc::Channel` passed to `start_terminal`, not the event bus, which drops events before `listen()`.
- `write_terminal` is deliberately sync: IPC arrival order is keystroke order. The child has its own lock, so a write mid-close doesn't wait on the ~250 ms kill.

### Key ownership between the app and the shell (Tauri)

While the terminal has focus, the shell gets everything except the toggle. `attachCustomKeyEventHandler` declines only that chord. Window handlers check `isFromTerminal` (`.xterm` ancestor), because xterm's input is a textarea. Order: terminal origin, in-field chords, field bail, the rest. The toggle is `Ctrl` only, since ⌘\` is macOS's.

### Pointer and clipboard (Tauri)

`WebLinksAddon` gates on the modifier and opens through `os::open_url`. OSC 52 reads are swallowed, since answering would type the clipboard into the TTY. Writes use the plugin, because WebKit needs a user gesture.

### Child environment: do not forward the parent env

`CommandBuilder::new` already builds the right environment. Never copy `std::env::vars()`: on Windows a Git Bash launch has an MSYS `PATH` and an Explorer launch a stale one. `session_env` adds only `TERM`, `COLORTERM` and, for Git Bash, `CHERE_INVOKING=1`.

### UTF-8 across read boundaries

`Utf8Decoder` holds up to 3 trailing bytes, because `from_utf8_lossy` per chunk leaves permanent U+FFFD.

### Shell discovery

- `shell.rs` probes, so the picker can't offer a shell that fails. Git Bash starts with `--login -i`.
- `terminal_pty_info` must run before xterm is constructed; ConPTY on build 21376+ enables reflow.
- Refocus reads `container.contains(activeElement)` at that moment rather than latching.

## GitHub layer

`gh.rs` shells out with `run_timed`: 20 s metadata, 600 s transfers, stderr verbatim. Deliberately no pull-request surface. `check_auth` stays wired but unused, since sign-in happens outside the app and a launch-time read goes stale.

## OS integration layer

`os.rs` hands off with a 15 s cap. A completed run is success whatever the exit code, because `explorer` returns non-zero on success. `open_url` allows only `https` and rejects whitespace, ``&^<>|"'` `` and `%`, which are syntax to `cmd`.

## Config & persistence

`LEOGIT_CONFIG_DIR` overrides the config dir for development. Files: `config.toml`, `repos-state.json`, `shell-path-cache.json`, `<file>.lock` sidecars.

- `write_atomically`: temp file, then rename. Only `config.toml` fsyncs, since `F_FULLFSYNC` per repo switch is too slow.
- Two locks: a process mutex and a `File::lock` sidecar, never the renamed file. Sidecars try ~2 s, then proceed unlocked with a log line.
- New `Config` fields need `serde(default)`; unknown keys are ignored.
- An unparseable file moves to `config.toml.corrupt-*` and defaults are written; an unreadable one errors.
- Blank scan paths drop, since one would walk all of home.
- `record_recent_repo` writes MRU (cap 50) and `last_opened_repo` in one go.

## Tauri capabilities

`core:*` defaults plus `core:window:allow-set-title`, `dialog:allow-open`, `clipboard-manager:allow-write-text`. No filesystem, shell or HTTP plugins reach the WebView. Clipboard read is withheld on purpose.

## Build / dev

You need Rust 1.91+, Node.js 20.19+ or 22.12+, and `pnpm`. The macOS client also needs Xcode, `brew install xcodegen`, and once, `xcodebuild -downloadComponent MetalToolchain`.

```bash
just install        # pnpm install in apps/tauri-app
just dev            # dev build with hot reload
just build          # debug bundle
just check          # svelte-check + cargo check --workspace
just test           # cargo test --workspace
just lint           # clippy pedantic
just bench <repo>   # time core git operations against a real repo
just mac-run        # build and launch the macOS app (--no-build to relaunch)
just bundle         # this platform's release bundle
just install-local  # install a Release build from this tree
just release        # publish a GitHub release; pass x.y.z to bump first
```

On Windows, `just` needs the `sh` from [Git for Windows](https://git-scm.com/download/win), so add `C:\Program Files\Git\usr\bin` to your `PATH`. The `bash` in `System32` is the WSL launcher, not what you want.

- `just bench` warms up once, then times three calls and counts spawns. Measure I/O changes with it, before and after.
- `pnpm check` is authoritative; `pnpm run check:native` is the fast TS 7 pass and can't read `.svelte`.
- `test_support.rs` isolates git config, because a global `core.hooksPath` disarms test hooks.
- `just check` doesn't build the macOS app.

### App icon (native client)

`Resources/AppIcon.icon` sits outside any catalog. Two silent failures: without `options.fileTypes.icon.file: true`, XcodeGen walks it as loose files; `ASSETCATALOG_COMPILER_APPICON_NAME` must equal `AppIcon`.

### Release pipeline (`scripts/`)

- One tag, one artifact per OS: macOS ships native, Linux and Windows ship Tauri (`RELEASE_CLIENT`). Required, because `update::artifact_name` only knows `cfg!(target_os)`.
- `deploy_release.py` runs once per OS against one tag. It refuses a dirty tree, disagreeing versions, or a version older than the live release. Re-running a version is the supported retry.
- Subprocesses resolve with `shutil.which`, because `CreateProcess` can't find npm's `.cmd` shims.
- macOS: clean `Release` xcodebuild, since incremental builds break the seal. Ad-hoc signed, per-arch because `libleogit_ffi.a` is one triple.
- Windows wipes `bundle/nsis` first, since Tauri never prunes it.
- `tauri.conf.json` is the version source; `_version.py` moves the rest. `CFBundleShortVersionString` must be declared in `project.yml`, or it's `1.0`. `leogit-core` and `leogit-ffi` stay at `0.1.0`.
- Platform naming exists three times: `install.sh`, `_common.py`, `artifact_name` (golden test).
- macOS `leogit()` is `open -a "<path>" "$dir"`: a path, since a bundle id may resolve to a Debug build, and not `--args`, which can't reach a running app.
- The Linux wrapper sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` per launch if `/dev/nvidia0` exists.
- Linux builds need `/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders` to exist, plus `NO_STRIP=true` and `APPIMAGE_EXTRACT_AND_RUN=1`.
- `_drop_bundled_gio_module()` removes `usr/lib/gio/`, because the half-bundled `libgiognutls.so` segfaults `ld.so` at window creation.

### In-app update check

- The running version is a parameter, because `CARGO_PKG_VERSION` inside core is core's own `0.1.0`.
- Parsing is strict: exactly three numeric parts, else no update.
- It requires the exact artifact name, because releases fill one platform at a time.
- `Err` is a retry signal, never shown.
- `LEOGIT_FAKE_UPDATE=<ver>` works in debug builds.

### Diff syntax highlighting

- Tokenize the file, never the diff: syntect is stateful. `Delete` rows read the old blob, the rest the new, with separate `ParseState`s.
- `scope_to_class` walks leaf-first with punctuation transparent, so `//` inherits `Comment`.
- Only `markup.raw.inline` maps to `Raw`, so fences never go flat.
- `single_tilde_strikes` undoes strikethrough from one tilde, because the compiled grammar is wrong there.
- Fences resolve by info string and run their own `ParseState` beside Markdown's.
- Lines over 1,024 parse but don't record; files over 20,000 lines bail before reading blobs.
- `highlight_diff` must stay `(async)`, or whole-blob tokenizing stalls the main thread.
- `FileDiff` stays lean because it round-trips over IPC. `build_sbs_pairs` is the only pairing.

## Accessibility patterns

- Overlays close on a backdrop target check, not `stopPropagation`.
- Resize handles are `role="slider"`, since Svelte flags a focusable separator.
- `use:autofocus`, never the attribute.
- `autocorrect`, `autocapitalize` and `spellcheck` are off once, on `<html>`.
- Shortcuts live on `window`, since a field can't catch them before focus.
- `runCommand` queues until the shell is up and flushes after output is attached.

## Notable invariants

- Flat line indices include every `@@` row. Patch builder and viewers rely on it.
- `parse_diff` restores the leading space on empty lines so patches stay valid.
- The remote is git's answer, resolved at call time, never cached, and never the first name `git remote` prints, since that list is alphabetical.
  - `get_tracking_remote` (automatic fetches, badge fetch, Pull): `branch.<name>.remote`, else `origin`, else first.
  - `get_push_remote` (Push, Publish Branch, force push): `pushRemote`, then `remote.pushDefault`, then tracking.
  - The user's Fetch is `fetch_all`, reaching every remote.
  - No remote returns `None`, never an invented `origin`, because doomed fetches open the breaker for other repos. Only a publish names one, via `DEFAULT_PUBLISH_REMOTE`.
  - A branch pointing at a renamed or deleted remote falls back, because the dead name would fail every automatic fetch.
- Automatic fetches use the background budget, or one dead remote holds up everything.
- Deliberately not in core: per-keystroke `$derived`s and scroll-frame logic (fuzzy matchers, arrow-key math, relative dates, `PathText` measurement), because an async IPC call visibly lags.
- A repo path is whatever `paths::canonicalize` returns. Every producer must agree, since the string keys de-dupe, MRU and caches.
- Terminal sessions die with the repo.
