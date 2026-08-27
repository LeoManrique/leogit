# Plan — Cross-client feature parity (SwiftUI ⇄ Tauri)

> Status: **in progress — WS-A shipped (2026-08-27), WS-B is next.**
> Accepted 2026-08-27 with every open decision resolved. Per-workstream state
> is in §6; what WS-A found that WS-B needs is in §6's WS-B entry, not here.
> Produced by a three-way
> audit of the SwiftUI client, the Tauri client, and the GitHub Desktop source
> at `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop` (reference only —
> used to judge approaches to features LeoGit already has, never as a source of
> new features). Eleven feature areas were audited in parallel, every claim
> cited to code, and the load-bearing ones re-verified by hand. Companion
> contract: [`FRONTEND.md`](../../FRONTEND.md), corrected as part of this
> audit — see §2.

## 1. Motivation

The two clients are supposed to be feature-equal on one shared `leogit-core`.
They are not, and the gap runs in **both directions**:

- The Tauri client was originally ported from GitHub Desktop by a weak model
  and grew by accretion; several of its features are wired but unreachable
  (the merge UI), gated on the wrong thing (amend/undo by row index), or
  silently broken (a remote-less repo poisons the connectivity breaker
  app-wide). The previous plan
  ([`native-refresh-and-diff-continuity.md`](native-refresh-and-diff-continuity.md))
  pulled the *native* client ahead on refresh machinery, diff continuity, and
  config handling — none of that has flowed back.
- The native client, built flow-by-flow, still lacks whole contract surfaces
  the Tauri client ships: the `leogit <dir>` CLI launch, the update checker,
  launch-time repo discovery, multi-select in the file list, and a long tail
  of keyboard and presentation details the docs claim are shared.

Parity looked done because every Tauri flow *is* ported at core-command
granularity. At behavior granularity it isn't, in both directions. This plan's
goals, in the user's priority order:

1. **Feature parity between the two clients** — every difference either closed,
   or recorded in FRONTEND.md §8 as a deliberate platform divergence. No new
   features; GitHub Desktop informs *how*, never *what*.
2. **Current features working well** — the audit found real defects (some
   destructive) in shipping behavior; they come first.
3. **End-user convenience** — where the clients differ, the more seamless /
   responsive / honest behavior wins, whichever client has it.
4. **Efficiency** — low or reasonable CPU, network, and memory. The audit
   quantified several standing wastes (§3.3); the fixes ride the same
   workstreams.
5. **Shared core over duplication** — where the same logic is hand-written
   twice (and in several cases has already drifted), hoist it into
   `leogit-core` unless the performance cost is real (§5).

## 2. Method and rules of engagement

- Eleven areas audited: repo management; changes/commit; diff viewer; history;
  branches/merge; sync/publish; background machinery + update checker;
  clone/gh; settings/config + AI; terminal; app shell. The findings are
  condensed into §4 with citations kept for every claim a decision rests on;
  the destructive/high-severity claims and the doc contradictions were
  re-verified by hand against the code before this document was written.
- Every difference is classified: **defect** (wrong today), **missing-in-X**
  (parity gap), **divergence** (both defensible — gets a verdict), or
  **efficiency** (same behavior, different cost).
- GitHub Desktop is cited only where it settles a "which approach is better"
  question about a feature LeoGit already has.
- Findings carry stable IDs (`BR-1`, `DF-13`, …) so workstreams and decisions
  can reference them without repetition.
- **The living docs were wrong and have been fixed.** ROADMAP, FRONTEND,
  DESIGN, STYLE, TECHNICAL and README each claimed behavior the code doesn't
  have: a contract mandating what neither client does, three FRONTEND §8 rows
  describing features that don't exist, open ROADMAP items written against
  unreachable controls, four false claims in one DESIGN paragraph. Those
  corrections are already applied, so every document describes today's
  behavior and this plan describes only the work. Two classes could not be
  reached that way and are carried in WS-H: stale comments in Swift and
  TypeScript source, and a few doc claims outside the audit's checklist.

## 3. Defect register — fix before parity work

These are behaviors that are wrong *today*, ranked by severity. All verified
in code. "Make the current features work well" starts here (workstream A).

### 3.1 Defects — fixed in WS-A

Twelve of the twenty are closed. Kept as a register (IDs are referenced from
§4) and trimmed to what each fix *is*, since the code now carries the reasoning.

| ID | Client | Fixed | Left over |
|---|---|---|---|
| D-1 | Tauri | **Destructive.** Amend/Undo/Checkout gated on the row's index into the *loaded window*, so past a slide Undo reset the real HEAD and seeded the composer from another commit. Now gated on `status.head_sha`, per FRONTEND §6.10. | — |
| D-2 | Tauri | A remote-less repo's doomed `git fetch origin` opened the breaker against every other repo. `fetchActiveRemote` now gates on `status.hasRemote`, like the tier path already did — and on the new `statusLoaded`, since `hasRemote` defaults to false and an unqualified read would decide "no remote" about a repo nobody has looked at yet. Natively, `silentFetch` returns `Bool?` so a slot conflict or a local `git remote` failure stops being reported as a network failure. | The dead `if (!remote)` guard it was supposed to have — H-2 makes `get_remote` honest. |
| D-3 | Tauri | Silent poll failures vanished forever. Three consecutive **background** failures now raise a non-blocking banner off `repoState.pollError` — native's shape and threshold, and its ownership: `refreshStatus` grew a `background` opt separate from `silent`, because four of the seven silent callers are user actions whose own `index.lock` races would otherwise accuse a healthy repo. Reset per repository in both clients. | BG-4's equality gate (the other half of that item). |
| D-8 | Native | ⌘W with a text field still focused dropped the typed value. `flushPendingSave` now also writes an edit that never scheduled a save, guarded by a diff against `lastPersisted` — which holds the *normalized* form of the fields, not the raw file, or a config written by the other client would be rewritten on an open-and-close that changed nothing. A completed debounce also clears `pendingSave` now (generation-guarded), which it never did. | — |
| D-10 | Tauri | A commit could land mid-Generate and have the late result overwrite the cleared composer. `canSubmit` gained `!isGenerating`, and the lockout runs off `isCommitInProgress` — `isCommitting` is still false while the embedded-repo confirmation waits, and its Confirm calls `performCommit` past `canSubmit` entirely, so the composer stayed live behind the dialog. | — |
| D-11 | Tauri | The HEAD-move reset was read as a backward slide and scrolled to the bottom of the fresh page, paging again. `log.resetSeq` now distinguishes a *replacement* from a *slide*; a replacement scrolls to the new HEAD, and the counter is monotonic across a repo switch too. | `refreshLog`'s `headChanged` still needs `skip > 0`, so a *same-offset* replacement (a new commit while parked at offset 0, or a checkout's different history) bumps nothing. HI-2's append model removes the case rather than patching it. |
| D-12 | Tauri | An empty parse fell through to "Select a file to view its diff" with a file selected. Both diff panes now have an explicit "No Textual Changes" state, blank while the fetch is in flight. The test is `hasRenderableDiff`, not `!== null`: `parse_diff` returns null only for empty input, while a mode change or pure rename parse into a header with zero hunks — a blank pane, the same dead end one layer along. | The *reason* (H-9 / DF-7); today the copy names the common causes. And a failed load currently reaches the same branch — DF-10 is what separates them. |
| D-13 | Tauri | The header hand-rolled a status write that skipped `is_merging`, the `userDeselected` reconciliation, and the badge feed. It now takes `refreshStatus` as a prop: **one status writer in the client**. Checkout and undo also reload branches. | SY-8's "post-op = status + log". |
| D-16 | Tauri | `Ctrl+P` reached the shell *and* pushed; ⌃` could not leave a focused terminal; Escape closed overlays instead of reaching `vim`. One rule now (FRONTEND §6.11): `attachCustomKeyEventHandler` releases only the toggle, and the window handlers test the event's origin. | TE-1's modifier narrowing (Tauri still accepts ⌘` too). |
| D-17 | Tauri | `tab_size: 999` and emptied fields persisted (and the emptied ones failed the save with a raw serde error). All four numeric fields clamp on save, and the clamped value is written back into the form. | H-10's `Config::normalized()` — the structural version, which also covers writers that never see this form. |
| D-18 | Native | The warm-up fetch ran offline, and against remote-less repos, discarding its outcome. Now gated on the breaker *and* `status.hasRemote`, and reports to the breaker (RM-10). Waits on the new `RepoStore.awaitLoadSettled()` so the gate reads a real status. | — |
| D-19 | Tauri | The `\ No newline at end of file` marker rendered its backslash twice. `linePrefix` no longer adds one — core keeps it in `content`. | DF-8's other three alignments. |

### 3.2 Defects still open

| ID | Client | Defect | Severity |
|---|---|---|---|
| D-4 | Tauri | **Terminal listener-registration race.** Output and exit listeners are registered two async IPC round trips *after* `start_terminal` returns, while the reader thread is already emitting; Tauri drops events with no listener (`Terminal.svelte:145,154,164`; `event_sink.rs:35-42`). A fast-printing shell loses its first prompt; an instantly-dying shell (the broken-`.zshrc` case the docs claim is handled) can lose `terminal-closed` entirely. Native passes the listener as an argument to the spawn — structurally immune. | High |
| D-5 | Tauri | **Config lost-update on a shared file.** `handleSave` posts the entire config captured when the overlay opened; `setProvider` spreads a possibly-hours-stale `$config` (`SettingsOverlay.svelte:47-56`; `CommitMessage.svelte:154-164`). A native-side `tab_size` change is silently reverted by the next Tauri save. Compounded by D-6. `config.toml` has **no write lock** in core (unlike `repos-state.json`). | High |
| D-6 | Tauri | **Config never re-read while running.** `resyncOnActive` doesn't call `refreshConfig` (`MainLayout.svelte:715-732`) — a native-side save never reaches a running Tauri app: theme, diff settings, auto-fetch, provider all stale with an unbounded window. Native reloads `AppConfigStore` on every activation. | High |
| D-7 | Both / core | **Empty-string AI config poisons Generate in both clients.** Tauri persists `""` for `ai_model` / `ollama_server_url`; `Some("")` is not `None`, so core's `unwrap_or` never fires — Ollama POSTs to `"/api/generate"`, Claude runs `--model ""` (`core/src/ai.rs:268-272,118-126`). One client's emptied field breaks Generate in the *other* client until a native save heals the file. | High |
| D-9 | Native | **Collapsing the terminal reflows the emulator to one row.** The zero-height frame is full-width, so SwiftTerm's degenerate-size bail (width *and* height zero) doesn't fire; the buffer reflows to `MINIMUM_ROWS = 1` and each collapse/expand cycle sends a spurious `SIGWINCH` (TerminalDock's `.frame(height: 0)` + SwiftTerm `AppleTerminalView.swift:353-356`). | Medium |
| D-14 | Native | **Stale-diff scroll: no reset on file switch.** No `ScrollViewReader` exists; `DiffRow.id` is a flat index, so switching files lands at the previous file's scroll offset (verified — no `scrollPosition`/`scrollTo` anywhere in `Sources/LeoGit`). Both Tauri and GitHub Desktop reset on file change. | Medium |
| D-15 | Native | **Copying from a diff yields garbage.** `.textSelection(.enabled)` spans the gutters, so a copy includes line numbers and `+`/`−` glyphs; tab expansion means tabs come out as spaces (`DiffView.swift:145`; `DiffLineText.swift:86-88`). GitHub Desktop rebuilds clipboard text from the model. | Medium |
| D-20 | Both | **Slow-load threshold destroys state it claims to keep**: native keeps the payload but replaces the `ScrollView` with a `ProgressView` (scroll lost — `DiffStore.swift:70-74`'s comment overstates); Tauri drops the payload entirely (full repaint + re-tokenize after every slow load). GitHub Desktop dims the old diff in place and never unmounts. | Low |

### 3.3 Standing efficiency wastes (quantified)

The user's second priority. Each is attached to a workstream; none requires a
behavior change the user would notice — except battery.

| ID | Where | Waste | Scale |
|---|---|---|---|
| E-1 | Both | `is_merging` is a separate `git rev-parse --git-dir` subprocess on every 2 s poll tick, for a boolean `get_status` could carry free (it already resolves the git dir and parses porcelain v2). | ~1 800 avoidable spawns/hour **per client** |
| E-2 | Tauri | `pollHeadSha` spawns `git rev-parse HEAD` every tick although `status.head_sha` from the same tick already holds it. | ~30 spawns/min |
| E-3 | Tauri | No visibility gating anywhere: hidden window keeps the 2 s poll (≈120 subprocesses/min if the engine doesn't throttle) and the tier scheduler fetching up to 19 remotes on a 2/5/10-min rotation, forever. Native: 30 s ladder + paused sweeps. | ~60× hidden-state cost |
| E-4 | Tauri | Three independent tier `setInterval`s collide every 10 min — up to 3 concurrent background `git fetch` + auto-fetch + a visible sweep. The "sequential" comment is only true within a tier. | 4 concurrent fetches worst case |
| E-5 | Tauri | Dropdown open fires `get_repo_identifier` + `get_last_commit_timestamp` per repo, **unbounded in parallel** — the only unbounded fan-out in either client. | 2N processes at once (N = repos) |
| E-6 | Tauri | Poll publishes a fresh `repoState` (plus two new `Set`s) every tick even when nothing changed → every subscriber re-renders every 2 s on an idle repo. Native equality-skips. | continuous idle re-render |
| E-7 | Native | Every discard/ignore triggers a **full** reload — status + up to 500-commit log + `is_merging` + a progress-bar flash — though neither can change history. Tauri does a silent status refresh. | one `git log`@500 per row action |
| E-8 | Native | Core builds phase-1 HTML (a `Vec<char>` per line) + side-by-side pairs on every native parse and the bridge discards both (`core/src/diff.rs:125-126` → `ffi/src/lib.rs:303-311`). | ~40 k allocations per 20 k-line diff load |
| E-9 | Native | Whole-status epoch re-tokenizes the open diff when *any* file changes (~19–140 ms + 2 `git show` per unrelated edit); a per-file `stat_stamp` compare would gate it. No phase-2 debounce either (Tauri: 80 ms). | up to ~140 ms background CPU per unrelated edit |
| E-10 | Native | `PathText.fittedParts` is recomputed on every body evaluation (~50 rows × log₂-probes per interaction) — TECHNICAL.md claims it's width-keyed; it isn't. | ~350 text measurements per interaction |
| E-11 | Tauri | Diff viewer mounts every row (no virtualization) and phase 2 re-parses N `innerHTML`s in one tick; no size guard in either client (GH Desktop has three). Terminal output crosses IPC as one JSON event per 4 KiB read. | unbounded in diff size; ~2 560 IPC crossings per 10 MB of terminal output |
| E-12 | Tauri | Terminal shims are sync `#[tauri::command]`s on the main thread — one hop per keystroke, and `close_terminal` blocks ~250 ms on every teardown. | visible teardown hitch |

## 4. The parity inventory

Per area: what differs, who is right, and why. Verdicts follow the priority
order in §1. Items already listed in §3 are referenced, not repeated.
"→ WS-x" names the workstream that closes each item. Where an item was a real
fork in the road, a bolded **Decided** clause carries the ruling and the
reasoning behind it; that clause is what binds the implementation.
Cross-cutting rulings that no single item owns are in §7. Behaviors the audit
checked and found equivalent are not listed — each area also produced a
parity-confirmed list (breaker numbers,
tier cadences, progress aggregation, commit semantics, seamless-diff rules,
terminal lifecycle, and so on), so an item's absence here means it was checked
and matched, not that it was skipped.

### 4.1 Repo management (RM)

- **RM-1 · No-repo state.** Native Welcome is a dead end (logo + two buttons, no
  discovery run); Tauri shows a searchable ranked picker. **Tauri right.**
  Reuse the switcher's list as the Welcome body and start discovery from
  Welcome. Adopt Tauri's "exactly one discovered repo → auto-open" rule
  (deliberately *not* after a Settings edit). Keep native's no-back-to-welcome
  model — both clients agree repos switch in place. → WS-E
- **RM-2 · Open a repo outside the scan paths.** Native has `.fileImporter`
  (Welcome ⌘O + switcher "Open Other…"); the Tauri client has **no** way to
  open such a repo except the CLI or a config edit. **Native right** (and
  GitHub Desktop's File ▸ Add Local Repository ⌘O agrees). Add "Open Other…"
  to `RepoDropdown`'s footer and the picker's empty state. → WS-C
- **RM-3 · Row-list membership.** Native unions discovery with the
  existence-checked MRU, so an Open-Other repo keeps its row across launches;
  Tauri's list is discovery-only — clones, CLI opens, and Open-Other rows all
  vanish on restart, and `last_opened_repo` restore is conditioned on
  discovery re-finding it. **Native right**; Tauri is throwing away state it
  already persists. Hoist the union rule to core (`known_repos`) and use it
  for the tier membership too — native currently tiers raw MRU entries that
  may no longer exist, burning a time-boxed fetch per dead entry per tier
  interval (the reverse defect). → WS-C, WS-B
- **RM-4 · Switcher sort order.** Native: MRU + active-first (zero
  subprocesses). Tauri: last-commit-time via one `git log -1` per repo
  (E-5), list reordering under the user as timestamps stream in, plus a
  persisted clock↔A-Z toggle the native client ignores (a shared-state hazard:
  a Tauri-set "alphabetical" silently does nothing natively). GitHub Desktop:
  alphabetical + a small Recent group. **Decided: MRU-of-use on both** — zero
  subprocesses, a list that doesn't move while you're aiming at it, and the
  signal a switcher is actually for (last-commit-time answers "where did a
  commit land most recently", which can be someone else's work you just
  fetched). Keep the persisted name toggle (native implements it — its rank
  function already has a name-ordered tail), and delete `repoActivity` +
  `get_last_commit_timestamp` (which then has no consumer). → WS-C
- **RM-5 · Row labels + search input set.** Tauri labels rows with the GitHub
  `owner/name` (colliding basenames get a muted `owner/` prefix — GitHub
  Desktop's rule) and searches over those names; native shows bare basenames
  and searches only them — so the "tier-for-tier identical" search contract
  (FRONTEND §6.9) has silently drifted on its *input set*, and Tauri's own startup
  picker disagrees with its own dropdown. **Tauri's labels right; the search
  rule belongs in core** (`match_repo`) so drift becomes impossible — which
  also gives Tauri the `$HOME` path-root for free. Port identifiers to native
  lazily (visible rows only) and bound Tauri's fan-out (E-5). → WS-B, WS-E
- **RM-6 · Switcher keyboard cursor.** Native: Return opens the first match,
  nothing else (confirmed; ROADMAP tracks it). Tauri: full ↑/↓ cursor with
  scroll-into-view across all three of its pickers. **Tauri right**; native
  gets it via `List(selection:)` in the popover. → WS-E
- **RM-7 · Empty/loading states.** Native's switcher distinguishes
  looking/none-found(+searched folders)/no-matches; Tauri's dropdown says
  "No repositories" for everything (the rich diagnosable state exists only in
  the startup picker) — and neither the native empty state nor the dropdown
  offers a "Choose folders to search" action (native's is a `SettingsLink`
  away). Port the states into the dropdown; add the CTA to both. → WS-C, WS-E
- **RM-8 · Switching mid-transfer.** Native disables the switcher while a
  network op runs; Tauri lets you switch away mid-push — the old repo's
  transfer keeps running while the new repo's header shows "Pushing…" with no
  progress, and the slot gates the new repo's polling for invisible reasons.
  **Native right** given the single global slot (GitHub Desktop allows it, but
  scopes state per repo — out of reach without per-repo op state). Same for
  Refresh/⌘R during a transfer (D-13's neighbor). → WS-C
- **RM-9 · Discovery freshness.** Native re-walks on every switcher open and
  re-reads scan paths every load; Tauri discovers once per launch, and a
  scan-path edit re-runs discovery **only** when Settings closes in the picker
  phase — from the main view it needs a restart. **Native right**; wire
  `rediscoverRepos()` into the main-phase Settings close and the dropdown
  open. One native refinement: run the walk concurrently with the badge sweep
  instead of before it. → WS-C
- **RM-10 · On-switch breaker feed.** ✅ *WS-A.* The native warm-up fetch now
  reports its outcome to the breaker like every other real attempt, in the
  extracted `ContentView.warmUpFetch` alongside D-18's gating.
- **RM-11 · Sweep re-check granularity.** Tauri re-checks the network slot
  between every repo of a sweep and bails mid-list; native's visible sweep
  checks once at entry (its tier runner *does* re-check — internal
  inconsistency). **Tauri right**; move the native guard inside the loop. → WS-G

### 4.2 Background machinery, connectivity, update checker (BG)

- **BG-1 · Cadence policy.** Native: 2/10/30 s status ladder by visibility,
  auto-fetch ×3 while hidden, sweeps paused while inactive, all under an App
  Nap assertion. Tauri: flat timers, nothing gated (E-3). **Native right
  and it is the GitHub Desktop model** (which pauses its indicator sweep on
  blur). Port to Tauri via `document.hidden`/`hasFocus()` and a
  self-scheduling `setTimeout` chain (which also delivers BG-2/BG-3 for free).
  Steal GitHub Desktop's one improvement on both: a once-per-session random
  0–30 s skew so multiple windows don't fetch in sync. → WS-D
- **BG-2 · Live re-arm of `auto_fetch` / `fetch_interval_ms`.** Native reads
  the shared config store on every tick — the store reloads at launch, on each
  save, and on activation, so a Settings change applies within one interval and
  a Tauri-side edit arrives on the next activation — and it idles on a 30 s
  re-check while disabled; Tauri's
  interval is armed at init/switch only, and `startAutoFetch(0)` clears it
  with nothing left to revive it (confirmed; ROADMAP tracks it as the auto-fetch re-arm item). → WS-D
- **BG-3 · Connectivity signal.** Native `NWPathMonitor` is authoritative;
  Tauri's `navigator.onLine` is hard-wired `true` on WebKitGTK, silently
  disabling the offline gate, the recovery kick, and the update-check retry on
  Linux (the breaker's lapsing backoff is the de-facto recovery — up to 5 min,
  not never). **Decided: build the core observer in this plan** (H-17):
  a `core::net` watcher emitting online/offline over the event seam, Linux
  backend first (netlink route watch — the broken platform, and a Linux test
  machine is available), then macOS and Windows; the Tauri client adopts it in
  WS-D, and the native client retires `NetworkPathObserver` once the macOS
  backend proves equivalent. Until it lands, `navigator.onLine` stays
  authoritative-negative only. → WS-B, WS-D
- **BG-4 · Poll equality + failure surfacing.** The failure half landed in WS-A
  (D-3: the 3-tick streak on the poll-owned `pollError` flag, reset on repo
  switch in both clients). What remains is E-6: port the native equality skip
  (a `stat_stamp`-aware fingerprint makes it free), which is also the hook for
  DF-1 (Tauri's stale open diff). → WS-D
- **BG-5 · Update checker.** Tauri-only today (confirmed: zero
  `check_for_update` references in the FFI). Everything platform-independent
  is already in core (release request, strict version compare, per-platform
  artifact gate, `install.sh` one-liner, fake-update override, five tests).
  Native needs: an async FFI export + `UpdateInfo` mirror, an app-scene-level
  checker (Tauri runs it pre-main too), and a chip. **Do not port the
  breaker gate** — gate on `isOnline` alone (the checker's own comment notes a
  GitHub API answer says nothing about git remotes, and D-2 shows the breaker
  can be open spuriously); give `NetworkPathObserver` multiple recovery
  subscribers rather than a second monitor. → WS-E
- **BG-6 · Typing guard.** Native queries the first responder at tick time
  (stateless); Tauri latches a `focusin/focusout` flag that strands `true`
  when a focused element is removed (killing a focused terminal) — auto-fetch
  silently dead for the session. Replace with an `activeElement` read at tick
  time. → WS-D
- **BG-7 · Un-occlude resync.** Tauri resyncs on visibility *and* focus;
  native only on app activation (documented — up to one 30 s beat after
  un-occluding without activating). Cheap to close: fire the existing resync
  from the policy's occlusion edge. → WS-G
- **BG-8 · Auto-fetch budget (shared).** Both clients run auto-fetch under the
  *user* network budget (15/30/600 s), not the 12 s background budget the tier
  fetches use — arguably wrong for an automatic fetch. Core-level, both
  clients inherit the fix. **Decided: scheduled here rather than filed** — it
  is a constant and a call site (H-19), cheap to take while the core is open.
  → WS-B

### 4.3 Sync & publish (SY)

- **SY-1 · Control shape.** Native: one adaptive `SyncProposal` ladder button
  (detached → Publish → Publish Branch → Pull → Push → Fetch), which **is**
  GitHub Desktop's `PushPullButton` cascade; no Refresh button. Tauri: a
  standalone Pull + a Push split-button + a Refresh button, with the ladder
  re-derived as three loose booleans. **Native/GH Desktop right** — the
  three-control layout is what *causes* SY-2 and SY-3. Port the ladder to
  Tauri; keep Tauri's on-button count badges (better than the native
  platform-forced standalone `↑N ↓N` text — that stays, documented in FRONTEND §8).
  The ladder is a pure function of `RepoStatus` → hoist to core (§5). → WS-C
- **SY-2 · Manual Fetch is unreachable in Tauri.** `gitApi.fetch`'s only call
  site is the automatic loop; the push menu never offers Fetch; the only
  user-driven way to contact the remote when in sync is a working-tree-mutating
  pull. GitHub Desktop puts Fetch in every dropdown state. Three-line interim
  fix (menu item + a `'fetch'` slot kind) worth shipping before SY-1. → WS-C
- **SY-3 · Tauri offers a push git will reject** on a diverged branch (both
  buttons enabled; the rejection lands in a blocking modal). Native's
  pull-outranks-push makes the state unreachable. Falls out of SY-1; interim:
  disable Push when behind > 0, never silently redirect. → WS-C
- **SY-4 · ⌘P semantics.** Native: the proposed action, menu item renaming
  itself. Tauri: always push/publish — **Pull has no keyboard route at all**.
  Falls out of SY-1 + the core ladder. → WS-C
- **SY-5 · Inferred counts hidden in Tauri.** Core computes ahead/behind
  against `refs/remotes/<remote>/<branch>` for unpublished branches
  (explicitly "so the Push badge updates"); native shows them in the
  publish-branch state, Tauri suppresses both badges there — a deliberate
  suppression, not a limitation. Show them (as status text; a Pull button is
  still wrong there). → WS-C
- **SY-6 · Publish dialog failure mode.** Native keeps the sheet open with
  gh's error inline and fields intact; Tauri stacks the blocking ErrorModal
  *over* the dialog (two dismissals before retrying a name collision) and has
  no progress indication beyond the button label. Native also has the
  org `owner/name` hint. **Native right**; port error-inline + indeterminate
  progress + hint. → WS-C
- **SY-7 · Force-push confirm.** Split verdict: Tauri's dialog lifetime is
  better (stays open, "Force-pushing…", one dismissal to retry a stale
  lease); native's *target naming* is correct (`status.upstream`, right even
  when the upstream branch name differs — Tauri composes `{remote}/{branch}`
  from a cached remote, wrong in that case, and spends an extra `git remote`
  per repo open purely for dialog text). Take each other's half. → WS-C, WS-F
- **SY-8 · Post-op refresh.** Native reloads status+log+`is_merging` so a
  pull's commits appear immediately; Tauri reloads status only and History
  catches up ≤2 s later via the poll. The collapse half landed in WS-A (D-13:
  one `refreshStatus`, which now carries `is_merging`), so what remains is
  making post-op = status **+ log** — one call added beside it. → WS-C
- **SY-9 · Chevron contents.** Tauri's menu in the publish states duplicates
  the face (a chevron revealing only what the button already says); GitHub
  Desktop hides the dropdown for publish-repo and offers Fetch for
  publish-branch — exactly native's shape. Falls out of SY-1/SY-2. → WS-C
- **SY-10 · Transfer error surface.** Tauri renders git's multi-line rejection
  in a selectable `<pre>`; native's `.alert` collapses whitespace and can't be
  copied (D-15's sibling). Route native sync failures to the selectable
  banner, or make the alert text monospaced + selectable. → WS-F
- **SY-11 · Progress presentation.** Native full-width strip with a real
  indeterminate state; Tauri in-button fill (closer to GH Desktop) with **no**
  indeterminate rendering — publish and (future) fetch show a spinner over a
  permanently empty bar. Keep each shape (document in FRONTEND §8); give Tauri the
  indeterminate case. → WS-C, §9

### 4.4 Branches & merge (BR)

- **BR-1 · Merge UI is dead code in Tauri** (verified: nothing ever sets
  `showMerge = true`; `mergeTarget` is never written; `countCommitsToMerge`
  has zero callers). The native client ships the full flow: source submenu →
  sheet with commit-count preview → Merge / Squash & Merge → conflicts as
  data → Abort Merge. **Port to Tauri** (shape per platform, not verbatim);
  two native refinements while there: hide the submenu while `isMerging`, and
  adopt GitHub Desktop's zero-count treatment ("already up to date" + disabled
  primary — native currently says "Brings in 0 commits." with a live button).
  → WS-C
- **BR-2 · Abort merge has no Tauri UI** — a user who *enters* a merge from
  the terminal sees the MERGING badge with no in-app exit. Arguably ahead of
  BR-1 in priority; ~15 lines against the already-polled `isMerging`. → WS-C
- **BR-3 · Branch-list freshness.** Native reloads on every menu open, on
  HEAD move, after undo/checkout, on ⌘R; Tauri reloads on exactly five sites
  — not on dropdown open, not from the poll — so a branch created in the
  embedded terminal can be invisible for the whole session. **Native right**
  (one cheap `for-each-ref` at the moment of intent). → WS-C
- **BR-4 · Busy state.** Tauri's dropdown has none — double-clicks issue
  overlapping checkouts that contend on `index.lock`; a slow checkout gives
  no feedback. Native serializes with `isBusy` — but its `run` helper returns
  `nil` for "dropped because busy", which callers read as success; fix while
  porting. → WS-C, WS-F
- **BR-5 · Same-branch re-select.** Tauri runs a redundant checkout + full
  refresh chain (~8 processes) when you click the branch you're on; native
  guards. One-line fix. → WS-G
- **BR-6 · Create-branch failure.** Tauri clears the typed name *before* the
  outcome and routes the error to the global modal over a closed dropdown;
  native keeps the sheet open with the error inline. **Native/GH Desktop
  right.** → WS-C
- **BR-7 · Delete confirmation.** Both `-D`; only native's dialog says
  "Unmerged commits are lost." Tauri's hover-only ✕ is also invisible to
  keyboard users. Adopt native's wording; a branch-row context menu (already
  proposed in QUICK-WINS) fixes discoverability and is the natural host for a
  future rename. GitHub Desktop's "also delete on the remote" checkbox is the
  shared target, deferred with BR-10 to a ROADMAP item that builds it on both
  clients at once (core has `delete_remote_branch`; a combined
  `delete_branch(…, include_remote)` keeps ordering semantics in one place).
  → WS-C
- **BR-8 · Detached/merging markers.** Native rides the branch chip's label;
  Tauri shows an icon swap + two yellow badges. Both platform-appropriate —
  **document as a FRONTEND §8 row** — but native's `· merging` suffix is easy to miss
  and truncates first; give it the same color treatment as the conflicted
  badge. → §9, WS-F
- **BR-9 · `is_merging` fold-in.** E-1: add `merging: bool` to `RepoStatus`
  in core (populated from the git dir `get_status` already resolves), keep the
  standalone command for compatibility. Removes a subprocess per tick per
  client *and* the whole class of "refresh path forgot isMerging" bugs (D-13).
  Highest-leverage single change in the area. → WS-B
- **BR-10 · Rename / remote-delete plumbing.** Tauri carries registered,
  wrapped, never-called commands; the FFI's dead-surface rule (don't export
  what nothing calls) is the better policy — the dead wiring is exactly how
  BR-1 rotted unnoticed. **Decided: defer the feature, delete the wiring** —
  rename is a genuine gap in both clients (today you rename in the terminal),
  but building it is new feature work, which this plan excludes; leaving dead
  wrappers standing is how BR-1 stayed broken. Delete the Tauri wrappers now
  (with `hasMergeConflicts` and the three other unconsumed derived stores);
  ROADMAP carries rename + delete-on-remote as one build-on-both-clients
  feature, homed in BR-7's branch-row context menu and backed by the combined
  core `delete_branch(…, include_remote)`. Branch management still reaches a
  reasonable bar inside this plan — BR-1…BR-8 and BR-11 are unaffected by the
  deferral. → WS-H
- **BR-11 · Large branch lists.** Native's `Menu` gets scrolling +
  type-select from AppKit; Tauri's hand-rolled dropdown has no filter, no
  keyboard nav, un-keyed `#each` — and DESIGN.md claims it matches the
  repo picker, which has both. Reuse the picker's `listNavigation` machinery.
  → WS-C

### 4.5 Changes tab & commit flow (CH)

- **CH-1 · Multi-select + bulk actions.** Tauri (and GH Desktop): row range
  selection with a separate checkbox anchor, bulk Space toggle, "Discard N
  Selected Changes…". Native: single-selection by construction — recorded as a
  FRONTEND §8 divergence, with §6.4's shared floor reduced to arrow-key row
  activation because of it. Close it natively
  (`List(selection: Set<String>)`); the efficiency case is real too (a 30-file
  discard is ~90 subprocesses + 30 reloads natively vs ~3 + 1 in Tauri).
  Delete the FRONTEND §8 row. → WS-F
- **CH-2 · Space / keyboard toggle.** Native has **no keyboard route to
  include/exclude a file at all** (the highest-frequency action in the app);
  Tauri and GH Desktop toggle on Space, bulk-toggle in a selection. → WS-F
- **CH-3 · Select-all header.** Tauri: true tri-state (the documented
  contract); native: a binary toggle that lies the moment one file is
  unchecked — but native's "3 of 12 files included" label beats Tauri's
  "12 changed files" (which duplicates the tab pill). Combine: tri-state +
  native's label, both clients. → WS-F, WS-C
- **CH-4 · Status badge style.** The conflicted letter differs between the
  clients (`U` in Tauri, `!` natively): **verdict `U`** — git's own porcelain
  letter, which is the vocabulary native's own comment claims to follow. Hoist
  `letter()`/`label()` onto core's `FileStatus` so the glyphs can't drift
  again. **Decided: conflicted gets its own purple status token** — a new
  light + dark pair in STYLE.md's status palette rather than reusing red,
  because red is already Deleted and a glance down the list has to separate
  "you deleted this" from "git couldn't merge this": opposite actions, and one
  of them blocks the commit. The token gets a second consumer immediately in
  BR-8's easily-missed `· merging` suffix. **Decided: the native 18×18 tinted
  plate behind the letter stays** and becomes the shared look — STYLE.md's old
  no-plate rule was a leftover of the earlier TUI-flavored direction, macOS
  style is the design authority today, and the rule is rewritten to describe
  the plate; the Tauri client adopts the plate here. → WS-F, WS-C
- **CH-5 · Rename display.** Tauri and GH Desktop render `old → new`
  (STYLE.md mandates it); native shows only the destination —
  indistinguishable from an add. Also missing in the native diff header
  (DF-8) and commit file list. → WS-F
- **CH-6 · Embedded/submodule row treatment.** Tauri swaps the status glyph
  for ↪ (the documented style); native appends a width-eating text tag. Adopt
  the glyph. → WS-F
- **CH-7 · Exclusion-set semantics.** The clients disagree and the native
  comment claims they don't: Tauri prunes a vanished path's opt-out every
  tick (DESIGN.md documents it); native persists it (safer failure mode —
  a file you excluded stays excluded through a formatter flicker; Tauri's can
  silently re-include and commit a file you deliberately excluded) but
  unbounded. **Decided: native semantics on both + prune after a grace of N
  consecutive absent ticks** (≈30 s at the visible cadence). The failure modes
  aren't symmetric: an over-long exclusion costs one visible checkbox click,
  a premature prune costs a commit you didn't mean to make and never saw
  happen — and the grace window bounds the set without reopening the flicker
  hole. Hoist the reconciliation to core (H-20; written twice today, already
  drifted). → WS-B
- **CH-8 · Discard confirmation copy.** Native names the actual per-file
  outcome (restored from HEAD vs moved to Trash) — what FRONTEND §6.10 asks for; Tauri
  states both rules generically and dismisses on backdrop click (STYLE.md
  violation). But native *guesses* the outcome from status while core decides
  it authoritatively via `ls-tree` — they can disagree on odd renames. Hoist
  `classify_discard` from core's own logic; both clients then state the real
  outcome. Native also lacks Tauri's in-flight busy state. → WS-B, WS-C, WS-F
- **CH-9 · Embedded-repo confirm.** Tauri's copy is better (names the outer
  repo, states the clone consequence, "Commit as link" verb); native's system
  `confirmationDialog` is the right container. Merge: native container +
  Tauri text; fix Tauri's backdrop-cancels-mid-commit guard. → WS-F, WS-C
- **CH-10 · Composer details.** Port to native: the 72-char summary counter
  (STYLE.md; skip Tauri's silent 200-char hard cap — it truncates pasted
  and AI-generated summaries), the included-row weight cue, tooltip only when
  truncated, keyboard resize on the handle (ROADMAP's composer-resize item), an in-flight
  "Committing…" label. Port to Tauri: window-wide ⌘↩/⌘G (the field-scoped
  version defeats the point; the a11y-lint excuse doesn't apply to a window
  listener), the height clamp against short windows (STYLE.md — cap the
  drag as well as the render so the stored height stays reachable), and the
  provider-revert (ST-7) — D-10's lockout landed in WS-A. Native: coalesce the per-drag-frame
  `UserDefaults` writes to drag-end. → WS-F, WS-C
- **CH-11 · Row-action errors.** Native's non-blocking banner beats Tauri's
  modal for non-decisional failures ("couldn't reveal file"); Tauri is
  internally inconsistent (commit failures whisper inline, reveal failures
  seize the window). Split by class in both: user-action failures → modal
  with retry; background/informational → banner (see also SH-8). → WS-C, WS-F
- **CH-12 · Copy File Path.** Tauri does an async IPC round trip to
  concatenate two strings; git paths are always `/`-separated and the
  in-repo join helper already exists. → WS-G
- **CH-13 · First-file auto-select.** Native and GH Desktop auto-open the
  first changed file; Tauri lands on an empty pane (its own commit-detail
  pane auto-selects — internal inconsistency). Port to Tauri; pairs with
  DF-1 so the auto-selected diff stays fresh. → WS-C

### 4.6 Diff viewer (DF)

- **DF-1 · Open-diff freshness (Tauri).** `stat_stamp` reaches the Tauri
  client on every poll and is never read (verified). Adopt the reload — but
  per-file (compare the active file's stamp), which is *better* than native's
  whole-status epoch: it also fixes E-9 on the native side by gating the
  reload on the open file's own stamp. The FRONTEND §8 staleness row then retires.
  → WS-D (Tauri), WS-G (native gate)
- **DF-2 · Side-by-side.** Tauri-only (sanctioned FRONTEND §8 row). Two facts change
  the calculus: core already computes `sbs_pairs` on the native path and the
  bridge throws them away (E-8), and GitHub Desktop treats split/unified as a
  **per-diff header control, not a Settings preference**. **Decided: build the
  native split view now, cleanly** — parity is meant literally and the data is
  already being computed, but the build must not fork the renderer: one row
  model feeds both arrangements, the pairs cross the bridge only when the
  split layout is active (H-8 stops producing them otherwise), and the toggle
  moves into the diff header in both clients. It is the largest single piece
  of new native UI in the plan and sizes WS-F accordingly. → WS-F
- **DF-3 · Structured wire.** FRONTEND §7's open decision (where the HTML
  collapse lives) should close **toward the structured token wire for both**
  — that is GitHub Desktop's own shape (worker tokens, view-side collapse),
  it halves Tauri's per-diff IPC, and it unblocks Tauri virtualization.
  Related core fixes regardless of timing: stop building HTML+pairs on the
  native path (E-8); make `DiffLine.text` an `Option` (it duplicates every
  line's content and is read only for Hunk/NoNewline rows — halves parsed-diff
  memory in both clients); fuse the raw-read+parse round trip into
  `get_parsed_diff` / `get_parsed_commit_diff`. → WS-B
- **DF-4 · Virtualization + size guards.** Tauri mounts every row.
  GitHub Desktop solves the exact variable-height
  problem Tauri gave up on (measured-height cache). Neither client has any
  large-diff guard; GH Desktop has three (70 MB buffer / ~4.4 MB "reasonable"
  / 5 000 chars-per-line → a "Show Diff anyway" state). **Decided: guard only
  in this plan** — the two fixes sit at very different sizes, and only the
  guard is cheap: it makes the pathological diff survivable and explained
  instead of a hang, with thresholds in core (H-15) so both clients inherit
  one policy. Virtualization only smooths the merely-large diff, so it becomes
  a ROADMAP item recording GitHub Desktop's measured-height approach, why it
  is a web-only concern (SwiftUI's `List`/`LazyVStack` already virtualize — no
  native work exists there), and that it is re-judged after DF-3's structured
  wire changes how rows are built, then taken only if large diffs still feel
  heavy. → WS-B
- **DF-5 · Dead per-line-selection scaffolding (Tauri).** Confirmed
  unreachable end to end (props hard-coded false, store field never written,
  patch commands uncalled) — and it costs today: hunk headers are focusable
  no-op buttons whose text can't be selected, one tab stop per hunk on an
  unvirtualized list. **Decided: the scaffolding stays** — per-line
  staging is the unfinished GitHub Desktop feature, and ROADMAP now commits
  to finishing it in the Tauri client and porting it natively (core's
  `build_patch` is complete and tested). Until it's wired, neutralize the
  cost: hunk headers stop being focusable buttons and their text becomes
  selectable. → WS-C (interim), ROADMAP (finish + port)
- **DF-6 · Model-based copy** (D-15). Adopt GitHub Desktop's approach in both
  clients: rebuild clipboard text from the line model (immune to gutters,
  prefixes, wrapping, side-by-side interleaving, and native's tab expansion,
  since `line.content` keeps real tabs). Interim: Tauri `user-select: none`
  on the `+/-` prefix; native scope `.textSelection` to the content text.
  A core helper ("plain text of a flat line range") keeps the two clients
  byte-identical. → WS-F, WS-C, WS-B
- **DF-7 · Empty-parse reason** (D-12). WS-A gave the Tauri client the *state*
  (both diff panes now render "No Textual Changes" rather than the
  no-selection copy), which is where the reason plugs in. Core still returns a
  bare `None` for both "empty input" and "no hunks"; return a reason
  (`NoTextualChanges | WhitespaceOnly | Empty`) so both clients render an
  honest message — the whitespace case matters because hide-whitespace makes
  it common — and add a *load failed* variant, or the Tauri branch keeps
  claiming "No Textual Changes" behind DF-10's error. → WS-B, WS-C
- **DF-8 · Header details.** Tauri shows `old → new` for renames (native:
  nothing — source it from the parsed diff, which describes what's rendered);
  Tauri suppresses `+0 −0` (native shows it on binary diffs, misleading);
  STYLE.md's `−` (U+2212) is what native uses and Tauri doesn't. Align the
  three (D-19's doubled `NoNewline` row was the fourth; fixed in WS-A).
  → WS-F, WS-C
- **DF-9 · Slow-load presentation** (D-20). Converge on GitHub Desktop: never
  unmount the old diff — dim it and overlay the spinner. Fixes native's
  scroll loss and Tauri's full repaint in one shape. Also write the
  scroll contract into FRONTEND §6.3: *same file → keep scroll; different file → reset*
  (D-14 is the native half). → WS-F, WS-C
- **DF-10 · Failure surface.** Native clears the stale payload and shows an
  inline pane error (right on safety); Tauri leaves the stale diff rendered
  behind a blocking modal (wrong on both). Inline + clear, both. → WS-C
- **DF-11 · Dirty-submodule pane.** Tauri explains ("Submodule changes …");
  native renders git's raw `Subproject commit …-dirty` — the one place the
  guard chain breaks (STYLE.md mandates the pane). Branch before the
  load and skip the pointless subprocess — in both (Tauri fetches then
  discards). → WS-F, WS-G
- **DF-12 · Phase-2 debounce.** Tauri debounces highlighting 80 ms; native
  starts a tokenize per file survived while arrowing. Add the same 80 ms +
  generation re-check natively; promote the constant next to
  `slowLoadThreshold`. → WS-G
- **DF-13 · Wrap break policy (risk, unverified).** Both Tauri
  (`overflow-wrap: anywhere`) and GH Desktop (`word-break: break-all`) force
  character-level breaking; native relies on SwiftUI `Text` defaults — a
  minified/base64 line may overflow the pane with no horizontal scroll to
  reach it. Needs one visual check; fix via `.byCharWrapping` or zero-width
  break insertion in the existing tab-expansion pass. → WS-F (check first)

### 4.7 History (HI)

- **HI-1 · HEAD gating** — ✅ *WS-A.* D-1: both clients now gate the rewriting
  actions on `status.head_sha`, and the Tauri context menu no longer carries a
  row index at all.
- **HI-2 · Log windowing.** Tauri's 500-row bidirectional window is the right
  *memory* policy; native's append-only "row 0 is always HEAD" is the right
  *correctness* policy (it's what makes D-1 impossible) — but native's
  truncate-to-500-on-HEAD-move discards the user's scrolled depth with no
  compensation. **Decided: append model, bounded from the tail only, both
  clients** — keep today's page sizes (50 in Tauri, 100 natively) and the
  500-commit retention cap, dropping only the oldest rows, so row 0 stays HEAD
  by construction. That keeps the property that makes D-1's bug class
  structurally impossible rather than merely fixed, and bounds memory at the
  far end of the list, away from HEAD. It retires the FRONTEND §8 paging row
  (§6.8 keeps only the shared invariants: the 500-commit refresh cap and
  refetch-on-HEAD-move). On HEAD move both clients prepend and scroll to top —
  WS-A already built the Tauri half of that signal for D-11 (`log.resetSeq`
  marks a window *replacement*, which `CommitList` answers by scrolling to row
  0 instead of compensating), so the append conversion inherits it rather than
  inventing one. → WS-F, WS-C
- **HI-3 · Selection behavior.** Native auto-selects the newest commit on
  entry and re-seats when the selected sha disappears (post-amend); Tauri
  lands on an empty pane and keeps rendering a *rewritten-away* commit's
  stale detail after an amend. Port both rules. Also: Tauri's right-click
  doesn't move the selection (menu describes B while the pane shows A — its
  own FileList re-selects; internal inconsistency). → WS-C
- **HI-4 · Loading/empty gating.** Tauri gates "No commits yet" on the first
  load finishing (native flashes it before the first `get_log` lands);
  native's detail pane distinguishes empty-repo from no-selection (Tauri
  invites selecting a commit in an empty repo). Take each other's half. → WS-C, WS-F
- **HI-5 · Relative dates.** Tauri re-ticks; native is a snapshot (its comment
  says so), and FRONTEND §8 now carries the difference. Add a visibility-gated
  10 s tick natively (reuse `BackgroundSchedulingPolicy` — don't invent a
  second gate) and retire that row; pin the
  tier vocabulary in FRONTEND §6.11 (the clients currently render "yesterday" vs
  "1 day ago"). Align the detail card's date format (Tauri's shows raw
  `toLocaleString()` seconds; both should use the abbreviated form — and
  both show the **author** date while DESIGN.md says committer; fix the doc).
  → WS-F, §9
- **HI-6 · Commit-list keyboard.** The one list in the Tauri app with no
  arrow navigation (every row a tab stop — also an a11y problem); native gets
  it free from `List`. Port `FileList`'s pattern. → WS-C
- **HI-7 · Detail loads.** Fuse `get_commit_files` + `get_commit_stats` into
  one `get_commit_detail` (two near-identical `git log -1` invocations —
  halves subprocesses per selection in both clients and removes the
  error-policy split: native shows a failure state, Tauri silently renders
  "No changes"). Port the error state to Tauri. Guard Tauri's re-select
  (currently blanks and refetches on clicking the selected row). Key native's
  detail task on `(repoPath, sha)` not sha alone (latent cross-repo defect;
  the `LoadKey` pattern exists one file over). Clear native's `commits` on
  repo switch (it briefly shows the previous repo's history). → WS-B, WS-C, WS-F
- **HI-8 · Paging.** A failed page opens a blocking modal mid-scroll in Tauri
  (demote to non-blocking); Tauri's paging sets repo-wide `isLoading`, which
  disables the Commit button on the other tab (give it its own flag); native
  fetches with zero prefetch margin (trigger at N−5) and pays E-7's full
  reload on row actions. Tauri's trailer list renders twice (body already
  contains trailers). Tag chips: STYLE.md specifies the neutral treatment —
  native's accent capsule diverges from its own unpushed plate two lines
  away. → WS-C, WS-F, WS-G
- **HI-9 · Checkout busy state.** Tauri holds the dialog with "Checking
  out…" and suppressed Escape; native dismisses instantly with no feedback
  and nothing preventing a second checkout. **Tauri right.** → WS-F
- **HI-10 · Undo details.** Tauri's "Undo last commit…" ellipsis promises a
  dialog that never appears (drop it — no-confirm is defensible for
  `--mixed`); Tauri re-seeds `lastHeadSha` after undo so the next poll
  doesn't redundantly refetch (native should copy); native reloads branches
  after undo for no reason (a `--mixed` reset can't change the branch list).
  → WS-C, WS-G

### 4.8 Clone & gh (CL)

- **CL-1 · Reachability.** Tauri: clone is unreachable with no repo open (the
  entry lives inside the main-phase dropdown) — the first-run user most
  likely to want it can't get to it. Native: reachable from Welcome and the
  switcher; but its entry sits under the switcher's transfer-disable (cloning
  a different repo contends with nothing — clone deliberately claims no slot
  in either client). Fix both: a picker-phase entry in Tauri; a menu-item /
  un-disabled entry natively. → WS-C, WS-F
- **CL-2 · List caching.** Native refetches `gh repo list` on every sheet
  open (a 20 s dead zone each time, and the filter is disabled during it);
  Tauri caches once per app run with no refresh affordance (stale until
  restart). GitHub Desktop: cache **plus** an always-visible refresh button —
  adopt that on both. Keep the filter live during loads (Tauri/GH Desktop are
  right). → WS-C, WS-F
- **CL-3 · Keyboard.** Native's GitHub tab is mouse-only (no autofocus, no
  cursor, no Enter-to-select — FRONTEND §6.9's first-row-acts-on-Return applies);
  Tauri has the full combobox pattern. Conversely Return-to-clone works
  natively (`defaultAction`) and does nothing in Tauri (no form). Port each
  other's half; GH Desktop's Enter-on-row-clones is the finishing touch.
  → WS-C, WS-F
- **CL-4 · URL/name derivation has drifted** (the "ported verbatim" pair):
  `.git`-on-shorthand handled differently, whitespace trimmed only natively
  (an untrimmed Tauri path creates literal `" …"` directories and persists the
  poisoned `last_clone_dir`), empty-destination preview shows `/repo` in
  Tauri. Both share two latent bugs (`owner/repo/` and scheme-less
  `github.com/…` enable Clone then fail). **Hoist:
  `derive_clone_target(raw_url, parent) -> {normalized_url, repo_name,
  target_path}`** in core beside `prepare_clone_target` (not inside it —
  separate responsibilities), unit-tested against the full matrix; gate the
  Clone button on a real parse. Single highest-value structural fix in the
  area. → WS-B
- **CL-5 · Mid-clone state.** Native freezes every input (correct); Tauri
  leaves tabs/filter/rows/URL/destination editable — clicking another repo
  mid-clone rewrites the "Clones into…" preview to a lie. One `<fieldset
  disabled>`. → WS-C
- **CL-6 · Progress.** Native always shows motion (indeterminate for gh
  clones); Tauri shows nothing for a gh clone but the button label.
  Port the indeterminate bar as the interim. **Decided: scheduled here rather
  than filed** — `gh repo clone` accepts pass-through git args, so `gh_clone`
  reuses the streaming seam `git clone` already has (H-18) and both clients
  get real progress instead of an indeterminate bar. → WS-C, WS-B
- **CL-7 · Small deltas.** Empty-state discrimination (no repos vs no
  matches — native right); description tooltip (native surfaces what both
  fetch); per-tab error state (GH Desktop's shape; native currently shows a
  URL-tab failure over the GitHub tab); persist the *tab*, reset the *inputs*
  across opens (GH Desktop's split — native resets everything, Tauri
  persists everything including a stale selection); sort-collation nit
  (diacritic-insensitive is friendlier; also add a stable tiebreak natively —
  Swift's sort isn't stable and equal names can flicker). Filter-then-sort +
  memoize natively (currently sorts 200 rows per keystroke per body pass).
  → WS-C, WS-F, WS-G
- **CL-8 · `check_auth`.** Tauri spawns `gh auth status` on every launch to
  write a field with **zero readers** (the PR feature that consumed it was
  retired); the FFI deliberately doesn't export it and gh's own error text
  ("Run `gh auth login`") is the better UX. Delete the call + wrapper; drop
  the command from the contract (surface 68) or record the exemption. → WS-H

### 4.9 Settings, config, AI (ST)

- **ST-1 · The field matrix.** All 15 `Config` fields audited (full matrix in
  the research notes). Live-apply: native applies auto-fetch/interval within
  one tick and diff settings immediately; Tauri needs a restart or repo
  switch for auto-fetch (BG-2) and never sees cross-client edits (D-6).
  Dead fields: `claude_timeout_secs` is read by **nothing** (core hardcodes
  120 s) yet Tauri ships a live-looking control for it; `ai_api_key` is
  mapped but no provider reads it. A control that persists a value nobody
  reads is worse than no control — the user believes the timeout is set — so
  ST-8's restructure wires `claude_timeout_secs` through `AiProviderConfig`
  into both providers and drops `ai_api_key`. → WS-B
- **ST-2 · Save semantics.** Native: load-fresh-overlay-write everywhere.
  Tauri: whole-object write-back (D-5). **Hoist `patch_config` into core
  with a lock** (the file two clients share currently has none) — that makes
  the pattern structural rather than a per-frontend discipline. Plus
  `Config::normalized()` (clamp numerics, empty-strings → absent) inside
  core's save so D-7/D-17-class poison can't land regardless of writer, and
  the AI entry points treating empty-after-trim as absent so already-poisoned
  files heal. → WS-B
- **ST-3 · Surface model.** **Decided: the Tauri client moves to
  instant-apply, matching native** — the simplest and least-code option: each
  control patches its own field through H-10's `patch_config` as it changes,
  the whole-object save (and D-5 with it) is deleted, Save/Cancel becomes a
  single Close, Escape just closes. The half-typed-value risk Cancel used to
  guard is covered instead by clamp-on-write (H-10 / ST-4) and parse-at-save
  fields; scan paths, the one destructive text input, is the exception to
  field-level instant-apply and locks behind ST-10's Edit ▸ Done cycle on
  both clients. Native still adopts Tauri's load-failure
  handling (don't render editable defaults that aren't the user's settings);
  D-8's lost text edit is already fixed. Port native's per-section footers ("Applies to the open diff
  immediately") to Tauri — honest only once BG-2 lands, which is a feature of
  the suggestion. → WS-C, WS-F
- **ST-4 · Units and bounds.** Native shows fetch interval in seconds (better;
  keep ms on the wire); Tauri still shows raw ms. Both clamp on save now
  (D-17), but in **three** disagreeing places — `SettingsStore`'s three ranges
  in seconds, `SettingsOverlay`'s `BOUNDS` in milliseconds, and core's
  `default_*` — which is exactly what H-10's `Config::normalized()` collapses.
  Native's load-clamp also floors to 1 s while its own control starts at 5 —
  clamp to the range. → WS-C, WS-F
- **ST-5 · `ai_provider` ownership.** Native has **two independent owners**
  (composer's CommitStore and SettingsStore) that never observe each other —
  with both windows open the pickers can disagree, and a Settings save of any
  unrelated field silently reverts a composer-side provider change. Tauri's
  single `$config` store is the shape to copy.
  Route both native surfaces through `AppConfigStore` (which exists precisely
  to be the single owner — and grow it the `scanPaths` accessor three other
  call sites currently bypass it for). → WS-F
- **ST-6 · AI mapping duplication.** The Config→`AiProviderConfig` mapping
  exists in the bridge (Rust) *and* still inline in `CommitMessage.svelte` —
  "moved" was actually "duplicated", and the pinning test asserts Rust
  against a described TS behavior it can't watch. Move the mapping into
  `core/src/ai.rs`, expose `load_ai_config` through a Tauri shim, delete the
  TS copy. → WS-B
- **ST-7 · Provider save failure.** Native reverts the picker; Tauri leaves
  the optimistic value lying until restart. Port the revert (4 lines). → WS-C
- **ST-8 · One model field, two providers.** Set `sonnet`, switch to Ollama,
  Generate fails — a shared design flaw; GitHub Desktop stores per-provider
  models. **Decided: split per provider, restructured cleanly** —
  backwards compatibility is explicitly waived (sole user; the config file
  can be regenerated), so the config gains a real per-provider shape (a
  Claude section with model + timeout, an Ollama section with model + server
  URL) instead of a migration shim; `ai_api_key` goes away in the same pass;
  unknown old keys keep parsing (`config_ignores_retired_keys`). The UI
  follows: each provider remembers its own model, so switching provider in
  the composer never requires a Settings trip. → WS-B, WS-C, WS-F
- **ST-9 · `check_provider_available`.** Dead wrapper in Tauri, deliberately
  unexported natively. **Decided: use it rather than delete it** — a dead
  command path is exactly how BR-1 rotted, and the probe earns its keep: gate
  Generate with a cheap `claude --version` at composer mount ("Claude CLI not
  found" beats failing after a long request), and export it natively too.
  → WS-C, WS-F
- **ST-10 · Scan-path editor.** **Decided: locked by default on both
  clients** — the field renders read-only with an **Edit** button beside it —
  the macOS list-editor pattern — Edit enables it, the button becomes
  **Done**, and Done parses, applies through `patch_config`, and locks the
  field again. Nothing touches the config until Done, so closing or Escape
  mid-edit simply discards the draft; no confirmation popup anywhere. Give
  the native field `.monospaced()`; keep parse-at-save on both (Tauri's
  parse-on-input transiently desyncs the textarea from the model).
  → WS-C, WS-F

### 4.10 Terminal (TE)

- **TE-1 · Key routing** — the routing half landed in WS-A (D-16: the shell
  owns every key but the panel's toggle, FRONTEND §6.11). What remains is the
  modifier: Tauri still accepts ⌘` too, hijacking macOS window cycling; native
  is deliberately ⌃` only. Narrow Tauri to Ctrl — in *two* places now, the
  window handler and `Terminal.svelte`'s custom key handler, which must keep
  agreeing or the chord becomes unreachable from inside the panel.
  WS-A's modifier-blind rule leaves ⌘,/⌘B/⌘L/⌘R/⌘P inert with the terminal
  focused, which is right for `Ctrl` (the shell really does want `Ctrl+R`) and
  wrong for `Cmd` (no shell consumes it, and macOS reserves ⌘, for Preferences).
  **Decided by the user: the modifier follows the platform** — ⌘ on macOS, Ctrl
  on Windows and Linux — which resolves the capture as a side effect rather than
  as a second rule. **Not scheduled here**: the narrowed capture is correct on
  the shipping platforms and only imperfect on a macOS Tauri build, so it ships
  as-is and reopens if it is actually noticed. ROADMAP carries the decision with
  the affected-chord table. → WS-C (the toggle's own ⌃-only narrowing)
- **TE-2 · Transport** — D-4 plus E-11/E-12: move Tauri to a
  frontend-created `Channel` passed into `start_terminal` (mirrors the
  native seam, kills the race and the per-chunk JSON), mark
  `start`/`close`/`resize` `(async)` (leave `write` sync — IPC arrival order
  is the keystroke-ordering guarantee). **Core hoist**: an optional
  coalescing flush (8 KiB / 8 ms) in the shared reader thread serves both
  hosts and simplifies the native relay; add a byte ceiling with *flow
  control* (pause PTY reads, never discard — native's relay is currently
  unbounded, xterm discards at 50 MB). → WS-C, WS-B
- **TE-3 · Collapse/resize** — D-9 (native emulator reflow; fix by pinning
  the inner frame) and the missing native 80 ms resize debounce (a divider
  drag is one SIGWINCH per column crossed today — put the coalescing in
  `TerminalController.resize`, not the delegate, to keep the one-shot
  initial-size push). Core: `resize_terminal` should reject `cols/rows < 2`
  itself — unbypassable for every host, instead of one explicit guard
  (native) and one accidental one (Tauri relying on FitAddon internals).
  → WS-F, WS-B
- **TE-4 · Scrollback.** 500 (native) vs 1000 (Tauri) — both library
  defaults, neither chosen. Set 1000 explicitly on both (`git log --stat`
  exceeds 500; VS Code ships 1000). → WS-F
- **TE-5 · Links + OSC 52.** Plain-click URLs work in Tauri, ⌘-click-only
  natively. **Decided: modifier-click on both, taught on hover** — the
  Terminal.app / iTerm convention wins over the plain-click web one, and the
  discoverability worry that argues for plain click is answered by the
  affordance instead of by dropping the modifier: keep SwiftTerm's ⌘-click,
  move the Tauri client from plain click to
  Ctrl/⌘-click, and both surface the convention on hover the way other
  terminals do ("Follow link (⌘ + click)": xterm's link-provider hover
  callback drives a tooltip; SwiftTerm's hover surface needs an API check —
  a tracking-area overlay if it exposes none). Becomes a shared FRONTEND §6
  rule, not a §8 row. OSC 52 clipboard works natively (write-only —
  correct), is ignored by Tauri (add the handler, write-only). → WS-F, WS-C
- **TE-6 · Refocus.** Confirmed, and ROADMAP tracks it: Tauri never refocuses after
  focus is stolen (only a click); native has the same call sites but AppKit
  restores the first responder. Add the `focusin` + reactivation handlers.
  → WS-C
- **TE-7 · Small parity.** Header label fallback ("Terminal") when no session
  (native has it; also mostly obsoletes ROADMAP's expand-hint idea); the "280"
  constant means dock-height in Tauri and emulator-height natively (~2 rows
  difference) — pick one meaning; shell preference read fresh per session
  natively (a native-side Settings change doesn't reach a running Tauri —
  read the config in `initBackend`); ⌃` needs a native menu-bar home
  (View ▸ Show/Hide Terminal owning the chord). → WS-C, WS-F

### 4.11 App shell (SH)

- **SH-1 · CLI launch, single-instance, init prompt** — the largest native
  contract gap. Core's half is done and framework-free. Native work:
  export `resolve_launch_target`/`set/take_pending_launch_target` +
  `init_repo` + `is_git_repo`; claim the target in the Welcome task ahead of
  `restoreLastRepo`; warm start via `NSApplicationDelegate.application(_:open:)`
  + a `CFBundleDocumentTypes` entry for `public.folder` — which also buys
  drag-a-folder-onto-the-Dock-icon and Finder "Open With" for free (GitHub
  Desktop's `open-file` equivalent), and lets FRONTEND §8's launch row stop
  reading "Tauri only";
  the "Create a repository here?" confirmation on the root view so it works
  over Welcome and over an open repo. `install.sh` needs a native branch
  (`open -a LeoGit --args "$dir"` — note the shell function currently points
  at a bundle id the native app doesn't use). LaunchServices gives
  single-instance for free — cheaper than Tauri's socket. → WS-E
- **SH-2 · Menu bar as the discovery surface.** Native's adaptive-⌘P menu
  approach is right and structurally more robust (menu key equivalents beat
  the first responder — the exact class of Tauri's D-16). Extend it: File ▸
  Open/Clone (fill the emptied `newItem` group), View ▸ ⌘1/⌘2 tabs (GH
  Desktop's absolute bindings beat Tauri's ⌘L toggle — add ⌘1/⌘2 to Tauri
  too), a Branch menu with ⌘B (Tauri has ⌘B; native has none — bind via the
  focused-scene-value pattern, not a toolbar shortcut, which never reaches
  the scene), View ▸ Show/Hide Terminal (TE-7). With those, native needs no
  `?` overlay — but today ⌘G/⌘↩/⌃`/⌘O exist only as button equivalents,
  discoverable nowhere. Tauri on macOS should eventually get a real
  `tauri::menu`; out of this plan's scope beyond recording it. → WS-E, WS-C
- **SH-3 · ⌘R.** Native: full reload, guarded against transfers. Tauri:
  status-only, unguarded (races a pull's lock files — the poll next to it
  pauses for exactly that reason), and swallowed while any field has focus.
  → WS-C
- **SH-4 · Escape.** Tauri's global stack is duplicated in two files (already
  drifted) and closes *all* overlays at once; fold into one topmost-closing
  stack. Native's per-surface AppKit handling is fine. → WS-C
- **SH-5 · Error model.** Split by class in both (CH-11): native's
  non-dismissible background banner needs a dismiss ✕; Tauri needs to stop
  seizing the window for informational failures and should finally pass the
  `onRetry` its ErrorModal already accepts (ROADMAP's `RetryAction` is
  the shared target). → WS-C, WS-F
- **SH-6 · Window.** Tauri: add `tauri-plugin-window-state` (opens 1280×800
  every launch today; native gets restoration free) and set the window title
  to the repo name (QUICK-WINS item; match native's value). Min-size
  disagreement (720×460 vs 900×600) isn't worth converging. → WS-C
- **SH-7 · Tab behavior.** Native preserves the active tab across repo
  switches (a view preference — right); Tauri resets to Changes as an
  accident of `defaultState` and remounts the history pane. Native loses the
  commit-list scroll position on tab round trips (Tauri keeps both panes
  mounted — its trade); close it with a `ScrollViewReader` restore to the
  hoisted selection instead of keeping subtrees alive. → WS-C, WS-F
- **SH-8 · Pre-main phases.** Tauri's loading/error-with-Retry phases are
  right; native will need a scan-failure surface on Welcome once RM-1 lands
  (inline row + Retry, not a phase swap). Native's deliberate silence about a
  missing restored repo stays. → WS-E

## 5. Core-hoist catalogue

Everything above that moves into `leogit-core`, collected. Rule of thumb
applied: hoist when the logic is pure, duplicated (or about to be), and
IPC-cost-free; keep per-platform when it's presentation or host-lifecycle.
None of these sacrifice measurable performance; several *save* subprocesses.

| # | Hoist | Replaces | Feeds |
|---|---|---|---|
| H-1 | `RepoStatus.merging: bool` filled by `get_status` (keep `is_merging` for compat) | one subprocess per tick per client (E-1) + the forgot-isMerging bug class | WS-B |
| H-2 | `get_remote` returns no-remote honestly (stop inventing `"origin"`; move the assumption to the publish path) | D-2's dead guard, D-18's doomed fetches | WS-B |
| H-3 | `sync_proposal(&RepoStatus) -> SyncProposal` (the ladder as a total function; titles/icons stay per-platform) | native `SyncControls` derivation + Tauri's three loose booleans; makes ROADMAP's force-push-recommended a one-place change | WS-C |
| H-4 | `derive_clone_target(url, parent)` beside `prepare_clone_target`, matrix-tested | the drifted TS/Swift pair + two shared latent bugs (CL-4) | WS-B |
| H-5 | `match_repo(query, path, names, scan_folders)` (the FRONTEND §6.9 rule) | two implementations already drifted on input set (RM-5); gives Tauri the home-dir root free | WS-B |
| H-6 | `known_repos(scan_paths, depth, recents)` (discovery ∪ existence-checked MRU) | Tauri's forgotten rows + native's dead-MRU tiering (RM-3) | WS-B |
| H-7 | `get_commit_detail(repo, sha)` (one `git log -1 --name-status --numstat`) | two subprocesses per commit selection per client + the error-policy split (HI-7) | WS-B |
| H-8 | `get_parsed_diff` / `get_parsed_commit_diff` (fuse read+parse); split parse from render (stop building HTML/sbs on demandless paths); `DiffLine.text: Option` | E-8, half of Tauri's per-diff IPC, the double traversal in both (DF-3) | WS-B |
| H-9 | Empty-parse reason enum | D-12 + native's generic copy (DF-7) | WS-B |
| H-10 | `patch_config` + a config write lock + `Config::normalized()`; AI entry points treat `""` as absent | D-5, D-7, D-17 structurally | WS-B |
| H-11 | Config→`AiProviderConfig` mapping into `core/src/ai.rs`; per-provider config sections (clean restructure, no migration shim, `ai_api_key` dropped — ST-8); `claude_timeout_secs` wired into both providers | the duplicated mapping + the untestable pin (ST-6) + the shared-model footgun (ST-8) | WS-B |
| H-12 | `classify_discard(repo, files) -> {restore, trash}` from core's own `ls-tree` logic | native's guessed dialog copy; upgrades Tauri's generic copy free (CH-8) | WS-B |
| H-13 | `FileStatus::letter()` / `label()` | three-way glyph drift (CH-4) | WS-B |
| H-14 | Terminal reader-thread coalescing (8 KiB/8 ms) + bounded buffer with flow control; `resize_terminal` rejects `< 2×2` | E-11/E-12 transport waste, native's unbounded relay, the FitAddon-internals dependency (TE-2/TE-3) | WS-B |
| H-15 | Large-diff thresholds (bytes / per-line chars) with a "show anyway" escape | the missing size guard in both (DF-4) | WS-B |
| H-16 | Copy-text helper (flat line range → plain content) | byte-identical clipboard in both (DF-6) | WS-B |
| H-17 | `core::net` connectivity observer emitting online/offline over the event seam — Linux netlink backend first, then macOS/Windows | Tauri's hard-wired `navigator.onLine` on WebKitGTK; eventually native's separate `NetworkPathObserver` (BG-3) | WS-B, WS-D |
| H-18 | `gh_clone` through the `git clone` streaming seam (pass-through progress args) | the progress-less gh clone in both clients (CL-6) | WS-B |
| H-19 | auto-fetch moved under the 12 s background network budget | an automatic fetch holding the single slot on the 15/30/600 s user budget (BG-8) | WS-B |
| H-20 | exclusion-set reconciliation (keep an opt-out through a grace window, drop it after N absent ticks) | the two hand-written, already-drifted exclusion rules (CH-7) | WS-B |

Deliberately **not** hoisted: sort collation (locale into a chrono-free core —
no), relative-date formatting (platform), scheduling policy (host lifecycle),
tab-expansion (single consumer), disambiguation labels (small, but H-6-adjacent
if it ever grows).

## 6. Workstreams

Per CLAUDE.md: one at a time, user-flow order inside each, visually verified
before the next starts. Every workstream ends with the §8 gates. Sizes are
relative (S/M/L).

1. ~~**WS-A — Defect burn-down (S/M).**~~ **Shipped 2026-08-27.** All thirteen
   items landed — D-1, D-2's client gate, D-3, D-8, D-10, D-11, D-12, D-13,
   D-16, D-17, D-18, D-19 and RM-10 — each summarized in §3.1 with what it left
   for a later workstream. Four pieces of state the plan did not anticipate,
   each because a gate is only as good as what it can see:
   `repoState.log.resetSeq` (a *replacement* of the commit window is not a
   *slide*, and D-11 cannot be fixed without saying which one happened),
   `repoState.statusLoaded` (D-2's gate reads `hasRemote`, which defaults to
   false, so "not loaded" had to be distinguishable from "no remote"),
   `RepoStore.awaitLoadSettled()` with a load-depth count (D-18's gate would
   otherwise read the `nil` status that `.task(id: repoPath)` races, or be
   released early by a `refresh()` nested inside an `open()`), and
   `SyncStore.silentFetch` returning `Bool?` (D-18 added a third breaker feed,
   and feeding it a *local* failure is the same poisoning D-2 is about). One new
   shared file, `apps/tauri-app/src/lib/utils/keyboard.ts`, holds D-16's rule.
   Gates: `pnpm check` 0/0, prettier clean, zero-warning `xcodebuild`, 120 core
   + 24 bridge tests, clippy-pedantic at the 184 baseline. (D-4/D-5/D-6/D-7/D-9/
   D-14/D-15/D-20 always needed their area's structure and remain in B/C/F.)
2. **WS-B — Core convergence layer (L). ← next.** H-1 … H-20 (in the table's
   order; H-2/H-10/H-11 carry D-2/D-5/D-7 to ground; H-17's Linux backend gets
   verified on the user's Linux machine). Mostly pure-Rust + bridge work, all
   test-covered; regenerated bindings; both hosts adopt as the later
   workstreams touch each surface. Tests pin every new function; clippy
   baseline unchanged. What WS-A changed under it:
   - **H-1** has exactly one Tauri adopter now: `MainLayout.refreshStatus` is
     the client's only status writer (D-13), so folding `merging` into
     `RepoStatus` deletes one `gitApi.isMerging` call there and two natively
     (`RepoStore.loadRepoData` and `refreshQuietly`).
   - **H-2** must update three call sites, not one: `MainLayout`'s
     `fetchActiveRemote` and `SyncStore.silentFetch` both call `get_remote`
     unconditionally *after* a `has_remote` gate WS-A added — keep the gates
     (they skip a subprocess), and drop the now-dead `if (!remote) return`.
     `Header`'s pull/push path is the third, and is the one place the
     `"origin"` assumption legitimately belongs (per the hoist's own note).
   - **H-9** lands in three render sites, all now shaped the same way: the
     Tauri changes pane, the Tauri commit pane, and native `DiffView`. Give the
     enum a variant for *load failed* as well, or DF-10 has to arrive with it —
     the Tauri branch currently cannot tell an empty parse from a failed load
     and will claim "No Textual Changes" behind the error modal.
   - **H-10**'s `Config::normalized()` replaces **three** bound tables that must
     already agree: `SettingsStore.fetchIntervalRange/scanDepthRange/
     tabSizeRange` (native, seconds), `SettingsOverlay`'s `BOUNDS` (Tauri,
     milliseconds), and core's own `default_*` functions. Collapsing them to one
     is most of this hoist's value; note the unit difference at the boundary.
   - **H-14**'s `resize_terminal` guard is unaffected by WS-A, but the terminal
     touched in D-16 is the same file — `Terminal.svelte` now owns an
     `attachCustomKeyEventHandler`, which TE-2's `Channel` rewrite must keep.
3. **WS-C — Tauri catches up (L).** The Tauri client adopts what the native
   client (and GH Desktop) got right: SY-1…SY-9 (adaptive ladder on H-3,
   fetch, publish dialog), BR-1…BR-7 + BR-11 (merge/abort/freshness/busy),
   DF-1 (stat-stamp reload), CH-3/CH-9/CH-11/CH-13, HI-3/HI-4/HI-6/HI-8's
   Tauri halves, CL-1/CL-2/CL-5/CL-6/CL-7's Tauri halves, ST-3/ST-4/ST-7,
   TE-1/TE-2/TE-6/TE-7's Tauri halves, SH-3…SH-7's Tauri halves, RM-2/RM-4/
   RM-7/RM-8/RM-9, CH-4's Tauri half (plate + `U`), ST-9's Generate gate,
   DF-5's interim neutralize.
4. **WS-D — Tauri background parity (M).** BG-1 (cadence ladder + sweep
   pause + skew), BG-2 (self-scheduling re-arm), BG-4's remaining half (the
   equality gate — the failure streak landed in A), BG-6, D-6 (activation config reload),
   E-2 (drop `pollHeadSha`), E-4 (single tier loop), E-5 (bounded fan-out —
   already halved by RM-4's `repoActivity` deletion), and H-17 adoption
   (`navigator.onLine` demoted to a negative-only hint). Mostly deletion and
   one structural shape (the timeout chain).
5. **WS-E — Native launch & shell (L).** SH-1 (CLI/init/open-repo — the
   contract debt), RM-1 (Welcome discovery + sole-repo rule + picker states),
   RM-5/RM-6 (labels + cursor), BG-5 (update checker), SH-2 (menu bar: File
   Open/Clone, ⌘1/⌘2, ⌘B, terminal item), SH-8.
6. **WS-F — Native lists, composer, diff polish (L).** CH-1/CH-2 (multi-select
   + Space), CH-3…CH-6, CH-8…CH-10's native halves, HI-2's native half +
   HI-5/HI-9 + HI-8's native bits, DF-2 (the native split view — the largest
   single UI piece here), DF-6/DF-8/DF-9/
   DF-11/DF-13 native halves, D-14/D-15/D-20, ST-5/ST-10 + ST-3's native
   half, TE-3/TE-4/TE-5/TE-7 native halves, SY-7/SY-10, BR-4's nil-return
   fix + BR-8, SH-5/SH-7 native halves, CL-3/CL-7 native halves.
7. **WS-G — Efficiency sweep (S/M).** Whatever of E-6…E-10 didn't land with
   its area: native refresh-scope split (E-7), per-file stamp gate + 80 ms
   debounce (E-9/DF-12), `PathText` caching (E-10), BR-5, CH-12, RM-11,
   BG-7, HI-10's branch-reload removal, DF-11's skipped subprocess.
8. **WS-H — Contract, dead surface, source comments (S/M).** The docs'
   factual corrections are already applied (§2); what remains is the work that
   can only land *with* the code:
   - **FRONTEND.md** — retire each §8 row the parity work closes (file-list
     selection and keyboard, history paging, relative-date ticking and the
     launch row as WS-C/E/F close them; the staleness row per DF-1) and add
     the divergences this plan keeps: counts placement, progress surface,
     error surface, loading presentation, detached/merging markers, settings
     surface. The terminal link convention becomes a shared §6 rule
     (TE-5: modifier-click on both), not a §8 row.
   - **Dead-surface deletions** — CL-8's `check_auth`, BR-10's rename and
     remote-delete wrappers (the feature is deferred, the wiring goes now),
     the `has_staged_changes` wrapper, the unconsumed derived stores. DF-5's
     scaffolding stays and ST-9's probe gets wired, so neither is deleted.
     Each deletion also drops a row from FRONTEND §3's command tables.
   - **Stale source comments** — outside the reach of a docs pass, all
     verified: `CommitStore.swift:20-23` (claims the Tauri client doesn't
     prune exclusions — it does); `SyncControls.swift:9-11,131` (a window
     subtitle that no longer exists); `TerminalStore.swift:32` (says ⌘` for a
     ⌃` binding); `CloneSheet.swift:8-10` (input-freezing parity that isn't
     there); `ContentView.swift:154-159` (warm-up-fetch "parity" — Tauri's is
     connectivity-gated, native's isn't); `TerminalSessionView.swift:124-126`
     (the skipped resize protects the PTY, not SwiftTerm's own reflow — D-9);
     `BackgroundSchedulingPolicy.swift:7` ("the Tauri client runs everything
     always" — it has no gating at all rather than a deliberate always);
     `repoSyncScheduler.ts:66-70` (sequential within a tier only).
   - **Doc claims outside the audit's checklist**, fixed as their area lands:
     TECHNICAL's width-keyed `PathText` cache (E-10 — it isn't); DESIGN's
     claim that the Tauri branch dropdown matches the repo picker (BR-11);
     DESIGN's committer-vs-author date for commit rows (HI-5); DESIGN's
     header-cluster list (ahead/behind are badges *on* the Pull/Push buttons,
     not a separate indicator).

Suggested order: **A → B → C → D → E → F → G → H**, with H's doc rows also
maintained incrementally as each workstream lands (per CLAUDE.md). C before E
because the Tauri client is further from the shared bar and its fixes are
mostly adoptions of already-proven native behavior.

## 7. Standing decisions

Rulings that cut across areas and therefore belong to no single item. Every
other decision lives inline with the item it governs, marked **Decided** in §4.

- Parity target per §1: whichever client has the better behavior wins;
  GH Desktop breaks ties. Both-wrong cases converge on GH Desktop's shape
  where cited (slow-load dimming, model-based copy, clone-list refresh,
  merge zero-count, per-diff SBS toggle placement).
- The dead-surface rule (don't ship wrappers/exports nothing calls) is now
  FRONTEND §1's policy — each host exposes what it consumes and records its
  exemptions, replacing the old "all 69 wrappers on both sides" mandate that
  neither host obeyed. The native FFI already works this way; Tauri's dead
  wiring is how BR-1 rotted.
- History converges on the bounded-append model (HI-2), not Tauri's sliding
  window.
- The exclusion-set rule is native semantics + grace pruning (CH-7).
- `wrap_long_lines`-style key retirements continue via
  `config_ignores_retired_keys` for the fields ST-8's restructure drops
  (`ai_api_key`, the shared `ai_model`).
- Backwards compatibility of `config.toml` is explicitly waived: the app has a
  single user who can regenerate the file, so config changes restructure
  cleanly instead of carrying migration shims.

## 8. Verification gates

Per workstream, matching the previous plan's bar:

- Zero-warning `xcodebuild` via `just mac-build`; `pnpm check` (svelte-check)
  0/0; `cargo test --workspace` green (baseline 120 core + 24 bridge — WS-B
  grows it; every hoist lands with tests); `cargo clippy --workspace
  --all-targets -- -W clippy::pedantic` at the 184 baseline or better, never
  worse.
- Visual checks per workstream (ask for confirmation, no screenshots), on
  **both** clients whenever a change touches shared core or a ported
  behavior — the checklist comes from the workstream's inventory items.
  WS-A's destructive items (D-1) additionally get a scripted repro against a
  throwaway repo before/after.
- DF-13's wrap check is a named visual item (a minified file in both
  clients) before WS-F closes.

## 9. Documentation updates on completion

The audit's factual corrections are already in the living docs (§2). What each
document still needs is the update that rides its workstream — per CLAUDE.md,
written as each chunk lands, no duplication between documents:

- **FRONTEND.md** — the contract carries the most: every §8 row a parity item
  closes is deleted rather than annotated, and each divergence this plan keeps
  gets a row (WS-H). FRONTEND §3's command tables lose every deleted wrapper;
  FRONTEND §5.2 tracks the diff wire as DF-3 changes it, and FRONTEND §7's
  open decision closes with it.
- **TECHNICAL.md** — new mechanics paragraphs only for genuinely new machinery
  (the core hoists, the Tauri channel transport, the native launch path), plus
  the claims WS-H lists as their areas land.
- **DESIGN.md** — flow 1 stops being Tauri-scoped once WS-E lands; the per-flow
  client hedges retire as parity closes them.
- **STYLE.md** — the status-letter row settles on `U` + the purple token when
  CH-4 lands (the tinted-plate rule is already rewritten); the header-strip
  bullet collapses to one description when SY-1 converges the two headers.
- **ROADMAP.md** — items close as their workstreams land; the deferrals this
  plan makes (per-line staging, diff virtualization, branch rename +
  delete-on-remote) are already filed there, so the plan itself adds nothing
  new.
- **README.md** — the merge scoping goes away once WS-C gives the Tauri client
  a reachable merge flow.

## 10. Findings log (out of scope here, worth keeping)

- GH Desktop's `BackgroundFetchMinimumInterval` idea (don't re-fetch a repo
  fetched < 30 min ago): neither client tracks last-fetched-per-repo;
  switching between two repos re-fetches both every time. Candidate for the
  tier scheduler after parity.
- GH Desktop's mergeability preview (`git merge-tree` conflict count before
  merging) — a genuine convenience neither client has; pairs with a future
  `merge_preview` core function (BR-3's cousin).
- A Tauri macOS `tauri::menu` (SH-2) — the platform-respect follow-up once
  the shortcut surface stabilizes.
- The `install.sh` bundle-id mismatch (SH-1) breaks *any* scripted launch of
  the native app today — fixed as part of WS-E's CLI work, noted here because
  it affects packaging beyond this plan.
