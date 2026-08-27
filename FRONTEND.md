# LeoGit — Frontend Contract (Source of Truth)

This document is the **single behavioral contract** shared by LeoGit's two
frontends. Both are built against it and must not diverge except where §8 records an
explicit per-platform exception.

- **Tauri + Svelte** — Windows and Linux (the current app).
- **SwiftUI** — macOS (planned; see [`docs/plans/swiftui-macos-frontend.md`](docs/plans/swiftui-macos-frontend.md)).

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
- Today's surface: **69 commands, 4 events, ~30 DTOs**. All 69 command wrappers live
  frontend-side in `tauri-app/src/lib/api/commands.ts` (Svelte) and must have a
  matching wrapper in the SwiftUI IPC client.

## 2. System context & architecture

```
 Svelte UI ──invoke/listen──►┐                        ┌────────────────┐
                             ├──►  leogit-core  ◄──────┤  SwiftUI UI     │
 (Tauri host, Win/Linux)  ───┘   (git/diff/gh/ai/      └────────────────┘
                                  terminal/config)        (bridge, macOS)
```

- **Request/response** — 69 commands, each `args → Result<T, Error>`. The
  frontend→backend direction is **only** request/response; there are no
  frontend→backend events.
- **Push (backend→frontend)** — 4 events (§4). Best-effort; a frontend must be able
  to recover authoritative state by re-issuing the relevant command (e.g. re-`get_status`).
- **State ownership** — durable state (config, repos MRU, terminal PTY sessions)
  lives in the core. Frontends hold only re-derivable view state.

## 3. Command surface (69)

Grouped by namespace. `args` are the logical inputs (camelCase on the wire);
`→` is the return DTO (§5). "async/net" marks network operations that may stream
progress (§4.1) and can be slow.

### 3.1 Config & state — 5
| Command | Args | Returns |
|---|---|---|
| `load_config` | – | `Config` |
| `save_config` | `cfg: Config` | `void` |
| `load_state` | – | `ReposState` |
| `patch_state` | `patch: ReposStatePatch` | `ReposState` (merged) |
| `record_recent_repo` | `path` | `ReposState` (authoritative MRU) |

### 3.2 Launch — 1
| Command | Args | Returns |
|---|---|---|
| `take_pending_launch_target` | – | `LaunchTarget \| null` (cold-start `leogit <dir>` claim) |

### 3.3 Git — status / diff / log — 10
| Command | Args | Returns |
|---|---|---|
| `get_status` | `repoPath` | `RepoStatus` |
| `get_head_sha` | `repoPath` | `string` |
| `get_diff` | `repoPath, file` | `string` (raw unified diff) |
| `get_diff_whitespace_ignored` | `repoPath, file` | `string` |
| `get_commit_diff` | `repoPath, sha, filePath` | `string` |
| `get_selected_diff` | `repoPath, files` | `string` |
| `get_log` | `repoPath, opts:{max_count, skip}` | `CommitInfo[]` |
| `get_commit_files` | `repoPath, sha` | `FileEntry[]` |
| `get_commit_stats` | `repoPath, sha` | `CommitStats` |
| `get_last_commit_timestamp` | `repoPath` | `number` |

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

### 3.5 Git — commit & staging — 8
| Command | Args | Returns |
|---|---|---|
| `commit` | `repoPath, message, files, amend` | `void` |
| `undo_last_commit` | `repoPath` | `void` |
| `has_staged_changes` | `repoPath` | `boolean` |
| `discard_files` | `repoPath, files` | `void` |
| `append_to_gitignore` | `repoPath, patterns` | `void` |
| `ignore_paths` | `repoPath, paths` | `void` |
| `format_commit_message` | `summary, description, coAuthors` | `string` |
| `effective_scan_paths` | `scanPaths` | `string[]` |

### 3.6 Git — sync / remote — 8
| Command | Args | Returns |
|---|---|---|
| `repo_sync_status` (net) | `repoPath, doFetch` | `RepoSync` |
| `fetch` (net) | `repoPath, remote` | `void` |
| `pull` (net) | `repoPath, remote` | `void` |
| `push` (net) | `repoPath, remote, branch, setUpstream, forceWithLease` | `void` |
| `get_ahead_behind` | `repoPath, upstream` | `AheadBehind` |
| `get_remote` | `repoPath` | `string` |
| `get_repo_identifier` | `repoPath` | `RepoIdentifier \| null` |
| `get_repo_name` | `path` | `string` |

### 3.7 Git — merge — 6
| Command | Args | Returns |
|---|---|---|
| `merge_branch` | `repoPath, branch` | `MergeResult` |
| `merge_squash` | `repoPath, branch` | `MergeResult` |
| `commit_squash_merge` | `repoPath` | `void` |
| `merge_abort` | `repoPath` | `void` |
| `is_merging` | `repoPath` | `boolean` |
| `count_commits_to_merge` | `repoPath, targetBranch` | `number` |

### 3.8 Git — discovery / init / clone — 4
| Command | Args | Returns |
|---|---|---|
| `discover_repos` | `scanPaths, maxDepth` | `string[]` |
| `is_git_repo` | `path` | `boolean` |
| `init_repo` | `path` | `string` |
| `clone_repo` (net) | `url, targetPath` | `string` |

### 3.9 OS shell — 3
| Command | Args | Returns |
|---|---|---|
| `reveal_path` | `repoPath, relPath` | `void` (reveal in file manager) |
| `open_path` | `repoPath, relPath` | `void` (open with default app) |
| `open_url` | `url` | `void` (open in browser) |

### 3.10 Diff parsing / patch — 3
| Command | Args | Returns |
|---|---|---|
| `parse_diff` | `raw` | `ParsedDiff \| null` |
| `generate_patch` | `repoPath, fileDiff, selection` | `void` (stage hunks) |
| `generate_inverse_patch` | `repoPath, fileDiff, selection` | `void` (discard hunks) |

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

### 3.13 AI — 2
| Command | Args | Returns |
|---|---|---|
| `generate_commit_message` (net) | `diff, provider, config: AiProviderConfig` | `CommitMessage` |
| `check_provider_available` | `provider, config` | `boolean` |

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
  `'New' | 'Modified' | 'Deleted' | 'Renamed' | 'Conflicted'`.
- `DiffLine.line_type` — PascalCase: `'Context' | 'Add' | 'Delete' | 'Hunk' | 'NoNewline'`.
- `BlobSource` — tagged union: `{kind:'workingTree', repoPath}` | `{kind:'commit', repoPath, sha}`.

### 5.2 Structures by domain
| Domain | Types (key fields) |
|---|---|
| Working tree / status | `FileEntry` (path, status, xy, display_name, display_dir, embedded, submodule_dirty, stat_stamp — an opaque mtime+size string so a status comparison sees content edits; compare, never parse); `RepoStatus` (branch, upstream, ahead, behind, files[], has_remote, unpushed_shas[], detached, head_sha) |
| History | `CommitInfo` (sha, short_sha, summary, body, author, committer, parents[], trailers[], co_authors[], body_without_coauthors, tags[]); `CommitStats` (additions, deletions) |
| Branches / remote | `BranchInfo` (name, is_remote, is_current); `AheadBehind`; `RepoSync` (ahead, behind, has_remote, fetched, dirty); `RepoIdentifier` (owner, name); `MergeResult` (success, fast_forward, conflicts[], error_message?) |
| Diff | `DiffLine` (incl. `intra_line_diff: IntraLineRange`), `IntraLineRange`, `HunkHeader`, `Hunk`, `FileDiff` (old_path, new_path, file_header, hunks[], is_binary); `SbsPair`; `ParsedDiff` (file_diff, html[], sbs_pairs[], additions, deletions); `Token` (start, end, class: `TokenClass`) / `TokenLine` — the structured highlight layer under the HTML (§7); `DiffSelection` |
| Commit composer | `CommitMessage` (title, description) |
| Config / persistence | `Config` (theme, fetch_interval_ms, ai_provider, ai_model, ai_api_key, auto_fetch, syntax_highlighting, scan_paths[], scan_depth, side_by_side_diff, hide_whitespace, wrap_long_lines, tab_size, claude_timeout_secs, ollama_server_url, terminal_shell?); `ReposState`; `ReposStatePatch` |
| GitHub | `GhRepo` (name_with_owner, name, description, is_private, pushed_at) |
| AI | `AiProviderConfig` (provider, model?, api_key?, base_url?) |
| Terminal | `ShellOption`; `PtyInfo` (backend, build_number); `StartedTerminal` (pid, shell_id, shell_label) |
| Events / launch / update | `GitProgressEvent`; `LaunchTarget` (path, is_repo); `UpdateInfo` (version, url, install_command?) |

> `ParsedDiff.html`/`highlight_diff` return **pre-rendered HTML** today — a web-shaped
> payload SwiftUI cannot use. But the structured layer under it already exists
> (`Token`/`TokenClass`, today `pub(crate)`; plus each line's `intra_line_diff`), and
> `render.rs` is a pure structured→HTML collapse. Per plan §7.1 the core exposes that
> layer as the wire and each frontend renders it; where the HTML collapse lives for
> Tauri is the open decision. When resolved, update this section.

## 6. Behavioral contract (must be identical across frontends)

The backend does the git work; these are the **frontend orchestration rules** that
define LeoGit's behavior and must match on both platforms. (Today they live in
`MainLayout.svelte`, `lib/services/`, `lib/stores/`.)

1. **Status polling** — poll `get_status` every **2s** and `get_head_sha`
   periodically while a repo is open. Auto-fetch (`fetch`/`repo_sync_status`) every
   **30s** when `auto_fetch` is on. **Pause all polling** while a network op is in
   flight and while the window is hidden/blurred; resync on refocus.
2. **Network-op mutual exclusion** — push/pull/publish are mutually exclusive; only
   one runs at a time, with a shared progress slot fed by `git-progress`.
3. **Seamless diff loads** — loading a file/commit diff must **guard stale responses**
   (drop results if the user moved on), keep the **previous diff on screen** while the
   replacement loads, and use a **150ms slow-load threshold** (`SLOW_DIFF_THRESHOLD_MS` /
   `DiffStore.slowLoadThreshold`) before falling back to a "Loading…" state — ported from
   GitHub Desktop's `SeamlessDiffSwitcher`. The native client additionally skips publishing
   a result equal to what's shown, so scroll and tokens survive; a permitted refinement,
   not a divergence (the observable rule — no flash under the threshold — is shared).
4. **File selection semantics** — maintain selection as `selectedFiles` **plus**
   `userDeselected` so the 2s poll does not re-check files the user just unchecked.
   Support shift-click range and keyboard (arrows/Home/End/Space). Staging is
   **whole-file** today (partial-hunk staging is scaffolded but inactive).
5. **Connectivity circuit-breaker** — after consecutive failures, back off
   (30s→5min) and gate background git ops on connectivity; recover on reconnect.
6. **Tiered background refresh** — repos refresh in tiers (2/5/10 min) with staggered
   kicks, an on-switch sweep, and an on-visible sweep.
7. **Commit composer** — AI generation via `generate_commit_message`; auto-summary
   from a single changed file; amend/undo re-seed the message. `format_commit_message`
   composes summary + description + co-authors.
8. **History** — commit history is a **flat linear list** (no DAG/graph layout),
   loaded in a sliding window via `get_log` `{max_count, skip}`.
9. **Repo search** — the filter over a repository list is **loose on names, strict on
   paths**: the query may appear as a scattered subsequence in a repo's name(s), but a
   path must contain it contiguously and only below the scan folder that found the repo
   (every row shares the folders above, so matching them matches everything). Results
   are ranked — exact name, prefix, substring, initials, subsequence, path — and each
   list's own sort order only breaks ties, because the first row is what Return or the
   keyboard cursor acts on. One implementation per frontend
   (`lib/utils/repoSearch.ts`, `Services/RepoSearch.swift`), kept tier-for-tier
   identical; the only sanctioned difference is in §8.
10. **Row context actions** — right-clicking a changed file offers discard (always
   confirmed), ignore-this-file / ignore-this-extension, copy absolute + relative
   path, and reveal / open-with-default (both disabled when the file is deleted, since
   nothing is left on disk). Right-clicking a commit offers amend and undo — enabled
   only on the actual `HEAD`, compared by `head_sha` and never by the row's index into a
   paged list — plus checkout (confirmed; anything but `HEAD`), copy SHA, and copy tag.
   Undo is further gated on the commit being provably unpushed, *or* on no upstream
   resolving at all, in which case nothing can prove it was pushed either. Discarding a
   never-committed file moves it to the OS trash rather than deleting it, and the
   confirmation must say so.
11. **Relative dates** — commit timestamps arrive as ISO-8601 strings
   (`author_date`/`committer_date`, e.g. `2026-08-12T14:03:11+0200`; the core is
   deliberately chrono-free) and each frontend renders them as relative ("5 minutes
   ago"), ticking live.

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
- **Open decision** (plan §7.1): where the HTML collapse lives for Tauri — the wire is
  structured either way.

## 8. Platform presentation mapping (intentional divergences)

Behavior/data are identical (§6); only platform-native presentation differs. Record
every deliberate difference here.

| Concern | Tauri (Win/Linux) | SwiftUI (macOS) |
|---|---|---|
| Window chrome | Tauri window | native `WindowGroup` / AppKit |
| Theme | CSS tokens in `app.css`, dark/light via `data-theme` | `Color` assets, system appearance |
| Folder picker | `plugin-dialog.open` | `NSOpenPanel` / `.fileImporter` |
| Home dir / path join | `@tauri-apps/api/path` | `FileManager` |
| Reveal / open / open-url | core `os::*` commands (unchanged) | core `os::*` commands (unchanged) |
| Second-instance / open-repo | `plugin-single-instance` → `open-repo` | AppKit app-activation / URL open → `open-repo` |
| Diff rendering | HTML spans (`{@html}`) | structured runs → `AttributedString` |
| Terminal widget | `xterm.js` | SwiftTerm (PTY backend reused) |
| Virtualized lists | hand-rolled windowing | native `List`/`LazyVStack`/`Table` |
| Pane geometry persistence | `localStorage` (sidebar width, composer height, commit-files width) | `UserDefaults` (composer height, `commitComposerHeight`); sidebar and commit-files widths are per-session |
| Repo-search path root (§6.9) | scan folders only — the frontend can't resolve `~`, so a repo outside them is searched by its whole path | scan folders, then `NSHomeDirectory()` |
| Context-menu scope (§6.10) | multi-row selection, so discard also acts on a whole selection | single-selection lists, so every item acts on the right-clicked row |
| Open-diff freshness | stale until reselect — the poll never reloads the open diff (adopting `stat_stamp` the same way would fix it; ROADMAP) | reloads within a poll tick: `stat_stamp` makes the status comparison see content edits, `workingTreeEpoch` re-keys the load, the equality skip absorbs no-ops |

## 9. Non-goals / intentionally absent

- No commit **graph** (history is a flat list).
- No **partial-hunk** staging in the shipped UI (scaffolded, inactive).
- No frontend→backend **events** (that direction is request/response only).
- No git library dependency — git is the `git` **CLI**; GitHub is the `gh` CLI.
