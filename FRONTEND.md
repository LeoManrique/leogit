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
- Today's surface: **4 events, ~35 DTOs**, and a command catalogue (§3) each host exposes
  **to the extent it consumes it**. The Tauri host registers **73** `#[tauri::command]`s,
  each with a wrapper in `apps/tauri-app/src/lib/api/commands.ts`; the UniFFI bridge
  exports **59** functions. The two sets are deliberately not identical, and a command
  reaching one host does not oblige the other — what is required is that the difference be
  recorded, here or in §8, never left silent.
  - No native export: `check_auth`, `check_for_update`, `check_provider_available`,
    `delete_remote_branch`, `generate_patch`, `generate_inverse_patch`, `get_ahead_behind`,
    `get_head_sha`, `get_last_commit_timestamp`, `get_repo_identifier`, `get_repo_name`,
    `has_staged_changes`, `highlight_diff`, `init_repo`, `is_git_repo`, `open_url`,
    `rename_branch`, `take_pending_launch_target`, `terminal_pty_info`. Three of those the
    native client reaches under another name (`repo_display_name` for `get_repo_name`,
    `resolve_repo_root` for `is_git_repo`, the structured `tokenize_diff` for the
    HTML-shaped `highlight_diff`); `get_head_sha` is redundant against `get_status`
    (§6.1); `take_pending_launch_target` and `init_repo` serve the launch path, which is
    Tauri-only (§8); and the rest the bridge omits because it carries no surface a client
    does not call.
  - No Tauri command: `core_version`, `fix_path_env`, `repo_display_name`,
    `resolve_repo_root`, `tokenize_diff`.
  - Registered Tauri-side but called by nothing in the Svelte client:
    `check_provider_available`, `copy_diff_text`, `generate_patch`,
    `generate_inverse_patch`, `get_ahead_behind`, `has_staged_changes`,
    `rename_branch`, `delete_remote_branch`. Each is a live item in the parity
    plan — either being wired (ST-9, DF-5, DF-6) or being deleted (WS-H) — and a
    command that is neither should not stay on this list.

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
- **Push (backend→frontend)** — 4 events (§4). Best-effort; a frontend must be able
  to recover authoritative state by re-issuing the relevant command (e.g. re-`get_status`).
- **State ownership** — durable state (config, repos MRU, terminal PTY sessions)
  lives in the core. Frontends hold only re-derivable view state.

## 3. Command surface (67)

Grouped by namespace. `args` are the logical inputs (camelCase on the wire);
`→` is the return DTO (§5). "async/net" marks network operations that may stream
progress (§4.1) and can be slow. This is the catalogue of operations core offers a
frontend — the Tauri host registers all 67; the native bridge exposes the subset it
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

### 3.3 Git — status / diff / log — 6
| Command | Args | Returns |
|---|---|---|
| `get_status` | `repoPath` | `RepoStatus` |
| `file_status_styles` | – | `FileStatusStyle[]` |
| `get_head_sha` | `repoPath` | `string` |
| `get_selected_diff` | `repoPath, files` | `string` (the AI input; never parsed) |
| `get_log` | `repoPath, opts:{max_count, skip}` | `CommitInfo[]` |
| `get_commit_detail` | `repoPath, sha` | `CommitDetail` (files + totals, one `git log`) |
| `get_last_commit_timestamp` | `repoPath` | `number` |

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

### 3.5 Git — commit & staging — 9
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
| `effective_scan_paths` | `scanPaths` | `string[]` |

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

### 3.13 AI — 3
| Command | Args | Returns |
|---|---|---|
| `load_ai_config` | – | `AiProviderConfig` (resolved for the selected provider) |
| `generate_commit_message` (net) | `diff, provider, config: AiProviderConfig` | `CommitMessage` |
| `check_provider_available` | `provider, config` | `boolean` |

`load_ai_config` is read fresh before every generate, never cached, so an edit
in either client applies on the next click. The config→provider mapping lives
in core: the model, server URL and timeout always belong to the provider
actually about to run, which splicing a picker value over a separately-loaded
config could not guarantee.

### 3.14 Terminal (PTY) — 5 + shells — 1
| Command | Args | Returns |
|---|---|---|
| `terminal_pty_info` | – | `PtyInfo` |
| `start_terminal` | `repoPath, shellId` | `StartedTerminal` (emits per-PID output/closed events) |
| `write_terminal` | `pid, data` | `void` |
| `resize_terminal` | `pid, cols, rows` | `void` |
| `close_terminal` | `pid` | `void` |
| `list_shells` | – | `ShellOption[]` |

### 3.15 Update — 1
| Command | Args | Returns |
|---|---|---|
| `check_for_update` (net) | – | `UpdateInfo \| null` |

## 4. Event surface (4, backend→frontend only)

Events are **best-effort deltas over an authoritative baseline**. On (re)connect or
any detected gap, a frontend must re-pull the relevant command. Two events are
**per-PID** (terminal); today's Tauri wire uses dynamic channel names
`terminal-output-<pid>`/`terminal-closed-<pid>`, but the contract is "output/closed
for a given pid" — a single stream carrying a `pid` field is an equivalent
representation (preferred for the SwiftUI/daemon path).

| Event | Payload | Meaning | Frontend action |
|---|---|---|---|
| `git-progress` | `GitProgressEvent {op:'push'\|'pull'\|'clone', path, percent, text}` | streamed during push/pull/clone | drive a progress indicator; final state from the command's `Result` |
| `terminal-output` (per-PID) | raw bytes | PTY stdout/stderr | feed the terminal emulator |
| `terminal-closed` (per-PID) | `TerminalExit {exit_code, signal}` | child exited and was reaped | clean exit (`0`, no signal) → close the panel; otherwise print `[Process exited with code N]` and keep the dead terminal on screen (VS Code behavior) |
| `open-repo` | `LaunchTarget {path, is_repo}` | warm-start / second-instance target | open that repo in the running window |

### 4.1 Progress reliability convention (recommended, from `leosync-src`)
For a streaming transport (SSE / callback): stamp events with a monotonic id; on a
gap or reconnect, emit a synthetic **resync** signal and have stores re-pull
authoritative state. `git-progress` is advisory only — the command's return value is
the source of truth for success/failure.

## 5. Data model (~30 DTOs)

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

### 5.2 Structures by domain
| Domain | Types (key fields) |
|---|---|
| Working tree / status | `FileEntry` (path, status, xy, display_name, display_dir, embedded, submodule_dirty, stat_stamp — an opaque mtime+size string so a status comparison sees content edits; compare, never parse); `RepoStatus` (branch, upstream, ahead, behind, files[], has_remote, unpushed_shas[], detached, head_sha, merging); `FileStatusStyle` (status, letter, label — the glyph table, fetched once; colour is per-platform); `DiscardPlan` (restore[], trash[]) |
| History | `CommitInfo` (sha, short_sha, summary, body, author, committer, parents[], trailers[], co_authors[], body_without_coauthors, tags[]); `CommitStats` (additions, deletions); `CommitDetail` (files[], stats) |
| Branches / remote | `BranchInfo` (name, is_remote, is_current); `AheadBehind`; `RepoSync` (ahead, behind, has_remote, fetched, dirty); `RepoIdentifier` (owner, name); `MergeResult` (success, fast_forward, conflicts[], error_message?) |
| Diff | `DiffLine` (content, line_type, line numbers, `intra_line_diff: IntraLineRange`, and `text?` — the raw patch line, present only on `Hunk` and `NoNewline` rows, which are the only ones that read it); `IntraLineRange`, `HunkHeader`, `Hunk`, `FileDiff` (old_path, new_path, file_header, hunks[], is_binary); `SbsPair`; `DiffOptions` (html, side_by_side, show_anyway); `ParsedDiff` (file_diff, html[], sbs_pairs[], additions, deletions, empty_reason?, size_guard?); `EmptyDiffReason` (`NoChanges`/`WhitespaceOnly`/`NoTextualChanges`); `DiffSizeGuard` (reason, bytes, longest_line); `Token` (start, end, class: `TokenClass`) / `TokenLine` — the structured highlight layer under the HTML (§7); `DiffSelection` |
| Commit composer | `CommitMessage` (title, description) |
| Config / persistence | `Config` (theme, fetch_interval_ms, ai_provider, auto_fetch, syntax_highlighting, scan_paths[], scan_depth, side_by_side_diff, hide_whitespace, tab_size, terminal_shell?, then the `claude` and `ollama` tables — **nothing scalar may follow them**, since a TOML table swallows every key after it); `ClaudeConfig` (model?, timeout_secs); `OllamaConfig` (model?, server_url, timeout_secs); `ConfigPatch` (every field optional — absent means "leave it alone", `""` means "clear it"); `Bounds`/`ConfigBounds`; `ReposState`; `ReposStatePatch` |
| Repo list | `RepoRow` (path, names[] — every label the user might type for that row); `CloneTarget` (normalized_url, repo_name, target_path) |
| GitHub | `GhRepo` (name_with_owner, name, description, is_private, pushed_at) |
| AI | `AiProviderConfig` (provider, model?, base_url?, timeout_secs) |
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

1. **Status polling** — poll `get_status` every **2s** while a repo is open, and refresh
   the commit log whenever **HEAD moved since the last tick**. `RepoStatus.head_sha` is
   what answers that: porcelain v2 emits the HEAD OID as `# branch.oid`, so `get_status`
   already carries it at no cost and the rule mandates no second command. Auto-fetch
   (`fetch`/`repo_sync_status`) every **30s** when `auto_fetch` is on. **Pause all
   polling** while a network op is in flight; resync on refocus. What happens while the
   window is hidden or blurred is platform policy — §8. Automatic fetches are additionally
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
2. **Network-op mutual exclusion** — push/pull/publish are mutually exclusive; only
   one runs at a time, with a shared progress slot fed by `git-progress`.
3. **Seamless diff loads** — loading a file/commit diff must **guard stale responses**
   (drop results if the user moved on), keep the **previous diff on screen** while the
   replacement loads, and use a **150ms slow-load threshold** (`SLOW_DIFF_THRESHOLD_MS` /
   `DiffStore.slowLoadThreshold`) before falling back to a "Loading…" state — ported from
   GitHub Desktop's `SeamlessDiffSwitcher`. The native client additionally skips publishing
   a result equal to what's shown, so scroll and tokens survive; a permitted refinement,
   not a divergence (the observable rule — no flash under the threshold — is shared).
   A diff with **nothing to render** is a state of its own, and the pane must say so rather
   than fall through to the nothing-is-selected copy — the user did select a file. It must
   also say *which* nothing it is: `empty_reason` distinguishes "this file matches its
   committed state" from "every change here is whitespace, and the setting is hiding
   them" from "the file changed without changing any lines" (a mode change or a pure
   rename). One caption covering all three told the user a file was unchanged when a
   setting was simply hiding the change. A **failed** load is not any of these — it
   rejects, and the pane must clear rather than leave a stale diff standing behind an
   error.
4. **File selection semantics** — inclusion is *derived*: every committable file is
   included unless the user opted it out, so the 2s poll cannot re-check a file they
   just unchecked. How long an opt-out survives its path leaving the list is where the
   two clients still differ (§8). The
   list is keyboard-navigable: arrows move the active row and load its diff. Staging is
   **whole-file** today (partial-hunk staging is scaffolded but inactive). How far the
   keyboard and the pointer go beyond that — range selection, a Space toggle, Home/End —
   is platform policy (§8): the shared floor is one row active at a time, reachable by
   arrow keys.
5. **Connectivity circuit-breaker** — after consecutive failures, back off
   (30s→5min) and gate background git ops on connectivity; recover on reconnect.
6. **Tiered background refresh** — repos refresh in tiers (2/5/10 min) with staggered
   kicks, an on-switch sweep, and an on-visible sweep.
7. **Commit composer** — AI generation via `generate_commit_message`; auto-summary
   from a single changed file; amend/undo re-seed the message. `format_commit_message`
   composes summary + description + co-authors.
8. **History** — commit history is a **flat linear list** (no DAG/graph layout), paged
   through `get_log` `{max_count, skip}`. Two invariants are shared: a refresh re-reads at
   most **500** commits however deep the user has scrolled (deeper rows re-grow on demand),
   and the log is refetched when HEAD moves rather than patched. The paging model itself is
   platform policy (§8) — one client slides a fixed window, the other appends.
9. **Repo search** — the filter over a repository list is **loose on names, strict on
   paths**: the query may appear as a scattered subsequence in a repo's name(s), but a
   path must contain it contiguously and only below the deepest root it sits under —
   a scan folder, or the home directory (every row shares the folders above, so
   matching them matches everything). Results are ranked — exact name, prefix,
   substring, initials, subsequence, path — and each list's own sort order only breaks
   ties, because the first row is what Return or the keyboard cursor acts on. **One
   implementation, in core** (`filter_repos`), because two hand-written ones had
   already drifted on the very set of labels they searched. A frontend supplies the
   rows and every label it displays for each — a basename, and where it is known the
   GitHub `owner/name` — and gets them back narrowed.
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
   normal mode. The collision is Tauri-shaped, because its chords are `Ctrl`-or-`Cmd` and
   the shell's modifier is `Ctrl` too: `xterm`'s `attachCustomKeyEventHandler` releases the
   toggle and swallows the rest, and the window-level handlers re-check each event's origin
   (`utils/keyboard.ts`) since xterm's input sink is a `<textarea>` and would otherwise pass
   for a text field. The **native** client has no collision to resolve — its chords are ⌘,
   the shell's are ⌃ — and AppKit key equivalents precede the first responder, so the
   toggle already works from inside SwiftTerm. The Tauri client accepts either modifier, so
   it swallows both; making its modifier follow the platform (⌘ on macOS, `Ctrl` elsewhere)
   would narrow that to the keys the shell actually wants, and is filed in ROADMAP with the
   chords it affects.
12. **Relative dates** — commit timestamps arrive as ISO-8601 strings
   (`author_date`/`committer_date`, e.g. `2026-08-12T14:03:11+0200`; the core is
   deliberately chrono-free) and each frontend renders them as relative ("5 minutes
   ago"), recomputed on every refresh. Whether an idle list also re-ticks between
   refreshes is platform policy (§8).

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
| Opening a repository from disk | no picker — repositories arrive from discovery under `scan_paths`, from the startup `RepoPicker`, or from `leogit <dir>`; the one `plugin-dialog.open` call chooses a *clone destination* | `.fileImporter` on Welcome (⌘O) and *Open Other…* in the repo switcher's footer, so a repo outside the scan folders opens directly (and keeps its row across launches) |
| Home dir / path join | `@tauri-apps/api/path` | `FileManager` |
| Reveal / open / open-url | core `os::*` commands (unchanged) | core `os::*` commands (unchanged) |
| Launch target / second instance | the whole contract: `leogit <dir>` resolves through `core::launch`, a cold start claims it with `take_pending_launch_target`, and a second invocation focuses the window and forwards an `open-repo` event via `plugin-single-instance` | not implemented — no app delegate, no `onOpenURL`, no `CFBundleURLTypes`/`CFBundleDocumentTypes`, and neither launch command is exported to the bridge. The native client restores `last_opened_repo` at launch and otherwise waits on Welcome |
| Diff rendering | HTML spans (`{@html}`) | structured runs → `AttributedString` |
| Terminal widget | `xterm.js` | SwiftTerm (PTY backend reused) |
| Virtualized lists | hand-rolled windowing | native `List`/`LazyVStack`/`Table` |
| Pane geometry persistence | `localStorage` (sidebar width, composer height, commit-files width) | `UserDefaults` (composer height, `commitComposerHeight`); sidebar and commit-files widths are per-session |
| Repo-search labels (§6.9) | rows carry the GitHub `owner/name` when it is known, and both it and the basename are searchable | basename only — GitHub identifiers are not fetched natively yet (ROADMAP) |
| Context-menu scope (§6.10) | multi-row selection, so discard also acts on a whole selection | single-selection lists, so every item acts on the right-clicked row |
| Open-diff freshness | stale until reselect — the poll never reloads the open diff (adopting `stat_stamp` the same way would fix it; ROADMAP) | reloads within a poll tick: `stat_stamp` makes the status comparison see content edits, `workingTreeEpoch` re-keys the load, the equality skip absorbs no-ops |
| Exclusion set (which changed files are left out of a commit) | an opt-out is pruned the tick its path leaves the file list, so a file that vanishes for one refresh — a formatter rewriting it — is silently re-included | an opt-out persists for the session, so nothing can re-include a file the user unchecked. Converging on the native rule plus a grace window is filed (parity plan CH-7/H-20); until then this is a real behavioural difference, not a presentation one |
| Background cadence while unfocused/hidden (§6.1) | no explicit pause and no visibility term: the status poll is a flat 2 s `setInterval` and auto-fetch a flat `fetch_interval_ms` one, so the hidden-window cadence is whatever the host WebView's timer throttling makes it. `document.hidden` is read only to *trigger* the on-activation resync | the active repo never stops, and the cadence is explicit: status poll 2 s frontmost / 10 s visible-unfocused / 30 s hidden, auto-fetch interval ×3 while hidden, only the multi-repo sweeps pausing when inactive (`BackgroundSchedulingPolicy`), with an App Nap assertion held while a repo is open so the timers are not coalesced away |
| File-list selection & keyboard (§6.4) | shift-click extends a multi-row selection (a separate anchor for the checkbox column, Finder/Gmail semantics), Space toggles the focused row's checkbox and bulk-toggles a multi-selection, Home/End jump to first/last | single-selection `List`: arrow keys move the active row, and there is no range selection, Space toggle, or Home/End |
| History paging (§6.8) | 50-commit pages into a bidirectional **sliding window** capped at 500: scrolling past either end drops from the far end and `windowStartOffset` tracks the absolute index of row 0, with `scrollTop` compensated so the visible row stays pinned. A HEAD move *replaces* the window with a fresh page 1 instead of sliding it, which is a distinct signal (`log.resetSeq`) precisely so it is not compensated — the list scrolls to the new HEAD | 100-commit pages **appended** without dropping, de-duplicated by sha against what is already loaded; only a *refresh* is capped, at the same 500 |
| Relative-date ticking (§6.12) | a 10 s tick re-renders the visible rows, skipped while the History pane is hidden or the window is backgrounded, so an open list never goes stale | formatted once per refresh and not re-ticked; the 2 s poll is what moves the labels on |
| Side-by-side diff (`side_by_side_diff`) | split layout toggle, honoured by `DiffViewer` | not implemented — unified only; a layout feature awaiting its own design pass (ROADMAP), the config field crosses saves untouched |

## 9. Non-goals / intentionally absent

- No commit **graph** (history is a flat list).
- No **partial-hunk** staging in the shipped UI (scaffolded, inactive).
- No frontend→backend **events** (that direction is request/response only).
- No git library dependency — git is the `git` **CLI**; GitHub is the `gh` CLI.
