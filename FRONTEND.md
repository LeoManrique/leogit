# LeoGit Frontend Contract

What the two clients share: commands, events, DTOs, and behavior that must match. Both embed the same `leogit-core`.

- **SwiftUI** on macOS, over UniFFI (`apps/swift-ui-app/ffi`).
- **Tauri + Svelte** on Windows and Linux, over `invoke` (`apps/tauri-app/src-tauri/src/shims`).

## 1. Maintenance charter (lockstep mandate)

Change this doc with the surface, then bring both clients into line. Commands go in §3, events in §4, DTOs in §5, policy in §6, deliberate differences in §8. Core computes, clients render and orchestrate: a client never re-derives git state core already returns.

## 2. System context & architecture

- Client to core is request/response only. Tauri rejects with a string, Swift throws `GitError.failed(message)`, and the text is core's either way.
- Core to client is push only (§4).
- Durable state (config, MRU, PTY sessions) lives in core. Clients hold re-derivable view state.

## 3. Command surface (78)

Each host exposes 78: 70 shared, 8 its own. A gap between hosts is fine if it's recorded here.

- **Tauri only:** `check_auth`, `generate_patch`, `generate_inverse_patch`, `get_repo_identifier`, `highlight_diff`, `is_git_repo`, `take_pending_launch_target`, `terminal_pty_info`.
- **Native only:** `core_version`, `fix_path_env`, `spawn_path_reprobe`, `repo_display_name`, `repo_identifier` (async counterpart of `get_repo_identifier`), `resolve_launch_target`, `resolve_repo_root` (for `is_git_repo`), `tokenize_diff` (for `highlight_diff`).
- **No Svelte caller:** `copy_diff_text`, `generate_patch`, `generate_inverse_patch`, reserved for multi-line copy and per-line staging. `check_auth` too: a gh surface asks at point of use, since a launch probe goes stale. Any other unwired command gets deleted, wrapper and registration together.

Argument keys are camelCase on the Tauri wire. Network calls are async on the bridge and can be slow: sync, transfer, `gh_*`, `generate_commit_message`, `check_for_update`.

### 3.1 Config & state (6)
`load_config`, `patch_config`, `config_bounds`, `load_state`, `patch_state`, `record_recent_repo`

`patch_config` is the only config writer. It names only the fields a surface owns, and core merges, normalizes and writes under one lock, so one client can't revert the other. Numeric controls take their range from `config_bounds`.

### 3.2 Launch (1)
`take_pending_launch_target`

One rule, `core::launch::resolve_launch_target`. Tauri runs it before the window exists and parks the answer. Native skips that global, which a UI can't observe, and feeds the resolver argv or the `application(_:open:)` path.

### 3.3 Git: status, diff, log (5)
`get_status`, `file_status_styles`, `get_selected_diff`, `get_log`, `get_commit_detail`

Deliberately no `get_head_sha`, it's on `RepoStatus`. `get_selected_diff` is AI input, never parsed.

### 3.4 Git: branches (5)
`list_branches`, `create_branch`, `switch_branch`, `checkout_commit`, `delete_branch`

`switch_branch` turns a remote row into a local tracking branch.

### 3.5 Git: commit & staging (9)
`commit`, `undo_last_commit`, `classify_discard`, `discard_files`, `append_to_gitignore`, `ignore_paths`, `format_commit_message`, `reconcile_exclusions`, `effective_scan_paths`

### 3.6 Git: sync & remote (8)
`repo_sync_status`, `fetch`, `fetch_all`, `pull`, `push`, `get_tracking_remote`, `get_push_remote`, `get_repo_identifier`

### 3.7 Git: merge, operation in progress, history actions (13)
`merge_branch`, `merge_squash`, `commit_squash_merge`, `count_commits_to_merge`, `continue_operation`, `abort_operation`, `rewrite_preflight`, `reorder_preflight`, `cherry_pick_commits`, `squash_draft`, `squash_commits`, `reorder_commits`, `undo_operation`

- `count_commits_to_merge` counts what its argument holds that HEAD doesn't, despite the name `targetBranch`.
- `continue_operation` stages only unmerged paths and keeps the replayed message. It refuses, touching nothing, while a file holds conflict markers or a rebase has other unstaged tracked edits. The next conflict is data. A pick resolved to nothing sets `skipped`. `started_at` is where git's own state says the operation began, read before the step.
- `abort_operation`'s string means it worked but wasn't a full rewind: a notice, not a failure.
- `rewrite_preflight` refuses as data (`blocked`): old git, operation in progress, detached or unborn HEAD, tracked changes. With `replayedFrom` (oldest replayed commit, null for cherry-pick), also a merge among those commits or a shallow boundary. `rewrites_pushed` means the next push is a force push. `reorder_preflight` is the same with the destination known.
- History actions preflight themselves, so a client's call is a courtesy. `squash_draft` is the dialog's opening summary, description and co-authors, so both clients match.

Three outcomes, which a client must tell apart:

- **`success: true`**: landed. `selection` lists the commits to select, `undo` the `UndoPoint`.
- **`success: false`**: stopped on a conflict and only that. `start` is set here and nowhere else.
- **error**: anything else (refusal, signer, hook). Core has aborted and put the branch back, or says where the user was left.

Per action:

- `cherry_pick_commits`: shas newest first, onto local `targetBranch`, which ends up checked out. `return_branch` is the source.
- `squash_commits`: folds any current-branch shas, gaps allowed, into the oldest, keeping its author and date. On a conflict the message is already inside the rebase, so no client holds it.
- `reorder_commits`: moves `shas` as one block to just under `beforeSha` (null is the tip). A no-op succeeds with `undo` null.

`undo_operation` takes the point as it came, or a `start` completed with `after_sha`. It checks the branch, not HEAD, and needs no confirmation.

- `undone: true`: `message` is what didn't follow (a refused checkout, a submodule left behind). Core never runs `git submodule update`, it may hit the network.
- `undone: false`: expired for good, drop the offer.
- error: may clear later, the offer stands.

### 3.8 Git: discovery, init, clone (7)
`known_repos`, `filter_repos`, `derive_clone_target`, `clone_target_path`, `is_git_repo`, `init_repo`, `clone_repo`

`filter_repos` is one crossing per keystroke, not per row. A null `derive_clone_target` disables Clone. `init_repo` returns the enclosing root rather than nesting.

### 3.9 OS shell (3)
`reveal_path`, `open_path`, `open_url`

### 3.10 Diff read + parse / patch (5)
`get_parsed_diff`, `get_parsed_commit_diff`, `copy_diff_text`, `generate_patch`, `generate_inverse_patch`

A rejection is a failure, `empty_reason` is nothing to show. Never merge them. `DiffOptions` says what to build: `html` for the WebView only, `side_by_side` only while that layout is showing.

### 3.11 Highlight (1)
`highlight_diff`

### 3.12 GitHub via `gh` (4)
`check_auth`, `gh_repo_list`, `gh_clone`, `gh_publish_repo`

### 3.13 AI (4)
`load_ai_config`, `generate_commit_message`, `check_provider_status`, `provider_status_from_failure`

Read `load_ai_config` fresh before every generate. Never splice a picker value over a loaded config.

### 3.14 Terminal (6)
`terminal_pty_info`, `start_terminal`, `write_terminal`, `resize_terminal`, `close_terminal`, `list_shells`

### 3.15 Update (1)
`check_for_update`

## 4. Backend→frontend surface

| Stream | Tauri | Native |
|---|---|---|
| Transfer progress | `git-progress` broadcast: `op`, `path`, `percent`, `text` | `SyncProgressListener` argument: `percent`, `text` |
| Launch target | `open-repo` broadcast | `application(_:open:)` |
| Terminal output, closed | `Channel` argument, tagged on `event` | `TerminalEventListener` argument |

The terminal stream is an argument to `start_terminal`, not a subscription, because the shell prints before the command returns. It's ordered and lossless. `closed` can land before `start_terminal` returns, so don't adopt the handle afterwards. A clean exit closes the panel, otherwise print `[Process exited with code N]` and keep it up.

### 4.1 Progress reliability convention
Progress is advisory and may drop. The command's result decides success.

## 5. Data model

Rust structs in `core` are the source. TypeScript hand-mirrors them in `lib/api/commands.ts`. Swift gets generated types through `#[uniffi::remote]`, where drift is a compile error.

### 5.1 Enums with non-default serialization
Fields are snake_case. Unit enums serialize as PascalCase variant names (`FileStatus`, `LineType`, `EmptyDiffReason`, `DiffSizeReason`, `OperationInProgress`, `SyncProposal`). `BlobSource` is tagged, with camelCase fields: `{kind:'workingTree', repoPath}` or `{kind:'commit', repoPath, sha}`.

### 5.2 Structures by domain
- `stat_stamp` is opaque. Compare it, never parse it.
- Status letters and labels come from `file_status_styles`, fetched once.
- Each client maps `SyncProposal` in one exhaustive table (`SYNC_FACES`, `SyncFace.swift`), so a new rung fails the build.
- `Config`: nothing scalar may follow the `claude` and `ollama` tables, a TOML table swallows later keys.
- **Blank means absent.** `Config::normalized()` turns blank-after-trim into absent on every read and write. `""` in a `ConfigPatch` clears a field. Optional numbers clamp to the nearest bound.

## 6. Behavioral contract (must be identical across frontends)

### 6.1 Status polling
- Cadence: **2 s** frontmost, **10 s** unfocused, **30 s** hidden. Auto-fetch runs on `fetch_interval_ms`, ×3 while hidden. Neither stops for focus or visibility, only badge sweeps do.
- All polling pauses during a network op. Resync on wake.
- When `head_sha` moves, reload the log and branch list. Read it from the status reply, never a second call.
- Publish reads in asked order and drop any that lands after a later-asked one. The poll doesn't pause for writes, so a stale tick could undo a write's reload.
- Nothing changed publishes nothing, comparing the whole `RepoStatus`.
- Re-read the open diff only when its `stat_stamp`, its `xy` or `head_sha` moved. A `--mixed` reset moves only `head_sha`.
- Hold auto-fetch while text has the keyboard, terminal included, asked at tick time since a latched flag strands. It reaches the tracking remote only, skips when `has_remote` is known false, and only a fetch that ran reports to the breaker. The first one gets a per-session 0 to 30 s skew.
- Background refresh failures are swallowed. Three in a row, per repo, from timers and resyncs only, raise a non-blocking banner that any good read clears.

### 6.2 Network-op mutual exclusion and sync
- One user transfer at a time. Automatic fetches claim no slot.
- The ladder is core's, on `RepoStatus.proposal`: detached → publish repository → publish branch → force push → pull → push → fetch. Both bind it to ⌘/Ctrl+P.
- Force Push is proposed only when the reflog shows the branch diverged from its own past and the push lands on the measured ref. Every doubt reads as Pull. A diverged branch offers both.
- The lease is pinned to a commit, never bare. The confirmation names `status.upstream`, never a composed `remote/branch`.
- Fetch is always reachable and uses `fetch_all`.
- No Refresh button. ⌘/Ctrl+R reloads status, history and branches, held back during a transfer.
- After any transfer, reload status and the log.

### 6.3 Seamless diff loads
- Drop stale responses and keep the previous diff on screen. Past **150 ms** (`SLOW_DIFF_THRESHOLD_MS`, `DiffStore.slowLoadThreshold`) dim it under a spinner, never unmount.
- A re-read of the same diff keeps scroll, a different one resets. Identity is paths plus commit, and the jump happens as the replacement lands.
- `+N`/`−N` render only when non-zero, and not while the old payload shows.
- A failed load clears the pane and says so inline, never in a window-taking modal. Re-selecting retries.
- A dirty submodule is decided before the read, so no `git diff` spawns.

### 6.4 File selection semantics
- Every committable file is included unless opted out.
- An opt-out survives its path leaving the list for **30 s and two consecutive misses** (`reconcile_exclusions`): wall clock because cadence varies, two misses because one 30 s read can land mid-rewrite. A reappearance resets both.
- Space and select-all share one rule: any excluded → include all, else exclude all. Select-all is tri-state.
- List rules live in one module per client (`utils/listSelection.ts`, `Services/ListSelection.swift`), never copied:
  - Selection is a set of stable ids, never indices.
  - A list with rows keeps one selected.
  - A reload prunes, and re-seats only when none survive.
  - Context menus act in list order. A right-click outside the selection re-selects first.

### 6.5 Connectivity circuit-breaker
Two consecutive failures open it, backing off from 30 s, doubling to 5 min, then one probe. It gates background git. Recover on reconnect.

### 6.6 Tiered background refresh
Tiers at 2, 5 and 10 min, over the next 4, 5 and 10 most recent repos, in one loop with three deadlines, because separate timers align on common multiples. Sequential, with policy re-checked before every repo. The active repo is in no tier.

### 6.7 Commit composer
- Generate gates on ready, not installed. `check_provider_status` probes ahead. Only `provider_status_from_failure` catches an expired session, whose credentials stay on disk.
- Keep only the blocked case, tagged with its provider. An unanswered probe leaves Generate enabled, a throwing one changes nothing, and an unrecognized failure can raise a block but never clear it. Re-ask on provider change and on focus return.
- Composer chords are window-wide, inert under dialogs, and gate exactly as their buttons do.

### 6.8 History
Flat, rooted at HEAD, append-only, deduped by sha. Page size may differ (50 Tauri, 100 native). Refetch only when HEAD moves, capped at **500** (`historyRefreshCap`), then scroll to row 0.

### 6.9 Repo pickers
- Both picker locations are one surface.
- Auto-open at launch only: `leogit <dir>`, a valid `last_opened_repo`, the sole discovered repo, else the picker.
- Recency means of use, not of commit (`repo_sort_mode`).
- Search is `filter_repos` only: loose on names, strict on paths.

### 6.10 Row and chip context actions
- Amend and undo compare `head_sha`, never a row index into a paged list. Undo also needs the commit provably unpushed, or no upstream.
- Discard wording comes from `classify_discard`, never a status letter. Never-committed files go to the OS trash.

### 6.11 Embedded-terminal key ownership
With terminal focus the shell owns every key except the panel toggle, ⌃\`, never gated on focus. In Tauri, xterm's key handler and `utils/keyboard.ts` are one rule written twice and must agree. In native the chord lives on the menu, since a button's equivalent can't get past SwiftTerm.

### 6.12 Relative dates
One vocabulary, deliberately no platform formatter: `just now`, `N minutes ago`, `N hours ago`, `N days ago` under 30 days, `N months ago` under a year, `N years ago`. Singular at 1, future reads `just now`. Re-tick every 10 s while visible.

### 6.13 Failure surfacing
Classified by whether the user is waiting, not severity, in one function per client (`reportActionError`, `reportNotice`; `ActionFailure`, `StatusStrip`).

- **First class:** something the user asked for didn't happen (branch change, commit, explicit refresh). A modal, with retry where it helps, showing git's text mono, selectable and capped. `NSAlert` reflows it, so native uses a sheet.
- **Second class:** never the user's task (an OS hand-off, background refresh). A non-blocking strip over the last good view.
- Strip conditions are separate channels, so a notice never costs the poll banner or an Undo.
- **Refinement:** a failure raised inside a dialog stays there with input intact. That's why native's discard is a sheet.

### 6.14 Branch actions
- `list_branches` runs on every menu open. Every HEAD-moving action reloads status, history and branches.
- A merge preview of zero disables both buttons. A refused merge is first class, it already changed the repo.
- Abort shows only while `operation` is set, named for it.
- During a rebase, cherry-pick or revert the primary button calls `continue_operation`, never `commit`, whose index reset would lose the replayed changes. It stays enabled over conflicts, and core refuses by name. `skipped` raises a notice.
- Amend mode ends when a status lands whose `head_sha` isn't the amended commit.

### 6.15 Settings apply as they are changed
No Save. Each control writes via `patch_config` with only its fields, re-renders from the reply, and reverts on failure. Scan paths are the exception (Edit ▸ Done), since a half-typed line is a different folder. Each client serializes its writes. An unreadable config shows the failure and no form.

### 6.16 Escape
Dismisses the frontmost surface only, decided by when it appeared (`actions/overlayStack.ts`, AppKit's responder order). An uncancellable surface takes the key and refuses it. App chords check the same stack.

### 6.17 Terminal pointer and clipboard
Links open on modifier-click only (⌘ on macOS, `Ctrl` elsewhere), a plain click belongs to selection. Detect bare URLs and OSC 8. OSC 52 is write-only: never answer the read form, or anything printed could exfiltrate the clipboard.

### 6.18 Terminal caret across activation
Returning restores the caret to the shell if it was there. Read focus on return, never a latched flag.

### 6.19 Launch targets
Walk up to the repo root. A missing path just opens the app. A non-repo folder raises the create prompt over whatever's showing. Re-invoking on the open repo only activates the window. A second invocation never opens a second window.

### 6.20 Release check
Once per session, retried every 30 min only while failing, and on offline→online. Gated on OS connectivity, never feeds the breaker, since a rate-limited GitHub API says nothing about git remotes. Dismissal lasts the session, deliberately unpersisted.

### 6.21 History actions and the write slot
**Write slot.** One per window, for every repository rewrite: commit, continue, branch actions, merge, abort, history actions and their undo, checkout, undo commit, discard.

- A refused start is never reported as success.
- Under another write, a dialog disables its button and says "Another operation is still running.", never a busy label.
- Pull claims write then network and releases in reverse. Other transfers and auto-fetches stay outside.

**Actions.**

- Disabled during an operation, on detached HEAD, or with the slot held. Squash and Reorder also with a merge at or above the oldest selected row. The rest is the preflight's, before any picker or dialog: on the click for cherry-pick and squash, on ⏎ for reorder.
- Act on the commits the menu opened on. `beforeSha` is the row above the line.
- `rewrites_pushed`: squash captions it, reorder asks first.
- Landed: reload, select `selection`, scroll to it. Conflict: reload, go to Changes, raise the modal. Failed: reload and show core's message (§8), with squash keeping its dialog and input.
- Aborting a conflicted cherry-pick returns to `start.return_branch`, read before asking core.

**Undo.** One strip line, set after the reload, in client memory.

- It goes on its ✕, a replacing landing, a repo switch, or a status showing `point.branch` checked out at a HEAD other than `after_sha`. Elsewhere core judges.
- Inert only during an operation or a held slot.
- Reload first, report second. Expired drops the offer. An error keeps it.

**Undo after a conflict.** Keep the open action (operation, `start`, commits) after the reload.

- Forget it when `operation` changes, for a cherry-pick also when the branch leaves `start.branch`, or on a repo switch. A stopped rebase reports detached.
- The poll can forget it before Continue answers, so take what you need first.
- A successful Continue offers the normal Undo with `after_sha` = `head_sha`, unless the operation is still open, the status isn't on `start.branch` or has no `head_sha`, `started_at` isn't `start.before_sha`, or `head_sha` equals `before_sha`.

## 7. Diff rendering contract

- `FileDiff`, `Hunk` and `DiffLine` are the structured truth. Highlighting comes from core: `render.rs` collapses it to HTML for Tauri, native maps `TokenClass` from `tokenize_diff`.
- Two-phase paint: structure paints the frame it lands. The syntax pass waits **80 ms** (`HIGHLIGHT_DEBOUNCE_MS`, `DiffStore.highlightDebounce`) and re-checks its generation after the wait and after the call.
- Changing the layout re-reads through the seamless path and keeps scroll.
- Four nothings, each stated: rejected call, `empty_reason`, `size_guard`, `is_binary`.
- Copy from the model, not the view. `@@` headers copy, `\ No newline` drops. Native does this via `copy_diff_text`; Tauri's drag-select excludes gutters and prefixes.

## 8. Platform presentation mapping (intentional divergences)

| Concern | Tauri | SwiftUI |
|---|---|---|
| Theme | `theme` config field | system appearance; `theme` is a web-only concept, never read |
| Second instance | `plugin-single-instance` forwards `open-repo` | LaunchServices, `public.folder` document type |
| Branch surface | ⌘B popover | stock `Menu`, no chord (AppKit can't open one) |
| Line copy (§7) | browser selection | gutter line handle; drag within one line only |
| Commit-list place across tabs | panes stay mounted | pane rebuilt, scrolls to selected sha |
| Pane geometry | `localStorage`, all three | `UserDefaults`, composer height only |
| Settings (§6.15) | modal overlay; every field but `side_by_side_diff` | `Settings` scene; same, minus `theme` |
| Link hint (§6.17) | hover hint, xterm's underline can't be conditional | none until ⌘, SwiftTerm has no hover callback |
| Error surface (§6.13) | `ErrorModal`, `StatusStrip.svelte`, dialog `error` prop | `ActionFailureSheet`, `StatusStrip`, local `errorMessage` |
| Progress | fill in the sync button | strip under the toolbar |
| Cadence enforcement (§6.1) | `setTimeout` chain; a WebView may throttle the hidden rung | App Nap assertion, exact 30 s |
| Multi-row selection (§6.4) | pane follows the row the gesture landed on | `List(selection:)`; pane holds still; `maintainsSelection` writes back |
| Shortcut discovery | `?` overlay | menu bar |
| Context menus | drawn `<div>`; `tauri::menu` is unthemeable on Windows, misplaced on Wayland | stock menu |
| Cherry-pick target (§6.21) | narrowed branch popover; failure takes the modal | `CherryPickSheet`; failure stays inside |
| Reorder (§6.21) | window key capture, `click` ends it; failure takes the modal | `.onKeyPress`, mouse-down ends it; failure stays in `ReorderSheet` |
| Pull under another write | `aria-disabled` face with tooltip | whole `Menu(primaryAction:)` dims, face refuses |
| Disabled History item | hover `title` | no reason; `NSMenu` items have no tooltip |
| Other widgets | `xterm.js`, windowed lists, joined icon buttons, count capsules on the button | SwiftTerm, `List`, segmented `Picker`, toolbar count text |

## 9. Non-goals / intentionally absent

- No commit graph.
- No partial-hunk staging in the shipped UI.
- No frontend→backend events.
- No git library: git is the `git` CLI, GitHub is the `gh` CLI.
- No per-folder open action. A missing repo means `scan_paths` is wrong.
