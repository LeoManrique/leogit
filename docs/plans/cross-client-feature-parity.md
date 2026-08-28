# Plan — Cross-client feature parity (SwiftUI ⇄ Tauri)

> Status: **in progress — WS-A, WS-B, WS-C, WS-D and WS-E shipped (2026-08-27), WS-F is next.**
> The remaining work was re-cut into seventeen smaller workstreams (C…S) on
> 2026-08-27 after WS-B proved too large to review as one piece — see §6.
> Accepted 2026-08-27 with every open decision resolved. Per-workstream state
> is in §6; what a shipped workstream found that the next one needs lives in
> that workstream's §6 entry, not here.
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
  reached that way and are carried in WS-S: stale comments in Swift and
  TypeScript source, and a few doc claims outside the audit's checklist.

## 3. Defect register — fix before parity work

These are behaviors that are wrong *today*, ranked by severity. All verified
in code. "Make the current features work well" starts here (workstream A).

### 3.1 Defects — fixed

Seventeen of the twenty are closed: twelve in WS-A, D-5 / D-7 / D-17's
structural half with their hoists in WS-B, D-6 in WS-C, and D-20 in WS-E. Kept
as a register (IDs are referenced from §4) and trimmed to what each fix *is*,
since the code now carries the reasoning.

| ID | Client | Fixed | Left over |
|---|---|---|---|
| D-1 | Tauri | **Destructive.** Amend/Undo/Checkout gated on the row's index into the *loaded window*, so past a slide Undo reset the real HEAD and seeded the composer from another commit. Now gated on `status.head_sha`, per FRONTEND §6.10. | — |
| D-2 | Tauri | A remote-less repo's doomed `git fetch origin` opened the breaker against every other repo. `fetchActiveRemote` now gates on `status.hasRemote`, like the tier path already did — and on the new `statusLoaded`, since `hasRemote` defaults to false and an unqualified read would decide "no remote" about a repo nobody has looked at yet. Natively, `silentFetch` returns `Bool?` so a slot conflict or a local `git remote` failure stops being reported as a network failure. | — (WS-B: `get_remote` answers `Option`, so the guard is live rather than dead). |
| D-3 | Tauri | Silent poll failures vanished forever. Three consecutive **background** failures now raise a non-blocking banner off `repoState.pollError` — native's shape and threshold, and its ownership: `refreshStatus` grew a `background` opt separate from `silent`, because four of the seven silent callers are user actions whose own `index.lock` races would otherwise accuse a healthy repo. Reset per repository in both clients. | BG-4's equality gate (the other half of that item). |
| D-8 | Native | ⌘W with a text field still focused dropped the typed value. `flushPendingSave` now also writes an edit that never scheduled a save, guarded by a diff against `lastPersisted` — which holds the *normalized* form of the fields, not the raw file, or a config written by the other client would be rewritten on an open-and-close that changed nothing. A completed debounce also clears `pendingSave` now (generation-guarded), which it never did. | — |
| D-10 | Tauri | A commit could land mid-Generate and have the late result overwrite the cleared composer. `canSubmit` gained `!isGenerating`, and the lockout runs off `isCommitInProgress` — `isCommitting` is still false while the embedded-repo confirmation waits, and its Confirm calls `performCommit` past `canSubmit` entirely, so the composer stayed live behind the dialog. | — |
| D-11 | Tauri | The HEAD-move reset was read as a backward slide and scrolled to the bottom of the fresh page, paging again. `log.resetSeq` marked the replacement. ✅ *WS-E* finished it: HI-2's append model deleted the slide, so `resetSeq` now means only "go to row 0" and the `skip > 0` hole it kept (a new commit while parked at offset 0 bumped nothing) has no case left to miss. | — |
| D-12 | Tauri | An empty parse fell through to "Select a file to view its diff" with a file selected. Both diff panes now have an explicit "No Textual Changes" state, blank while the fetch is in flight. The test is `hasRenderableDiff`, not `!== null`: `parse_diff` returns null only for empty input, while a mode change or pure rename parse into a header with zero hunks — a blank pane, the same dead end one layer along. | — (WS-B: H-9 supplies the reason, and a failed load is an `Err` rather than an empty parse; WS-E: DF-10's presentation half). |
| D-13 | Tauri | The header hand-rolled a status write that skipped `is_merging`, the `userDeselected` reconciliation, and the badge feed. It now takes `refreshStatus` as a prop: **one status writer in the client**. Checkout and undo also reload branches. | SY-8's "post-op = status + log". |
| D-16 | Tauri | `Ctrl+P` reached the shell *and* pushed; ⌃` could not leave a focused terminal; Escape closed overlays instead of reaching `vim`. One rule now (FRONTEND §6.11): `attachCustomKeyEventHandler` releases only the toggle, and the window handlers test the event's origin. | TE-1's modifier narrowing (Tauri still accepts ⌘` too). |
| D-17 | Tauri | `tab_size: 999` and emptied fields persisted (and the emptied ones failed the save with a raw serde error). WS-B replaced the form's own clamp with `Config::normalized()`, which every writer passes through — including ones that never see this form — and whose bounds the controls now read (`config_bounds`) instead of restating. | — |
| D-18 | Native | The warm-up fetch ran offline, and against remote-less repos, discarding its outcome. Now gated on the breaker *and* `status.hasRemote`, and reports to the breaker (RM-10). Waits on the new `RepoStore.awaitLoadSettled()` so the gate reads a real status. | — |
| D-19 | Tauri | The `\ No newline at end of file` marker rendered its backslash twice. `linePrefix` no longer adds one — core keeps it in `content`. | — (WS-E took DF-8's minus sign; the two remaining alignments are native's, WS-P). |
| D-20 | Both | **Slow-load threshold destroyed the state it claimed to keep.** ✅ *WS-E*: crossing it now dims the pane and overlays a spinner in both clients instead of replacing its contents — Tauri through a shared `SeamlessDiffPane` wrapper, native through `.opacity` + `.overlay` on `content` rather than a branch beside it. The native comment claiming scroll survived was false for exactly this reason: a branch gives SwiftUI a different view to build, so the `ScrollView` was destroyed and rebuilt at the top and the store's equality skip was preserving something nothing could see. | — |
| D-5 | Tauri | **Config lost-update on a shared file.** A save posted the whole config as it looked when the dialog *opened*, so a native-side `tab_size` change was silently reverted. `patch_config` (H-10) is now the only writer: a surface names the fields it owns and cannot touch the rest, and core reads-edits-writes under a lock the file never had. | — |
| D-6 | Tauri | **Config never re-read while running** — a native-side save reached a running Tauri window never, so theme, diff settings, auto-fetch and provider stayed at their launch values for the lifetime of the app. WS-C: `resyncOnActive` calls `refreshConfig` first, before the refreshes that consume it. D-5 stopped this client from *clobbering* the shared file; this is the other half — reading it. | BG-2's live re-arm of the fetch timer (a config read still doesn't restart the interval) → WS-J. |
| D-7 | Both / core | **Empty-string AI config poisoned Generate in both clients.** `Some("")` is not `None`, so `--model ""` and a hostless Ollama URL sailed past every `unwrap_or`. `Config::normalized()` (H-10) treats blank-after-trim as absent on every read and every write, so an already-poisoned file heals on first load whichever client opens it. | — |

### 3.2 Defects still open

| ID | Client | Defect | Severity |
|---|---|---|---|
| D-4 | Tauri | **Terminal listener-registration race.** Output and exit listeners are registered two async IPC round trips *after* `start_terminal` returns, while the reader thread is already emitting; Tauri drops events with no listener (`Terminal.svelte:145,154,164`; `event_sink.rs:35-42`). A fast-printing shell loses its first prompt; an instantly-dying shell (the broken-`.zshrc` case the docs claim is handled) can lose `terminal-closed` entirely. Native passes the listener as an argument to the spawn — structurally immune. | High |
| D-9 | Native | **Collapsing the terminal reflows the emulator to one row.** The zero-height frame is full-width, so SwiftTerm's degenerate-size bail (width *and* height zero) doesn't fire; the buffer reflows to `MINIMUM_ROWS = 1` and each collapse/expand cycle sends a spurious `SIGWINCH` (TerminalDock's `.frame(height: 0)` + SwiftTerm `AppleTerminalView.swift:353-356`). | Medium |
| D-14 | Native | **Stale-diff scroll: no reset on file switch.** No `ScrollViewReader` exists; `DiffRow.id` is a flat index, so switching files lands at the previous file's scroll offset (verified — no `scrollPosition`/`scrollTo` anywhere in `Sources/LeoGit`). Both Tauri and GitHub Desktop reset on file change. | Medium |
| D-15 | Native | **Copying from a diff yields garbage.** `.textSelection(.enabled)` spans the gutters, so a copy includes line numbers and `+`/`−` glyphs; tab expansion means tabs come out as spaces (`DiffView.swift`; `DiffLineText.swift:86-88`). GitHub Desktop rebuilds clipboard text from the model. WS-E took the Tauri half of the interim (`user-select: none` on the gutter and prefix); native's `.textSelection` is still pane-wide. | Medium |

### 3.3 Standing efficiency wastes (quantified)

The user's second priority. Each is attached to a workstream; none requires a
behavior change the user would notice — except battery.

| ID | Where | Waste | Scale |
|---|---|---|---|
| E-1 | ✅ *WS-B.* `merging` rides on `RepoStatus`, answered by a filesystem read of `<repo>/.git` rather than a subprocess (H-1). The waste it named was real; its explanation was not — `get_status` never resolved the git dir. | was ~1 800 spawns/hour **per client** |
| E-2 | Tauri | `pollHeadSha` spawns `git rev-parse HEAD` every tick although `status.head_sha` from the same tick already holds it. | ~30 spawns/min |
| E-3 | Tauri | No visibility gating anywhere: hidden window keeps the 2 s poll (≈120 subprocesses/min if the engine doesn't throttle) and the tier scheduler fetching up to 19 remotes on a 2/5/10-min rotation, forever. Native: 30 s ladder + paused sweeps. | ~60× hidden-state cost |
| E-4 | Tauri | Three independent tier `setInterval`s collide every 10 min — up to 3 concurrent background `git fetch` + auto-fetch + a visible sweep. The "sequential" comment is only true within a tier. | 4 concurrent fetches worst case |
| E-5 | Tauri | **Half closed in WS-C**: RM-4's MRU sort deleted the `get_last_commit_timestamp` call with the store that made it. Dropdown open still fires `get_repo_identifier` per repo, unbounded in parallel — the only unbounded fan-out left in either client. | was 2N processes at once, now N (N = repos) |
| E-6 | Tauri | Poll publishes a fresh `repoState` (plus two new `Set`s) every tick even when nothing changed → every subscriber re-renders every 2 s on an idle repo. Native equality-skips. | continuous idle re-render |
| E-7 | Native | Every discard/ignore triggers a **full** reload — status + up to 500-commit log + `is_merging` + a progress-bar flash — though neither can change history. Tauri does a silent status refresh. | one `git log`@500 per row action |
| E-8 | ✅ *WS-B.* `DiffOptions` makes the render artifacts opt-in, so the native path no longer builds HTML and pairings for the bridge to drop (H-8). `DiffLine.text` became `Option` in the same pass, dropping a duplicate of every line's content from both wires. | was ~40 k allocations per 20 k-line diff load |
| E-9 | Native | Whole-status epoch re-tokenizes the open diff when *any* file changes (~19–140 ms + 2 `git show` per unrelated edit); a per-file `stat_stamp` compare would gate it. No phase-2 debounce either (Tauri: 80 ms). | up to ~140 ms background CPU per unrelated edit |
| E-10 | Native | `PathText.fittedParts` is recomputed on every body evaluation (~50 rows × log₂-probes per interaction) — TECHNICAL.md claims it's width-keyed; it isn't. | ~350 text measurements per interaction |
| E-11 | Tauri | Diff viewer mounts every row (no virtualization) and phase 2 re-parses N `innerHTML`s in one tick. **Half closed in WS-B**: the size guard landed in core (H-15), and terminal output now coalesces under back-pressure instead of crossing once per 4 KiB read (H-14). Virtualization is the ROADMAP item DF-4 defers. | terminal half closed; virtualization deferred to ROADMAP |
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
  model — both clients agree repos switch in place. **The same change removes
  Welcome's `Open Repository…` / ⌘O** (RM-2): it exists only because there is
  no list here yet, and until the list replaces it, it is the sole way into a
  repository on a machine with no `last_opened_repo`. Neither half ships
  without the other. → WS-L
- **RM-2 · Open a repo outside the scan paths.** **Decided: neither client gets
  a per-folder open action**, GitHub Desktop's File ▸ Add Local Repository ⌘O
  notwithstanding. A repo list is *what the scan paths cover*, so a local
  repository missing from it means the paths are wrong; sending the user to
  Settings fixes the cause and holds next launch, where a one-off open patches
  one symptom and invites the list to disagree with its own configuration. The
  empty state's "Choose folders to search" CTA is the sanctioned route, and a
  repo genuinely outside every scan path still arrives by clone or
  `leogit <dir>` and then keeps its row via RM-3's MRU union. ✅ *WS-C*: neither
  switcher offers one — the Tauri client never had it, and native's footer
  action is gone, leaving both footers `Clone Repository…` alone.

  **One entry point survives, and only because it is load-bearing.** Native's
  Welcome screen has no discovery list at all (RM-1), no toolbar, and no
  `leogit <dir>` (SH-1) — so its `Open Repository…` / ⌘O is not a second way
  into a list, it is the *only* way in on a machine with no
  `last_opened_repo`. **It goes as part of RM-1, in the same change that makes
  Welcome the discovery picker** — removing it before that strands a fresh
  install on a clone-only screen. → WS-L
- **RM-3 · Row-list membership.** Native unions discovery with the
  existence-checked MRU, so an Open-Other repo keeps its row across launches;
  Tauri's list is discovery-only — clones, CLI opens, and Open-Other rows all
  vanish on restart, and `last_opened_repo` restore is conditioned on
  discovery re-finding it. **Native right**; Tauri is throwing away state it
  already persists. ✅ *WS-B*: the union rule is core's (`known_repos`) and both
  clients call it, so a clone, a CLI open or an Open-Other repo keeps its row
  across restarts and a path that no longer exists loses one — which also stops
  native tiering dead MRU entries and burning a time-boxed fetch on each per
  tier interval (the reverse defect). ✅ *WS-C* for the rest: the
  `last_opened_repo` restore now tests that the path is still a repository
  instead of that this launch's walk re-found it, and lists it itself when the
  union didn't — a repo outside the scan paths that aged out of the MRU's cap
  used to drop the user into the picker with the repo they were just in
  missing from it.
- **RM-4 · Switcher sort order.** Decided: **MRU-of-use on both** — zero
  subprocesses, a list that doesn't move while you're aiming at it, and the
  signal a switcher is actually for (last-commit-time answers "where did a
  commit land most recently", which can be someone else's work you just
  fetched). ✅ *WS-C* for the Tauri half: the dropdown ranks active → MRU index
  → name-ordered tail, native's rank function in TS, and `repoActivity` +
  `get_last_commit_timestamp` are deleted with their last consumer — which is
  also half of E-5. The persisted clock↔A-Z toggle stays; **native still
  ignores it** (a Tauri-set "alphabetical" silently does nothing there), which
  is the shared-state hazard left. → WS-L
- **RM-5 · Row labels + search input set.** Tauri labels rows with the GitHub
  `owner/name` (colliding basenames get a muted `owner/` prefix — GitHub
  Desktop's rule) and searches over those names; native shows bare basenames
  and searches only them — so the "tier-for-tier identical" search contract
  (FRONTEND §6.9) has silently drifted on its *input set*, and Tauri's own startup
  picker disagrees with its own dropdown. **Tauri's labels right; the search
  rule belongs in core**. ✅ *WS-B*: `match_repo` and the batch `filter_repos`
  both hosts call replaced the two drifted files, which are deleted. Tauri
  gained the `$HOME` path-root and native gained case- and
  separator-normalized prefix matching (its raw `hasPrefix` silently made the
  whole absolute path searchable whenever a scan folder's case differed).
  Native still searches one label — porting identifiers to it lazily (visible
  rows only) and bounding Tauri's fan-out (E-5) is what's left. → WS-L
- **RM-6 · Switcher keyboard cursor.** Native: Return opens the first match,
  nothing else (confirmed; ROADMAP tracks it). Tauri: full ↑/↓ cursor with
  scroll-into-view across all three of its pickers. **Tauri right**; native
  gets it via `List(selection:)` in the popover. → WS-L
- **RM-7 · Empty/loading states.** Native's switcher distinguishes
  looking/none-found(+searched folders)/no-matches; Tauri's dropdown said
  "No repositories" for everything, with the rich state only in the startup
  picker. ✅ *WS-C* for Tauri: one shared `RepoListEmptyState` answers all three
  in both lists, and the "Choose folders to search" CTA is on **both** dead
  ends, not only the empty one — "none matched" is what you see when the repo
  you want lives somewhere discovery was never pointed at. (The "looking" state
  is unreachable in the dropdown by construction: the open repo is always
  listed there. It is live in the picker, the phase that can have none.)
  Native still has no CTA at all. → WS-L
- **RM-8 · Switching mid-transfer.** **Native right** given the single global
  slot (GitHub Desktop allows it, but scopes state per repo — out of reach
  without per-repo op state). ✅ *WS-C*: the Tauri switcher chip disables on
  `activeNetworkOp`, with a title saying why. Switching away used to leave the
  old repo's transfer running while the new repo's header read "Pushing…" with
  no progress, and gated the new repo's polling for invisible reasons.
  Refresh/⌘R during a transfer (D-13's neighbor) is the other half.
  → WS-F (⌘R with SH-3)
- **RM-9 · Discovery freshness.** **Native right.** ✅ *WS-C* for Tauri:
  `services/repoDiscovery.ts` re-walks on dropdown open and on Settings close
  in *both* phases (button, Escape and ⌘, all route through one handler), with
  a single in-flight pass shared rather than duplicated and the open repo
  re-added if a walk racing the fire-and-forget MRU write would drop its row.
  The main view used to need a restart for a scan-path edit or a terminal
  clone. One native refinement left: run the walk concurrently with the badge
  sweep instead of before it. → WS-L
- **RM-10 · On-switch breaker feed.** ✅ *WS-A.* The native warm-up fetch now
  reports its outcome to the breaker like every other real attempt, in the
  extracted `ContentView.warmUpFetch` alongside D-18's gating.
- **RM-11 · Sweep re-check granularity.** Tauri re-checks the network slot
  between every repo of a sweep and bails mid-list; native's visible sweep
  checks once at entry (its tier runner *does* re-check — internal
  inconsistency). **Tauri right**; move the native guard inside the loop. → WS-S

### 4.2 Background machinery, connectivity, update checker (BG)

- **BG-1 · Cadence policy.** Native: 2/10/30 s status ladder by visibility,
  auto-fetch ×3 while hidden, sweeps paused while inactive, all under an App
  Nap assertion. Tauri: flat timers, nothing gated (E-3). **Native right
  and it is the GitHub Desktop model** (which pauses its indicator sweep on
  blur). Port to Tauri via `document.hidden`/`hasFocus()` and a
  self-scheduling `setTimeout` chain (which also delivers BG-2/BG-3 for free).
  Steal GitHub Desktop's one improvement on both: a once-per-session random
  0–30 s skew so multiple windows don't fetch in sync. → WS-J
- **BG-2 · Live re-arm of `auto_fetch` / `fetch_interval_ms`.** Native reads
  the shared config store on every tick — the store reloads at launch, on each
  save, and on activation, so a Settings change applies within one interval and
  a Tauri-side edit arrives on the next activation — and it idles on a 30 s
  re-check while disabled; Tauri's
  interval is armed at init/switch only, and `startAutoFetch(0)` clears it
  with nothing left to revive it (confirmed; ROADMAP tracks it as the auto-fetch re-arm item). → WS-J
- **BG-3 · Connectivity signal.** Native `NWPathMonitor` is authoritative;
  Tauri's `navigator.onLine` is hard-wired `true` on WebKitGTK, silently
  disabling the offline gate, the recovery kick, and the update-check retry on
  Linux (the breaker's lapsing backoff is the de-facto recovery — up to 5 min,
  not never). **Decided: build the core observer in this plan** (H-17):
  a `core::net` watcher emitting online/offline over the event seam, Linux
  backend first (netlink route watch — the broken platform, and a Linux test
  machine is available), then macOS and Windows; the Tauri client adopts it
  there, and the native client retires `NetworkPathObserver` once the macOS
  backend proves equivalent. Until it lands, `navigator.onLine` stays
  authoritative-negative only. **Decided by the user: built beside its
  adopter** — a workstream earlier would have meant an unverifiable, uncalled
  backend, which is the dead-wiring shape this plan otherwise deletes. It is
  now a workstream of its own (see §6's WS-K entry) because three OS backends
  on a Linux machine is not a rider on anything. → WS-K
- **BG-4 · Poll equality + failure surfacing.** The failure half landed in WS-A
  (D-3: the 3-tick streak on the poll-owned `pollError` flag, reset on repo
  switch in both clients). What remains is E-6: port the native equality skip
  (a `stat_stamp`-aware fingerprint makes it free), which is also the hook for
  DF-1 (Tauri's stale open diff). → WS-J
- **BG-5 · Update checker.** Tauri-only today (confirmed: zero
  `check_for_update` references in the FFI). Everything platform-independent
  is already in core (release request, strict version compare, per-platform
  artifact gate, `install.sh` one-liner, fake-update override, five tests).
  Native needs: an async FFI export + `UpdateInfo` mirror, an app-scene-level
  checker (Tauri runs it pre-main too), and a chip. **Do not port the
  breaker gate** — gate on `isOnline` alone (the checker's own comment notes a
  GitHub API answer says nothing about git remotes, and D-2 shows the breaker
  can be open spuriously); give `NetworkPathObserver` multiple recovery
  subscribers rather than a second monitor. → WS-M
- **BG-6 · Typing guard.** Native queries the first responder at tick time
  (stateless); Tauri latches a `focusin/focusout` flag that strands `true`
  when a focused element is removed (killing a focused terminal) — auto-fetch
  silently dead for the session. Replace with an `activeElement` read at tick
  time. → WS-J
- **BG-7 · Un-occlude resync.** Tauri resyncs on visibility *and* focus;
  native only on app activation (documented — up to one 30 s beat after
  un-occluding without activating). Cheap to close: fire the existing resync
  from the policy's occlusion edge. → WS-S
- **BG-8 · Auto-fetch budget (shared).** Both clients run auto-fetch under the
  *user* network budget (15/30/600 s), not the 12 s background budget the tier
  fetches use. ✅ *WS-B.* `fetch` takes a `background` flag and both clients'
  automatic paths pass it, so an unreachable remote gives up in 12 s instead of
  holding the single network slot for ten minutes with every other repo's
  refresh queued behind it. A fetch the user asked for keeps the generous
  budget a large transfer needs.

### 4.3 Sync & publish (SY)

- **SY-1 · Control shape.** Native: one adaptive `SyncProposal` ladder button
  (detached → Publish → Publish Branch → Pull → Push → Fetch), which **is**
  GitHub Desktop's `PushPullButton` cascade; no Refresh button. Tauri: a
  standalone Pull + a Push split-button + a Refresh button, with the ladder
  re-derived as three loose booleans. **Native/GH Desktop right** — the
  three-control layout is what *causes* SY-2 and SY-3. Port the ladder to
  Tauri; keep Tauri's on-button count badges (better than the native
  platform-forced standalone `↑N ↓N` text — that stays, documented in FRONTEND §8).
  The ladder is a pure function of `RepoStatus` → hoist to core (§5). → WS-F
- **SY-2 · Manual Fetch is unreachable in Tauri.** `gitApi.fetch`'s only call
  site is the automatic loop; the push menu never offers Fetch; the only
  user-driven way to contact the remote when in sync is a working-tree-mutating
  pull. GitHub Desktop puts Fetch in every dropdown state. Three-line interim
  fix (menu item + a `'fetch'` slot kind) worth shipping before SY-1. → WS-F
- **SY-3 · Tauri offers a push git will reject** on a diverged branch (both
  buttons enabled; the rejection lands in a blocking modal). Native's
  pull-outranks-push makes the state unreachable. Falls out of SY-1; interim:
  disable Push when behind > 0, never silently redirect. → WS-F
- **SY-4 · ⌘P semantics.** Native: the proposed action, menu item renaming
  itself. Tauri: always push/publish — **Pull has no keyboard route at all**.
  Falls out of SY-1 + the core ladder. → WS-F
- **SY-5 · Inferred counts hidden in Tauri.** Core computes ahead/behind
  against `refs/remotes/<remote>/<branch>` for unpublished branches
  (explicitly "so the Push badge updates"); native shows them in the
  publish-branch state, Tauri suppresses both badges there — a deliberate
  suppression, not a limitation. Show them (as status text; a Pull button is
  still wrong there). → WS-F
- **SY-6 · Publish dialog failure mode.** Native keeps the sheet open with
  gh's error inline and fields intact; Tauri stacks the blocking ErrorModal
  *over* the dialog (two dismissals before retrying a name collision) and has
  no progress indication beyond the button label. Native also has the
  org `owner/name` hint. **Native right**; port error-inline + indeterminate
  progress + hint. → WS-F
- **SY-7 · Force-push confirm.** Split verdict: Tauri's dialog lifetime is
  better (stays open, "Force-pushing…", one dismissal to retry a stale
  lease); native's *target naming* is correct (`status.upstream`, right even
  when the upstream branch name differs — Tauri composes `{remote}/{branch}`
  from a cached remote, wrong in that case, and spends an extra `git remote`
  per repo open purely for dialog text). Take each other's half. → WS-F, WS-Q
- **SY-8 · Post-op refresh.** Native reloads status+log+`is_merging` so a
  pull's commits appear immediately; Tauri reloads status only and History
  catches up ≤2 s later via the poll. The collapse half landed in WS-A (D-13:
  one `refreshStatus`, which now carries `is_merging`), so what remains is
  making post-op = status **+ log** — one call added beside it. → WS-F
- **SY-9 · Chevron contents.** Tauri's menu in the publish states duplicates
  the face (a chevron revealing only what the button already says); GitHub
  Desktop hides the dropdown for publish-repo and offers Fetch for
  publish-branch — exactly native's shape. Falls out of SY-1/SY-2. → WS-F
- **SY-10 · Transfer error surface.** Tauri renders git's multi-line rejection
  in a selectable `<pre>`; native's `.alert` collapses whitespace and can't be
  copied (D-15's sibling). Route native sync failures to the selectable
  banner, or make the alert text monospaced + selectable. → WS-Q
- **SY-11 · Progress presentation.** Native full-width strip with a real
  indeterminate state; Tauri in-button fill (closer to GH Desktop) with **no**
  indeterminate rendering — publish and (future) fetch show a spinner over a
  permanently empty bar. Keep each shape (document in FRONTEND §8); give Tauri the
  indeterminate case. → WS-F, §9

### 4.4 Branches & merge (BR)

- **BR-1 · Merge UI is dead code in Tauri** (verified: nothing ever sets
  `showMerge = true`; `mergeTarget` is never written; `countCommitsToMerge`
  has zero callers). The native client ships the full flow: source submenu →
  sheet with commit-count preview → Merge / Squash & Merge → conflicts as
  data → Abort Merge. **Port to Tauri** (shape per platform, not verbatim);
  two native refinements while there: hide the submenu while `isMerging`, and
  adopt GitHub Desktop's zero-count treatment ("already up to date" + disabled
  primary — native currently says "Brings in 0 commits." with a live button).
  → WS-G
- **BR-2 · Abort merge has no Tauri UI** — a user who *enters* a merge from
  the terminal sees the MERGING badge with no in-app exit. Arguably ahead of
  BR-1 in priority; ~15 lines against the already-polled `isMerging`. → WS-G
- **BR-3 · Branch-list freshness.** Native reloads on every menu open, on
  HEAD move, after undo/checkout, on ⌘R; Tauri reloads on exactly five sites
  — not on dropdown open, not from the poll — so a branch created in the
  embedded terminal can be invisible for the whole session. **Native right**
  (one cheap `for-each-ref` at the moment of intent). → WS-G
- **BR-4 · Busy state.** Tauri's dropdown has none — double-clicks issue
  overlapping checkouts that contend on `index.lock`; a slow checkout gives
  no feedback. Native serializes with `isBusy` — but its `run` helper returns
  `nil` for "dropped because busy", which callers read as success; fix while
  porting. → WS-G, WS-Q
- **BR-5 · Same-branch re-select.** Tauri runs a redundant checkout + full
  refresh chain (~8 processes) when you click the branch you're on; native
  guards. One-line fix. → WS-G
- **BR-6 · Create-branch failure.** Tauri clears the typed name *before* the
  outcome and routes the error to the global modal over a closed dropdown;
  native keeps the sheet open with the error inline. **Native/GH Desktop
  right.** → WS-G
- **BR-7 · Delete confirmation.** Both `-D`; only native's dialog says
  "Unmerged commits are lost." Tauri's hover-only ✕ is also invisible to
  keyboard users. Adopt native's wording; a branch-row context menu (already
  proposed in QUICK-WINS) fixes discoverability and is the natural host for a
  future rename. GitHub Desktop's "also delete on the remote" checkbox is the
  shared target, deferred with BR-10 to a ROADMAP item that builds it on both
  clients at once (core has `delete_remote_branch`; a combined
  `delete_branch(…, include_remote)` keeps ordering semantics in one place).
  → WS-G
- **BR-8 · Detached/merging markers.** Native rides the branch chip's label;
  Tauri shows an icon swap + two yellow badges. Both platform-appropriate —
  **document as a FRONTEND §8 row** — but native's `· merging` suffix is easy to miss
  and truncates first; give it the same color treatment as the conflicted
  badge. → §9, WS-Q
- **BR-9 · `is_merging` fold-in** — ✅ *WS-B.* `RepoStatus.merging` is filled
  from a filesystem read of the git dir, so it costs the poll nothing and no
  refresh path can forget to ask (D-13's bug class). The standalone command
  was deleted rather than kept for compatibility: with both clients reading
  the status field it had no callers, and an unused second route back to the
  same answer is how they diverge again.
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
  deferral. → WS-S
- **BR-11 · Large branch lists.** Native's `Menu` gets scrolling +
  type-select from AppKit; Tauri's hand-rolled dropdown has no filter, no
  keyboard nav, un-keyed `#each` — and DESIGN.md claims it matches the
  repo picker, which has both. Reuse the picker's `listNavigation` machinery.
  → WS-G

### 4.5 Changes tab & commit flow (CH)

- **CH-1 · Multi-select + bulk actions.** Tauri (and GH Desktop): row range
  selection with a separate checkbox anchor, bulk Space toggle, "Discard N
  Selected Changes…". Native: single-selection by construction — recorded as a
  FRONTEND §8 divergence, with §6.4's shared floor reduced to arrow-key row
  activation because of it. Close it natively
  (`List(selection: Set<String>)`); the efficiency case is real too (a 30-file
  discard is ~90 subprocesses + 30 reloads natively vs ~3 + 1 in Tauri).
  Delete the FRONTEND §8 row. → WS-N
- **CH-2 · Space / keyboard toggle.** Native has **no keyboard route to
  include/exclude a file at all** (the highest-frequency action in the app);
  Tauri and GH Desktop toggle on Space, bulk-toggle in a selection. → WS-N
- **CH-3 · Select-all header.** Decided: **tri-state checkbox + native's label**,
  both clients. ✅ *WS-D* for the label half — Tauri now says
  "3 of 12 files included", counting *committable* files so a dirty submodule is
  in neither figure. Native's binary toggle, which lies the moment one file is
  unchecked, is the half left. → WS-N
- **CH-4 · Status badge style.** The conflicted letter differs between the
  clients (`U` in Tauri, `!` natively): **verdict `U`** — git's own porcelain
  letter, which is the vocabulary native's own comment claims to follow. Hoist
  `letter()`/`label()` onto core's `FileStatus` so the glyphs can't drift
  again. ✅ *WS-B* for the glyph and the token: `file_status_styles()` is the
  one table both clients read (once, at startup — colour stays per-platform),
  the conflicted letter settled on `U`, and conflicted gained its own purple
  status token in both palettes rather than reusing red, because red is
  already Deleted and a glance down the list has to separate "you deleted
  this" from "git couldn't merge this": opposite actions, and one of them
  blocks the commit. The token's second consumer is BR-8's easily-missed
  `· merging` suffix. **Decided: the native 18×18 tinted plate behind the
  letter stays** and becomes the shared look — STYLE.md's old no-plate rule was
  a leftover of the earlier TUI-flavored direction, macOS style is the design
  authority today. ✅ *WS-D*: the Tauri badge is that plate — 18×18, 4px radius,
  the status colour at 15% behind its own letter — set from one `--badge-tint`
  per row so the letter and the wash can't name different statuses, and applied
  to the embedded / dirty-submodule glyphs too so the column doesn't read ragged.
- **CH-5 · Rename display.** Tauri and GH Desktop render `old → new`
  (STYLE.md mandates it); native shows only the destination —
  indistinguishable from an add. Also missing in the native diff header
  (DF-8) and commit file list. → WS-N
- **CH-6 · Embedded/submodule row treatment.** Tauri swaps the status glyph
  for ↪ (the documented style); native appends a width-eating text tag. Adopt
  the glyph. → WS-N
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
  drifted). **Decided by the user: land it with BG-4's equality gate** —
  the grace counter has to advance every tick, so it cannot be gated on "the
  file list changed" and would cost the Tauri poll a second crossing every 2 s;
  BG-4 is restructuring that poll anyway. → WS-J
- **CH-8 · Discard confirmation copy.** Native names the actual per-file
  outcome (restored from HEAD vs moved to Trash) — what FRONTEND §6.10 asks for; Tauri
  states both rules generically and dismisses on backdrop click (STYLE.md
  violation). But native *guesses* the outcome from status while core decides
  it authoritatively via `ls-tree`. ✅ *WS-B*: `classify_discard` returns the
  same plan the discard itself runs on, and both dialogs render it — so the
  three cases the guess got wrong (a staged re-add of a path that exists in
  HEAD, a rename whose original is *not* in HEAD, and every file under an
  unborn HEAD) now read truthfully instead of promising something the action
  then doesn't do. Native still lacks Tauri's in-flight busy state. → WS-N
- **CH-9 · Embedded-repo confirm.** Tauri's copy is better (names the outer
  repo, states the clone consequence, "Commit as link" verb); native's system
  `confirmationDialog` is the right container. Merge: native container +
  Tauri text. ✅ *WS-D* for the Tauri half: one `canCancel` gate now answers
  the backdrop, Escape and the Cancel button, where only Escape had checked —
  the tell that a per-dismissal list had already drifted once. → WS-N
- **CH-10 · Composer details.** Port to native: the 72-char summary counter
  (STYLE.md; skip Tauri's silent 200-char hard cap — it truncates pasted
  and AI-generated summaries), the included-row weight cue, tooltip only when
  truncated, keyboard resize on the handle (ROADMAP's composer-resize item), an
  in-flight "Committing…" label, and coalescing the per-drag-frame
  `UserDefaults` writes to drag-end. ✅ *WS-D* for the Tauri half: ⌘↩/⌘G moved
  to `MainLayout`'s window handler above its `inField` bail (the fields' own
  listeners made them reachable only once you were already typing), inert on
  History and under any dialog; and the height clamp landed, measured off a
  wrapper around both tab panes because the Changes pane reports zero while
  History shows. D-10's lockout landed in WS-A, and ST-7's revert turned out to
  have landed in WS-B. → WS-N
- **CH-11 · Row-action errors** (with **SH-5**). Decided: **split by class in
  both** — an operation the user is waiting on takes the modal, with a retry
  where the same attempt can just be made again; a failure that was never their
  task states itself in the strip. ✅ *WS-D* for Tauri: `reportActionError` /
  `reportNotice` in the repo store are that choice, so a call site picks a
  function instead of copying the `repoState.update` shape from its neighbour —
  which is how every failure in the client, down to "couldn't reveal the file",
  came to seize the window. The strip gained a second, dismissible variant (the
  poll's own has no ✕ because its own recovery retires it; nothing can retire
  this one), and `ErrorModal` was finally passed the `onRetry` it has always
  accepted. The rule is now FRONTEND §6.13. Native's remaining drift: discard,
  checkout and undo failures go to its banner where the rule puts them in the
  modal, and its banner still has no dismiss. → WS-N, WS-Q
- **CH-12 · Copy File Path.** ✅ *WS-D.* Tauri crossed to the backend and back to
  concatenate two strings it already held. `utils/path.ts` gained `absolutePath`
  — the one place the filesystem path and git's always-`/` path meet — which
  reads the separator off the repo root rather than assuming one, so a Windows
  paste still gets backslashes.
- **CH-13 · First-file auto-select.** ✅ *WS-D.* Tauri landed on an empty pane
  beside a list of files while its own commit-detail pane had always
  auto-selected. It now re-seats on native's two conditions and no others
  (nothing open, or what was open left the tree), keyed on a `$derived` of the
  *path list* so a 2 s tick that only changes content can't move the selection.
  Pairs with DF-1 so the auto-selected diff stays fresh.

### 4.6 Diff viewer (DF)

- **DF-1 · Open-diff freshness (Tauri).** `stat_stamp` reaches the Tauri
  client on every poll and is never read (verified). Adopt the reload — but
  per-file (compare the active file's stamp), which is *better* than native's
  whole-status epoch: it also fixes E-9 on the native side by gating the
  reload on the open file's own stamp. The FRONTEND §8 staleness row then retires.
  → WS-J (Tauri), WS-P (native gate)
- **DF-2 · Side-by-side.** Tauri-only (sanctioned FRONTEND §8 row). Two facts change
  the calculus: core already computes `sbs_pairs` on the native path and the
  bridge throws them away (E-8), and GitHub Desktop treats split/unified as a
  **per-diff header control, not a Settings preference**. **Decided: build the
  native split view now, cleanly** — parity is meant literally and the data is
  already being computed, but the build must not fork the renderer: one row
  model feeds both arrangements, the pairs cross the bridge only when the
  split layout is active (H-8 stops producing them otherwise), and the toggle
  moves into the diff header in both clients. It is the largest single piece
  of new native UI in the plan, and is a workstream of its own. → WS-O
- **DF-3 · Structured wire.** FRONTEND §7's open decision (where the HTML
  collapse lives) should close **toward the structured token wire for both**
  — that is GitHub Desktop's own shape (worker tokens, view-side collapse),
  it halves Tauri's per-diff IPC, and it unblocks Tauri virtualization.
  ✅ *WS-B* for the core half: `DiffOptions` makes HTML and side-by-side pairs
  opt-in (E-8), `DiffLine.text` is an `Option` filled only for the Hunk and
  NoNewline rows that read it, and `get_parsed_diff` /
  `get_parsed_commit_diff` fuse the read and the parse. FRONTEND §7's open
  decision — whether the *phase-2* wire becomes structured tokens for both —
  is untouched and still open.
- **DF-4 · Virtualization + size guards.** Tauri mounts every row.
  GitHub Desktop solves the exact variable-height
  problem Tauri gave up on (measured-height cache). Neither client has any
  large-diff guard; GH Desktop has three (70 MB buffer / ~4.4 MB "reasonable"
  / 5 000 chars-per-line → a "Show Diff anyway" state). **Decided: guard only
  in this plan** — the two fixes sit at very different sizes, and only the
  guard is cheap. ✅ *WS-B*: `DiffSizeGuard` withholds a patch over 4 MiB, or
  one with a line over 5 000 bytes, and both viewers render the measurements
  with a "Show diff anyway" button that re-asks with the guard lifted (per
  request, not sticky — moving to another file gets the guard back), so a
  pathological diff is survivable and explained instead of a hang. The
  long-line limit earns its place separately: a minified bundle is slow at a
  total size the byte bar waves through. Virtualization only smooths the
  merely-large diff, so it stays a ROADMAP item recording GitHub Desktop's
  measured-height approach, why it is a web-only concern (SwiftUI's
  `List`/`LazyVStack` already virtualize — no native work exists there), and
  that it is re-judged after DF-3's structured wire changes how rows are
  built, then taken only if large diffs still feel heavy.
- **DF-5 · Dead per-line-selection scaffolding (Tauri).** **Decided: the
  scaffolding stays** — per-line staging is the unfinished GitHub Desktop
  feature, and ROADMAP commits to finishing it here and porting it natively
  (core's `build_patch` is complete and tested). ✅ *WS-E* neutralized its cost:
  the hunk header renders as a control only under `showSelection`, so today's
  band is plain selectable text instead of one focusable no-op button per hunk
  on an unvirtualized list. Two branches rather than conditional attributes,
  because a click handler without a role is what the a11y lint is for.
  → ROADMAP (finish + port)
- **DF-6 · Model-based copy** (D-15). Adopt GitHub Desktop's approach in both
  clients: rebuild clipboard text from the line model (immune to gutters,
  prefixes, wrapping, side-by-side interleaving, and native's tab expansion,
  since `line.content` keeps real tabs). ✅ *WS-E* for the Tauri interim —
  `user-select: none` on the prefix, joining the gutter that already had it, so
  an ordinary drag-select yields the file's own lines. Native's interim (scope
  `.textSelection` to the content text) is open. The core helper that keeps the
  two byte-identical landed in WS-B (`copy_text`, exported as `copy_diff_text`)
  and is still deliberately unconsumed — the workstream's one exception to the
  no-dead-surface rule, and the only thing that closes D-15 rather than
  narrowing it. → WS-P
- **DF-7 · Empty-parse reason** (D-12) — ✅ *WS-B.* `EmptyDiffReason` names the
  three situations one caption used to cover, and both clients render each
  honestly. The whitespace case needed the fused call to exist at all: when the
  filtered diff has nothing to render, core re-reads the unfiltered one to tell
  "unchanged" from "re-indented, and the setting is hiding it". A *load
  failed* variant turned out to be unnecessary — the fusion makes a failure an
  `Err` rather than an empty parse (see DF-10).
- **DF-8 · Header details.** ✅ *WS-E* for the minus sign: Tauri renders `−`
  (U+2212) in the file header's `−N`, the commit card's totals and the removed-line
  prefix, so both clients now use one glyph and STYLE.md carries the header
  rule it only had for rows. Still open, both native: `old → new` for renames
  (source it from the parsed diff, which describes what is rendered) and
  suppressing `+0 −0`, which native shows on binary diffs. D-19's doubled
  `NoNewline` row was the fourth alignment; fixed in WS-A. → WS-P
- **DF-9 · Slow-load presentation** (D-20). ✅ *WS-E, both clients.* Neither
  unmounts the old diff any more: it dims and takes a spinner overlay. Tauri
  through a `SeamlessDiffPane` wrapper shared by the two panes that had the rule
  written twice; native by making the threshold a modifier on `content` rather
  than a branch beside it — a branch was what destroyed the `ScrollView`'s
  identity, which is why the store's equality skip preserved nothing visible.
  FRONTEND §6.3 now also carries the scroll contract (*same file → keep scroll;
  different file → reset*), which both clients already keyed on the rendered
  diff's own paths. D-14 (native's scroll reset on file switch) is the half
  left. → WS-P
- **DF-10 · Failure surface.** ✅ *WS-E.* Tauri clears the stale payload and
  states the failure inline in the pane, matching native. **The retry WS-D put
  on these two loads was dropped deliberately**, not lost: native has none, a
  button on one client only is the parity gap WS-D warned about, and the gesture
  survives anyway — the payload is cleared, so the loader's "already open"
  short-circuit no longer fires and clicking the row is the retry. FRONTEND
  §6.3 now says why this is outside §6.13's two classes: nothing is blocked, and
  it is the user's own task.
- **DF-11 · Dirty-submodule pane.** ✅ *WS-E, both clients.* Decided before the
  read in each: Tauri stopped fetching a diff it then discarded, and native —
  which had no branch at all and rendered git's raw `Subproject commit …-dirty`
  line, against STYLE.md's explicit rule — gained the pane and the skipped
  subprocess together.
- **DF-12 · Phase-2 debounce.** Tauri debounces highlighting 80 ms; native
  starts a tokenize per file survived while arrowing. Add the same 80 ms +
  generation re-check natively; promote the constant next to
  `slowLoadThreshold`. → WS-P
- **DF-13 · Wrap break policy (risk, unverified).** Both Tauri
  (`overflow-wrap: anywhere`) and GH Desktop (`word-break: break-all`) force
  character-level breaking; native relies on SwiftUI `Text` defaults — a
  minified/base64 line may overflow the pane with no horizontal scroll to
  reach it. Needs one visual check; fix via `.byCharWrapping` or zero-width
  break insertion in the existing tab-expansion pass. → WS-O (check first)

### 4.7 History (HI)

- **HI-1 · HEAD gating** — ✅ *WS-A.* D-1: both clients now gate the rewriting
  actions on `status.head_sha`, and the Tauri context menu no longer carries a
  row index at all.
- **HI-2 · Log windowing.** ✅ *WS-E.* **The append model is now both clients'**:
  the log is append-only and rooted at HEAD, `commits[0]` is HEAD by
  construction, paging drops nothing from the front, and a HEAD move re-reads
  from offset 0 at the depth the user has paged (capped at 500, oldest rows
  dropped) and scrolls to row 0. Tauri's bidirectional sliding window and its
  `windowStartOffset` scroll compensation are deleted with it — the window
  worked, and every defect around it existed because row 0 stopped being HEAD
  when it advanced (D-1's bug class, D-11's replacement-vs-slide signal, and the
  `skip > 0` gate that made a commit at offset 0 bump nothing). Re-reading from
  the top makes all three unrepresentable rather than fixed. `resetSeq` survives
  as "go to row 0" alone. **One deliberate reading of this entry's text**: it
  said "prepend", which neither client had and which only differs from a capped
  re-read past 500 loaded rows — the re-read also refreshes tag decorations on
  the visible rows, and §6.8's own invariant is *refetch*, so the simpler shape
  won. FRONTEND's §8 paging row is retired and §6.8 now carries the shared
  model; only page size stays per-platform (50 / 100).
- **HI-3 · Selection behavior.** ✅ *WS-E.* Tauri auto-selects the newest commit
  and re-seats on native's exact two conditions — nothing selected, or the
  selected sha is no longer in the list, which is what an amend or an undo does
  to it; it used to land on an empty pane and then keep rendering a
  rewritten-away commit's detail. Built as the `$derived` key + `untrack`ed
  `$effect` WS-D established, gated on History being the visible tab so the pane
  behind Changes doesn't spend a `git log` on a selection nobody sees.
  Right-click now selects the row it opens on, as this client's own `FileList`
  already did.
- **HI-4 · Loading/empty gating.** ✅ *WS-E* for Tauri: the detail pane tells
  "no history" from "nothing selected" rather than inviting the user to select a
  commit from a list that has none. Native still flashes "No commits" before the
  first `get_log` lands. → WS-Q
- **HI-5 · Relative dates.** Tauri re-ticks; native is a snapshot (its comment
  says so), and FRONTEND §8 now carries the difference. Add a visibility-gated
  10 s tick natively (reuse `BackgroundSchedulingPolicy` — don't invent a
  second gate) and retire that row; pin the
  tier vocabulary in FRONTEND §6.11 (the clients currently render "yesterday" vs
  "1 day ago"). Align the detail card's date format (Tauri's shows raw
  `toLocaleString()` seconds; both should use the abbreviated form — and
  both show the **author** date while DESIGN.md says committer; fix the doc).
  → WS-Q, §9
- **HI-6 · Commit-list keyboard.** ✅ *WS-E.* `FileList`'s `focusRowAt` ported
  verbatim — arrows, Home/End (⌘↑/⌘↓ on macOS), scroll-into-view, and the
  `tick()` ordering a virtualized list needs to focus a row that isn't mounted
  yet. It was the one list in the app where an arrow key did nothing.
- **HI-7 · Detail loads.** ✅ *WS-B* for the fusion: `get_commit_detail` is one
  `git log -1 -z --raw --numstat`, halving subprocesses per selection in both
  clients and removing the error-policy split — the files and the totals now
  come from one read, so neither can describe a different commit than the
  other. ✅ *WS-E* for Tauri's re-select, which used to blank the pane and
  refetch what it was already showing. Native's half is structural rather than
  guarded (`List(selection:)` writes the same value and `.task(id:)` doesn't
  re-fire), so it needs nothing there. Still open, both native: key the detail
  task on `(repoPath, sha)` not sha alone (`repoPath` is published before the
  new log lands, so the window is real; the `LoadKey` pattern is one file over),
  and clear `commits` on repo switch. → WS-Q
- **HI-8 · Paging.** ✅ *WS-E* for the three Tauri halves: a failed page is a
  `reportNotice` in the banner rather than a modal mid-scroll (the history on
  screen is still correct and scrolling re-asks); paging owns `log.isPaging`
  instead of the repo-wide `isLoading` that was disabling Commit on the other
  tab — which left `isLoading` with no writer at all, so it and its term in
  `canCommit` are deleted; and the detail card's trailer list is gone, since
  `%b` already ends in them. Still open, both native: zero prefetch margin
  (trigger at N−5) and E-7's full reload on row actions; tag chips, where the
  accent capsule diverges from STYLE's neutral treatment and from its own
  unpushed plate two lines away. → WS-Q
- **HI-9 · Checkout busy state.** Tauri holds the dialog with "Checking
  out…" and suppressed Escape; native dismisses instantly with no feedback
  and nothing preventing a second checkout. **Tauri right.** → WS-Q
- **HI-10 · Undo details.** ✅ *WS-E* for the ellipsis: "Undo last commit" now
  says what it does, and **"Checkout commit…" gained the one it had earned** —
  the rule is a promise about asking first, and applying it to only the item
  that broke it would have left the same menu inconsistent in the other
  direction. Still open, both native: copy Tauri's `lastHeadSha` re-seed after
  undo so the next poll doesn't redundantly refetch, and stop reloading branches
  after an undo (a `--mixed` reset cannot change the branch list). → WS-Q

### 4.8 Clone & gh (CL)

- **CL-1 · Reachability.** ✅ *WS-C* for Tauri: `Clone Repository…` is in the
  footer of the startup picker as well as the switcher, so the first-run user
  most likely to want it can finally reach it. Native is reachable from Welcome
  and the switcher, but its entry sits under the switcher's transfer-disable —
  cloning a different repo contends with nothing, since clone deliberately
  claims no slot in either client. → WS-L
- **CL-2 · List caching.** GitHub Desktop's shape — cache **plus** an
  always-visible refresh button — adopted on both. ✅ *WS-C* for Tauri: the
  once-per-run cache keeps its per-open speed and gains a Refresh button beside
  the filter, so a repo created since launch is no longer unreachable until
  restart; the filter already stayed live during loads. Native still refetches
  on every sheet open (a 20 s dead zone each time, filter disabled throughout).
  → WS-L
- **CL-3 · Keyboard.** ✅ *WS-C* for Tauri: Return clones from anywhere in the
  dialog when Clone is enabled — the `defaultAction` the native sheet has and
  this one, having no `<form>`, never did. In the gh list, Return on a row the
  cursor hasn't picked yet selects it and the *next* Return clones: one press
  would clone before the derived destination path had ever been on screen.
  Native's GitHub tab is still mouse-only (no autofocus, no cursor, no
  Enter-to-select — FRONTEND §6.9's first-row-acts-on-Return applies). → WS-L
- **CL-4 · URL/name derivation has drifted** (the "ported verbatim" pair):
  `.git`-on-shorthand handled differently, whitespace trimmed only natively
  (an untrimmed Tauri path creates literal `" …"` directories and persists the
  poisoned `last_clone_dir`), empty-destination preview shows `/repo` in
  Tauri. Both share two latent bugs (`owner/repo/` and scheme-less
  `github.com/…` enable Clone then fail). ✅ *WS-B*:
  `derive_clone_target(raw_url, parent)` and `clone_target_path(parent, name)`
  live in `core::repos`, matrix-tested, and both dialogs gate Clone on a real
  parse — so the preview and the button can no longer disagree about whether
  the app is about to succeed. Both latent bugs are covered, and the
  destination join keeps a bare root (`/`) that the old trailing-slash strip
  turned into a relative path.
- **CL-5 · Mid-clone state.** ✅ *WS-C.* Tauri left tabs/filter/rows/URL/
  destination editable, so clicking another repo mid-clone rewrote the
  "Clones into…" preview to a path nothing was being written to. One
  `<fieldset disabled>` now covers every control that chooses *what* to clone —
  a group rather than a per-control list a later control can forget to join —
  dimmed so the freeze is visible, with progress and errors moved outside it so
  the bar reporting the clone isn't dimmed by the clone.
- **CL-6 · Progress.** Native always shows motion (indeterminate for gh
  clones); Tauri shows nothing for a gh clone but the button label.
  ✅ *WS-B*: `gh repo clone … -- --progress` forwards to `git clone`, so
  `gh_clone` reuses the streaming seam a URL clone already had and both routes
  report real numbers. Nothing was wrong with the plumbing — nobody had passed
  the flag through.
- **CL-7 · Small deltas.** ✅ *WS-C* for Tauri: empty-state discrimination (you
  have none vs none matched), the description as a row tooltip, the *tab kept /
  inputs cleared* split across opens (it used to persist everything, including
  a stale selection with Clone already lit), and a name tiebreak on the
  recency sort. Its per-tab error state needed nothing — `selectTab` already
  clears the clone error and a gh-list failure is its own state inside the
  list. Native still owes: the same empty-state split, the per-tab error state
  (a URL-tab failure currently shows over the GitHub tab), tab-vs-input reset
  (it resets everything), diacritic-insensitive collation with a stable
  tiebreak (Swift's sort isn't stable and equal names can flicker), and
  filter-then-sort + memoize (it sorts 200 rows per keystroke per body pass).
  → WS-L
- **CL-8 · `check_auth`.** Tauri spawns `gh auth status` on every launch to
  write a field with **zero readers** (the PR feature that consumed it was
  retired); the FFI deliberately doesn't export it and gh's own error text
  ("Run `gh auth login`") is the better UX. Delete the call + wrapper; drop
  the command from FRONTEND §3's tables (and its registered count) or record
  the exemption. → WS-S

### 4.9 Settings, config, AI (ST)

- **ST-1 · The field matrix.** All 15 `Config` fields audited (full matrix in
  the research notes). Live-apply: native applies auto-fetch/interval within
  one tick and diff settings immediately; Tauri now re-reads the shared file on
  every window activation (D-6, ✅ *WS-C*), so a cross-client edit lands within
  one focus — but still needs a restart or repo switch for the auto-fetch
  *timer* to re-arm (BG-2).
  ✅ *WS-B* for the dead fields: the AI timeout now travels on
  `AiProviderConfig` and bounds both providers' requests — a control that
  persisted a value nobody read was worse than no control, because the user
  believed the timeout was set — and `ai_api_key`, mapped but read by neither
  provider, is gone. → WS-H, WS-J
- **ST-2 · Save semantics** — ✅ *WS-B.* `patch_config` is the only writer, and
  it reads-edits-normalizes-writes under a lock the shared file never had, so
  a surface can only change the fields it names. `Config::normalized()` runs on
  both the read and the write, which is what heals an already-poisoned file on
  first load. Both clients' whole-object writes are deleted; so is
  `save_config`'s export.
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
  the suggestion. → WS-H, WS-R
- **ST-4 · Units and bounds.** ✅ *WS-B* for the bounds: `config_bounds()` is
  the one declaration, read by both forms and enforced by the one writer, so a
  control can no longer offer a value the writer then clamps away (native's
  load-clamp floored to 1 s while its own control started at 5). Units are
  unchanged and deliberate — native shows seconds, the wire stays
  milliseconds. Still open: Tauri shows raw ms. → WS-H
- **ST-5 · `ai_provider` ownership.** Native has **two independent owners**
  (composer's CommitStore and SettingsStore) that never observe each other —
  with both windows open the pickers can disagree, and a Settings save of any
  unrelated field silently reverts a composer-side provider change. Tauri's
  single `$config` store is the shape to copy.
  Route both native surfaces through `AppConfigStore` (which exists precisely
  to be the single owner — and grow it the `scanPaths` accessor three other
  call sites currently bypass it for). → WS-R
- **ST-6 · AI mapping duplication** — ✅ *WS-B.* `ai::provider_config` and
  `ai::load_ai_config` live in core; the bridge is the delegation it always
  claimed to be, the TS copy is deleted, and both clients call
  `load_ai_config` per generate. The three behavioural differences between the
  old copies go with it — including the one that mattered: the model and the
  server URL now always belong to the provider actually about to run, rather
  than being spliced from a picker value over a separately-loaded config.
- **ST-7 · Provider save failure.** ✅ *WS-B*, ahead of the workstream it was
  scheduled to — `patch_config`'s adoption rewrote `setProvider` and brought the
  revert with it. WS-D found it already done and narrowed it: the rollback puts
  back the one field rather than the whole config snapshot, which would have
  reverted everything the store learned meanwhile — D-5's lost update, one layer
  up. The Settings overlay's picker still has no revert, which ST-3 inherits.
  → WS-H
- **ST-8 · One model field, two providers.** Set `sonnet`, switch to Ollama,
  Generate fails — a shared design flaw; GitHub Desktop stores per-provider
  models. **Decided: split per provider, restructured cleanly** —
  backwards compatibility is explicitly waived (sole user; the config file
  can be regenerated), so the config gains a real per-provider shape (a
  Claude section with model + timeout, an Ollama section with model + server
  URL) instead of a migration shim; `ai_api_key` goes away in the same pass;
  unknown old keys keep parsing (`config_ignores_retired_keys`). ✅ *WS-B*, UI
  included: both Settings surfaces show the selected provider's own fields, so
  switching provider in the composer never requires a Settings trip. Field
  order in `Config` is now load-bearing — a TOML table swallows every key after
  it, so nothing scalar may be declared below those two (pinned by a
  round-trip test).
- **ST-9 · `check_provider_status`.** **Decided: use it rather than delete
  it** — a dead command path is exactly how BR-1 rotted. ✅ *WS-D*, both clients:
  the probe (`ProviderStatus { ready, reason, fix_command }`) plus
  `provider_status_from_failure`, the status strip, and the offer to run the fix
  command in each client's own terminal. Details in FRONTEND §6.7 and TECHNICAL.
  It took **four** corrections across three visual checks, and none of them was a
  bug in the code — each was a wrong model of the problem. Read them before
  building any other readiness gate:
  - **`claude --version` answers the wrong question.** It proves the binary
    exists, not that it will answer. Hence the second question, `claude auth
    status`, read from its JSON payload rather than its exit code.
  - **A probe cannot see an expired session at all.** Signing out *deletes* the
    credentials; an expiry leaves them on disk, so `auth status` still reports a
    signed-in CLI and only a real request discovers the refresh failed — with a
    different message (`Not logged in · Please run /login` vs `Failed to
    authenticate: OAuth session expired…`). **Testing with `logout` proves
    nothing about the expired case.** That was the trap, and the user caught it.
  - **Chained `{:else if}` made the remedy unreachable.** It was written after
    the error block, so on a failed generate — the one case it existed for — it
    was structurally impossible to render. A conditional whose sibling is *also*
    true in the case that matters is not a chain.
  - **A fix shipped to one client is not shipped.** The whole first pass was
    Tauri-only while the parity plan's own subject is the two clients agreeing.
    Port in the same change, or the gap is what the user finds.
- **ST-10 · Scan-path editor.** **Decided: locked by default on both
  clients** — the field renders read-only with an **Edit** button beside it —
  the macOS list-editor pattern — Edit enables it, the button becomes
  **Done**, and Done parses, applies through `patch_config`, and locks the
  field again. Nothing touches the config until Done, so closing or Escape
  mid-edit simply discards the draft; no confirmation popup anywhere. Give
  the native field `.monospaced()`; keep parse-at-save on both (Tauri's
  parse-on-input transiently desyncs the textarea from the model).
  → WS-H, WS-R

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
  the affected-chord table. → WS-I (the toggle's own ⌃-only narrowing)
- **TE-2 · Transport** — D-4 plus E-11/E-12: move Tauri to a
  frontend-created `Channel` passed into `start_terminal` (mirrors the
  native seam, kills the race and the per-chunk JSON), mark
  `start`/`close`/`resize` `(async)` (leave `write` sync — IPC arrival order
  is the keystroke-ordering guarantee). ✅ *WS-B* for the core half: the reader
  thread now feeds a bounded channel and a second thread coalesces, so output
  is slowed rather than dropped when a host falls behind, and a flood arrives
  in a few dozen deliveries instead of one per 4 KiB read. The batching is
  back-pressure-driven, not a fixed window — see §6's WS-B entry for why, and
  for what that means for the `Channel` rewrite. → WS-I
- **TE-3 · Collapse/resize** — D-9 (native emulator reflow; fix by pinning
  the inner frame) and the missing native 80 ms resize debounce (a divider
  drag is one SIGWINCH per column crossed today — put the coalescing in
  `TerminalController.resize`, not the delegate, to keep the one-shot
  initial-size push). ✅ *WS-B* for the core half: `resize_terminal` ignores a
  `< 2×2` grid itself, so no host can announce a collapsed panel to the PTY —
  which leaves D-9 as purely the native inner-frame pin. → WS-R
- **TE-4 · Scrollback.** 500 (native) vs 1000 (Tauri) — both library
  defaults, neither chosen. Set 1000 explicitly on both (`git log --stat`
  exceeds 500; VS Code ships 1000). → WS-R
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
  correct), is ignored by Tauri (add the handler, write-only). → WS-I, WS-R
- **TE-6 · Refocus.** Confirmed, and ROADMAP tracks it: Tauri never refocuses after
  focus is stolen (only a click); native has the same call sites but AppKit
  restores the first responder. Add the `focusin` + reactivation handlers.
  → WS-I
- **TE-7 · Small parity.** Header label fallback ("Terminal") when no session
  (native has it; also mostly obsoletes ROADMAP's expand-hint idea); the "280"
  constant means dock-height in Tauri and emulator-height natively (~2 rows
  difference) — pick one meaning; shell preference read fresh per session
  natively (a native-side Settings change doesn't reach a running Tauri —
  read the config in `initBackend`); ⌃` needs a native menu-bar home
  (View ▸ Show/Hide Terminal owning the chord). → WS-I, WS-R

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
  single-instance for free — cheaper than Tauri's socket. → WS-M
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
  `tauri::menu`; out of this plan's scope beyond recording it. → WS-H, WS-M
- **SH-3 · ⌘R.** Native: full reload, guarded against transfers. Tauri:
  status-only, unguarded (races a pull's lock files — the poll next to it
  pauses for exactly that reason), and swallowed while any field has focus.
  → WS-F
- **SH-4 · Escape.** Tauri's global stack is duplicated in two files (already
  drifted) and closes *all* overlays at once; fold into one topmost-closing
  stack. Native's per-surface AppKit handling is fine. → WS-H
- **SH-5 · Error model.** Split by class in both — the ruling and the Tauri half
  are in CH-11. Native's remaining half: its background banner still has no
  dismiss ✕, and the classes it puts in the banner that the rule puts in the
  modal. → WS-N, WS-Q
- **SH-6 · Window.** Tauri: add `tauri-plugin-window-state` (opens 1280×800
  every launch today; native gets restoration free) and set the window title
  to the repo name (QUICK-WINS item; match native's value). Min-size
  disagreement (720×460 vs 900×600) isn't worth converging. → WS-H
- **SH-7 · Tab behavior.** Native preserves the active tab across repo
  switches (a view preference — right); Tauri resets to Changes as an
  accident of `defaultState` and remounts the history pane. Native loses the
  commit-list scroll position on tab round trips (Tauri keeps both panes
  mounted — its trade); close it with a `ScrollViewReader` restore to the
  hoisted selection instead of keeping subtrees alive. → WS-H, WS-Q
- **SH-8 · Pre-main phases.** Tauri's loading/error-with-Retry phases are
  right; native will need a scan-failure surface on Welcome once RM-1 lands
  (inline row + Retry, not a phase swap). Native's deliberate silence about a
  missing restored repo stays. → WS-L

## 5. Core-hoist catalogue

Everything above that moves into `leogit-core`, collected. Rule of thumb
applied: hoist when the logic is pure, duplicated (or about to be), and
IPC-cost-free; keep per-platform when it's presentation or host-lifecycle.
None of these sacrifice measurable performance; several *save* subprocesses.

Eighteen shipped in WS-B. The two that didn't are the two the rule above
disqualifies for opposite reasons: H-20 costs IPC on every tick (→ WS-J, with
the poll restructure it belongs to), and H-17 is three OS backends that cannot
be verified from macOS (→ WS-K, a workstream of its own).

H-3 is the only hoist still outstanding. It lands in WS-F and **both** clients
adopt it there.

| # | Hoist | Replaces | Feeds |
|---|---|---|---|
| H-1 | ✅ `RepoStatus.merging: bool` filled by `get_status` | one subprocess per tick per client (E-1) + the forgot-isMerging bug class | shipped |
| H-2 | ✅ `get_remote` returns no-remote honestly (`Option`); `DEFAULT_PUBLISH_REMOTE` carries the assumption at the one call site that creates a remote | D-2's dead guard, D-18's doomed fetches | shipped |
| H-3 | `sync_proposal(&RepoStatus) -> SyncProposal` (the ladder as a total function; titles/icons stay per-platform) | native `SyncControls` derivation + Tauri's three loose booleans; makes ROADMAP's force-push-recommended a one-place change | WS-F |
| H-4 | ✅ `derive_clone_target(url, parent)` + `clone_target_path`, matrix-tested | the drifted TS/Swift pair + two shared latent bugs (CL-4) | shipped |
| H-5 | ✅ `match_repo` + the batch `filter_repos` both hosts call | two implementations already drifted on input set (RM-5); gives Tauri the home-dir root free | shipped |
| H-6 | ✅ `known_repos(scan_paths, depth)` (discovery ∪ existence-checked MRU) | Tauri's forgotten rows + native's dead-MRU tiering (RM-3) | shipped |
| H-7 | ✅ `get_commit_detail(repo, sha)` (one `git log -1 -z --raw --numstat`) | two subprocesses per commit selection per client + the error-policy split (HI-7) | shipped |
| H-8 | ✅ `get_parsed_diff` / `get_parsed_commit_diff` (fuse read+parse); `DiffOptions` splits parse from render; `DiffLine.text: Option` | E-8, half of Tauri's per-diff IPC, the double traversal in both (DF-3) | shipped |
| H-9 | ✅ `EmptyDiffReason` (`NoChanges` / `WhitespaceOnly` / `NoTextualChanges`) | D-12 + native's generic copy (DF-7) | shipped |
| H-10 | ✅ `patch_config` + a config write lock + `Config::normalized()` + `config_bounds()`; blank-means-absent everywhere | D-5, D-7, D-17 structurally | shipped |
| H-11 | ✅ `ai::provider_config` / `load_ai_config` in core; per-provider config sections; `timeout_secs` wired into both providers; `ai_api_key` dropped | the duplicated mapping + the untestable pin (ST-6) + the shared-model footgun (ST-8) | shipped |
| H-12 | ✅ `classify_discard(repo, files) -> DiscardPlan` from core's own `ls-tree` logic | native's guessed dialog copy; upgrades Tauri's generic copy free (CH-8) | shipped |
| H-13 | ✅ `FileStatus::letter()` / `label()` + the `file_status_styles()` table both hosts read once | three-way glyph drift (CH-4) | shipped |
| H-14 | ✅ Terminal reader→emitter split with a bounded channel (flow control) and back-pressure-driven coalescing; `resize_terminal` ignores `< 2×2` | E-11/E-12 transport waste, native's unbounded relay, the FitAddon-internals dependency (TE-2/TE-3) | shipped |
| H-15 | ✅ `DiffSizeGuard` (4 MiB total / 5 000 bytes per line) with a `show_anyway` escape | the missing size guard in both (DF-4) | shipped |
| H-16 | ✅ `copy_text(file_diff, start, end)` | byte-identical clipboard in both (DF-6) | shipped |
| H-17 | `core::net` connectivity observer emitting online/offline over the event seam — Linux netlink backend first, then macOS/Windows | Tauri's hard-wired `navigator.onLine` on WebKitGTK; eventually native's separate `NetworkPathObserver` (BG-3) | WS-K |
| H-18 | ✅ `gh_clone` through the `git clone` streaming seam (`gh repo clone … -- --progress`) | the progress-less gh clone in both clients (CL-6) | shipped |
| H-19 | ✅ `fetch(.., background)` picks the 8/8/12 s budget for automatic fetches | an automatic fetch holding the single slot on the 15/30/600 s user budget (BG-8) | shipped |
| H-20 | exclusion-set reconciliation (keep an opt-out through a grace window, drop it after N absent ticks) | the two hand-written, already-drifted exclusion rules (CH-7) | WS-J |

Deliberately **not** hoisted: sort collation (locale into a chrono-free core —
no), relative-date formatting (platform), scheduling policy (host lifecycle),
tab-expansion (single consumer), disambiguation labels (small, but H-6-adjacent
if it ever grows), and the per-status *colour* (H-13 hoists the glyph and the
name; the tint resolves against each host's own palette).

## 6. Workstreams

Per CLAUDE.md: one at a time, user-flow order inside each, visually verified
before the next starts. Every workstream ends with the §8 gates. Sizes are
relative (S/M/L).

**Sizing rule, learned from WS-B.** A workstream is *one client, one or two
adjacent feature areas, roughly a dozen item-halves* — small enough that one
agent holds the whole surface in context and the user can walk its visual check
in a single sitting. WS-B broke that: it was a core-wide layer touching every
area of both clients at once, and while it landed, it was too large to review
as one piece. WS-A's shape (thirteen items, one theme) is the target instead.
A heavy single item counts for several — BR-1's merge port, DF-2's split view,
TE-2's transport rewrite, SH-1's launch path and H-17's three OS backends each
fill most of a workstream alone, so each gets one.

Order is user flow *within* each client — how you get into a repo, then what
you do there, then the machinery underneath. The Tauri block runs first for the
reason it always did: that client is further from the shared bar and its work
is mostly adoption of already-proven native behavior.

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
   + 24 bridge tests, clippy-pedantic at the 184 baseline. (D-5 and D-7 went to
   ground with their hoists in WS-B. Of the rest, D-4 needs the Channel
   transport, D-9 the native terminal frame, and D-14/D-15/D-20 the native diff
   renderer, so they stay with those areas in WS-I/WS-R/WS-P — but D-6 turned
   out not to need its area at all and shipped early, in WS-C.)
2. ~~**WS-B — Core convergence layer (L).**~~ **Shipped 2026-08-27.** Eighteen
   of the twenty hoists landed with tests, regenerated bindings, and adoption
   in **both** clients — a hoist nothing calls is the dead wiring BR-1, CL-8
   and BR-10 are cautionary tales about, so each one replaced its duplicates
   rather than sitting beside them. Per-hoist state is in §5; what that cost
   and closed elsewhere:
   - Three defects went to ground with their hoist, as planned: **D-2**'s dead
     guard (H-2 makes `get_remote` answer `None`, so "skip when there's no
     remote" can finally fire), **D-5** (H-10's `patch_config` — a surface now
     names the fields it owns, so it cannot revert the other client's), and
     **D-7** (H-10's blank-means-absent rule, applied on every read and every
     write rather than per settings form). **D-17**'s clamp became structural
     with them, and **D-12** gained its reason (H-9).
   - Four surfaces were deleted rather than left standing: `is_merging`,
     `get_diff`/`get_diff_whitespace_ignored`/`get_commit_diff`/`parse_diff`,
     `get_commit_files`/`get_commit_stats`, and `discover_repos` — each
     replaced by the hoist that subsumed it. Two hand-written files went with
     them: `apps/tauri-app/src/lib/utils/repoSearch.ts` and
     `apps/swift-ui-app/Sources/LeoGit/Services/RepoSearch.swift`, the drifted
     pair H-5 exists to retire.
   - Gates: 159 core + 24 bridge tests (from 120 + 24), `pnpm check` 0/0,
     prettier clean, zero-warning `xcodebuild`, clippy-pedantic **170** (from
     the 184 baseline).

   Findings the later workstreams need:
   - **§3.3's E-1 is right about the cost and wrong about the mechanism.**
     `get_status` does *not* resolve the git dir — it runs one
     `git status --porcelain=2` and nothing else. `merging` is free anyway
     because the git dir is a *filesystem* question: `<repo>/.git` is either
     the directory or a one-line `gitdir:` pointer (worktree, submodule), so
     `git::git_dir` reads it and keeps `rev-parse --git-dir` only as the
     fallback for shapes that file can't describe. Any future "fold it into
     the status tick" item should check the same way before assuming a value
     is free.
   - **`--name-status` and `--numstat` do not combine** — git honours only the
     former. H-7 uses `-z --raw --numstat`, which does, and whose records are
     told apart by shape (`:` opens a raw record) rather than by section order.
     A rename's numstat record puts its paths in *following* NUL segments, so
     the parse advances by a variable number of segments; the two tests pin it.
   - **H-8 changed the shape of "empty", which DF-10 now depends on.** A failed
     load is an `Err` from `get_parsed_diff`; an empty parse is an `Ok` with
     `empty_reason` set. The Tauri branch can therefore already tell them
     apart — DF-10's remaining work is presentation only (inline pane error
     instead of the blocking modal), not a new core variant. The `LoadFailed`
     enum variant the previous entry asked for is unnecessary and was not
     added.
   - **A blank pane is not one situation.** `parse_diff` returns nothing for a
     pure rename *and* for an empty patch *and* for a whitespace-only edit
     under hide-whitespace, and only the fused call can distinguish the last:
     when the filtered diff has nothing to render, core re-reads the unfiltered
     one and compares. That second `git diff` runs only on the path where the
     pane would otherwise be blank.
   - **H-13 hoists the glyph, not the colour.** Both hosts now read
     `file_status_styles()` once at startup — per row per repaint would be
     absurd for ten short strings. Conflicted settled on `U` and gained its own
     purple token in both palettes (`--status-purple` in Tauri), which is the
     token BR-8's `· merging` suffix should reuse.
   - **`ConfigPatch` fields default to "leave it alone"** (`#[uniffi(default =
     None)]` on the bridge side), so a one-field write is one line. Clearing an
     optional field is patching it to `""` — the config's standing
     blank-means-absent rule doing double duty instead of a second `Option`
     layer every host would have to model. ST-3's instant-apply rewrite in
     WS-H is now a small change per control rather than a redesign.
   - **`config.toml` gained `[claude]` and `[ollama]` tables.** Field order in
     `Config` is load-bearing: `toml` serializes in declaration order and a
     table swallows every key after it, so nothing scalar may be declared below
     those two. A round-trip test pins it.
   - **H-14's coalescing is back-pressure-driven, not a fixed 8 KiB/8 ms
     window.** A fixed threshold with 4 KiB reads only ever halves the delivery
     count; gathering whatever is *already queued* is self-tuning — nothing is
     held when the host keeps up, and a `cat` flood arrives in a few dozen
     deliveries instead of thousands. A lone chunk with nothing behind it goes
     out immediately, so echo latency is unchanged. TE-2's `Channel` rewrite
     inherits this and should not add a second layer of batching; the native
     relay's own coalescing is now redundant but harmless (WS-R may simplify
     it to the main-actor hop it still needs).
   - **`resize_terminal` now ignores a `< 2×2` grid itself**, so D-9's fix is
     purely the native inner-frame pin — the PTY can no longer be told about a
     collapsed panel from either client.
   - **H-16 has no consumer yet.** `copy_text` and `copy_diff_text` are the one
     deliberate exception to the no-dead-surface rule in this workstream,
     because DF-6 lands their UI in WS-E/WS-P and splitting the helper from its
     tests would have been worse. Wire it there or delete it.
3. ✅ **WS-C — Tauri repo switcher & clone. Shipped 2026-08-27.** RM-3's
   remainder, RM-4, RM-7(T), RM-8, RM-9, CL-1/2/3/5/7(T) and **D-6**; RM-2 was
   decided against instead, in *both* clients — the one line of native this
   workstream touched (see its §4.1 entry). Both Tauri repo
   lists gained a `Clone Repository…` footer, which is what finally put clone in
   the startup picker; `last_opened_repo` restores on the path being a
   repository rather than on this launch's walk re-finding it; discovery re-runs
   on switcher-open and Settings-close in both phases through one coalesced pass
   (`services/repoDiscovery.ts`); both lists answer *looking / none found (with
   the folders walked) / none matched* from one shared component; the switcher
   sorts active → MRU → name tail, deleting `repoActivity` and
   `get_last_commit_timestamp` (half of E-5); the switcher locks mid-transfer;
   config is re-read on window activation (D-6). Clone gained Return-to-clone, a
   Refresh button beside the cached gh list, tab-kept/inputs-cleared re-arming,
   a `<fieldset disabled>` mid-clone freeze with progress outside it, row
   description tooltips, a *none / no matches* split, and a name tiebreak.

   **What the later workstreams should know.**
   - **A repo list is exactly what the scan paths cover** — neither client has a
     per-folder open action, by decision (RM-2). A workstream that finds a
     repository unreachable should reach for discovery or the scan-path setting,
     not add a second way in. The one exception is native Welcome's
     `Open Repository…`, which **WS-L must delete in the same change that gives
     Welcome the discovery list** (RM-1). `resolve_repo_root` is FFI-only; the
     Tauri launch restore uses `is_git_repo`, the cheap existence check it wants.
   - **Settings' close is now a handler, not `showSettings = false`.** All three
     dismissals in `MainLayout` (button, Escape, `⌘,`) route through
     `closeSettings()`. **WS-H rewrites this dialog (ST-3) and must keep every
     dismissal on that handler**, or a scan-path edit silently stops re-walking.
   - **`RepoListEmptyState.svelte` is the shared empty state for both repo
     lists.** WS-L ports the same three answers natively; change the strings
     there, not in a copy.
   - **The `<fieldset disabled>` freeze is the pattern for uncancellable
     dialogs** (STYLE.md, *Modals / dialogs*). Reset `min-inline-size: 0` or the
     fieldset refuses to shrink and widens the dialog; keep progress and errors
     outside it or the bar reporting the operation is dimmed by it.
   - **`$effect` in Svelte 5 tracks every reactive read, including ones inside a
     branch.** The clone dialog's re-arm effect read `tab` to decide whether to
     lazy-load, which made switching tabs re-run the whole reset and wipe a
     half-typed destination path. It now reads only `isOpen`, with the rest
     under `untrack`. WS-D hit the same wall twice (the provider probe, the
     first-file auto-select) and the fix generalized — see its entry for the
     derived-key form.
   - **CL-7's per-tab error state needed no Tauri change**: `selectTab` already
     clears `cloneError`, and the gh-list failure is a separate state rendered
     inside the list. That half of CL-7 belongs to WS-L alone.
   - **RM-7's "still looking" state is unreachable in the main-view dropdown**
     by construction — the open repo is always in the list, so `repos` is never
     empty there. It is live in the startup picker, which is the phase that can
     have none. Not a gap; don't "fix" it by showing a spinner over a populated
     list.
   - **One deliberate divergence from this plan's text.** CL-3 called for GitHub
     Desktop's Enter-on-row-clones; the dialog implements it as **two presses**
     (the first commits the cursor's row as the selection, the second clones),
     because the destination path is derived from the selection and one press
     would clone before the user had seen where it lands. Flipping to one press
     is a two-line change in `handleListKeyDown` if the user prefers it.
4. ✅ **WS-D — Tauri changes tab & composer. Shipped 2026-08-27.** The main loop.
   CH-13, CH-3(T), CH-4(T), CH-9(T), CH-10(T), CH-11 + SH-5(T), CH-12 and
   ST-9's Generate gate **on both clients**; ST-7 turned out to have shipped in
   WS-B. The changes
   list now opens its first file and heads its rows with "N of M files
   included"; the status letter sits on WS-B's specified 18×18 tinted plate;
   ⌘↩/⌘G are window-wide; the composer can no longer clip its own Commit button
   in a short window; Generate is gated on the provider answering; the
   embedded-repo dialog's backdrop stops cancelling mid-commit; Copy File Path
   stopped crossing the IPC boundary to join two strings. The largest piece is
   the error split: **`reportActionError` / `reportNotice` in the repo store**,
   with `ErrorModal` finally receiving the `onRetry` it always accepted.

   **ST-9 was corrected four times across three visual checks**, and it grew a
   native half in the process — it is the one item here that ended up shipping to
   both clients. The gate now asks two sources, a probe before the click and a
   reading of the request that failed, because an expired session is invisible to
   the first; the fix command runs in each client's own terminal (Tauri
   `Terminal.runCommand`, native `TerminalStore.run` → `TerminalController.run`,
   both queueing until the shell is up). See ST-9 for the four wrong models.

   **What WS-E and later workstreams should know.**
   - **Ship both clients in the same change.** ST-9's first pass was Tauri-only,
     and the user's next message was that nothing had appeared natively. The
     subject of this plan is the two clients agreeing; a fix that lands in one is
     a new parity item, not a finished one. Cost of porting late: an FFI export,
     a bridge wrapper, a store, a view, and a re-verified count in FRONTEND §1.
   - **Never clear state on the way *into* an async refresh.** Blanking the
     provider verdict before re-probing made the remedy visibly blink out and
     back on every window focus — the trigger the re-probe is bound to. Write the
     answer when it arrives; hold the old one until then, and tag it with what it
     describes so staleness is a comparison rather than a clearing step someone
     forgets. Any WS-E pane that refetches on focus has this shape.
   - **One strip per surface, not one per source.** The composer grew a red error
     line and a separate caption row at opposite ends of the box, both describing
     one state. They are now one block, and the remedy *replaces* the failure it
     was read out of. HI-8 and DF-10 add rows to panes that already have an error
     slot — join it rather than stacking beside it.
   - **Check the code before implementing an inventory item.** ST-7 was listed
     as open here and had been closed by WS-B, which rewrote `setProvider` for
     `patch_config` and brought the revert along. WS-B's hoists reached further
     into the clients than §5 records; the same is likely true of items still
     marked open in areas it touched.
   - **STYLE.md was ahead of the code, twice.** The 18×18 plate and the
     composer's height clamp were both already written there as the shared
     target — WS-D was implementing a rule, not inventing one. Read STYLE for
     the surface you are about to change before designing it.
   - **Failure classification is two functions, and WS-E owns call sites of
     both.** HI-8's page-error demotion is `reportNotice`, not a third path.
     DF-10's inline pane error *replaces* the `reportActionError(…, retry)` WS-D
     put on the two diff loads — take the retry with it or drop it deliberately,
     don't leave a modal and an inline error describing the same failure.
   - **`modalOpen` in `MainLayout` is the one list of "something is on top".**
     Overlays, confirmations, the error modal. The composer chords read it; a
     new overlay joins it or joins nothing. SH-4 (WS-H) turns it into the real
     topmost-closing stack, and should fold Escape's own list into it — Escape
     still keeps a separate condition today because it needs the clone dialog's
     busy state, which the chords don't.
   - **A window-level chord that acts on a component reaches it by `bind:this`
     plus an exported function**, not by lifting the component's state.
     `CommitMessage.requestCommit` / `.requestGenerate` gate exactly as their
     buttons do, which is the point: one gate, two entry points.
   - **Auto-select is a `$derived` key read by an `untrack`ed `$effect`.** The
     effect must not read the store directly, or it re-runs on every one of the
     poll's ticks; a derived string of the path list settles first, so the body
     runs when the *set* changes. **HI-3 is the same shape** — auto-select the
     newest commit, re-seat when the selected sha is rewritten away — and should
     reuse it rather than watch `commits` directly.
   - **Measure a pane through a wrapper, not the pane.** The Changes tab pane is
     `display: none` while History shows and reports zero height, which would
     collapse the composer's cap on every tab round trip. `.tab-panes` exists to
     be the thing with a height.
   - **Native's error split disagrees with the rule this plan just wrote.**
     Discard, checkout and undo failures go to native's banner; FRONTEND §6.13
     puts them in the modal. WS-N and WS-Q close it, and native's banner needs
     its dismiss ✕ in the same pass.
5. ✅ **WS-E — Tauri history & diff panes. Shipped 2026-08-27.** HI-2, HI-3,
   HI-4(T), HI-6, HI-7(T), HI-8(T), HI-10(T), DF-5's interim, DF-6(T), DF-8(T),
   DF-10 — plus **DF-9 and DF-11 on both clients**, and D-20 closed with them.
   The log became an append-only list rooted at HEAD, deleting the sliding
   window and the three defects that only existed because its top could drift;
   the list gained arrow keys, selection-on-right-click, and native's
   auto-select/re-seat rule; the detail card stopped printing the commit's
   trailers twice; paging left the repo-wide loading flag and its failure left
   the modal; and both diff panes stopped blanking themselves on a slow load
   and started stating a failed read inline.

   **What WS-F and later workstreams should know.**
   - **A plan entry is a decision, not a specification, and the code gets a
     vote.** HI-2 said "prepend on HEAD move". Neither client had a prepend, and
     against a capped re-read it only differs past 500 loaded rows — while the
     re-read also refreshes tag decorations and is what §6.8's own invariant
     says (*refetch*, not patch). The simpler shape shipped, and the entry now
     records that. Read what the clients actually do before building what an
     entry describes; three of this workstream's items were already half-true.
   - **Deleting the mechanism beats fixing it.** Three separate items (D-1's
     gate, D-11's replacement-vs-slide signal, and the `skip > 0` hole that made
     a commit at offset 0 bump nothing) were all the sliding window's shadow.
     Re-reading from offset 0 made row 0 = HEAD *true by construction*, and none
     of the three has a case left to get wrong. When several items in one area
     keep pointing at one mechanism, price replacing it before patching each.
   - **A flag with no writer left is a flag to delete.** Moving paging onto
     `log.isPaging` left `repoState.isLoading` unwritten; it and its term in
     `canCommit` went with it. The same sweep is worth doing after any item that
     re-homes state — WS-F's SY-1 retires three loose booleans the same way.
   - **Ship both clients in the same change — and the reverse also bites.** Two
     WS-E items (DF-9, DF-11) were listed as Tauri-only with a native half filed
     under WS-P. Shipping the Tauri half alone would have put Tauri *ahead*, which
     is the same parity gap in the other direction, so both were done here.
     WS-P is smaller by exactly those two. Check which direction a "Tauri half"
     would leave the clients pointing before deferring the other one.
   - **The retry a previous workstream added is part of the surface you are
     replacing.** DF-10 moved the diff failure inline, which displaced WS-D's
     `reportActionError(…, retry)`. Dropping it was the right call *because the
     gesture survived elsewhere* (a cleared payload makes re-clicking the row a
     real re-read) — not because it was in the way. State which, either way.
   - **Applying a rule to only the item that broke it leaves the surface
     inconsistent in the other direction.** HI-10 asked for one ellipsis to go;
     the same menu had another item that confirms and lacked one. The rule is
     "an ellipsis means it asks first", and it is worth spending the second
     character to make the menu obey it.
   - **`.opacity` + `.overlay` is a modifier; a `ProgressView` branch is a
     different view.** The native pane's own comment claimed scroll survived a
     slow reload; it could not, because a branch changes what SwiftUI builds and
     the `ScrollView` was rebuilt at the top. Any native "keep it on screen while
     it reloads" is a modifier on the same view, never a sibling branch.
   - **Two branches beat conditional a11y attributes.** DF-5's hunk header
     needed `role`/`tabindex`/handlers to appear together or not at all;
     `{#if}` around a five-line block is what keeps `pnpm check` at zero, and it
     reads better than three ternaries.
   - **FRONTEND was contradicting itself on its own counts** — §1 said 73 Tauri
     commands, §3's heading said 68; §1 said ~35 DTOs, §5's heading ~30. The
     catalogue itself is complete and correct (73 documented, 73 registered,
     verified against `generate_handler`); only the headings were stale. Both
     fixed. Re-verify a count you are about to cite rather than carrying it.
6. **WS-F — Tauri sync ladder (M). ← next.** One core hoist and the control it feeds.
   **H-3** (`sync_proposal(&RepoStatus)` as a total function) lands first and
   *both* clients adopt it — native swaps its `SyncControls` derivation for it,
   Tauri replaces three loose booleans. Then SY-1 (three controls collapse to
   the adaptive ladder, keeping Tauri's on-button count badges), which makes
   SY-3, SY-4 and SY-9 fall out; SY-2 (manual fetch is *unreachable* today —
   ship its three-line interim first, since it is the gap most likely to be
   noticed), SY-5 (stop suppressing inferred counts), SY-6 (error inline in the
   publish dialog + indeterminate progress + the org hint), SY-7's Tauri half
   (name the force-push target from `status.upstream`, which also retires a
   `git remote` per repo open), SY-8 (post-op = status **+** log), SY-11's
   indeterminate case. SH-3 rides along: ⌘R's semantics are decided by SY-1
   removing the Refresh button, and its transfer guard is RM-8's neighbor.
7. **WS-G — Tauri branches & merge (M).** BR-1 is most of it — the merge flow
   is fully built and completely unreachable (nothing ever sets `showMerge`), so
   this is a port of native's source submenu → commit-count preview → Merge /
   Squash & Merge → conflicts-as-data → Abort, plus the two native refinements
   (hide the submenu while merging; GH Desktop's "already up to date"
   zero-count treatment). BR-2 (Abort has no Tauri UI at all — ~15 lines
   against the already-polled `merging`, worth shipping ahead of BR-1), BR-3
   (reload on dropdown open), BR-4's Tauri half (busy state — double-clicks
   contend on `index.lock` today), BR-5 (guard same-branch re-select, ~8
   processes), BR-6 (create-branch error inline), BR-7 (native's "Unmerged
   commits are lost." wording + a branch-row context menu), BR-11 (reuse the
   picker's `listNavigation`).
8. **WS-H — Tauri settings & window chrome (S/M).** ST-3 is the bulk: the move
   to instant-apply, each control patching its own field through WS-B's
   `patch_config`, Save/Cancel collapsing to Close. WS-B's `ConfigPatch`
   defaults make that a small change per control rather than a redesign. With
   it: ST-1's Tauri live-apply rows, ST-4's Tauri half (stop showing raw ms),
   ST-10's Tauri half (the scan-path Edit ▸ Done lock — the one field that
   stays out of instant-apply), and the window chrome that belongs nowhere
   else: SH-2's Tauri half (⌘1/⌘2), SH-4 (fold two drifted Escape stacks into
   one topmost-closing stack), SH-6 (`tauri-plugin-window-state` + the repo name
   in the title), SH-7's Tauri half (preserve the active tab across switches).
9. **WS-I — Tauri terminal transport (M).** TE-2 and **D-4** together, because
   the race is a property of the transport: a frontend-created `Channel` passed
   into `start_terminal` mirrors the native seam and closes the two-round-trip
   window where the reader thread emits with no listener registered.
   `start`/`close`/`resize` become `(async)` (E-12); `write` stays sync — IPC
   arrival order *is* the keystroke-ordering guarantee. Do not add a second
   layer of batching: WS-B's coalescing is already back-pressure-driven
   underneath. Then TE-1's remaining narrowing (⌃-only, in *both* the window
   handler and `Terminal.svelte`'s custom key handler — they must keep agreeing
   or the chord becomes unreachable from inside the panel), TE-5's Tauri half
   (modifier-click + hover affordance + an OSC 52 write-only handler), TE-6
   (refocus after focus is stolen), TE-7's Tauri halves.
10. **WS-J — Tauri background cadence (M).** The machinery underneath, and
    where the efficiency register concentrates. BG-1 (the visibility ladder +
    paused sweeps + GH Desktop's once-per-session 0–30 s skew), BG-2
    (self-scheduling `setTimeout` chain, which delivers the `auto_fetch` re-arm
    for free), BG-4's equality gate (E-6), BG-6 (read `activeElement` at tick
    time instead of latching a `focusin` flag that strands `true`), DF-1's
    Tauri half (the `stat_stamp` that already arrives every tick and is never
    read), E-2 (`pollHeadSha` duplicates `status.head_sha` from the same tick),
    E-4 (one tier loop, not three colliding), E-5's remainder (bound the
    fan-out; RM-4 already halves it). Plus **H-20 / CH-7**, deferred here by the
    user: the exclusion-set grace counter has to advance every tick, so it
    cannot be gated on "the file list changed", and BG-4 is restructuring
    exactly that poll. Until it lands the two clients keep their disagreeing
    rules.
11. **WS-K — Connectivity observer, both clients (M).** **H-17** and BG-3,
    alone because they are a mini-project on a different machine: three OS
    backends, a new `CoreEvent` variant and a new dependency, none of it
    verifiable from macOS. Linux netlink first — it is the broken platform
    (`navigator.onLine` is hard-wired `true` on WebKitGTK, silently disabling
    the offline gate, the recovery kick and the update-check retry) and the
    user's Linux machine is where the only consumer is written. Then macOS, and
    only once it proves equivalent does native retire `NetworkPathObserver`.
    Until then `navigator.onLine` stays authoritative-negative only.
12. **WS-L — Native welcome, switcher & clone (M).** The native block starts
    where the user does. RM-1 (Welcome is a dead end today — reuse the
    switcher's list as its body, run discovery from it, adopt the
    exactly-one-discovered-repo auto-open rule, and **delete Welcome's
    `Open Repository…` / ⌘O with the `.fileImporter` behind it** — RM-2's last
    entry point, which only exists because there is no list here yet and must
    not outlive the list arriving, or precede it), RM-5's native half (GitHub
    identifiers on the rows, the half WS-B's shared search rule doesn't supply
    — port them lazily, visible rows only), RM-6 (↑/↓ cursor via
    `List(selection:)`), RM-7's native half (the "Choose folders to search"
    CTA), SH-8 (the scan-failure surface RM-1 creates the need for — an inline
    row + Retry, not a phase swap), and the native halves of CL-1 (clone
    shouldn't sit under the transfer-disable — it claims no slot), CL-2 (cache
    + refresh instead of a 20 s dead zone per open), CL-3 (the GitHub tab is
    mouse-only) and CL-7. **RM-1 comes before SH-1** deliberately: SH-1 claims
    its launch target inside the Welcome task, so rebuilding Welcome first
    means touching it once.
13. **WS-M — Native launch, menus & updater (M).** SH-1, the largest native
    contract gap: export `resolve_launch_target` /
    `set`+`take_pending_launch_target` / `init_repo` / `is_git_repo`, claim the
    target ahead of `restoreLastRepo`, warm start via
    `NSApplicationDelegate.application(_:open:)` + a `CFBundleDocumentTypes`
    entry for `public.folder` (which buys drag-onto-Dock and Finder "Open With"
    free), the "Create a repository here?" confirmation on the root view, and
    `install.sh`'s native branch — whose bundle-id mismatch breaks *any*
    scripted launch today. LaunchServices gives single-instance free. With it
    SH-2's native half (File ▸ Open/Clone, View ▸ ⌘1/⌘2, a Branch menu with ⌘B
    via the focused-scene-value pattern, View ▸ Show/Hide Terminal owning ⌃`) —
    today ⌘G/⌘↩/⌃`/⌘O are discoverable nowhere — and BG-5 (the update checker:
    async FFI export + `UpdateInfo` mirror + scene-level checker + chip, gated
    on `isOnline` alone, **not** the breaker).
14. **WS-N — Native file list & composer (M).** CH-1 (multi-select via
    `List(selection: Set<String>)`) and CH-2 (Space to include/exclude — the
    highest-frequency action in the app has no native keyboard route at all)
    carry the rest: CH-3's native half (true tri-state), CH-5 (`old → new` for
    renames, indistinguishable from an add today), CH-6 (the ↪ glyph instead of
    a width-eating text tag), CH-8's native half (in-flight busy state), CH-9's
    native half (native container + Tauri's text), CH-10's native half (72-char
    counter, included-row weight cue, truncation-only tooltip, keyboard resize,
    "Committing…", drag-end coalescing of the `UserDefaults` writes), CH-11's
    native half (a dismiss ✕ on the banner). **E-7 moves here from the
    efficiency sweep**: the full reload on every discard/ignore is the same code
    path CH-1's bulk discard rewrites, and a 30-file discard is ~90 subprocesses
    + 30 reloads today. **E-10** with it — `PathText.fittedParts` is what
    renders these rows.
15. **WS-O — Native split diff (M).** DF-2 alone, the largest single piece of
    new native UI in the plan. The constraint is that it must not fork the
    renderer: one row model feeds both arrangements, the pairs cross the bridge
    only when the split layout is active (WS-B's `DiffOptions` already stops
    producing them otherwise), and the toggle moves into the diff header on
    both clients — GH Desktop treats split/unified as a per-diff control, not a
    Settings preference. DF-13's wrap check rides here rather than in the polish
    pass, because rebuilding the row model is when the break policy is decided.
16. **WS-P — Native diff polish (S/M).** Smaller than it was: WS-E took DF-9
    (with D-20) and DF-11 on both clients rather than leaving native behind.
    What is left: **D-14** (no scroll reset on file switch — no `ScrollViewReader`
    exists, and FRONTEND §6.3 now states the contract this has to meet).
    DF-6 + D-15 (rebuild clipboard text from the line model — **this is where
    WS-B's `copy_text` gets its consumer, or gets deleted**; it is the plan's
    one standing dead surface, and the Tauri interim WS-E shipped narrows the
    damage without closing it). DF-8's remaining native half (rename header from
    the parsed diff, suppress `+0 −0`). DF-12 + E-9 (the 80 ms debounce and the
    per-file `stat_stamp` gate — DF-1's native half, which is what stops a
    whole-status epoch re-tokenizing the open diff on an unrelated edit).
17. **WS-Q — Native history & sync polish (M).** All small, one screen each.
    HI-4's native half (gate "No commits yet" on
    the first load), HI-5 (a visibility-gated 10 s tick reusing
    `BackgroundSchedulingPolicy` — don't invent a second gate — plus the shared
    tier vocabulary and the detail card's date format), HI-7's native halves
    (key the detail task on `(repoPath, sha)`, clear `commits` on repo switch),
    HI-8's native bits (prefetch at N−5, the tag-chip treatment STYLE.md
    specifies), HI-9 (checkout busy state), HI-10's native half (a `--mixed`
    reset can't change the branch list), SY-7's native half (Tauri's dialog
    lifetime), SY-10 (selectable, monospaced transfer errors), BR-4's
    nil-return fix (callers read "dropped because busy" as success), BR-8 (give
    the `· merging` suffix WS-B's purple token), SH-5 and SH-7's native halves.
18. **WS-R — Native settings & terminal (S/M).** ST-5 (route both provider
    owners through `AppConfigStore` — with both windows open the pickers can
    disagree today; grow it the `scanPaths` accessor three call sites bypass),
    ST-3's native half (don't render editable defaults that aren't the user's
    settings), ST-9's native export, ST-10's native half (Edit ▸ Done +
    `.monospaced()`), TE-3 + **D-9** (pin the inner frame so a collapsed panel
    stops reflowing the emulator to one row — WS-B already made the PTY side
    safe, so this is purely the frame — plus the missing 80 ms resize debounce,
    placed in `TerminalController.resize` and *not* the delegate, to keep the
    one-shot initial-size push), TE-4 (scrollback 1000 explicitly, both
    clients), TE-5's native half (the hover affordance — SwiftTerm's hover
    surface needs an API check), TE-7's native halves.
19. **WS-S — Sweep & contract cleanup (S/M).** What genuinely had no home, plus
    the work that can only land once everything else has:
    - **Leftover efficiency**: RM-11 (move the native sweep's slot re-check
      inside the loop — its own tier runner already does), BG-7 (fire the
      existing resync from the policy's occlusion edge).
    - **Dead-surface deletions** — CL-8's `check_auth` (spawns `gh auth status`
      every launch to write a field with zero readers), BR-10's rename and
      remote-delete wrappers (the feature is deferred to ROADMAP, the wiring
      goes now), the `has_staged_changes` wrapper, the unconsumed derived
      stores. DF-5's scaffolding stays and ST-9's probe is wired by then, so
      neither is deleted. Each deletion also drops a row from FRONTEND §3's
      command tables.
    - **FRONTEND.md** — retire each §8 row the parity work closed, and add the
      divergences this plan keeps: counts placement, progress surface, error
      surface, loading presentation, detached/merging markers, settings surface.
      The terminal link convention becomes a shared §6 rule (TE-5), not a §8 row.
    - **Stale source comments**, all verified: `CommitStore.swift:20-23`,
      `SyncControls.swift:9-11,131`, `TerminalStore.swift:32`,
      `CloneSheet.swift:8-10`, `ContentView.swift:154-159`,
      `TerminalSessionView.swift:124-126`,
      `BackgroundSchedulingPolicy.swift:7`, `repoSyncScheduler.ts:66-70`.
    - **Doc claims outside the audit's checklist**, fixed as their area lands:
      TECHNICAL's width-keyed `PathText` cache (E-10 — it isn't); DESIGN's claim
      that the Tauri branch dropdown matches the repo picker (BR-11); DESIGN's
      committer-vs-author date for commit rows (HI-5); DESIGN's header-cluster
      list (ahead/behind are badges *on* the Pull/Push buttons).

Suggested order: **A → B → … → S**, as lettered. Each workstream maintains its
own doc rows as it lands (per CLAUDE.md); WS-S carries only what needs the whole
plan finished.

Two sequencing notes that are not free to reorder. **H-3 (WS-F) is the last core
hoist, and both clients adopt it there**, which makes WS-F the only Tauri-block
entry that also touches native code. And **WS-K can run at any point after
WS-J** — it needs a Linux machine rather than a predecessor, so schedule it when
that machine is available instead of blocking the native block behind it.

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
  0/0; `cargo test --workspace` green (**164 core + 24 bridge** after WS-D,
  from the 120 + 24 this plan started at — every hoist and every probe rule
  landed with tests);
  `cargo clippy --workspace --all-targets -- -W clippy::pedantic` at
  **166** or better, never worse (the plan opened at 184; WS-B took it to 170,
  WS-C to 166 with the command it deleted). A Tauri workstream also runs
  `pnpm build` clean, so the bundle is proven rather than only typechecked.
- Visual checks per workstream (ask for confirmation, no screenshots), on
  **both** clients whenever a change touches shared core or a ported
  behavior — the checklist comes from the workstream's inventory items.
  WS-A's destructive items (D-1) additionally get a scripted repro against a
  throwaway repo before/after.
- DF-13's wrap check is a named visual item (a minified file in both
  clients) before WS-O closes.

## 9. Documentation updates on completion

The audit's factual corrections are already in the living docs (§2). What each
document still needs is the update that rides its workstream — per CLAUDE.md,
written as each chunk lands, no duplication between documents:

- **FRONTEND.md** — the contract carries the most: every §8 row a parity item
  closes is deleted rather than annotated, and each divergence this plan keeps
  gets a row (WS-S). FRONTEND §3's command tables lose every deleted wrapper;
  FRONTEND §5.2 tracks the diff wire as DF-3 changes it, and FRONTEND §7's
  open decision closes with it.
- **TECHNICAL.md** — new mechanics paragraphs only for genuinely new machinery
  (the core hoists, the Tauri channel transport, the native launch path), plus
  the claims WS-S lists as their areas land.
- **DESIGN.md** — flow 1 stops being Tauri-scoped once WS-L and WS-M land; the
  per-flow
  client hedges retire as parity closes them.
- **STYLE.md** — the status-letter row settled on `U` + the purple token with
  H-13 (done); WS-C added the *Repo pickers* section (the two lists are one
  component family, with the shared footer and empty state) and the
  `<fieldset disabled>` rule for uncancellable dialogs; the header-strip bullet
  collapses to one description when SY-1 converges the two headers.
- **ROADMAP.md** — items close as their workstreams land; the deferrals this
  plan makes (per-line staging, diff virtualization, branch rename +
  delete-on-remote) are already filed there. WS-B added one: GitHub identifiers
  in the native repo switcher's rows, the half of RM-5 the shared search rule
  doesn't supply.
- **README.md** — the merge scoping goes away once WS-G gives the Tauri client
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
  the native app today — fixed as part of WS-M's CLI work, noted here because
  it affects packaging beyond this plan.
