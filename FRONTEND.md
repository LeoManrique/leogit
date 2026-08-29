# LeoGit — Frontend Contract (Source of Truth)

This document is the **single behavioral contract** shared by LeoGit's two
frontends. Both are built against it and must not diverge except where §8 records an
explicit per-platform exception.

- **Tauri + Svelte** — Windows and Linux.
- **SwiftUI** — macOS, over a UniFFI bridge into the same core.

The **visual/interaction design language** both frontends target lives in
[`STYLE.md`](STYLE.md); this document is the **functional/data** contract (commands,
events, DTOs, behavior). Where the two platforms diverge in presentation, §8 records it.

Both talk to the **same `leogit-core`** (Rust). The contract is written
**bridge-agnostic**: it describes logical operations, payloads, and behavior — not
UniFFI method names or HTTP routes — so it holds whether the bridge is UniFFI
static-linking or a local daemon (that decision is open; see the plan).

> Field-naming convention: DTO fields are **`snake_case` on the wire** (Rust serde
> default; two enums use `PascalCase` string values, noted below). TypeScript mirrors
> the snake_case exactly; Swift `Codable` uses `.convertFromSnakeCase`/
> `.convertToSnakeCase` so Swift properties are camelCase. Command **argument** keys
> are **camelCase** (Tauri auto-converts camelCase→snake_case params today; a
> non-Tauri transport must replicate that mapping).

---

## 1. Maintenance charter (lockstep mandate)

- This document is updated **first or alongside** any change to the frontend-facing
  surface. Then **both** frontends are brought into conformance.
- Add/change a command → update §3. A DTO/enum → §5. An event → §4. A cadence or
  policy → §6. A deliberate per-platform difference → §8 (never a silent divergence).
- The backend **categorizes and computes**; frontends **render and orchestrate**.
  Frontends never re-derive git state the core already returns (e.g. file status
  categories, ahead/behind, merge conflicts).
- Today's surface: **4 events, ~45 DTOs**, and a command catalogue (§3) each host exposes
  **to the extent it consumes it**. The Tauri host registers **73** `#[tauri::command]`s,
  each with a wrapper in `apps/tauri-app/src/lib/api/commands.ts`; the UniFFI bridge
  exports **67** functions. The two sets are deliberately not identical, and a command
  reaching one host does not oblige the other — what is required is that the difference be
  recorded, here or in §8, never left silent.
  - No native export: `check_auth`, `delete_remote_branch`, `generate_patch`,
    `generate_inverse_patch`, `get_ahead_behind`, `get_repo_identifier`,
    `get_repo_name`, `has_staged_changes`, `highlight_diff`, `is_git_repo`,
    `rename_branch`, `take_pending_launch_target`, `terminal_pty_info`. Four of those the
    native client reaches under another name (`repo_display_name` for `get_repo_name`,
    `resolve_repo_root` for `is_git_repo`, the structured `tokenize_diff` for the
    HTML-shaped `highlight_diff`, and `repo_identifier` for `get_repo_identifier` —
    async so the picker's per-row `git config` reads never hold a Swift cooperative
    thread). `take_pending_launch_target` is core's process-global slot for a
    cold-start target, which the native client deliberately does not use: it has a
    second source for the same thing (`application(_:open:)`, which fires at any
    time), and a global the UI cannot observe would have to be polled — one
    observable Swift store owns both routes instead. The rest the bridge omits
    because it carries no surface a client does not call.
  - No Tauri command: `core_version`, `fix_path_env`, `repo_display_name`,
    `repo_identifier`, `resolve_launch_target`, `resolve_repo_root`,
    `tokenize_diff`.
  - Registered Tauri-side but called by nothing in the Svelte client:
    `copy_diff_text`, `generate_patch`, `generate_inverse_patch`,
    `get_ahead_behind`, `has_staged_changes`, `rename_branch`,
    `delete_remote_branch`. Each is a live item in the parity plan — either being
    wired (DF-5, DF-6) or being deleted (WS-S) — and a command that is neither
    should not stay on this list.

## 2. System context & architecture

```
 Svelte UI ──invoke/listen──►┐                        ┌────────────────┐
                             ├──►  leogit-core  ◄──────┤  SwiftUI UI     │
 (Tauri host, Win/Linux)  ───┘   (git/diff/gh/ai/      └────────────────┘
                                  terminal/config)        (bridge, macOS)
```

- **Request/response** — the §3 catalogue, each `args → Result<T, Error>`. The
  frontend→backend direction is **only** request/response; there are no
  frontend→backend events.
- **Push (backend→frontend)** — 4 payload kinds (§4): two broadcasts, best-effort, from
  which a frontend must be able to recover authoritative state by re-issuing the relevant
  command (e.g. re-`get_status`), and one terminal session's two-message stream, which is
  delivered on a listener handed in at `start_terminal` and may lose nothing.
- **State ownership** — durable state (config, repos MRU, terminal PTY sessions)
  lives in the core. Frontends hold only re-derivable view state.

## 3. Command surface (73)

Grouped by namespace. `args` are the logical inputs (camelCase on the wire);
`→` is the return DTO (§5). "async/net" marks network operations that may stream
progress (§4.1) and can be slow. This is the catalogue of operations core offers a
frontend — the Tauri host registers all 73; the native bridge exposes the subset it
consumes, plus seven of its own (§1).

### 3.1 Config & state — 6
| Command | Args | Returns |
|---|---|---|
| `load_config` | – | `Config` (normalized) |
| `patch_config` | `patch: ConfigPatch` | `Config` (merged + normalized) |
| `config_bounds` | – | `ConfigBounds` |
| `load_state` | – | `ReposState` |
| `patch_state` | `patch: ReposStatePatch` | `ReposState` (merged) |
| `record_recent_repo` | `path` | `ReposState` (authoritative MRU) |

`patch_config` is the **only** writer. A surface names the fields it owns and
nothing else, so it cannot revert what the other client changed while its
window was open; core reads, edits, normalizes and writes under one lock.
Clearing an optional field is patching it to `""` — the config's standing
"blank means absent" rule (§5.2). `config_bounds` is where a numeric control's
`min`/`max` come from, so a form can never offer a value the writer clamps away.

### 3.2 Launch — 1
| Command | Args | Returns |
|---|---|---|
| `take_pending_launch_target` | – | `LaunchTarget \| null` (cold-start `leogit <dir>` claim) |

One rule (`core::launch::resolve_launch_target`), two ways to reach it. The
Tauri host runs it itself before a window exists and parks the answer in a
process global the frontend claims through the command above; the native bridge
exports the **resolver** instead (`resolve_launch_target`, one of the seven
native-only functions in §1), feeding it either the process's argv or the single
path AppKit hands it in `application(_:open:)` as a one-element argv — so a
folder delivered by double-click resolves by the same rule as one typed at a
prompt, subdirectory walk-up included.

### 3.3 Git — status / diff / log — 5
| Command | Args | Returns |
|---|---|---|
| `get_status` | `repoPath` | `RepoStatus` |
| `file_status_styles` | – | `FileStatusStyle[]` |
| `get_selected_diff` | `repoPath, files` | `string` (the AI input; never parsed) |
| `get_log` | `repoPath, opts:{max_count, skip}` | `CommitInfo[]` |
| `get_commit_detail` | `repoPath, sha` | `CommitDetail` (files + totals, one `git log`) |

There is no `get_head_sha`: `RepoStatus.head_sha` is already in every status
reply (porcelain v2 emits the HEAD OID as `# branch.oid`), so asking for it
separately was a second subprocess per poll tick for a field the first one had
already delivered. §6.1's "HEAD moved since the last tick" rule reads that field.

The raw-diff getters are gone: reading and parsing were always done together,
and fusing them (§3.10) removed a round trip per file selection and gave the
"empty because whitespace is hidden" answer somewhere to be computed.
`file_status_styles` is fetched once, not per row — it is a table of ten short
strings that a changed-file list draws on every repaint.

### 3.4 Git — branches — 7
| Command | Args | Returns |
|---|---|---|
| `list_branches` | `repoPath` | `BranchInfo[]` |
| `create_branch` | `repoPath, name, startPoint` | `void` |
| `switch_branch` | `repoPath, branch` | `void` |
| `checkout_commit` | `repoPath, sha` | `void` (detaches HEAD) |
| `delete_branch` | `repoPath, name` | `void` |
| `delete_remote_branch` (net) | `repoPath, remote, branch` | `void` |
| `rename_branch` | `repoPath, oldName, newName` | `void` |

### 3.5 Git — commit & staging — 10
| Command | Args | Returns |
|---|---|---|
| `commit` | `repoPath, message, files, amend` | `void` |
| `undo_last_commit` | `repoPath` | `void` |
| `has_staged_changes` | `repoPath` | `boolean` |
| `classify_discard` | `repoPath, files` | `DiscardPlan` |
| `discard_files` | `repoPath, files` | `void` |
| `append_to_gitignore` | `repoPath, patterns` | `void` |
| `ignore_paths` | `repoPath, paths` | `void` |
| `format_commit_message` | `summary, description, coAuthors` | `string` |
| `reconcile_exclusions` | `excluded: Exclusion[], present: string[], elapsedMs` | `Exclusion[]` (the survivors) |
| `effective_scan_paths` | `scanPaths` | `string[]` |

`reconcile_exclusions` is the grace window behind §6.4, and the client skips the
crossing while nothing is excluded — the usual case. `elapsedMs` is wall-clock
time since the previous call rather than a tick count, because the poll's
cadence changes with what the window is doing (§6.1) and counting ticks would
make one window mean anything between 30 seconds and seven minutes. The reply
also carries a count of consecutive misses, because elapsed time alone is not
enough in the other direction: at the 30 s rung a single read is charged the
whole window, and that read is exactly the one that can land mid-rewrite.

### 3.6 Git — sync / remote — 8
| Command | Args | Returns |
|---|---|---|
| `repo_sync_status` (net) | `repoPath, doFetch` | `RepoSync` |
| `fetch` (net) | `repoPath, remote, background` | `void` |
| `pull` (net) | `repoPath, remote` | `void` |
| `push` (net) | `repoPath, remote, branch, setUpstream, forceWithLease` | `void` |
| `get_ahead_behind` | `repoPath, upstream` | `AheadBehind` |
| `get_remote` | `repoPath` | `string \| null` |
| `get_repo_identifier` | `repoPath` | `RepoIdentifier \| null` |
| `get_repo_name` | `path` | `string` |

### 3.7 Git — merge — 5
| Command | Args | Returns |
|---|---|---|
| `merge_branch` | `repoPath, branch` | `MergeResult` |
| `merge_squash` | `repoPath, branch` | `MergeResult` |
| `commit_squash_merge` | `repoPath` | `void` |
| `merge_abort` | `repoPath` | `void` |
| `count_commits_to_merge` | `repoPath, targetBranch` | `number` |

`RepoStatus.merging` answers "is a merge in progress" on every refresh, so
there is no separate command for it — a second route to the same answer is how
one refresh path came to forget to ask.

### 3.8 Git — discovery / init / clone — 7
| Command | Args | Returns |
|---|---|---|
| `known_repos` | `scanPaths, maxDepth` | `string[]` (discovery ∪ live MRU) |
| `filter_repos` | `query, rows: RepoRow[], scanFolders` | `string[]` (ranked) |
| `derive_clone_target` | `rawUrl, parent` | `CloneTarget \| null` |
| `clone_target_path` | `parent, repoName` | `string \| null` |
| `is_git_repo` | `path` | `boolean` |
| `init_repo` | `path` | `string` |
| `clone_repo` (net) | `url, targetPath` | `string` |

`filter_repos` is a batch call by design: one crossing per keystroke rather
than one per row is what makes a shared search rule affordable for a list that
re-filters as the user types. A `null` `derive_clone_target` is the Clone
button's disable condition, so the preview and the button always agree.

### 3.9 OS shell — 3
| Command | Args | Returns |
|---|---|---|
| `reveal_path` | `repoPath, relPath` | `void` (reveal in file manager) |
| `open_path` | `repoPath, relPath` | `void` (open with default app) |
| `open_url` | `url` | `void` (open in browser) |

### 3.10 Diff read + parse / patch — 5
| Command | Args | Returns |
|---|---|---|
| `get_parsed_diff` | `repoPath, file, hideWhitespace, options: DiffOptions` | `ParsedDiff` |
| `get_parsed_commit_diff` | `repoPath, sha, filePath, options` | `ParsedDiff` |
| `copy_diff_text` | `fileDiff, start, end` | `string` |
| `generate_patch` | `repoPath, fileDiff, selection` | `void` (stage hunks) |
| `generate_inverse_patch` | `repoPath, fileDiff, selection` | `void` (discard hunks) |

A **failure** is a rejected promise; **nothing to show** is a resolved one with
`empty_reason` set. Those are different events and a viewer must not merge
them: a stale diff behind an error is the one thing the pane must never show.
`DiffOptions` says what to build alongside the parse — a `WebView` host asks
for `html` and, in the split layout, `sbs_pairs`; the native host asks for
neither and pays for neither.

### 3.11 Highlight — 1
| Command | Args | Returns |
|---|---|---|
| `highlight_diff` | `fileDiff, source?: BlobSource` | `string[]` (per-line, see §7) |

### 3.12 GitHub (`gh` CLI) — 4
| Command | Args | Returns |
|---|---|---|
| `check_auth` | – | `boolean` |
| `gh_repo_list` (net) | `limit` | `GhRepo[]` |
| `gh_clone` (net) | `nameWithOwner, targetPath` | `string` |
| `gh_publish_repo` (net) | `repoPath, name, description, isPrivate` | `void` |

### 3.13 AI — 4
| Command | Args | Returns |
|---|---|---|
| `load_ai_config` | – | `AiProviderConfig` (resolved for the selected provider) |
| `generate_commit_message` (net) | `diff, provider, config: AiProviderConfig` | `CommitMessage` |
| `check_provider_status` | `provider, config` | `ProviderStatus` |
| `provider_status_from_failure` | `provider, error` | `ProviderStatus` |

`load_ai_config` is read fresh before every generate, never cached, so an edit
in either client applies on the next click. The config→provider mapping lives
in core: the model, server URL and timeout always belong to the provider
actually about to run, which splicing a picker value over a separately-loaded
config could not guarantee.

### 3.14 Terminal (PTY) — 5 + shells — 1
| Command | Args | Returns |
|---|---|---|
| `terminal_pty_info` | – | `PtyInfo` |
| `start_terminal` | `repoPath, shellId, listener` | `StartedTerminal` (the session's stream runs on `listener`, §4) |
| `write_terminal` | `pid, data` | `void` |
| `resize_terminal` | `pid, cols, rows` | `void` |
| `close_terminal` | `pid` | `void` |
| `list_shells` | – | `ShellOption[]` |

### 3.15 Update — 1
| Command | Args | Returns |
|---|---|---|
| `check_for_update` (net) | – | `UpdateInfo \| null` |

## 4. Backend→frontend surface (4, one direction only)

Two of these are **broadcasts** — anything in the UI may want them, they name what
they are about, and a dropped one costs only a stale indicator, so they are
best-effort deltas over an authoritative baseline: on (re)connect or any detected
gap, a frontend must re-pull the relevant command.

The other two are **one terminal session's stream**, and they are the opposite
kind of thing: they belong to a single panel, nothing may be dropped, and the
first of them is produced before the command that started the session has
returned. **A subscription cannot express that** — the frontend can only subscribe
after it learns what to subscribe to, and the shell is already printing into the
gap. So the listener is an *argument to `start_terminal`*, held for the life of
the session: a `Channel` the frontend constructs with its handler already
attached (Tauri), a `TerminalEventListener` callback object (UniFFI). Neither
carries a pid, because the listener is the session. Both transports deliver in
order.

| Stream | Payload | Meaning | Frontend action |
|---|---|---|---|
| `git-progress` (broadcast) | `GitProgressEvent {op:'push'\|'pull'\|'clone', path, percent, text}` | streamed during push/pull/clone | drive a progress indicator; final state from the command's `Result` |
| `open-repo` (broadcast) | `LaunchTarget {path, is_repo}` | warm-start / second-instance target | open that repo in the running window |
| terminal `output` (per session) | decoded text, coalesced by core | PTY stdout/stderr | feed the terminal emulator |
| terminal `closed` (per session) | `TerminalExit {exit_code, signal}` | child exited and was reaped | clean exit (`0`, no signal) → close the panel; otherwise print `[Process exited with code N]` and keep the dead terminal on screen (VS Code behavior). It can arrive **before** `start_terminal` returns — a shell that dies on a broken `.zshrc` does exactly that — so the handle must not be adopted afterwards |

### 4.1 Progress reliability convention (recommended, from `leosync-src`)
For a streaming transport (SSE / callback): stamp events with a monotonic id; on a
gap or reconnect, emit a synthetic **resync** signal and have stores re-pull
authoritative state. `git-progress` is advisory only — the command's return value is
the source of truth for success/failure.

## 5. Data model (~45 DTOs)

Authoring model today: **hand-mirrored** on each side (Rust serde struct ↔ TS
interface ↔ future Swift `Codable`), kept in sync by convention + this document. No
codegen (`ts-rs`/`specta`) is in use. Both reference projects also hand-mirror; a
codegen decision is open (plan §10.7).

### 5.1 Enums with non-default serialization
- `FileEntry.status` — `#[serde(rename_all="PascalCase")]`:
  `'New' | 'Modified' | 'Deleted' | 'Renamed' | 'Conflicted'`. The letter and
  the human label for each come from `file_status_styles`, never from a
  frontend's own table.
- `DiffLine.line_type` — PascalCase: `'Context' | 'Add' | 'Delete' | 'Hunk' | 'NoNewline'`.
- `BlobSource` — tagged union: `{kind:'workingTree', repoPath}` | `{kind:'commit', repoPath, sha}`.
- `RepoStatus.proposal` — `SyncProposal`, PascalCase: `'Loading' | 'Detached' |
  'PublishRepository' | 'PublishBranch' | 'Pull' | 'Push' | 'Fetch'`. Core's sync
  ladder (§6.2); each client maps it to its own title, icon and chevron rule.

### 5.2 Structures by domain
| Domain | Types (key fields) |
|---|---|
| Working tree / status | `FileEntry` (path, status, xy, display_name, display_dir, embedded, submodule_dirty, stat_stamp — an opaque mtime+size string so a status comparison sees content edits; compare, never parse); `RepoStatus` (branch, upstream, ahead, behind, files[], has_remote, unpushed_shas[], detached, head_sha, merging, proposal — the sync ladder's answer, carried here for the same reason `merging` is: every refresh path renders it, and a second route to it is how the two clients' ladders drifted); `FileStatusStyle` (status, letter, label — the glyph table, fetched once; colour is per-platform); `DiscardPlan` (restore[], trash[]) |
| History | `CommitInfo` (sha, short_sha, summary, body, author, committer, parents[], trailers[], co_authors[], body_without_coauthors, tags[]); `CommitStats` (additions, deletions); `CommitDetail` (files[], stats) |
| Branches / remote | `BranchInfo` (name, is_remote, is_current); `AheadBehind`; `RepoSync` (ahead, behind, has_remote, fetched, dirty); `RepoIdentifier` (owner, name); `MergeResult` (success, fast_forward, conflicts[], error_message?) |
| Diff | `DiffLine` (content, line_type, line numbers, `intra_line_diff: IntraLineRange`, and `text?` — the raw patch line, present only on `Hunk` and `NoNewline` rows, which are the only ones that read it); `IntraLineRange`, `HunkHeader`, `Hunk`, `FileDiff` (old_path, new_path, file_header, hunks[], is_binary); `SbsPair`; `DiffOptions` (html, side_by_side, show_anyway); `ParsedDiff` (file_diff, html[], sbs_pairs[], additions, deletions, empty_reason?, size_guard?); `EmptyDiffReason` (`NoChanges`/`WhitespaceOnly`/`NoTextualChanges`); `DiffSizeGuard` (reason, bytes, longest_line); `Token` (start, end, class: `TokenClass`) / `TokenLine` — the structured highlight layer under the HTML (§7); `DiffSelection` |
| Commit composer | `CommitMessage` (title, description); `Exclusion` (path, absent_ms, absent_reads — how long and over how many consecutive status reads an opt-out's path has been missing from the file list; both zero while it is present, and §6.4's window needs both to expire) |
| Config / persistence | `Config` (theme, fetch_interval_ms, ai_provider, auto_fetch, syntax_highlighting, scan_paths[], scan_depth, side_by_side_diff, hide_whitespace, tab_size, terminal_shell?, then the `claude` and `ollama` tables — **nothing scalar may follow them**, since a TOML table swallows every key after it); `ClaudeConfig` (model?, timeout_secs); `OllamaConfig` (model?, server_url, timeout_secs); `ConfigPatch` (every field optional — absent means "leave it alone", `""` means "clear it"); `Bounds`/`ConfigBounds`; `ReposState`; `ReposStatePatch` |
| Repo list | `RepoRow` (path, names[] — every label the user might type for that row); `CloneTarget` (normalized_url, repo_name, target_path) |
| GitHub | `GhRepo` (name_with_owner, name, description, is_private, pushed_at) |
| AI | `AiProviderConfig` (provider, model?, base_url?, timeout_secs), `ProviderStatus` (ready, reason, fix_command) |
| Terminal | `ShellOption`; `PtyInfo` (backend, build_number); `StartedTerminal` (pid, shell_id, shell_label) |
| Events / launch / update | `GitProgressEvent`; `LaunchTarget` (path, is_repo); `UpdateInfo` (version, url, install_command?) |

> **Blank means absent.** An emptied text setting persists as `""`, which is
> not `None`, so every `unwrap_or` downstream sails past it — `--model ""`, a
> hostless server URL. `Config::normalized()` turns blank-after-trim into
> absent on every read *and* every write, which is also what heals a file
> another client already poisoned. Optional numeric settings clamp to
> `config_bounds()` the same way, landing on the nearest bound rather than
> reverting to the default.
>
> `ParsedDiff.html`/`highlight_diff` return **pre-rendered HTML** — a web-shaped
> payload SwiftUI cannot use, which is why `DiffOptions.html` exists: the
> native host asks for neither the HTML nor the side-by-side pairs and core
> builds neither. The structured layer under it already exists
> (`Token`/`TokenClass`, plus each line's `intra_line_diff`), and `render.rs`
> is a pure structured→HTML collapse. Whether the *phase-2* wire becomes that
> structured layer for Tauri too is still open (§7); when resolved, update this
> section.

## 6. Behavioral contract (must be identical across frontends)

The backend does the git work; these are the **frontend orchestration rules** that
define LeoGit's behavior and must match on both platforms. (Today they live in
`MainLayout.svelte`, `lib/services/`, `lib/stores/`.)

1. **Status polling** — poll `get_status` while a repo is open, and refresh the commit
   log **and the branch list** whenever **HEAD moved since the last tick**.
   `RepoStatus.head_sha` is what answers that: porcelain v2 emits the HEAD OID as
   `# branch.oid`, so `get_status` already carries it at no cost and the rule mandates no
   second command. A checkout made in the terminal moves the branch menu's checkmark as
   well as the history, which is why both reload.
   **The cadence is the window's activity ladder, in both clients**: **2 s** frontmost,
   **10 s** visible but not focused, **30 s** hidden, with the automatic fetch's own
   interval stretched **×3** while hidden. Neither loop ever *stops* for focus or
   visibility — a visible-but-unfocused window that has quietly gone stale is the failure
   this prevents, and a hidden window that keeps ticking slowly is what makes coming back
   to it show a current screen instead of a catch-up. Only the multi-repo badge sweeps
   pause when the window is neither focused nor on screen: nobody is looking at a badge
   for a repository in a window nobody is looking at, and the wake-up resync is their
   catch-up path. Auto-fetch (`fetch`/`repo_sync_status`) runs on `fetch_interval_ms`
   (default **30s**) when `auto_fetch` is on, and is additionally **held back while text
   has the keyboard** — a fetch can reorder the file list under a half-written commit
   message, and the embedded terminal counts as text entry because it is exactly where
   that list is being changed from. That question is asked *at the moment of the tick*;
   a latched focus flag strands, because removing a focused element raises no
   `focusout`.
   **Pause all polling** while a network op is in flight; resync when the window wakes
   up. A **once-per-session 0–30 s skew** offsets the first automatic fetch so two windows
   started together don't stay in lockstep on the same repositories; the status poll is
   deliberately not skewed.
   A tick that finds **nothing changed publishes nothing** — an idle repository must not
   re-render the window every cadence — and the comparison is of the *whole*
   `RepoStatus`, not a hand-picked list of fields that a later field would silently fall
   out of. `stat_stamp` is what makes that comparison see content edits at all (§5.2).
   Automatic fetches are additionally
   gated on `RepoStatus.has_remote`: `get_remote` answers `"origin"` for a repo with no
   remote (§ *Notable invariants*), so an ungated tick spawns a doomed `git fetch` whose
   failures then open the connectivity breaker against every *other* repo. The gate skips
   only when the answer is **known** — a status not yet loaded is not a repo without a
   remote — and only a fetch that actually ran reports to the breaker: a slot conflict or a
   failed local `git remote` says nothing about the network.
   A failed **background** refresh is swallowed — a repository mid-write is momentarily
   unreadable — but **three consecutive failures raise a non-blocking banner** owned by
   those refreshes: the last good snapshot stays on screen behind it, any successful read
   clears it, and it is never a modal, which a recurring condition would re-raise every
   tick. The streak counts the app's own timers and resyncs *only*. A refresh that follows a
   user action is silent for a different reason — the action reported its own outcome — so
   three `index.lock` races in a row must not accuse a healthy repository of having
   vanished. The streak is also per repository: a switch resets it.
2. **Network-op mutual exclusion** — fetch/push/pull/publish are mutually exclusive;
   only one runs at a time, with a shared progress slot fed by `git-progress`. The
   *automatic* fetches (the timer, the resyncs, the badge sweeps) deliberately claim
   no slot: nobody is waiting on them, and taking it would disable the whole action
   cluster on a timer.
   **The sync surface is one adaptive control, and the ladder that drives it is
   core's** (`sync_proposal`, carried on `RepoStatus.proposal`): detached → publish
   repository → publish branch → pull → push → fetch, with a neutral disabled Fetch
   until the first status read lands. It is a *total function of the status*, which
   is the point — three independent booleans could all be true at once, and that is
   how one client came to offer a push git would reject on a diverged branch. Pull
   outranks push, so the state that needs doing first is the one proposed, and the
   push is simply not reachable. Both clients render the same answer and bind the
   same chord to it (⌘/Ctrl+P), so the button and the menu item can never disagree.
   **Fetch is always reachable** — as the proposal when in sync, and from the
   chevron in every state that has one — because it is the only way to ask the
   remote a question without a pull moving the working tree. A chevron appears only
   where it offers something the face does not; force-push-with-lease joins it only
   on a genuinely diverged branch, behind a confirmation naming `status.upstream`
   (composing `remote/branch` is wrong whenever the upstream branch is named
   something else). **Neither client has a Refresh button**: the poll keeps the view
   current, and ⌘/Ctrl+R forces a full local reload — status, history *and*
   branches — held back while a transfer runs, since a `git status` racing a pull
   contends for the lock files it is writing. Titles, icons, which states get a
   chevron, and where the pending counts sit are presentation (§8).
   **After any transfer both clients reload status *and* the log**: a pull brings in
   commits, and reloading status alone left History up to a poll tick behind.
3. **Seamless diff loads** — loading a file/commit diff must **guard stale responses**
   (drop results if the user moved on), keep the **previous diff on screen** while the
   replacement loads, and use a **150ms slow-load threshold** (`SLOW_DIFF_THRESHOLD_MS` /
   `DiffStore.slowLoadThreshold`) before saying so — ported from GitHub Desktop's
   `SeamlessDiffSwitcher`. Crossing the threshold **dims what is on screen and lays a
   spinner over it; it never unmounts it**. Unmounting throws away the rendered rows,
   their syntax tokens and the user's scroll position on every slow load, including the
   ones that come back identical — which is the case the native client's equality skip
   exists for, and which the view has to stop undoing. A first load with nothing old to
   keep showing stays blank under the threshold and takes the spinner over the blank
   pane past it. The native client additionally skips publishing a result equal to
   what's shown, so scroll and tokens survive; a permitted refinement, not a divergence
   (the observable rule — no flash under the threshold — is shared).
   **Scroll**: a reload of the *same* file keeps the user's scroll offset; a *different*
   file resets to the top. Both clients key that on the rendered diff's own paths, so a
   forced re-read (a focus return, a whitespace-setting toggle) counts as the same file.
   A diff with **nothing to render** is a state of its own, and the pane must say so rather
   than fall through to the nothing-is-selected copy — the user did select a file. It must
   also say *which* nothing it is: `empty_reason` distinguishes "this file matches its
   committed state" from "every change here is whitespace, and the setting is hiding
   them" from "the file changed without changing any lines" (a mode change or a pure
   rename). One caption covering all three told the user a file was unchanged when a
   setting was simply hiding the change. A **failed** load is not any of these — it
   rejects, and the pane must clear and state the failure **inline, where the diff would
   have been**. It is not one of §6.13's two classes: the user is not blocked (the rest
   of the repository is untouched and readable) and it is their task (so a strip that
   sits above the content is the wrong place for it). Re-selecting the row is the retry,
   and works because the payload is gone.
   A **dirty submodule** — changed inside, with the commit the parent records unmoved —
   is decided *before* the read, in both clients: `git diff` answers with a bare
   `Subproject commit <sha>-dirty` line, so the pane explains instead and the
   subprocess is never spawned.
4. **File selection semantics** — inclusion is *derived*: every committable file is
   included unless the user opted it out, so a poll tick cannot re-check a file they
   just unchecked. **An opt-out outlives its path leaving the list, for a grace window
   of 30 seconds *and* at least two consecutive misses** (`reconcile_exclusions`,
   §3.5) — the two failure modes are not symmetric, and both clients run the one rule.
   Dropping it the moment the path disappears re-includes a file the user deliberately
   unchecked, because a formatter rewrote it between two reads, and the next commit takes
   it: a commit nobody meant to make and never saw happen. Keeping it forever costs
   nothing but an unbounded set.
   **Both terms of the window are load-bearing, and each covers what the other cannot.**
   It is wall-clock rather than a count of ticks because the cadence is not a constant
   (§6.1) and "fifteen ticks" would mean anything from 30 seconds to seven minutes. But
   elapsed time alone fails at the other end of the same ladder: at the 30 s rung one
   read is charged the entire window, and a transfer can hand over minutes in a single
   lump, so a purely time-based rule would drop the opt-out on the **first** look — which
   is precisely the look that can land in the half-second a formatter has the file
   renamed away. Two consecutive misses cannot be one unlucky read, at any cadence.
   Both counters advance on **every** tick rather than on a changed file list — a path is
   dropped for having been *absent* long enough, which an unchanged list keeps being true
   of — and a single reappearance resets both.
   **Selection and inclusion are different things, in both clients**: the checkbox column
   is what the next commit will contain, the highlight is what the pointer and keyboard
   are pointing at, and either moves without the other. The highlight is a **set** —
   arrows move it one row and load that row's diff, shift extends it — because discard
   acts on all of it, and because Space toggles inclusion for the whole selection. The
   rule Space follows is the select-all checkbox's own sentence, deliberately: *any
   excluded → include them all, otherwise exclude them all*, so pressing it twice over a
   sweep leaves exactly that sweep included whatever state its rows were in. The
   select-all control is therefore **tri-state**; a two-state one reads "off" over a list
   that is mostly on, and this is the control people use to answer *what is going in?* at
   a glance. Right-clicking inside a multi-row selection acts on the whole selection;
   right-clicking outside it re-selects that one row first, so the menu and the diff pane
   always describe the same files. Staging is **whole-file** today (partial-hunk staging
   is scaffolded but inactive).
   **The diff pane follows a single-row selection and holds still for a multi-row one.**
   Extending a selection is choosing a group to act on, and the diff being read while a
   discard selection is built around it is the one thing that must not move. Which row an
   extension leaves open is where the two clients differ (§8).
5. **Connectivity circuit-breaker** — after consecutive failures, back off
   (30s→5min) and gate background git ops on connectivity; recover on reconnect.
6. **Tiered background refresh** — repos refresh in tiers (2/5/10 min) with staggered
   kicks, an on-switch sweep, and an on-visible sweep. **One loop with three deadlines,
   not three timers**: independent timers come due together on their common multiple and
   fire three fetch fan-outs in the same turn, which is the opposite of the sequential
   fan-out each tier promises. Sequential within a tier *and* across them, with the
   policy re-checked before every repository so a transfer starting — or the window
   going away — abandons the rest of the tier rather than finishing a fan-out nobody is
   waiting for. The active repo is in no tier: the status poll feeds its badge for free.
7. **Commit composer** — AI generation via `generate_commit_message`; auto-summary
   from a single changed file; amend/undo re-seed the message. `format_commit_message`
   composes summary + description + co-authors. **Generate is gated on the provider
   being ready to answer, not merely installed** — two different questions, and asking
   only the first lets an installed Claude CLI with a dead session light the button up
   and fail every request. **Two sources answer, and neither is sufficient alone.**
   `check_provider_status` probes ahead of the click and catches what is visible from
   outside: a missing binary, a signed-out CLI, an Ollama that isn't listening.
   `provider_status_from_failure` reads a request that already failed, and is the *only*
   thing that catches an expired session — signing out deletes the credentials, so a
   probe sees it, while an expiry leaves them on disk, so the probe reports a signed-in
   CLI and only a real request discovers the refresh failed. Both return the same shape:
   whether it is ready, the reason, and where core knows one, the shell command that
   fixes it. The client writes the reason beside the button rather than hiding it in a
   tooltip, and offers to run the command in its own terminal.
   Both clients keep **only the blocked case, tagged with the provider it describes**, so
   absence means "ready" and "not asked yet" alike and a switched provider drops the old
   block by comparison instead of by a clearing step. The answer is written only once it
   arrives — never cleared on the way into a probe, which made the remedy blink out and
   back on every focus.
   Four rules make a *disabled* Generate safe: an unanswered probe leaves it
   **enabled** (refusing on "not yet known" is worse than letting a doomed request
   report itself, and an answer core cannot interpret opens the gate for the same
   reason); a probe that *throws* changes nothing, because a wiring failure is not
   evidence and must not clear a block a real failure proved; a failure core does not
   recognize may never *clear* a block, only raise one; and the question is re-asked on
   every event that could have fixed it — the provider changing, and the app regaining
   focus while blocked, since every remedy leaves it for a browser or a terminal.
   A raised block **replaces** the generate failure that produced it rather than stacking
   under it: both describe one state, the remedy is the half the user can act on, and the
   provider's own wording survives as the row's tooltip and in the client's log. A commit
   failure is untouched by this — it has to stay visible while the provider is separately
   blocked, so the two rows are independent.
   **The composer's own chords are window-wide**, not scoped to its fields: a shortcut
   you must first click into the message to use is one nobody reaches mid-sentence. They
   are inert under any dialog and on any other tab, and they gate exactly as their
   buttons do — a keyboard route past the commit/generate lockout is still a way for a
   late AI result to land on a composer the commit just cleared.
8. **History** — commit history is a **flat linear list** (no DAG/graph layout), paged
   through `get_log` `{max_count, skip}`, and the model is shared: the list is
   **append-only and rooted at HEAD**. `commits[0]` is the repository's HEAD, always;
   paging only ever adds older rows to the end, deduplicated by sha against what is
   already loaded, and nothing is ever dropped from the front. That is what makes the
   rewriting actions' gate unambiguous rather than merely correct — Amend and Undo test
   `status.head_sha`, and a list whose top can drift is how they came to be offered on
   the wrong commit. Only page size is per-platform (50 in Tauri, 100 natively), and it
   is a scroll-feel choice, not a behavioural one. Two further invariants: the log is
   **refetched when HEAD moves** rather than patched — and only then, since an existing
   commit's content cannot change without its sha changing — and that refetch re-reads at
   most **500** commits however deep the user has scrolled, dropping the oldest rows,
   which re-grow on demand. The bound sits at the far end of the list, away from HEAD.
   The list scrolls to row 0 on a refetch: it is a commit the user has not seen, and an
   offset measured against a list whose top just changed means nothing.
   **Selection** follows the same two conditions in both clients: keep the newest commit
   selected on arrival, and re-seat when a refetch drops the selected sha — which is what
   an amend or an undo does to it. A right-click selects the row it opens on, so the menu
   and the detail pane can never describe two different commits.
9. **Repo pickers** — each client shows the repository list in two places (the screen
   shown while nothing is open, and the header/toolbar switcher), and **they are one
   surface**: same rows, same labels, same three empty states, same footer. Only the
   room they have differs.
   - **Which repository opens by itself, at launch**: a folder named on the command
     line (§6.19) outranks everything, else the recorded `last_opened_repo` if it is
     still a repository, else — when discovery found **exactly one** — that one, else
     the picker. The auto-open belongs to launch alone: a later scan-path edit that
     happens to narrow the list to one must not pull the user out of the picker they
     are standing in.
   - **Rows** are labelled by the remote's repository name where one is parseable, the
     folder's name otherwise, with a muted `owner/` prefix on the rows whose label
     another row shares — a repository with no remote has no owner to disambiguate
     with, so it keeps its bare name.
   - **Order**: the open repository first, then either most-recently-opened or A-Z,
     from a toggle persisted in `repos-state.json` as `repo_sort_mode`. Recency **of
     use**, not of last commit — a switcher answers "where was I".
   - **Keyboard**: the filter field keeps focus, ↑/↓ move a cursor over the rows
     (wrapping, scrolling the least amount that reveals the row), and Return opens the
     cursor's row. A new query snaps the cursor back to the top match.
   - **Emptiness says which one it is** — still looking, nothing found anywhere (with
     the folders that were searched), or nothing matched the filter. The last two both
     carry *Choose folders to search*, because "none matched" is what you see when the
     repository you want lives somewhere discovery was never pointed at. A failed walk
     is one inline row with a Retry above the rows a previous walk found, never a
     screen that replaces them.
   - **Search** is **loose on names, strict on paths**: the query may appear as a
     scattered subsequence in a repo's name(s), but a path must contain it contiguously
     and only below the deepest root it sits under — a scan folder, or the home
     directory (every row shares the folders above, so matching them matches
     everything). Results are ranked — exact name, prefix, substring, initials,
     subsequence, path — and each list's own sort order only breaks ties, because the
     first row is what Return or the keyboard cursor acts on. **One implementation, in
     core** (`filter_repos`), because two hand-written ones had already drifted on the
     very set of labels they searched. A frontend supplies the rows and every label it
     displays for each, and gets them back narrowed.
10. **Row context actions** — right-clicking a changed file offers discard (always
   confirmed), ignore-this-file / ignore-this-extension, copy absolute + relative
   path, and reveal / open-with-default (both disabled when the file is deleted, since
   nothing is left on disk). Right-clicking a commit offers amend and undo — enabled
   only on the actual `HEAD`, compared by `head_sha` and never by the row's index into a
   paged list — plus checkout (confirmed; anything but `HEAD`), copy SHA, and copy tag.
   Undo is further gated on the commit being provably unpushed, *or* on no upstream
   resolving at all, in which case nothing can prove it was pushed either. Discarding a
   never-committed file moves it to the OS trash rather than deleting it, and the
   confirmation must say so — reading the outcome from `classify_discard`, which
   returns the same plan the discard runs on. A status letter cannot answer it: a
   staged re-add of a path that exists in `HEAD` is restorable, a rename whose original
   is *not* in `HEAD` is not, and under an unborn `HEAD` nothing is.
11. **Embedded-terminal key ownership** — while the terminal holds keyboard focus the shell
   owns every key, with exactly one exception: the chord that toggles the panel, which stays
   reachable from *inside* the panel. Nothing else the app binds may fire from there —
   `Ctrl+P` is readline's previous-history, `Ctrl+R` its reverse search, `Escape` vim's
   normal mode. **The toggle is ⌃`** in both clients — VS Code's binding, and deliberately
   not ⌘`, which macOS owns for cycling an app's windows — **and it is never gated on
   focus.** Nobody types `Ctrl` + `` ` `` into a commit message, and the terminal is exactly
   where you go *from* the composer to run the thing you are about to describe, so a focused
   field is no reason to refuse it. The native client gets it from a **menu** key
   equivalent, which AppKit matches before it consults whatever holds focus; the Tauri
   client has to place the test above its
   own "a field has focus, leave it alone" bail, where the composer's chords already sit.
   The collision is Tauri-shaped,
   because its other chords are `Ctrl`-or-`Cmd` and the shell's modifier is `Ctrl` too:
   `xterm`'s `attachCustomKeyEventHandler` releases the toggle and swallows the rest, and the
   window-level handlers re-check each event's origin (`utils/keyboard.ts`) since xterm's
   input sink is a `<textarea>` and would otherwise pass for a text field. **Those two tests
   are one rule written twice and have to keep agreeing**: narrow one without the other and
   the chord stops working from inside the panel, which is the one place it is most wanted.
   The **native** client has no collision to resolve — its chords are ⌘, the shell's are ⌃ —
   but the menu is load-bearing rather than decorative: a key equivalent on a *button* is
   matched through the responder chain, which SwiftTerm sits at the head of. So the chord
   lives on View ▸ Show/Hide Terminal and not on the panel's own toggle, which is also
   where a user looks for it. Making the Tauri client's *other* chords follow the platform (⌘ on
   macOS, `Ctrl` elsewhere) would narrow its capture to the keys the shell actually wants,
   and is filed in ROADMAP with the chords it affects.
12. **Relative dates** — commit timestamps arrive as ISO-8601 strings
   (`author_date`/`committer_date`, e.g. `2026-08-12T14:03:11+0200`; the core is
   deliberately chrono-free) and each frontend renders them as relative ("5 minutes
   ago"), recomputed whenever the list is republished — which, under §6.1's equality
   gate, an idle repository never is. Whether an idle list also re-ticks on its own is
   platform policy (§8), and it is the only thing that keeps such a list honest.
13. **Failure surfacing is classified, not uniform.** Every failure lands in one of two
   places, and which one is decided by *whether the user is waiting on it*, never by how
   severe it looks. An operation the user asked for and is waiting on — a transfer, a
   branch change, a commit, an explicit refresh — did not happen, so it takes the window
   in a modal, and that modal offers a retry wherever the same attempt can simply be made
   again. A failure that was never the user's task — an OS hand-off that didn't take, the
   app's own background refresh — states itself in a non-blocking strip and leaves the
   last good view of the repository readable behind it (§6.1 covers the refresh streak
   that raises the app's own). The classification lives in one function per client rather
   than at each call site, because a `report the failure` shape copied from the site next
   door is how *every* failure in the Tauri client, down to "couldn't reveal the file in
   Finder", ended up seizing the window. Each client has exactly one: the Tauri store's
   `reportActionError` / `reportNotice` pair, and native's `ActionFailure` +
   `.actionFailureAlert` beside `ErrorBanner`. Native still routes checkout and undo
   failures to its strip where this rule puts them in the modal — the parity plan's WS-Q
   closes that.
   **One refinement, in both clients: a failure raised from inside a dialog stays in
   that dialog**, under its fields, with everything typed intact. The dialog is
   already the retry surface the modal would be offering — a rejected publish name,
   a refused force-push lease and a discard that lost an `index.lock` race are all
   fixed and re-submitted right there — so stacking a second window over it costs two
   dismissals to change one character, and leaves the dialog underneath still holding
   the input that failed. This is why a discard confirmation is a **sheet** in the
   native client rather than a system confirmation: a confirmation dismisses on the
   click, so it can neither hold a failure nor say it is still working, and a
   thirty-file discard is not instant.
14. **Branch actions are one menu, re-read at the moment of intent.** Switch, create,
   merge (regular or squash), abort and delete live together behind the branch control
   in both clients, and `list_branches` runs on **every** open: the status poll notices
   only branches that move HEAD, so a branch created in the embedded terminal would
   otherwise stay invisible for the whole session. **One operation at a time** — two
   checkouts issued by a double-click contend on `index.lock` — and a start refused for
   that reason is never reported as a success, which is the trap in answering "nothing
   went wrong" to "nothing ran". Switching to the branch already checked out is a no-op
   both clients guard rather than a checkout plus a full refresh chain; a **remote** row
   becomes a local tracking branch rather than a detached HEAD, decided in
   `switch_branch` so neither client carries a rule about it.
   **A merge previews its size** through `count_commits_to_merge` — which counts what
   its argument holds that HEAD does not, despite the parameter being named
   `targetBranch` — and a preview of **zero reads *already up to date* and disables both
   merge buttons**, rather than offering a merge that does nothing and then reports
   success. Squash is two calls, `merge_squash` then `commit_squash_merge`, so the
   message is git's. A refused merge belongs to §6.13's **first** class, not its dialog
   refinement: it has already changed the repository, and pressing the same button again
   cannot resolve a conflict — the work continues in the changes list, where the
   conflicted files are. **Abort is offered only while `RepoStatus.merging`**, the one
   action with no meaning outside that state, and it is what makes a merge begun in a
   terminal escapable from the app. Every branch action that moves HEAD reloads status,
   history *and* the branch list, and drops amend mode with them: the commit the
   composer was amending is no longer HEAD, and may not be on this branch at all.
   How the menu is *shaped* is presentation (§8).
15. **Settings apply as they are changed.** There is no Save button in either client
   and nothing to cancel: a discrete control (checkbox, picker) writes on the click, a
   text or numeric field when it loses focus or takes a Return. Each write is a
   `patch_config` naming **only the fields that control owns** — the two clients share
   one config file, so a whole-object write posts it as it looked when the window
   opened and silently reverts whatever the other client saved meanwhile (that is D-5,
   and a patch listing every field the *form* holds is the same bug at form scale).
   Core clamps and normalizes and hands the result back, which the form re-renders
   from, so a value out of range corrects itself in front of the user rather than being
   dropped — and a write that fails puts its control back, because with no pending Save
   a control still showing the rejected value is claiming a setting that isn't on disk.
   **Scan paths are the one field outside this**, behind an Edit ▸ Done cycle: they
   decide which repositories exist as far as the app is concerned, and a half-typed line
   is a different folder rather than a shorter one. Nothing is written until Done, so
   leaving mid-edit by any route discards the draft; applying them re-walks discovery
   there and then, where the change was made. Every setting the running app consumes
   takes effect without a restart — the diff settings through the config the viewers
   already read, `auto_fetch` and `fetch_interval_ms` by re-arming the timer when either
   moves, including when the *other* client moved it.
   Native's three remaining drifts, all owned by the parity plan's WS-R: its patch names
   every field the window holds rather than the one that changed (D-5's lost update at
   form scale — a `tab_size` written by the other client while the Settings window
   stands open is reverted by the next unrelated toggle), its scan-path field is a plain
   one, and a rejected write leaves its control showing the value that didn't land.
   Fields the two clients don't both expose are listed in §8.
16. **Escape dismisses the frontmost surface, and only that one.** A confirmation
   raised from a popover closes itself and leaves the popover standing; a surface
   running an operation that can't be called off — a clone mid-transfer, a commit past
   its confirmation — takes the key and refuses it rather than letting it fall through
   to whatever is underneath. A surface with a step inside it (the branch picker's
   create form and its two picking modes) backs out of the step first. Which surface is
   frontmost is answered by **when it appeared**, never by a list of flags: the Tauri
   client registers each surface as it mounts (`actions/overlayStack.ts`), the native
   client leans on AppKit's own responder order. The same registration answers "is
   anything on top of the repository view", which the app's own chords check before
   firing — a ⌘↩ that means *commit* must not answer a dialog asking about that commit.
17. **The terminal's pointer and clipboard follow terminal conventions, not the web's.**
   A URL in the scrollback opens on **modifier-click** — ⌘ on macOS, `Ctrl` elsewhere — never
   on a plain one, because a plain click belongs to the selection and dragging across a line
   that happens to contain a URL must not navigate. What that costs in discoverability is
   paid back by the affordance rather than by dropping the modifier: hovering a link names
   the gesture ("Follow link (⌘ + click)"), the way Terminal.app and iTerm do. **OSC 52 is
   honoured write-only.** A shell, `tmux` or `vim` — including one on the far side of an SSH
   session, where nothing else can reach the local clipboard — may *set* it; the sequence's
   read form, which types the clipboard back down the TTY, is swallowed and never answered,
   because anything that can print to the terminal could otherwise exfiltrate whatever the
   user last copied. The write goes through the OS, not the WebView's clipboard API, which
   refuses without a recent click — and a shell asking for the clipboard is not a click.
18. **The terminal keeps the caret across app activation.** Coming back to the window must
   put the caret back in the shell if that is where it was, because an emulator painted as
   focused with nothing behind it drops every keystroke until the user clicks. AppKit
   restores the first responder for the native client; the Tauri client does it by hand, and
   reads *whether the terminal holds focus* at the moment the window returns rather than
   latching a flag on the way out — a flag set from `focusin` is only cleared by another
   `focusin`, and clicking a plain element raises none, so it strands `true` and the shell
   takes the caret back from wherever the user actually went.

19. **A folder named from outside the app opens it, and a folder that isn't a
   repository asks.** `leogit <dir>`, and on macOS also a drop on the Dock icon or
   Finder's *Open With*, resolves through `resolve_launch_target`: the first non-flag
   argument, relative paths against the working directory, canonicalized, and walked
   **up** to the repository root so `leogit src/` opens the repository rather than
   offering to nest one inside it. A path that doesn't exist, or isn't a directory,
   just launches or focuses the app — a bare `leogit` is the normal way to open it.
   An existing folder **always** resolves, because the answer to "that isn't a
   repository" is the prompt, not silence: **"Create a repository here?"**, naming the
   folder and its path, saying that creating one leaves the files where they are and
   commits nothing. It is raised over whatever is showing — the picker, another
   repository, the first launch — because it belongs to none of them, keeps its context
   and states git's refusal in place if `init_repo` fails, and re-runs are safe:
   `init_repo` returns the enclosing repository's root rather than nesting.
   Re-invoking on the repository already open is window activation and nothing else —
   no reset, no refetch, no MRU bump. A repository outside every scan path keeps its
   row from then on through the shared MRU (§6.9).
   **A second invocation must never open a second window.** The Tauri host intercepts
   it with `plugin-single-instance`; macOS does it in LaunchServices (§8). (The `leogit`
   shell function `install.sh` writes still targets the Tauri bundle, so today it is
   `open -a LeoGit <dir>`, the Dock and Finder that exercise the native path.)
20. **The release check is once per session, and never interrupts.** One
   `check_for_update` on start, retried every 30 minutes **only while attempts keep
   failing** and once more on the offline→online edge — launching offline is exactly
   when the first attempt was skipped. Any *answer* ends it, "you are current"
   included: releases do not ship often enough to poll for. It is gated on the OS
   connectivity signal **alone, never on the circuit-breaker**, and its outcome
   **never feeds the breaker**: the breaker guards git remotes, and a rate-limited
   GitHub API answer says nothing about those in either direction. A failed check is
   not an error the user is shown — it is "couldn't check", which is what the retry is
   for. A found release surfaces as one quiet chip offering the `install_command` to
   copy, the release page, and a dismissal that lasts the session and is deliberately
   **not** persisted, so a skipped release resurfaces on the next start rather than
   being forgotten forever. Never a banner, modal, or toast.

## 7. Diff rendering contract

`FileDiff`/`Hunk`/`DiffLine` are the **structured** truth. Syntax highlighting and
intra-line (word) diff are computed in the core (`highlight.rs`, syntect + intra-line
ranges) as a structured layer — `Token {start, end, class: TokenClass}` per line, plus
each `DiffLine`'s `intra_line_diff` range. `render.rs` is a pure *structured → HTML*
collapse over that layer (`css_class()` is the only web-specific step), emitted today
as `ParsedDiff.html`/`highlight_diff` spans for the Svelte `{@html}` renderer.

- **Wire**: the structured layer (`Token`/`TokenClass` + intra-line ranges) is the
  cross-frontend contract; `TokenClass` → presentation is a per-platform map.
- **Svelte (Tauri)**: renders spans — either from `render.rs` kept on the Tauri host
  (the exact HTML it gets today, unchanged) or built in Svelte from the structured layer.
- **SwiftUI**: maps `TokenClass` → colour/traits (the mirror of `css_class`) into an
  `AttributedString`.
- **Who pays for what**: `DiffOptions` decides. The HTML array and the
  side-by-side pairs are built only for a host that asked; the native host asks
  for neither, so the phase-1 collapse no longer runs on a path that discards it.
- **Nothing to render is not one situation.** A viewer must distinguish four
  outcomes and say which: a rejected call (the load failed — never leave a stale
  diff on screen behind it), `empty_reason` (`NoChanges` /
  `WhitespaceOnly` / `NoTextualChanges`), `size_guard` (withheld for its size,
  with a "show anyway" that re-asks with `DiffOptions.show_anyway`), and
  `is_binary` (a stand-in, not a diff).
- **Copying comes from the model**, never from the rendered view:
  `copy_diff_text` over a flat line range, so a copy can't pick up gutters,
  `+`/`−` prefixes, side-by-side filler cells or a viewer's tab expansion.
- **Open decision** (parity plan, DF-3): whether the *phase-2* wire becomes the
  structured layer for Tauri too, or the HTML collapse stays on the Tauri host.

## 8. Platform presentation mapping (intentional divergences)

Behavior/data are identical (§6); only platform-native presentation differs. Record
every deliberate difference here.

| Concern | Tauri (Win/Linux) | SwiftUI (macOS) |
|---|---|---|
| Window chrome | Tauri window | native `WindowGroup` / AppKit |
| Theme | CSS tokens in `app.css`, dark/light via `data-theme`, driven by the `theme` config field | `Color` assets, system appearance — the `theme` field is never read (permanent exemption: a stored theme is a web-only concept) |
| Opening a repository from disk | the one `plugin-dialog.open` call chooses a *clone destination*; repositories otherwise arrive from discovery, a clone, or `leogit <dir>` | the same: the one `.fileImporter` chooses a clone destination, and repositories arrive from discovery or a clone |
| Home dir | `@tauri-apps/api/path`'s `homeDir` | `FileManager` |
| Reveal / open / open-url | core `os::*` commands (unchanged) | core `os::*` commands (unchanged) |
| Launch target / second instance | a cold start resolves argv in `main` and parks it for the frontend to claim; a second invocation is intercepted by `plugin-single-instance`, which focuses the window and forwards an `open-repo` event | LaunchServices does both: `CFBundleDocumentTypes` declares `public.folder`, so `open -a LeoGit <dir>` activates the running instance and delivers the folder to `NSApplicationDelegate.application(_:open:)` — cold and warm through one callback, with Finder's *Open With* and a drop on the Dock icon coming free. Same resolution rule (`resolve_launch_target`), same precedence, same init prompt |
| Opening the branch surface | ⌘B toggles the branch popover | no equivalent: AppKit matches a key equivalent by walking *into* submenus, so a menu-bar menu cannot be opened by a chord, and binding ⌘B to one of its items would give the same chord different meanings in the two clients. The Branch menu itself is the discovery surface, and macOS's own ⌃F2 focuses the menu bar |
| Diff rendering | HTML spans (`{@html}`) | structured runs → `AttributedString` |
| Terminal widget | `xterm.js` | SwiftTerm (PTY backend reused) |
| Virtualized lists | hand-rolled windowing | native `List`/`LazyVStack`/`Table` |
| Pane geometry persistence | `localStorage` (sidebar width, composer height, commit-files width) | `UserDefaults` (composer height, `commitComposerHeight`); sidebar and commit-files widths are per-session |
| Window frame persistence | `tauri-plugin-window-state` saves size and position on exit and restores them at launch; the `tauri.conf.json` size is the first-run default | AppKit frame autosave on the `WindowGroup`, with `.defaultSize` as the first-run default |
| Settings surface (§6.15) | a modal overlay inside the one window, with a header ✕ and a footer **Close** — there is nothing to save, so the button only dismisses | the stock SwiftUI `Settings` scene, a separate window with ⌘, and the standard title-bar close and no content buttons at all; a text field also commits on `.onDisappear` |
| Settings field coverage (§6.15) | every `Config` field the app reads has a control | no control for `theme` (a permanent exemption, above), `side_by_side_diff` (awaiting the layout, above), or the two AI timeouts — so a timeout set in the Tauri client bounds native's requests but cannot be changed there. Closing the timeout gap is the parity plan's WS-R |
| Background-cadence enforcement (§6.1) | the ladder is a self-scheduling `setTimeout` chain, so a WebView free to throttle a backgrounded document can only make the hidden rung *slower* than 30 s; the wake-up resync is what guarantees a current screen | an App Nap assertion is held while a repo is open, so the same ladder's timers are not coalesced away, and the hidden rung is exactly 30 s (`AppNapSuppressor`) |
| File-list selection & keyboard (§6.4) | two anchors and hand-rolled key handling: shift-click on the row body extends from a sticky row anchor, shift-click on a checkbox range-toggles from a second one that *does* move, and Home/End jump to first/last. Plain click and ⌘-click both collapse to one row. An extension **activates the shift-clicked row**, so the diff follows the far end | one `List(selection: Set<String>)`, so the range and multi-row gestures are AppKit's own and behave like every other macOS list, and the checkbox column has no separate anchor. The gesture that produced a selection is not recoverable from a `Set`, so an extension leaves the diff on the row it was already showing rather than guessing which row was clicked |
| Relative-date ticking (§6.12) | a 10 s tick re-renders the visible rows, skipped while the History pane is hidden or the window is backgrounded, so an open list never goes stale | formatted once per republish and not re-ticked, so an idle repository's labels stop ageing until something in its status moves |
| Side-by-side diff (`side_by_side_diff`) | split layout toggle, honoured by `DiffViewer` | not implemented — unified only; a layout feature awaiting its own design pass (ROADMAP), the config field crosses saves untouched |
| Pending-count placement (§6.2) | `↓N` / `↑N` capsules on the sync button's trailing edge, each with its own arrow | plain `↑N ↓N` text in its own toolbar item left of the button: macOS renders a toolbar control's label as text and icon only, so no custom view can ride the face, and no system API badges a toolbar item |
| Transfer progress surface | inside the control that started it — a fill wiping across the sync button, sweeping where git reports no percentage | a full-width strip under the toolbar with a real indeterminate state, plus git's line verbatim |
| Shortcut discovery | no menu bar, so the chords are documented by a `?` overlay listing them | the menu bar is the documentation: File ▸ Clone Repository… (⇧⌘O), View ▸ Changes/History (⌘1/⌘2), View ▸ Show/Hide Terminal (⌃`), View ▸ Refresh (⌘R), Branch ▸ the same items the toolbar control offers (⇧⌘N on New Branch), Repository ▸ the sync ladder's proposal (⌘P). A menu equivalent is also matched ahead of the responder chain, which is what makes ⌃` work from inside the terminal |
| Update-chip placement (§6.20) | in the header's trailing cluster, outside the repo-scoped controls, so it shows in the pre-main phases too | a toolbar item on the repository screen and a control beneath the list on the picker — the native picker has no toolbar to put it in, and both native surfaces render the one `UpdateChip` |
| Branch-menu shape (§6.14) | a popover: filter input, keyboard cursor over the rows, the four actions as a footer, and the two that need a branch narrowing the same list under a header that states the question | a stock `Menu`: an inline `Picker` for locals, a plain-button section for remotes, and the same four actions with `Merge into “…”` and `Delete Branch` as submenus. AppKit supplies the scrolling, type-select and cursor the popover hand-rolls |

Neither client offers a per-folder open action anywhere, deliberately: a repo
list is exactly what `scan_paths` covers, so a local repository missing from it
means the paths are wrong — a Settings edit, which every empty state links to
and which still holds next launch, where a one-off open would be forgotten. A
repository genuinely outside every scan path still arrives by clone or
`leogit <dir>` and then keeps its row through the shared MRU.

## 9. Non-goals / intentionally absent

- No commit **graph** (history is a flat list).
- No **partial-hunk** staging in the shipped UI (scaffolded, inactive).
- No frontend→backend **events** (that direction is request/response only).
- No git library dependency — git is the `git` **CLI**; GitHub is the `gh` CLI.
