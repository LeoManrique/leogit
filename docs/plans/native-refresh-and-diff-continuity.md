# Plan — Background-refresh parity & diff continuity (native macOS app)

> Status: **planning**. Nothing here is implemented yet. This lays out the target
> shape for the five fixes identified in the Swift-vs-Tauri background/refresh
> audit (2026-08-26), ordered by user flow, each testable on its own before the
> next begins. Companion contract: [`FRONTEND.md`](../../FRONTEND.md) §6
> (behavioral contract) and §8 (intentional divergences) — both get updated as
> each workstream lands.

## 1. Motivation

The audit found the native client is *architecturally* faithful to the Tauri
client's refresh machinery (same tiers, same breaker numbers, same poll cadence)
but *feels* different for two reasons:

1. **Every diff reload blanks the pane.** `DiffStore.load` clears
   `payload/rows/tokens` before fetching, and `statusEpoch` re-keys the load on
   every status change and every app activation — so refocusing the app, or any
   background tick that changes status, flashes the open diff to a spinner and
   resets its scroll. The Tauri client keeps the old diff on screen and only
   falls back to a spinner after 150 ms (`SLOW_DIFF_THRESHOLD_MS`, ported from
   GitHub Desktop's `SeamlessDiffSwitcher`).
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

## 3. Workstream A — Seamless diff switching (kill the blank)

**Current.** [`DiffStore.load`](../../apps/swift-ui-app/Sources/LeoGit/Stores/DiffStore.swift)
opens with `payload = nil; rows = []; tokens = nil`, so every re-key of
`DiffView`'s `.task(id: LoadKey)` drops the pane to `ProgressView`, loses scroll
position, and re-parses + re-tokenizes even when the result is identical.

**Target.** The Tauri/GH-Desktop contract, natively:

- The previous diff **stays on screen** while the replacement loads.
- A **150 ms slow-load threshold**: only if the load outlives it does the pane
  fall back to a spinner (constant shared in name and value with the Tauri
  client's `SLOW_DIFF_THRESHOLD_MS`).
- **Value-equality skip**: the generated `DiffPayload`/`FileDiff` are already
  `Equatable` — if the fresh parse equals what's shown, publish nothing (rows,
  scroll, and tokens all survive untouched). This is what makes epoch bumps
  (Workstream B) harmless.
- When the payload *did* change, swap `rows` in place. Rows are `Identifiable`
  by flat index, so SwiftUI diffs the list instead of rebuilding it; tokens
  reset to `nil` and the plain-text phase shows immediately — that two-phase
  paint is already the contract (`FRONTEND.md` §7).

**Design.**

- `DiffStore` gains an explicit tiny state model instead of four loose flags:
  the published surface becomes `payload/rows/tokens` (what's on screen) plus
  `phase: Phase` where `Phase` is `idle / loading(slow: Bool) / failed(String)`.
  The view's rule collapses to: show content whenever `payload != nil`, spinner
  only when `payload == nil || phase == .loading(slow: true)`.
- The slow-threshold timer is a child `Task.sleep(for: .milliseconds(150))`
  guarded by the existing `generation` counter (the blocking FFI call can't be
  cancelled, so generation stays the correctness mechanism; the timer is just
  another racer against it).
- Re-tokenization: when the new payload equals the old, still refresh `tokens`
  in the background and swap them in place (recolor without flash) — context
  lines can change color when surrounding blob content changed even though the
  diff text didn't. No debounce timer needed: unlike the Svelte effect, `load`
  only runs on a real key change, and the equality skip absorbs the churny
  callers. (This also settles ROADMAP's deferred "token cache" item for now:
  the skip guard removes the repeat-tokenize cost that motivated it.)

**Files.** `Stores/DiffStore.swift`, `Screens/DiffView.swift`. No FFI changes.

**Test (visual, per CLAUDE.md).** Open a large diff, scroll mid-way, ⌘Tab away
and back → no flash, scroll preserved. Switch between two files → old diff
visible until the new one paints. Throttle with a huge diff → spinner appears
only after the threshold.

---

## 4. Workstream B — `statusEpoch` semantics (reload without guessing)

**Current.** `RepoStore` bumps `statusEpoch` on any status delta, on every
explicit `refresh()`, and unconditionally on refocus — each bump re-keys the
open `DiffView`.

**Analysis.** Narrowing the bump *at the RepoStore level* is a dead end:
`git status` cannot tell whether the *selected file's diff content* changed
when its row looks identical (modified → still modified). Any row-comparison
heuristic would reintroduce the Tauri client's staleness bug (its poll never
reloads the open diff, so edits made through the embedded terminal go stale
until reselect). The native client's "reload whenever status moved" is the
*more correct* behavior — it was only expensive because reloads blanked.

**Target.** Keep the epoch honest and cheap instead of clever:

- Rename to `workingTreeEpoch` and document its one meaning: *"the working
  tree may differ from what any derived view shows — re-derive if you care."*
  Bump when status changed, on explicit refresh, and on refocus
  (`forceDiffReload`), exactly as today.
- Correctness of "did anything actually change" lives **only** in
  `DiffStore`'s equality skip (Workstream A). RepoStore signals possibility;
  DiffStore verifies reality. Single responsibility, no duplicated guessing.
- The only real waste left is a `git diff` + parse per epoch bump for an
  unchanged file. That is one short-lived subprocess on a 2 s-poll *status
  change* (not per tick) — accepted, and cheap to revisit later with a
  blob-OID cache (ROADMAP item) if it ever measures hot.

**Files.** `Stores/RepoStore.swift` (rename + doc comment),
`Screens/ChangesDetailPane.swift`, `Stores/DiffStore.swift` (comment linking the
two halves of the contract).

**Test.** With a file's diff open, `touch`/edit a *different* file in a
terminal → open diff does not repaint (equality skip); edit the *open* file →
diff updates in place within a poll tick, no flash.

---

## 5. Workstream C — Background scheduling policy (focus, visibility, App Nap)

**Current.** One predicate (`activeOperation != nil || !NSApp.isActive`) pauses
the status poll, auto-fetch, tier scheduler, and sweeps alike.

**Target policy** (the GH-Desktop split, plus a native improvement):

| Work | Pauses on network op | Pauses when app inactive | Pauses when window not visible |
|---|---|---|---|
| 2 s status poll (active repo, local) | yes | **no** — slows to 10 s | yes |
| Auto-fetch loop (active repo, network) | yes | no | yes |
| Tier scheduler + sweeps (other repos) | yes | **yes** (GH Desktop model) | yes |

Rationale: a visible-but-not-key window keeps telling the truth (the audit's
"stale in plain sight" case — the web clients never had this failure mode
because DOM timers don't know about key windows); the multi-repo fetch fan-out
is the only genuinely deferrable work, and the existing refocus resync remains
its catch-up path. The slower unfocused poll cadence keeps the visible window
honest without running full-rate forever.

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
into the policy).

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

1. **A — seamless diff** (biggest felt win; no dependencies).
2. **B — epoch semantics** (tiny once A's equality skip exists; A depends on
   nothing, B depends on A).
3. **C — scheduling policy + App Nap** (independent of A/B; lands the
   "works while unfocused" behavior the audit was about).
4. **D — connectivity observer** (composes with C's policy; do after C so
   recovery kicks respect visibility).
5. **E — diff settings + AppConfigStore** (uses A's reload path; touches the
   most files, so it goes last).

Each workstream ends with: `just mac-run` + the listed visual checks (ask for
confirmation, no screenshots), `cargo clippy --workspace` clean for any Rust
touched (E), and the doc updates below.

## 9. Documentation updates on completion

- `TECHNICAL.md` — new Services types, `AppConfigStore` ownership,
  `DiffStore` phase model, the scheduling-policy table.
- `FRONTEND.md` — §6: seamless-diff threshold becomes a shared behavioral
  rule; §8: focus/visibility divergence row, `theme`/`side_by_side_diff`
  exemption rows.
- `ROADMAP.md` — check off "Settings re-arm intervals" (native half), note the
  token-cache item's motivation change (A's equality skip), add
  `side_by_side_diff` native support as an explicit item; keep the update
  checker / CLI-launch gaps (out of scope here) listed.
- `DESIGN.md` — the unfocused-but-visible freshness behavior is a product
  decision worth recording.
