# Plan — Background-refresh parity & diff continuity (native macOS app)

> Status: **in progress — A and B landed and visually confirmed (2026-08-27);
> C, D, E not started.** Five fixes from the Swift-vs-Tauri background/refresh
> audit (2026-08-26), ordered by user flow, each visually verified before the
> next begins. Landed workstreams are kept as compact as-built records
> (decision history included); pending ones keep their full design. This file
> stays the source of truth for what was actually built. Companion contract: [`FRONTEND.md`](../../FRONTEND.md) §6
> (behavioral contract) and §8 (intentional divergences) — both get updated as
> each workstream lands.

## 1. Motivation

The audit found the native client is *architecturally* faithful to the Tauri
client's refresh machinery (same tiers, same breaker numbers, same poll cadence)
but *feels* different for two reasons:

1. **Every diff reload blanked the pane** *(fixed — A and B below)*.
   `DiffStore.load` cleared `payload/rows/tokens` before fetching, and the
   status epoch re-keyed the load on every status change and every app
   activation — so refocusing the app, or any background tick, flashed the
   open diff to a spinner and reset its scroll. The Tauri client keeps the old
   diff on screen and only falls back to a spinner after 150 ms
   (`SLOW_DIFF_THRESHOLD_MS`, ported from GitHub Desktop's
   `SeamlessDiffSwitcher`).
2. **Everything pauses when the app isn't frontmost.** `backgroundPaused()`
   gates *all three* loops on `!NSApp.isActive`. The Tauri client never pauses
   on blur; GitHub Desktop pauses only the multi-repo indicator sweep and keeps
   the active repo's background fetcher alive. Worse, `NSApp.isActive` is false
   while the window is *visible but not key* — LeoGit sitting on half the screen
   next to an editor goes stale in plain sight, which the web-stack clients
   never do. And even a loop we *want* running unfocused would be throttled by
   App Nap unless we hold an activity assertion.

Plus three smaller gaps: no OS connectivity signal (the breaker recovers only by
timeout or refocus), the diff-rendering settings in `config.toml` are silently
ignored by the native diff path, and the update checker / CLI launch integration
are missing entirely (out of scope here — tracked in ROADMAP).

## 2. Design principles

These keep the work maintainable rather than a pile of point fixes:

- **Policy in one place.** "May background work run right now?" becomes one
  small type with named predicates, not boolean expressions scattered across
  loops. Every loop states *which* predicate it obeys, and the type's doc
  comment is the single place the policy is written down.
- **Stores decide *what changed*; views never guess.** `RepoStore` only signals
  "the working tree may have changed"; `DiffStore` — the owner of the actual
  diff content — decides whether anything on screen needs to move. No
  upstream heuristics about which file's diff is stale.
- **Pure logic stays OS-free.** The breaker's backoff math keeps zero AppKit /
  Network dependencies (it is trivially testable); OS signals are separate tiny
  observers that *feed* it.
- **Config gets one native owner.** A single observable `AppConfigStore`
  replaces ad-hoc `GitBridge.appConfig()` calls, so "a setting changed" is one
  observation, not N re-reads with N different staleness windows.
- **Lockstep with `FRONTEND.md`.** Where the native client deliberately
  diverges (focus policy, occlusion awareness), the divergence is recorded in
  §8, not left as an unexplained behavioral drift.

Per `CLAUDE.md`: before writing each workstream's code, pull current docs for
the SwiftUI/AppKit APIs it touches (context7 / developer.apple.com) — the ones
named below (`occlusionState`, `beginActivity`, `NWPathMonitor`,
`onGeometryChange`) are exactly the churn-prone kind.

---

## 3. Workstream A — Seamless diff switching ✅ (landed 2026-08-27)

**As built** (`Stores/DiffStore.swift`, `Screens/DiffView.swift`; no FFI
changes — the full mechanics live in TECHNICAL.md's diff paragraph):

- `DiffStore` no longer clears `payload/rows/tokens` when a load starts; it
  tracks `phase: idle / loading(slow:) / failed` beside them.
- A 150 ms slow-load escalation (`slowLoadThreshold`, Tauri's
  `SLOW_DIFF_THRESHOLD_MS`) runs as an unstructured timer task racing the load
  under the existing `generation` guard — unstructured because `.task(id:)`
  cancelling the load must not cancel the escalation while the blocking FFI
  call is still running.
- **Value-equality skip**: a parse equal to what's shown publishes nothing
  (`DiffPayload: Equatable`) — rows, scroll, tokens survive. Tokens still
  refresh in the background on an equal payload (context lines can recolour
  when blob content changed without the diff text changing) and swap in only
  when different.
- View rule: last-shown state stays during a reload; spinner only on
  `loading(slow: true)`; a fast first load stays blank rather than flashing a
  sub-threshold spinner.

**Decision (kept from implementation):** one deliberate improvement over
Tauri — at the slow threshold Tauri also *drops* the old payload, so a
slow-but-identical reload repaints from scratch; the native store keeps it, so
the equality skip preserves scroll even then. The spinner still replaces the
content visually — only the store state survives.

---

## 4. Workstream B — Working-tree epoch + content stamp ✅ (landed 2026-08-27)

**As built:**

- `RepoStore.statusEpoch` → `workingTreeEpoch`, one documented meaning: *"the
  working tree may differ from what any derived view shows — re-derive if you
  care."* Bumps on status change, explicit refresh, and refocus, exactly as
  planned. RepoStore signals possibility; `DiffStore`'s equality skip (A) is
  where reality is checked — deliberately no narrower heuristic, which would
  reintroduce the Tauri client's staleness bug.
- **Amendment found in visual testing:** "bump when status changed" could not
  see *content* edits at all — `RepoStatus`/`FileEntry` carried nothing
  content-derived (porcelain v2 has HEAD/index hashes, no worktree hash), so
  editing a file whose row already read "modified"/"new" produced a
  byte-identical status and the open diff went stale until refocus/reselect.
  Options considered: (a) an opaque **stat stamp in core**; (b) bump the epoch
  every poll tick (Swift-only, but a per-tick `git diff` subprocess and a
  starvation risk for loads slower than the tick); (c) accept Tauri parity.
  **Chosen: (a)** for long-term correctness.
- `FileEntry.stat_stamp: Option<String>` — opaque `"{mtime_ns}:{size}"`, git's
  own stat-cache pair; filled only by `get_status` in one end-of-function
  pass; `None` off-disk (deletions) and in `get_commit_files` (immutable
  history). A string because the Tauri wire is JSON, where nanosecond mtimes
  exceed 2^53 and a number would silently lose precision. Core derives
  `PartialEq` on `FileEntry`/`RepoStatus` (+ `Eq`/`Copy` on `FileStatus`);
  pinned by `stat_stamp_sees_content_edits_and_absence`.
- Scope grew beyond the original file list: `core/src/git.rs`,
  `ffi/src/lib.rs` (mirror field), regenerated bindings, the Tauri TS
  `FileEntry` type (additive, unused there for now). Clippy-pedantic baseline
  unchanged (184); 120 core + 24 bridge tests green.

---

## 5. Workstream C — Background scheduling policy (focus, visibility, App Nap)

**Current.** One predicate (`activeOperation != nil || !NSApp.isActive`) pauses
the status poll, auto-fetch, tier scheduler, and sweeps alike.

**Target policy** (the GH-Desktop split, plus a native improvement):

| Work | Pauses on network op | Pauses when app inactive | Pauses when window not visible |
|---|---|---|---|
| 2 s status poll (active repo, local) | yes | **no** — slows to 10 s | **no** — slows to 30 s |
| Auto-fetch loop (active repo, network) | yes | no | **no** — interval stretched ×3 |
| Tier scheduler + sweeps (other repos) | yes | **yes** (GH Desktop model) | yes |

Rationale: a visible-but-not-key window keeps telling the truth (the audit's
"stale in plain sight" case — the web clients never had this failure mode
because DOM timers don't know about key windows), and a hidden window keeps
refreshing slowly so refocusing reveals a current screen instead of a sudden
catch-up. The multi-repo fetch fan-out is the only genuinely deferrable work,
and the existing refocus resync remains its catch-up path. The cadence ladder
keeps every state honest without running full-rate forever.

**Design.**

- New `Services/BackgroundSchedulingPolicy.swift`: a small `@MainActor
  @Observable` type owning the three inputs and exposing named predicates —
  `canPollStatus`, `canAutoFetch`, `canRunRepoSweeps` — plus
  `statusPollInterval` (2 s active / 10 s inactive). Inputs:
  - `networkOpInFlight` — fed by `SyncStore` (replaces the closure capture).
  - `isAppActive` — `NSApplication.didBecomeActive/didResignActive`
    notifications.
  - `isWindowVisible` — the key window's `occlusionState`
    (`NSWindow.didChangeOcclusionStateNotification`); a fully occluded or
    miniaturized window stops everything, which is *more* battery-polite than
    today while the *visible* case gets livelier.
- `ContentView`'s loops and `RepoDirectoryStore.runScheduler`/`sweepVisible`/
  `refocusSweep` take the policy object instead of the `isPaused` closure; each
  call site names the predicate it checks. The policy type's doc comment
  carries the table above — the one place the rules live.
- **App Nap**: a `Services/AppNapSuppressor.swift` holding a
  `ProcessInfo.beginActivity(options: .background, reason: "leogit background
  git refresh")` token **only while** (a repo is open) ∧ (`canPollStatus` ∨
  `canAutoFetch`). Acquired/released from the policy's state transitions, so
  the assertion can never outlive the reason for it. Without this, unfocused
  `Task.sleep` timers get coalesced and the whole workstream silently doesn't
  work — this is the platform constraint the audit surfaced.
- `FRONTEND.md` §8 gains a row: *focus/visibility scheduling — intentional
  divergence; native pauses per-window visibility, Tauri runs always.*

**Files.** New `Services/BackgroundSchedulingPolicy.swift` +
`Services/AppNapSuppressor.swift`; edits in `Screens/ContentView.swift`,
`Stores/RepoDirectoryStore.swift`, `Stores/SyncStore.swift` (publish op state
into the policy), `Screens/RepoSwitcher.swift` (threads the policy through to
`sweepVisible` — it carried the old `isPaused` closure).

**Decisions made while implementing (2026-08-27, pending visual
confirmation):**

- **"The key window" can't literally feed `isWindowVisible`:**
  `NSApp.keyWindow` is *nil* while the app is inactive — exactly the
  visible-but-not-key case this workstream exists to fix. Instead the policy
  tracks the window hosting the repo UI: a zero-sized `NSViewRepresentable`
  in the policy's file (`View.trackWindowVisibility(with:)`, attached to
  `ContentView`'s root) reports its hosting `NSWindow` via
  `viewDidMoveToWindow`, and the policy observes that window's occlusion
  notification. Side effect worth knowing: a visible Settings window doesn't
  keep loops alive while the repo window is minimized — the repo window is
  the one that counts.
- **Classic notification API, not typed messages:** the concurrency-native
  `NSWindow.DidChangeOcclusionStateMessage` / `NotificationCenter`
  message types are macOS 27-beta; the app targets macOS 26. Block observers
  on `.main` with `MainActor.assumeIsolated` inside (these AppKit
  notifications post on the main thread), removed in an `isolated deinit`
  (SE-0371) — a plain deinit is nonisolated under Swift 6 and can't touch
  the non-Sendable tokens.
- **A fourth policy input, `isRepoOpen`:** the App Nap formula (repo open ∧
  work allowed) needs repo-open state, which the three planned inputs don't
  carry. Fed by `ContentView` from `store.repoPath`; only the assertion
  reads it — the loops it would gate don't exist without a repo.
- **`SyncStore` gets the policy by init injection** and mirrors
  `activeOperation` into `networkOpInFlight` from a `didSet`, so any future
  writer of the slot keeps the mirror honest; `ContentView.init` creates the
  pair together.
- **Amendment after the first visual pass (user decision): hidden ≠ paused.**
  The as-planned build stopped everything while the window was occluded or
  minimized, so refocusing revealed a sudden catch-up. Chosen instead: the
  active repo's work never stops — the status poll slows to 30 s while
  hidden (ladder 2 s / 10 s / 30 s) and auto-fetch stretches its configured
  interval ×3 while hidden ("a bit more efficient than GH Desktop", which
  fetches at one flat interval regardless). Only the multi-repo tier
  scheduler and sweeps still pause when the app is inactive. Consequence:
  the App Nap assertion is now held for effectively the whole time a repo
  is open (released only while a user transfer holds the network slot);
  `isWindowVisible` feeds cadences, not gates. Un-occluding without
  activating the app can take up to one 30 s beat to catch up — activating
  resyncs immediately, as before.

**Test.** Window visible beside a terminal, app not focused: commit from the
terminal → History/Changes update within ~10 s without touching LeoGit.
Minimize the window → Activity Monitor shows no periodic `git` spawns. ⌘Tab
back → refocus resync still fires once (unchanged).

---

## 6. Workstream D — OS connectivity signal (`NWPathMonitor`)

**Current.** `ConnectivityBreaker` is the Tauri breaker minus its
`navigator.onLine` half — documented at the time as having "no free AppKit
analogue." Network.framework's `NWPathMonitor` *is* the analogue.

**Design.**

- `ConnectivityBreaker` stays exactly as is: pure backoff math, no OS imports,
  unit-testable. (Single responsibility — don't grow it.)
- New `Services/NetworkPathObserver.swift`: wraps one `NWPathMonitor`, hops its
  queue callbacks to the main actor, and publishes `isOnline`
  (`path.status == .satisfied`). On the offline→online edge it invokes a
  registered `onRecover` callback.
- Composition happens where the breaker already lives
  (`RepoDirectoryStore.breaker`'s owner): background gates become
  `observer.isOnline && breaker.shouldAttempt` — mirroring the Tauri client's
  `shouldAttemptBackground()` shape exactly. `onRecover` does what the Tauri
  `initConnectivity` kick does: reset the breaker, one silent fetch + quiet
  refresh of the active repo, one tier-0 sweep. Recovery honors Workstream C's
  policy (an invisible window doesn't fetch just because Wi-Fi returned; it
  catches up on next visibility).
- While `isOnline == false`, fetching syncs degrade to local recomputes — the
  breaker's existing downgrade path, now reached without burning failures first.

**Files.** New `Services/NetworkPathObserver.swift`; edits in
`Services/ConnectivityBreaker.swift` (doc comment only — the deviation note
retires), `Stores/RepoDirectoryStore.swift`, `Screens/ContentView.swift`.

**Test.** Toggle Wi-Fi off: within a tick, no `git fetch` spawns (Console
logs); make a local commit → dirty/ahead badges still update. Toggle Wi-Fi on:
badges refresh within seconds, without waiting out a backoff window.

---

## 7. Workstream E — Diff settings: wire or explicitly exempt

**Current.** `SettingsStore` round-trips `hide_whitespace`,
`side_by_side_diff`, `wrap_long_lines`, `tab_size`, `syntax_highlighting`, and
`theme` through `config.toml` untouched; the native diff path reads none of
them. `get_diff_whitespace_ignored` exists in core
([`git.rs:910`](../../core/src/git.rs#L910)) but has no FFI export.

**Scope decision — wire the content settings, exempt the presentation ones:**

| Setting | Native fate | Why |
|---|---|---|
| `hide_whitespace` | **wire** | Content-level; core already implements it |
| `syntax_highlighting` | **wire** | Skipping phase two is one guard |
| `tab_size` | **wire** | One rendering constant in `DiffLineText` |
| `wrap_long_lines` | **wire** | Wrap vs. horizontal scroll in `DiffRowView` |
| `side_by_side_diff` | **exempt** (ROADMAP) | A layout feature, not a flag — needs its own design pass |
| `theme` | **exempt** (permanent) | Native follows system appearance; a web-only concept |

Exemptions get one sentence each in `SettingsStore`'s header comment and a
`FRONTEND.md` §8 row, so "silently ignored" becomes "documented divergence."

**Design.**

- New `Stores/AppConfigStore.swift` (`@MainActor @Observable`): the single
  native owner of the shared `Config`. Loaded at launch; `reload()` called on
  Settings save (via `flushPendingSave`/`scheduleSave` completion) and on the
  activation resync (picks up edits made from the Tauri client). The auto-fetch
  loop reads it instead of re-reading TOML every tick — the live re-arm
  behavior survives because Settings saves reload the store in the same
  process.
- FFI: export `get_diff_whitespace_ignored` in
  [`ffi/src/lib.rs`](../../apps/swift-ui-app/ffi/src/lib.rs) + a `GitBridge`
  wrapper — same thin-delegation pattern as `get_diff`.
- `DiffView`'s `LoadKey` gains `hideWhitespace: Bool`; toggling the setting
  re-keys the task and reloads through the seamless path (Workstream A), which
  is exactly the Tauri client's `lastHideWhitespace` effect, for free.
  `DiffStore.load` picks the raw-diff call by that flag and skips phase two
  when highlighting is off. `tab_size`/`wrap_long_lines` flow into
  `DiffLineText`/`DiffRowView` as plain parameters from the config store.
- Settings window gains a **Diff** section (four controls, same
  debounced-save plumbing as the existing sections).

**Files.** New `Stores/AppConfigStore.swift`; edits in `ffi/src/lib.rs`,
`IPC/GitBridge.swift`, `Stores/DiffStore.swift`, `Screens/DiffView.swift`,
`Design/DiffLineText.swift`, `Stores/SettingsStore.swift`,
`Screens/SettingsView.swift`, `Screens/ContentView.swift` (auto-fetch loop
reads the store).

**Test.** Toggle hide-whitespace with an indentation-only diff open → diff
empties/refills in place, no flash. Toggle highlighting off → colors drop,
plain text stays. Change the same settings from the Tauri client → native
picks them up on next activation.

---

## 8. Implementation order & dependencies

Per `CLAUDE.md`: one workstream at a time, in user-flow order, visually
verified before the next starts.

1. ✅ **A — seamless diff** (landed; biggest felt win, no dependencies).
2. ✅ **B — epoch semantics + stat stamp** (landed; B depends on A).
3. **C — scheduling policy + App Nap** (next; independent of A/B; lands the
   "works while unfocused" behavior the audit was about).
4. **D — connectivity observer** (composes with C's policy; do after C so
   recovery kicks respect visibility).
5. **E — diff settings + AppConfigStore** (uses A's reload path; touches the
   most files, so it goes last).

Each workstream ends with: `just mac-run` + the listed visual checks (ask for
confirmation, no screenshots), `cargo clippy --workspace` clean for any Rust
touched (E), and the doc updates below.

## 9. Documentation updates on completion

Done for A + B: `TECHNICAL.md` (DiffStore phase model + seamless mechanics;
`workingTreeEpoch` + stat-stamp contract), `FRONTEND.md` (§6.3 seamless rule
shared across clients; §5.2 `stat_stamp` field; §8 open-diff-freshness
divergence row), `ROADMAP.md` (token-cache motivation note; new item: Tauri
adopting `stat_stamp` to fix its own open-diff staleness).

Still owed by C–E:

- `TECHNICAL.md` — new Services types, `AppConfigStore` ownership, the
  scheduling-policy table.
- `FRONTEND.md` — §8: focus/visibility divergence row,
  `theme`/`side_by_side_diff` exemption rows.
- `ROADMAP.md` — check off "Settings re-arm intervals" (native half), add
  `side_by_side_diff` native support as an explicit item; keep the update
  checker / CLI-launch gaps (out of scope here) listed.
- `DESIGN.md` — the unfocused-but-visible freshness behavior is a product
  decision worth recording.

## 10. Findings log (pending items discovered en route)

- **Tauri open-diff staleness is now fixable for free** — `stat_stamp` reaches
  the Tauri client on every poll; adopting the native reload shape would
  retire its "stale until reselect" behavior. Filed in ROADMAP; §8's
  divergence row stands until then.
- **Equal-payload re-tokenize is the one remaining repeat cost** on an epoch
  bump (the token-cache ROADMAP item's note). Not measured hot; revisit only
  if it ever is.
