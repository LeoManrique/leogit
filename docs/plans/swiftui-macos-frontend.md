# Plan — Native SwiftUI frontend for macOS (Tauri stays for Windows/Linux)

> Status: **planning**. This document is a migration plan, not an executed change.
> It contains no committed decisions — it lays out the target shape, the mechanical
> steps that are decision-independent, and the branching choices with enough detail
> that each can be decided. Companion contract: [`FRONTEND.md`](../../FRONTEND.md).

## 1. Goal & scope

Add a **native SwiftUI app for macOS** that reuses LeoGit's existing Rust logic,
while the **current Tauri + Svelte app keeps serving Windows and Linux**. This is
the same dual-frontend shape Leo already ships in two other projects:

- `git-projects-manager` — SwiftUI (macOS) + Tauri (Win/Linux), one Rust core,
  bridged with **UniFFI** (static-linked).
- `leosync-src` — SwiftUI (macOS) + Tauri (Win/Linux), one Rust core, bridged with
  a **local daemon over HTTP+JSON+SSE**.

We are **not** removing the Tauri client. The end state is: **one shared Rust
core**, **two thin frontends** (Svelte-in-Tauri, SwiftUI), one behavioral contract
(`FRONTEND.md`) both frontends conform to.

Non-goals: iOS/iPadOS, changing git behavior, replacing `git`-CLI with a library,
rewriting the Svelte app.

## 2. Where we are today (migrate-from)

LeoGit is already a **thin-frontend / fat-Rust-backend** app, which is why this is
feasible. Verified facts (see [`TECHNICAL.md`](../../TECHNICAL.md) for the live
architecture):

| Layer | Size | Notes |
|---|---|---|
| Rust backend (`tauri-app/src-tauri/src`) | ~9,960 LOC, 18 `.rs` files | All git/diff/highlight/GitHub/AI/terminal logic |
| Svelte frontend (`tauri-app/src`) | ~11,900 LOC | ~36% CSS; holds **no** git algorithms |
| IPC contract | **69 commands + 4 events + ~30 DTOs** | One choke point: `tauri-app/src/lib/api/commands.ts` |

Backend characteristics that make it portable:

- **Git is done by shelling out to the `git` CLI** (`git.rs`), not `git2`/`gix`.
  GitHub is the `gh` CLI (`gh.rs`); AI is the `claude` CLI + `reqwest` (`ai.rs`);
  updates are `reqwest` (`update.rs`); terminal is `portable-pty` (`terminal.rs`);
  highlighting is `syntect`/`two-face` (`highlight.rs`).
- **Almost no Tauri coupling.** `0` commands use `tauri::State`/`Window`/managed
  state (global state is plain `std::sync` statics). Only **5** functions touch
  Tauri at all, and only to emit events (`AppHandle`): `git::pull`, `git::push`,
  `git::clone_repo`, the private `progress_forwarder`, and `terminal::start_terminal`.
- **2 Tauri plugins**: `tauri-plugin-dialog` (native file picker) and
  `tauri-plugin-single-instance` (forwards a 2nd `leogit <dir>` launch).
- **4 backend→frontend events** (nothing flows frontend→backend as an event; that
  direction is 100% request/response `invoke`):
  - `git-progress` — `{op, path, percent, text}` during push/pull/clone
    (`git.rs:378`, const `GIT_PROGRESS_EVENT`).
  - `terminal-output-<pid>` — raw PTY bytes (`terminal.rs:267`), **dynamic per-PID**.
  - `terminal-closed-<pid>` — child exit (`terminal.rs:284`), **dynamic per-PID**.
  - `open-repo` — launch/second-instance target (`launch.rs:114`, `OPEN_REPO_EVENT`).

One backend output is **web-shaped**, and the terminal needs a native widget —
together the main non-mechanical porting work (see §7):

- `diff::parse_diff` returns `ParsedDiff.html: string[]` and
  `highlight::highlight_diff` returns **HTML** span strings, both built by `render.rs`
  (syntect tokens → CSS classes) for `{@html}`. SwiftUI can't render that — but the
  HTML is only a leaf: `render.rs` is a pure *structured → HTML* collapse over the
  `Token`/`TokenClass` model (today `pub(crate)`) plus each line's intra-line range,
  so exposing that already-present structured layer is most of the fix (§7.1).
- The **terminal** UI is `xterm.js`; there is no SwiftUI equivalent widget. Its
  *payload* is not web-shaped, though — `terminal-output-<pid>` is raw ANSI PTY bytes,
  which a native emulator (SwiftTerm) consumes the same way (§7.2).

## 3. Target architecture

Three layers. The first (a shared, Tauri-free core) is required by **every** option
and is where most of the de-risking happens. The bridge layer is the one decision.

```
                         ┌───────────────────────────────┐
                         │   leogit-core   (Rust crate)   │
                         │   Tauri-free. All logic:       │
                         │   git/diff/highlight/gh/ai/    │
                         │   terminal/config/os/update    │
                         │   + EventSink trait (§5)       │
                         └───────────────┬───────────────┘
                    path dep             │            path dep / link / spawn
        ┌────────────────────────────────┼────────────────────────────────┐
        ▼                                                                  ▼
┌───────────────────────┐                                   ┌──────────────────────────┐
│ tauri-app/src-tauri    │  (Windows/Linux)                 │ macos-app  (macOS)        │
│ thin #[tauri::command] │                                   │ SwiftUI + bridge (§6)     │
│ shims → core           │                                   │ Option A: UniFFI link     │
│ Svelte frontend        │                                   │ Option B: HTTP+SSE daemon │
└───────────────────────┘                                   └──────────────────────────┘
        └──────────────────────────  FRONTEND.md  ──────────────────────────┘
                       (one behavioral contract both conform to)
```

## 4. Step 1 — Extract `leogit-core` (decision-independent, do this first)

This is a **mechanical refactor with no behavior change**: the Tauri app keeps
working exactly as today, and it de-risks whichever bridge is chosen. The backend
agent confirmed this is "strip attributes + expose a surface," not a rewrite.

### 4.1 Turn the single crate into core + host

Current: one crate `leogit` (lib `leogit_lib` + bin `leogit`) under
`tauri-app/src-tauri`, with all logic in `src/commands/*.rs`.

Target: a new **`core/`** crate (`leogit-core`, lib `leogit_core`) that owns all of
`commands/*.rs` **minus** the Tauri glue, and the Tauri host reduced to
`main.rs` + thin `#[tauri::command]` shims that call core.

Modules that move to `leogit-core` unchanged (already Tauri-free): `git`, `diff`,
`highlight`, `progress`, `paths`, `config`, `gh`, `ai`, `shell`, `os`, `update`,
plus `process` (drop its one `tauri::async_runtime` use → `tokio`), `terminal` and
`launch` (rework their event emits — §5). `render.rs` is the one presentation-specific
module — a pure structured→HTML collapse — so `parse_diff`/`highlight_diff` in core
return the structured layer (§7.1) and `render.rs` moves to the Tauri host. Rename the
`commands` module to something transport-neutral (e.g. `api` or keep `commands`, but
they are no longer Tauri commands — they are core functions returning `Result<T, String>`).

> **Decision (open):** Cargo **workspace** vs **independent crates joined by path
> deps**. `leosync-src` uses a workspace (`resolver="3"`, shared `target/`,
> `version.workspace = true`); `git-projects-manager` uses **no** workspace (each
> crate independent, path deps only). Either works; the workspace gives single-sourced
> version + one `target/`, the non-workspace keeps crates fully independent.

### 4.2 The event seam — `EventSink`

The only real Tauri entanglement is 4 event emissions. Replace the direct
`app.emit(...)` calls with a transport-agnostic sink the core owns, so each frontend
plugs in its own delivery:

```rust
// leogit-core: events.rs
#[derive(Clone, serde::Serialize)]
pub enum CoreEvent {
    GitProgress(GitProgressPayload),           // was "git-progress"
    TerminalOutput { pid: u32, data: Vec<u8> },// was "terminal-output-<pid>"
    TerminalClosed { pid: u32 },               // was "terminal-closed-<pid>"
    OpenRepo(LaunchTarget),                     // was "open-repo"
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: CoreEvent);
}
```

- `git::pull/push/clone_repo` and `progress_forwarder`: replace the `AppHandle`
  param with `&dyn EventSink` (or keep today's `impl FnMut(&str)` progress callback
  and let each host adapt it). The streaming machinery in `process.rs`
  (`run_timed_streaming`, reader threads, `\r`/`\n` splitting) is unchanged.
- `terminal::start_terminal`: takes an `Arc<dyn EventSink>`; its per-PID reader
  thread calls `sink.emit(CoreEvent::TerminalOutput{pid, data})` instead of
  `app.emit(format!("terminal-output-{pid}"), …)`.
- `launch`: `set_pending_launch_target` / second-instance forwarding stay; the
  emit becomes `sink.emit(CoreEvent::OpenRepo(target))`.

Each host implements `EventSink` once:
- **Tauri host** → `app.emit(event_name, payload)` (preserves today's exact wire).
- **UniFFI (Option A)** → a `#[uniffi::export(callback_interface)]` forwarded to Swift.
- **Daemon (Option B)** → push onto the SSE `EventBus`.

### 4.3 Verification for Step 1

`just check` (svelte-check + `cargo check`), `just build`, run the Tauri app — it
must behave identically. No frontend change. This is the safe checkpoint before any
Swift work.

## 5. Step 2 — Write `FRONTEND.md` (the contract)

Before building the second frontend, freeze the contract both must honor. Full
skeleton is in [`FRONTEND.md`](../../FRONTEND.md); it enumerates the 69 commands,
4 events, ~30 DTOs, and the behavioral rules (poll cadences, visibility gating,
selection semantics, diff-load races) that must be identical across frontends. It is
written **bridge-agnostic** (logical operations, not HTTP routes or UniFFI method
names) so it holds under either Option A or B.

## 6. Step 3 — The bridge (THE decision) — two proven options

Both are patterns Leo already ships. Neither is chosen here. LeoGit's specifics
(streaming git-progress, a stateful per-PID PTY, HTML-shaped diff payloads) load
onto the two options differently — that's the crux of the decision.

### Option A — UniFFI, static-linked (the `git-projects-manager` pattern)

A second thin crate `macos-app/ffi/` (`leogit-ffi`, `crate-type=["staticlib","lib"]`)
depends on `leogit-core`, mirrors DTOs as `#[derive(uniffi::Record/Enum)]` with
`From<core::T>` impls, and exposes a `#[derive(uniffi::Object)]` with
`#[uniffi::export]` methods (`#[uniffi::export(async_runtime="tokio")]` for the
network ops → Swift `async throws`). Events cross via a
`#[uniffi::export(callback_interface)]` `EventSink` the Swift app implements.

- **Bridge crate**: `uniffi = { version = "0.32", features = ["cli","tokio"] }`,
  `uniffi::setup_scaffolding!()`, plus a `[[bin]] uniffi-bindgen` that calls
  `uniffi::uniffi_bindgen_main()`.
- **Build** (`macos-app/scripts/build-rust.sh`, run as an Xcode pre-build phase):
  ```bash
  cargo build [--release]                              # → libleogit_ffi.a
  cargo run --bin uniffi-bindgen -- generate \
      --library target/<profile>/libleogit_ffi.a \
      --language swift --out-dir ../generated          # → leogit_ffi.swift + .h + .modulemap
  mv -f ../generated/leogit_ffiFFI.modulemap ../generated/module.modulemap
  ```
- **Xcode**: **XcodeGen** (`project.yml`, `.xcodeproj` gitignored). Add
  `generated/leogit_ffi.swift` as a source, `SWIFT_INCLUDE_PATHS=$(SRCROOT)/generated`,
  `OTHER_LDFLAGS=-lleogit_ffi -lz -liconv`, `LIBRARY_SEARCH_PATHS=…/ffi/target/<profile>`,
  `SWIFT_DEFAULT_ACTOR_ISOLATION=nonisolated` (UniFFI + Swift 6, uniffi-rs #2818),
  `ONLY_ACTIVE_ARCH=YES` (host-arch only; no `lipo`/xcframework).
- **Reference versions** (from `git-projects-manager`): uniffi 0.32, Rust edition
  2024, macOS deploy 26.0, Swift 6.0, generated dir + `.xcodeproj` gitignored,
  pre-build script `basedOnDependencyAnalysis: false`.

Consequences for LeoGit:
- ✅ No socket, no listening port, no daemon lifecycle; one process.
- ✅ Type-checked FFI; `async` maps cleanly.
- ⚠️ **Events need a callback interface.** `git-projects-manager` has *no* events, so
  this is net-new versus that reference: LeoGit's 4 events (esp. high-frequency
  `terminal-output-<pid>` byte streams) must go through a UniFFI callback interface.
  Feasible, but the terminal byte-stream throughput over a callback needs a look.
- ⚠️ **A second API contract to keep in lockstep** (the UniFFI method set + Record
  twins), on top of the Tauri command surface. This is the "drift tax"
  `leosync-src`'s `IOS_SUPPORT.md` calls out.
- ⚠️ Static lib bakes in native deps → extra link flags; per-arch builds.

### Option B — Local daemon, HTTP+JSON+SSE (the `leosync-src` pattern)

Compile `leogit-core` into a standalone binary `leogitd` (a new
`daemon/leogitd` bin crate, `leogit-core` its only local dep) that serves an
`axum` router over a **Unix domain socket** (macOS/Linux) / **named pipe**
(Windows). Both GUIs become thin HTTP clients; the 4 events become an **SSE** stream
(`GET /api/events`) with monotonic IDs + `stream:resync` on gap.

- **Swift client**: `AsyncHTTPClient` (swift-server/NIO) — `URLSession` can't dial a
  UDS, but AsyncHTTPClient supports the `http+unix` scheme and streaming bodies.
  ~1 `IPCClient` class with per-route async wrappers; `JSONDecoder`
  `.convertFromSnakeCase`. SwiftPM package (`Package.swift`), **no Xcode project**.
- **Tauri host** also becomes an HTTP client to the same `leogitd` (in `leosync-src`
  the Tauri host does **not** link the core — it proxies to the daemon via `hyper` +
  `hyperlocal`/named-pipe, and re-emits SSE onto the Tauri event bus). This keeps a
  **single** wire contract for both frontends.
  - > **Sub-decision (open):** for LeoGit's Tauri side you could instead keep the
    > current in-process `#[tauri::command]` → `leogit-core` path (no daemon on
    > Win/Linux) and run the daemon only for macOS. That is simpler for Tauri but
    > creates *two* backend shapes; `leosync-src` deliberately chose one shape.
- **Build/package** (`leosync-src` `install_macos.sh`): `cargo build -p leogitd`
  and `swift build -c release`, then assemble `LeoGit.app` copying **`leogitd`
  side-by-side** with the GUI in `Contents/MacOS/`, ad-hoc codesign the nested
  daemon first then the bundle. GUI locates the daemon next to its own executable
  then on `PATH`, spawns it detached (`leogitd --socket <path>`), polls
  `GET /api/ping` for readiness, and does a version handshake to kill stale daemons.

Consequences for LeoGit:
- ✅ **Streaming is native** — SSE is a natural fit for `git-progress` and (framed
  per-PID) `terminal-output`; the reliability convention (2s reconnect,
  `Last-Event-ID`, resync-on-gap, durable recent-events ring) is already worked out.
- ✅ **One wire contract** for both frontends → no second type surface to lockstep;
  FRONTEND.md *is* the HTTP contract.
- ✅ Daemon can outlive the window (matters if background fetch should continue).
- ⚠️ Ships a background process + socket lifecycle (spawn/discovery/readiness/version
  handshake/stale-daemon cleanup) — more moving parts than a linked lib.
- ⚠️ The stateful PTY sessions live in the daemon; per-PID output must be framed on
  one SSE stream (or a pid field), not the dynamic per-PID channel names used today.

### Option C — In-process axum router (noted, not detailed)

`leosync-src`'s `IOS_SUPPORT.md` documents a variant: link the core statically and
call the **same axum `Router` as an in-process `tower::Service`**
(`ipc_call(method, path, body) -> (status, bytes)` + one event callback), **no
socket/port**. Only the transport chokepoint changes vs. Option B, reusing the same
route/DTO contract. Worth considering if the daemon's socket lifecycle is unwanted
but the single-wire-contract benefit is.

### 6.1 Decision matrix (LeoGit-weighted)

| Axis | A: UniFFI link | B: Daemon HTTP+SSE | C: In-proc router |
|---|---|---|---|
| Streaming (git-progress) | callback interface (net-new) | SSE (native fit) | callback |
| Terminal PTY byte stream | callback throughput TBD | SSE frame per-PID | callback |
| # of contract surfaces to lockstep | 2 (UniFFI + Tauri) | **1** (the wire) | 1 (routes) |
| Extra process / socket | none | daemon + socket | none |
| Background work outlives window | no | yes | no |
| Build tooling | XcodeGen + uniffi-bindgen | SwiftPM + app-bundle script | SwiftPM + bindgen-ish |
| Precedent in Leo's repos | `git-projects-manager` | `leosync-src` | (documented only) |
| macOS packaging | linked staticlib | side-by-side daemon | linked staticlib |

## 7. LeoGit-specific porting work (independent of bridge)

These exist regardless of A/B/C and should be scoped explicitly:

1. **Diff rendering: expose the structured layer under the HTML.** The structured
   model already exists internally — `highlight.rs`'s `Token { start, end, class:
   TokenClass }` (today `pub(crate)`) plus each `DiffLine`'s `intra_line_diff:
   IntraLineRange`; `render.rs` is a pure *structured → HTML* collapse over it and
   `css_class()` is the only web-specific step. So core returns that layer
   (`parse_diff` → `FileDiff` + `sbs_pairs` + counts, no `html`; `highlight_diff` →
   `Vec<TokenLine>`, with `Token`/`TokenClass` made `pub`), and each frontend renders
   it: SwiftUI maps `TokenClass` → colour/traits (the mirror of `css_class`) into an
   `AttributedString`. That per-platform style map is correct divergence, not
   duplication.
   - > **Decision (open):** where the HTML collapse lives for Tauri. Keep `render.rs`
     > on the **Tauri host** so the Svelte side gets the exact HTML it does today (zero
     > Svelte churn, and the WebView main thread stays out of span-building — the
     > reason `render.rs` exists, `highlight.rs:83`) — **or** render in Svelte so the
     > wire is structured-only for both. Either way the *wire* is structured; only
     > Tauri's side of it changes.
2. **Terminal widget.** No SwiftUI equivalent to `xterm.js`; use **SwiftTerm** (or
   similar). Backend PTY (`start/write/resize/close_terminal`) is reused as-is; only
   the emulator widget + the per-PID output channel wiring is new.
3. **Re-port ~1,500–2,000 LOC of frontend orchestration** (framework-agnostic logic,
   currently in `MainLayout.svelte` + `lib/services/` + `lib/stores/`): the 2s status
   poll + 30s auto-fetch + HEAD-SHA poll with **visibility/blur gating**; diff-load
   **stale-response race guarding** + 150ms slow-load threshold; file-selection
   **set-math** (`selectedFiles`/`userDeselected` surviving polls); the connectivity
   **circuit-breaker** (`connectivity.ts`); the **tiered sync scheduler**
   (`repoSyncScheduler.ts`); AI/amend/undo flows. None of this is git logic; it maps
   to `@Observable` stores + `.task`/scene-phase in SwiftUI.
4. **Native replacements for Tauri plugin JS APIs**: folder picker
   (`plugin-dialog.open` → `NSOpenPanel`/`.fileImporter`), `path.homeDir`/`join`
   (→ `FileManager`), second-instance/open-repo (→ AppKit app-activation / URL open).
   The custom `os::{reveal_path,open_path,open_url}` commands stay in core.
5. **Theme + layout**: `app.css` design tokens (dark/light) → SwiftUI `Color` assets;
   the 3 `localStorage` pane-width keys → `UserDefaults`; hand-rolled virtualization
   → native `List`/`LazyVStack`/`Table` (throwaway).

## 8. Proposed repo layout (after migration)

```
leogit/
├── Cargo.toml                 # workspace root — OR omit (path-dep style). DECISION §4.1
├── core/                      # leogit-core (Tauri-free)  ← extracted in Step 1
│   ├── Cargo.toml
│   └── src/{git,diff,highlight,progress,paths,config,gh,ai,shell,os,
│            update,terminal,launch,events}.rs   # render.rs → Tauri host (§7.1)
├── tauri-app/                 # unchanged UX; host now depends on ../core
│   ├── src/                   # Svelte frontend (unchanged)
│   └── src-tauri/             # thin #[tauri::command] shims → leogit-core
│                              # (Option B: OR an HTTP proxy to leogitd)
├── macos-app/                 # NEW SwiftUI app (macOS)
│   ├── (Option A) ffi/        #   leogit-ffi UniFFI crate + generated/ (gitignored)
│   ├── (Option A) project.yml #   XcodeGen; .xcodeproj gitignored
│   ├── (Option B) Package.swift  # SwiftPM; AsyncHTTPClient client
│   ├── scripts/build-rust.sh  #   (A) cargo+uniffi-bindgen  |  (B) app-bundle assembly
│   └── Sources/LeoGit/…       #   App, Screens, Stores, IPC, Design
├── (Option B) daemon/leogitd/ # bin crate wrapping leogit-core over a socket
├── FRONTEND.md                # the contract (Step 2)
├── docs/plans/swiftui-macos-frontend.md   # this file
└── justfile                   # add macOS recipes (build-rust, xcodegen/swift build, bundle)
```

## 9. Execution order (so another agent can run it)

Follow LeoGit's rule: implement in user-flow order, verify each step before the next
(per `CLAUDE.md`). Suggested phases:

0. **Extract `leogit-core`** (§4) — no behavior change; Tauri app still passes
   `just check`/`just build` and runs identically. **Checkpoint.**
1. **Write `FRONTEND.md`** (§5) — freeze the contract from the current 69/4/~30 surface.
2. **Pick the bridge** (§6) — record the decision here and in FRONTEND.md §3.
3. **Bridge scaffolding** — (A) `leogit-ffi` + build-rust.sh + XcodeGen project that
   compiles and exposes `ping`; or (B) `leogitd` serving `/api/ping` + the Swift
   `IPCClient` dialing the socket. **Checkpoint: Swift app talks to core.**
4. **Diff render contract** (§7.1) — add structured-run output; decide shared-vs-dual.
5. **SwiftUI shell** — window, repo picker, header, changes/history panes, diff
   viewer (structured runs → `AttributedString`). Port orchestration logic (§7.3)
   store-by-store. Verify each screen against FRONTEND.md before the next.
6. **Streaming features** — git-progress (push/pull/clone) and the terminal
   (SwiftTerm + PTY reuse) over the chosen event transport.
7. **Packaging & release** — ✅ shipped with the parity plan's WS-T. `.app`
   assembly + ad-hoc codesign, and `scripts/deploy_release.py` ships the SwiftUI
   app on macOS while Win/Linux keep shipping Tauri. The version is
   single-sourced from `tauri.conf.json` across five files by
   `scripts/_version.py`, with a test in each host crate pinning its half.

Per `CLAUDE.md` visual-testing rule, each UI screen is confirmed by Leo visually
before it counts as done. Keep meaningful debug logs on both sides during dev.

## 10. Decisions to make (explicitly deferred)

1. **Bridge mechanism** — Option A (UniFFI link) vs B (daemon HTTP+SSE) vs C (in-proc
   router). §6 + matrix §6.1.
2. **Cargo workspace vs independent path-dep crates.** §4.1.
3. **Tauri side under Option B** — keep in-process `#[tauri::command] → core`, or make
   the Tauri host an HTTP client to `leogitd` too (one wire contract). §6 Option B.
4. **Diff payload** — the wire is the structured `Token`/`TokenClass` layer either
   way (§7.1); the open call is where the HTML collapse lives — `render.rs` on the
   Tauri host (Svelte unchanged) vs rendering in Svelte (structured-only for both).
5. **macOS project tooling** — XcodeGen `.xcodeproj` (A) vs pure SwiftPM (B).
6. **Terminal emulator library** on macOS (SwiftTerm vs alternative). §7.2.
7. **Type-sync discipline** — hand-mirrored (both references do this; unified by a
   snake_case↔camelCase convention) vs introducing codegen (ts-rs/specta). Both
   references chose hand-mirrored + a source-of-truth doc.
8. **Event transport for the terminal byte stream** — callback interface (A) vs a
   framed SSE channel (B); validate throughput either way.

## 11. Reference index (Leo's existing implementations)

| Concern | `git-projects-manager` (UniFFI) | `leosync-src` (daemon) |
|---|---|---|
| Core crate | `core/` (`gpm-core`) | `desktop/daemon/core/` (`leosync-core`) |
| Bridge | `macos/ffi/` UniFFI 0.32, `src/lib.rs`, `uniffi-bindgen.rs` | `desktop/daemon/leosyncd/` bin |
| macOS build | `macos/scripts/build-rust.sh`, `macos/project.yml` (XcodeGen) | `scripts/install_macos.sh` (SwiftPM + bundle) |
| Swift↔Rust | static lib + generated Swift | `AsyncHTTPClient` over `http+unix` |
| Events | none (request/response) | SSE `/api/events`, monotonic id, resync |
| Tauri↔core | path dep + `#[tauri::command]` | HTTP client to daemon (`hyper`+`hyperlocal`) |
| Contract doc | `FRONTEND.md` (domain + platform mapping) | `FRONTEND.md` (wire: routes/DTOs/events) |
| Type sync | hand-mirrored, camelCase serde convention | hand-mirrored, snake↔camel coders |
| Options analysis | — | `plans/IOS_SUPPORT.md` (A–E trade-offs) |
