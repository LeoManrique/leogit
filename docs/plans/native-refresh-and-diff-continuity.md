# Plan — Background-refresh parity & diff continuity (native macOS app)

> Status: **complete — all five workstreams (A–E) landed and visually
> confirmed (2026-08-27).** Five fixes from the Swift-vs-Tauri background/refresh
> audit (2026-08-26), ordered by user flow, each visually verified before the
> next began. Every workstream is kept as a compact as-built record
> (decision history included). This file
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
2. **Everything pauses when the app isn't frontmost** *(fixed — C below)*.
   `backgroundPaused()` gated *all three* loops on `!NSApp.isActive`. The Tauri client never pauses
   on blur; GitHub Desktop pauses only the multi-repo indicator sweep and keeps
   the active repo's background fetcher alive. Worse, `NSApp.isActive` is false
   while the window is *visible but not key* — LeoGit sitting on half the screen
   next to an editor goes stale in plain sight, which the web-stack clients
   never do. And even a loop we *want* running unfocused would be throttled by
   App Nap unless we hold an activity assertion.

Plus three smaller gaps: no OS connectivity signal *(fixed — D below)*, the
diff-rendering settings in `config.toml` were silently ignored by the native
diff path *(fixed — E below: wired or documented-exempt, and `wrap_long_lines`
removed everywhere)*, and the update checker / CLI launch integration are
missing entirely (out of scope here — tracked in ROADMAP).

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

## 5. Workstream C — Background scheduling policy ✅ (landed 2026-08-27)

**As built** (new `Services/BackgroundSchedulingPolicy.swift` +
`Services/AppNapSuppressor.swift`; rewired `Screens/ContentView.swift`,
`Stores/RepoDirectoryStore.swift`, `Stores/SyncStore.swift`,
`Screens/RepoSwitcher.swift` — full mechanics in TECHNICAL.md): one
`@MainActor @Observable` policy owns the inputs (network-op slot, app
activation, repo-window occlusion, repo-open) and exposes named predicates —
`canPollStatus` / `canAutoFetch` (block only on the network op) and
`canRunRepoSweeps` — plus the cadences (`statusPollInterval` ladder,
`autoFetchInterval(configured:)` ×3 stretch). Every loop guard names its
predicate; the old `backgroundPaused()` closure is gone. `AppNapSuppressor`
holds `ProcessInfo.beginActivity(.background)` exactly while (repo open) ∧
(work allowed) — with the final table that is "whenever a repo is open and no
user transfer runs".

**Final policy** (amended in visual testing — see decisions):

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

**Decisions (kept from implementation):**

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
  resyncs immediately, as before. Visually confirmed: terminal commits
  appear in an unfocused window, and a hidden window is already current on
  return.

---

## 6. Workstream D — OS connectivity signal ✅ (landed 2026-08-27)

**As built** (new `Services/NetworkPathObserver.swift`; edits in
`Services/ConnectivityBreaker.swift`, `Stores/RepoDirectoryStore.swift`,
`Screens/ContentView.swift` — full mechanics in TECHNICAL.md): one
`NWPathMonitor` wrapped in a `@MainActor @Observable` observer publishing
`isOnline` (`path.status == .satisfied`, queue callbacks hopped to the main
actor); `RepoDirectoryStore.shouldAttemptBackground` composes
`isOnline && breaker.shouldAttempt` — the Tauri `shouldAttemptBackground()`
shape — and gates the tier `sync`, the auto-fetch loop, and the activation
resync. While offline, fetching syncs degrade to fetch-less local recomputes
without burning failures into the breaker first. The offline→online edge
fires `ContentView.resyncOnReconnect` (the Tauri `initConnectivity` kick):
breaker reset, silent fetch + quiet refresh of the active repo, throttled
tier-0 sweep. `ConnectivityBreaker`'s "no free AppKit analogue" deviation
note is retired; the breaker itself stays pure backoff math.

**Decisions (kept from implementation):**

- **`ConnectivityBreaker` grew one method after all:** the recovery kick
  needs to close the breaker, and `record(success: true)` at a site where
  nothing was fetched would fabricate a success report — so `reset()` says
  what it means ("stop waiting"), and `record`'s success path delegates to
  it. Everything else about the breaker is untouched.
- **The composed gate lives on `RepoDirectoryStore`:**
  `shouldAttemptBackground` (`networkObserver.isOnline &&
  breaker.shouldAttempt`) sits beside the breaker it composes, consumed by
  the tier `sync`, the auto-fetch loop, and the activation resync — the
  Tauri `shouldAttemptBackground()` shape, one owner.
- **Recovery honors the *amended* C policy, not the original wording:** the
  plan's "an invisible window doesn't fetch just because Wi-Fi returned"
  predates C's hidden-≠-paused amendment. As built, the kick runs unless a
  user transfer holds the slot (`canAutoFetch`) — a hidden window *does*
  catch up, by design — and the tier-0 sweep keeps its own
  `canRunRepoSweeps` gate plus the 30 s refocus throttle.
- **`onRecover` registers with the repository screen** (its unkeyed
  `.task`), reading the open repo at fire time; on Welcome nothing is
  registered because nothing needs catching up. `isOnline` starts `true` so
  the monitor's first real report can't fire a spurious launch kick.

Visually confirmed: Wi-Fi off → background fetches stop, local commits keep
updating badges; Wi-Fi on → badges refresh within seconds, no backoff wait.

---

## 7. Workstream E — Diff settings ✅ (landed 2026-08-27)

**As built** (new `Stores/AppConfigStore.swift`; edits in `ffi/src/lib.rs`,
`IPC/GitBridge.swift`, `Stores/DiffStore.swift`, `Screens/DiffView.swift`,
`Design/DiffLineText.swift`, `Stores/SettingsStore.swift`,
`Screens/SettingsView.swift`, `Screens/ContentView.swift`, plus — for the
wrap removal — `core/src/config.rs`, the Tauri `DiffViewer`/`MainLayout`/
`SettingsOverlay`/`commands.ts`; full mechanics in TECHNICAL.md):
`AppConfigStore` is the single native `Config` owner (created in `LeoGitApp`,
in both scenes' environment; reloaded at launch, on every successful Settings
save, and on the activation resync — the Tauri-edit path). The auto-fetch
loop reads it per tick instead of the TOML file. `get_diff_whitespace_ignored`
crossed the FFI (`GitBridge.rawDiffIgnoringWhitespace`); `DiffStore.load`
gained `hideWhitespace` (working-tree targets only) and `highlight` (off
skips phase two and drops tokens); both sit in `DiffView.LoadKey`, so
toggles reload through A's seamless path. `tab_size` renders via tab
expansion in `DiffLineText`. The Settings window gained a **Diff** section
(hide whitespace / syntax highlighting / tab size 1–16, Tauri's labels).

**Final scope table** (amended in visual testing — see decisions):

| Setting | Fate | How |
|---|---|---|
| `hide_whitespace` | **wired** | picks `git diff -w` for working-tree loads; re-keys the open diff |
| `syntax_highlighting` | **wired** | in `LoadKey` too; off skips tokenize and drops colors live |
| `tab_size` | **wired** | tab expansion in `DiffLineText` (see decisions) |
| `wrap_long_lines` | **removed everywhere** | long lines always wrap — both clients, core schema too |
| `side_by_side_diff` | exempt (ROADMAP) | a layout feature needing its own design pass |
| `theme` | exempt (permanent) | native follows system appearance |

Exemptions are documented in `SettingsStore`'s header and FRONTEND.md §8.

**Decisions (kept from implementation):**

- **`syntax_highlighting` joined `LoadKey`** beyond the planned guard: the
  toggle must act on the *open* diff, and re-keying through the seamless
  path costs one subprocess with the equality skip absorbing the repaint —
  the Tauri client re-renders without refetching, a mechanism difference
  with identical behavior (plain text stays, colors drop/return live).
- **`tab_size` is baked into the string, not an attribute:** doc check
  confirmed SwiftUI `Text` renders no paragraph-style attributes (only the
  SwiftUI scope + a subset of Foundation intents), so tab stops can't be
  set the AppKit way. `DiffLineText` expands tabs to spaces with CSS
  `tab-size` stop math and remaps every token/intra range; a no-tab line
  pays one `contains` scan. Known cost: copying a tabbed line yields
  spaces where the WebView preserves tabs.
- **Amendment after the first visual pass (user decision):
  `wrap_long_lines` removed everywhere, not wired.** The as-planned no-wrap
  mode (two-axis ScrollView + per-row min-width) broke the lazy layout, and
  GitHub Desktop — checked in `lms-github-desktop` — offers no wrap setting
  at all: diff content is permanently `pre-wrap`; its tab size is one CSS
  property. Chosen: always wrap, delete the setting from the native app,
  the Tauri app, and the core schema. The Tauri `DiffViewer` lost its
  fixed-height virtualization with it (only the no-wrap mode used it — the
  default wrap mode already rendered whole). Old config files still parse:
  the key is an ignored unknown, pinned by `config_ignores_retired_keys`
  next to `show_pull_requests`.
- **`AppConfigStore` reaches `SettingsStore` by property injection**
  (`SettingsView`'s `.task` sets `store.configStore` — environment values
  can't reach a store's init); `save()` calls `reload()` after the write
  lands, which is the whole live-propagation story.
- **Tab size only shows on tab-indented files** (Go, Makefiles) — space-
  indented files render identically at any width; noted here because the
  first visual pass read that as "setting does nothing".

Visually confirmed: hide-whitespace empties/refills an indentation-only
diff in place; highlighting off drops colors with plain text staying;
Settings edits apply to the open diff on save, Tauri-side edits on
activation; both clients wrap long lines identically. Verified: zero-warning
`xcodebuild`, `svelte-check` clean (129 files), 120 core + 24 bridge tests,
clippy-pedantic baseline unchanged (184).

---

## 8. Implementation order & dependencies

Per `CLAUDE.md`: one workstream at a time, in user-flow order, visually
verified before the next starts.

1. ✅ **A — seamless diff** (landed; biggest felt win, no dependencies).
2. ✅ **B — epoch semantics + stat stamp** (landed; B depends on A).
3. ✅ **C — scheduling policy + App Nap** (landed; the "works while
   unfocused" behavior the audit was about, amended to "never stops").
4. ✅ **D — connectivity observer** (landed; composes with C's policy —
   recovery kicks run under its predicates).
5. ✅ **E — diff settings + AppConfigStore** (landed; used A's reload path,
   and grew cross-client scope with the `wrap_long_lines` removal).

Each workstream ended with: `just mac-run` + the listed visual checks (ask
for confirmation, no screenshots), `cargo clippy --workspace` clean for any
Rust touched (B, E), and the doc updates below.

## 9. Documentation updates on completion

Done for A + B: `TECHNICAL.md` (DiffStore phase model + seamless mechanics;
`workingTreeEpoch` + stat-stamp contract), `FRONTEND.md` (§6.3 seamless rule
shared across clients; §5.2 `stat_stamp` field; §8 open-diff-freshness
divergence row), `ROADMAP.md` (token-cache motivation note; new item: Tauri
adopting `stat_stamp` to fix its own open-diff staleness).

Done for C: `TECHNICAL.md` (policy + suppressor mechanics folded into the
refresh-machinery paragraph), `FRONTEND.md` (§6.1 hidden/blurred behavior
made platform policy; §8 background-cadence divergence row), `DESIGN.md`
(flow 7: the freshness-while-unfocused product decision). Nothing in
`ROADMAP.md` — no existing item covered it.

Done for D: `TECHNICAL.md` (observer + composed gate + recovery kick folded
into the same paragraph), `ROADMAP.md` (the sixth-chunk entry's "no OS
online/offline signal" deviation marked *since superseded*). `FRONTEND.md` /
`DESIGN.md` needed nothing — D restores §6.5 parity rather than diverging,
and DESIGN's offline bullet already described the recovered behavior.

Done for E: `TECHNICAL.md` (`AppConfigStore` ownership + reload sites in the
Settings paragraph; the diff-settings wiring, tab expansion, and always-wrap
folded into the diff paragraph; the Config mirror sentence updated to 15
fields), `FRONTEND.md` (§5 Config row minus `wrap_long_lines`; §8 `theme`
exemption folded into the Theme row + a new `side_by_side_diff` row),
`ROADMAP.md` (re-arm item's native parenthetical now names `AppConfigStore`;
new native side-by-side item; eighth-chunk entry's "diff toggles cross
untouched" marked *since superseded*; update checker / CLI-launch gaps stay
listed), `DESIGN.md` (native Settings sentence gains the Diff section and
the two exemptions; the Tauri diff-viewer bullet records always-wrap).

## 10. Findings log (pending items discovered en route)

- **Tauri open-diff staleness is now fixable for free** — `stat_stamp` reaches
  the Tauri client on every poll; adopting the native reload shape would
  retire its "stale until reselect" behavior. Filed in ROADMAP; §8's
  divergence row stands until then.
- **Equal-payload re-tokenize is the one remaining repeat cost** on an epoch
  bump (the token-cache ROADMAP item's note). Not measured hot; revisit only
  if it ever is.
