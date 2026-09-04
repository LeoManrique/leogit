# Plan — Re-skin the Tauri client onto the native design language

> Status: **planning**. No code has been written for this. The audits and the
> platform research behind every claim here are done and cited, and the design
> question is settled — the native client wins (§8). What is left is the
> per-item work in §6 and three on-target checks.
> Companions: [`STYLE.md`](../../STYLE.md) (the design language),
> [`FRONTEND.md`](../../FRONTEND.md) §8 (the divergences that stay).

## 1. Goal & scope

Make the Tauri client — which ships on **Windows and Linux only**; macOS runs the
native SwiftUI app — read as the same product as the native client.

**One look on every platform.** leogit wears the macOS-derived language in
`STYLE.md` on Windows and Linux too. The point is that leogit looks like one
product, not that it impersonates each host. The exception is chrome the **host
OS owns rather than the app**: traffic-light window controls, the window frame,
the menu bar. Those stay whatever the platform draws.

That exception has a consequence which decides §6.2 and §6.6, so it is stated
here rather than discovered later: **"use the real platform control" and "look
like macOS" are opposite instructions on Windows and Linux.** A native context
menu on Windows is a classic win32 `HMENU`; a native window material is Mica.
Neither resembles the reference. Where the platform's own widget is the *only*
route to native behaviour we take it and accept the look (the window frame);
where the behaviour is reproducible in CSS we draw it ourselves and match the
reference (menus). This is a deliberate inversion of the rule the native client
follows, and it holds only because the reference platform is not the ship
platform here.

**Out of scope:** behaviour. Nothing in this plan changes what a control does,
only what it looks like. Behavioural gaps are
[`cross-client-feature-parity.md`](cross-client-feature-parity.md).

## 2. Where we are today

Better than the docs claimed before this plan was written. Verified by direct
audit of `apps/tauri-app/src/`, not from the docs:

**Done.** Both theme palettes in [`app.css`](../../apps/tauri-app/src/app.css)
carry `STYLE.md`'s Light and Dark tables **token for token**, alphas included.
The Primer hexes (`#0d1117`, `#58a6ff`, `#3fb950`) appear nowhere in the client.
All twelve of `STYLE.md`'s named anti-patterns are absent: zero
`text-transform`, zero `@font-face`/`@import url`, no `border-radius: 999px` on
any button, no accent glow shadows, no radio-card pickers. Tabular numerics are
set globally on `body` and re-asserted in 13 numeric columns; no numeric column
is missing them. Transitions are 80–120 ms throughout. No custom titlebar —
zero hits for `data-tauri-drag-region`.

Some components already match the reference to the pixel. The status badge at
[`FileList.svelte:648-661`](../../apps/tauri-app/src/lib/components/FileList.svelte#L648-L661)
is 18×18, radius 4, 15 % tint, 10 px mono bold — which is exactly
`FileStatusBadge` at
[`FileStatusStyle.swift:67-72`](../../apps/swift-ui-app/Sources/LeoGit/Design/FileStatusStyle.swift#L67-L72).

**Not done.** The chrome layer, itemised in §6.

## 3. The reference vocabulary

What the native client actually renders, as measured numbers rather than
adjectives. These are the targets for §6.

| Surface | Native client | File |
|---|---|---|
| Tab bar | custom, not a segmented `Picker`: 13 px, semibold active / regular plain, 14×10 padding, 2 pt `.tint` underline at the bottom edge, text `.primary` / `.secondary` hovered / `.tertiary` idle | `Screens/RepoTabBar.swift:28-68` |
| Status badge | 10 px mono **bold**, 18×18, radius 4, `tint.opacity(0.15)` | `Design/FileStatusStyle.swift:67-72` |
| Tag chip | 10.5 px mono, `.secondary`, height 16, radius 5, `.quaternary` fill — neutral, never accent | `Screens/HistorySidebar.swift:243-249` |
| Unpushed plate | 9 px bold arrow, 16×16, radius 5, same `.quaternary` | `Screens/HistorySidebar.swift:218-222` |
| Lists | `List(selection:)`, `.listStyle(.inset)`, `.alternatingRowBackgrounds()`, no explicit row height — rows are 2 pt (files) / 3 pt (commits) vertical padding | `Design/ChangedFileList.swift:103-114`, `Screens/HistorySidebar.swift:95-109` |
| Composer chrome | amend notice and error strip are a **2 pt leading rule** (yellow / red), not a filled banner | `Screens/CommitComposer.swift:180,241` |
| Resize handle | stock `Divider()` with 3 pt vertical padding — a 7 pt grab zone on a 1 px rule | `Design/RowResizeHandle.swift` |
| Sheets | fixed widths 420 / 460 / 480 pt, height always intrinsic; title is `.title3.weight(.semibold)` | passim |
| Buttons | `.borderedProminent` for every confirm; `.plain` for in-row; `.link` for inline text actions; `.accessoryBar` exactly once (terminal strip) | passim |
| Scroll views | entirely stock — no `.scrollIndicators`, no custom scrollbar anywhere | passim |
| Animation | **one** in the whole app: `.easeOut(duration: 0.12)` on the diff's slow-reload spinner | `Screens/DiffView.swift:82` |

Two rules the native code states outright and this plan inherits:

- The tab bar is the one place a custom control **wins**, and the reason is
  recorded: "a segment can only hold text or an image, and the Changes tab
  carries a count pill" (`RepoTabBar.swift:3-7`). The Tauri tab bar is already
  the same shape.
- Colour is semantic everywhere except one file. The native client defines no
  colour assets; `Design/DiffPalette.swift` is the sole literal-hex palette, and
  its values are the ones `app.css` already assigns. Everything else is
  `.primary`/`.secondary`/`.tertiary`/`.quaternary`/`.separator`/`.tint`.

## 4. Platform reality

Researched against current docs, not from memory. Version-dated, with the
"could not verify" cases marked as such — they are the ones §8 asks about.

**Engines.** Windows: Evergreen WebView2, stable runtime 152.x (Aug 2026),
Chromium-based. Linux: `webkit2gtk-4.1`. Tauri 2.11 (`tauri = "2.11.2"`).

**Scrollbars.** `scrollbar-width` / `scrollbar-color` landed in Chromium 121;
WebKitGTK gained `scrollbar-color` in 2.52.3 (2026-04-16) and `scrollbar-width`
is **not named in any release note** — unverified. The two systems are
**mutually exclusive**: a non-`auto` `scrollbar-color`/`-width` overrides
`::-webkit-scrollbar-*`. Overlay behaviour is an engine/OS property, not CSS:
WebKitGTK has painted real overlay scrollbars **by default since 2.12**, so
Linux gets the reference behaviour free; Chromium ships Fluent scrollbars
**non-overlay** on Windows, and whether the returning `#overlay-scrollbars` flag
is reachable through WebView2 `AdditionalBrowserArguments` is **not
documented**. CSSWG `scrollbar-style: overlay`
([csswg-drafts#13218](https://github.com/w3c/csswg-drafts/issues/13218)) is open
with no resolution.

**Context menus.** `tauri::menu::ContextMenu` is gated `cfg(desktop)`, **not** by
OS — `popup()` and `popup_at()` work on Windows *and* Linux via muda's
`show_context_menu_for_gtk_window` / `_for_hwnd`, contrary to the common
folklore that Linux is unsupported. Submenus, check items, separators, disabled
state, accelerators and icon items are all supported. But: **there is no theming
API at all** — no theme, colour or font item anywhere in `tauri::menu`;
[muda#97](https://github.com/tauri-apps/muda/issues/97) (dark mode) was closed
without one, and [muda#167](https://github.com/tauri-apps/muda/issues/167)
confirms Windows menus are classic win32 `HMENU`, not WinUI, so not even the
Windows 11 rounded/dark menu. Also: **Wayland positioning is broken**
([tauri#13608](https://github.com/tauri-apps/tauri/issues/13608), open) — the
menu lands at screen centre; X11 and Windows are accurate. No dismissal event
([muda#161](https://github.com/tauri-apps/muda/issues/161)).

**Window chrome.** `titleBarStyle`, `hiddenTitle` and `trafficLightPosition` are
**macOS-only** and inert on both ship platforms. `windowEffects` exists in Tauri
2 core but `Mica`/`Acrylic`/`Tabbed` are Windows-only and `set_effects` is
documented "Linux: Unsupported"; `window-vibrancy` 0.8.0 is explicitly
unsupported on Linux; `tauri-plugin-decorum` is Windows+macOS and in maintenance
mode. **No blur or material is available on Linux at all.**

**Form controls.** `accent-color` affects checkbox, radio, range and
`<progress>` — **never `<select>`**. `appearance: base` ships **nowhere**
(MDN; [csswg-drafts#10804](https://github.com/w3c/csswg-drafts/issues/10804)).
But customizable select **is** shipping as `appearance: base-select` — Chrome/
Edge 135 (Apr 2025), with `::picker(select)`, `::checkmark` and
`<selectedcontent>`. WebView2 stable is far past 135, so it is **available on
Windows**; no WebKitGTK release note mentions it, so assume **unavailable on
Linux** and verify on target.

## 5. What this means

Three of the four goals are reachable on both platforms with CSS alone. The
divergences that cannot be closed are **overlay scrollbars** (free on Linux,
not reachable on Windows), **window materials** (Windows only), and
**customizable `<select>`** (Windows only). Each is recorded in §6 as a decision
rather than left to be rediscovered.

## 6. The work

### 6.1 Scrollbars — delete the dead block, keep the standard properties

[`app.css:283-312`](../../apps/tauri-app/src/app.css#L283-L312) sets **both**
systems on `*`: `scrollbar-width: thin` + `scrollbar-color`, and a
`::-webkit-scrollbar` block. Because the standard properties win in Chromium
121+, **the entire `::-webkit-scrollbar` block is dead code on Windows** and the
8 px width, 4 px radius and hover colour it specifies never render. Delete it;
keep `scrollbar-width` + `scrollbar-color`.

Windows keeps non-overlay Fluent scrollbars — not reachable, see §4 — and Linux
keeps GTK's overlay ones. Accept the divergence and record it in `FRONTEND.md`
§8, which is what that section is for.

One consequence to test rather than assume:
[`FileList.svelte:543`](../../apps/tauri-app/src/lib/components/FileList.svelte#L543)
sets `scrollbar-gutter: stable`, which reserves space on Windows and is inert
under Linux's overlay scrollbars — a genuine one-platform layout difference in
the densest list in the app.

### 6.2 Context menus — keep drawing them, draw them like AppKit

**Decision: do not adopt Tauri's native context menu.** It is feasible on both
platforms, and it is the wrong call here on five separate grounds: the Windows
menu is a classic win32 `HMENU` (§4), so it defeats goal #1 outright; there is
no theming API to bring it back; the destructive-red item that
[`ContextMenu.svelte`](../../apps/tauri-app/src/lib/components/ContextMenu.svelte)
has today would be lost; Wayland positioning is broken; and there is no
dismissal event to drive the callers' state.

Instead, restyle the existing component — used by
[`FileList`](../../apps/tauri-app/src/lib/components/FileList.svelte#L516),
[`CommitList`](../../apps/tauri-app/src/lib/components/CommitList.svelte#L423),
[`Header`](../../apps/tauri-app/src/lib/components/Header.svelte#L697) and
[`BranchDropdown`](../../apps/tauri-app/src/lib/views/BranchDropdown.svelte) —
to AppKit's metrics. It already does separators, disabled items and keyboard
navigation, which is the expensive half.

This contradicts [`STYLE.md:179`](../../STYLE.md#L179) ("Row context menus are
**stock system menus** … Nothing here is hand-drawn or re-themed; the platform
owns the look"), which is written from the native client's position. See §7.

### 6.3 Two colour literals, repeated 26 times

Not 26 decisions — two, applied 26 times:

- **15** sites hardcode `rgba(0, 0, 0, 0.4)` as the modal backdrop. That is
  wrong in *both* themes: `STYLE.md:216` specifies 0.3 light / 0.5 dark. Add
  `--overlay-backdrop` to both theme blocks and replace all 15.
- **11** sites hardcode `color: #ffffff` on primary-CTA text. Add `--on-accent`
  and replace all 11.

14 of 31 components use tokens exclusively today; this takes it to 30. The
remainder is terminal black (`#000000`), which `STYLE.md:248` explicitly allows.

### 6.4 Per-component conformance

- **Focus ring**
  [`FileList.svelte:637`](../../apps/tauri-app/src/lib/components/FileList.svelte#L637)
  uses `outline: 2px solid var(--border-active)` — solid accent, where every
  other ring in the app is the low-alpha `0 0 0 2px var(--cursor-bg)`.
- **Letter-spacing on lowercase**
  [`MainLayout.svelte:3010`](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L3010)
  (`.shell-name`). `STYLE.md:24` forbids it.
- **Headings** — settled by "the native client wins", which resolves the three
  oversized ones **differently**, because each has a different counterpart:
  - [`App.svelte:357`](../../apps/tauri-app/src/App.svelte#L357) — the
    error-screen `h1` at 17 px. Native's equivalent failure title is
    `.title3.weight(.semibold)` (`Design/ActionFailureSheet.swift:68`, and every
    sheet). **17 px → 15 px.**
  - [`CommitDetail.svelte:105`](../../apps/tauri-app/src/lib/views/CommitDetail.svelte#L105)
    — the commit summary at 15 px. Native renders the same string with
    `.headline` (`Screens/HistoryDetailPane.swift:148`), which is 13 pt
    semibold on macOS. **15 px → 13 px** — this one comes *down*.
  - [`MainLayout.svelte:2943`](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L2943)
    — the dirty-submodule title at 15 px. Native uses a stock
    `ContentUnavailableView` here (`Screens/DiffView.swift:359-370`), whose
    title the system sizes well above 13 pt. **Stays 15 px.**

  The pattern underneath: native has three heading registers, not one — a sheet
  or failure title (`.title3.semibold`), a detail-pane heading (`.headline`),
  and a system-sized pane empty state. `STYLE.md:210`'s single 13 px rule
  cannot express that, which is why §7 amends it.
- **The one unshared shadow** —
  [`Terminal.svelte:437`](../../apps/tauri-app/src/lib/components/Terminal.svelte#L437)
  uses a literal `0 2px 8px rgb(0 0 0 / 35%)` where everything else uses
  `--shadow-popover`.

**Not a defect:** `font-weight: 700` at `FileList.svelte:659`. It matches the
native badge exactly (§2), and `STYLE.md:9` says the native app wins. The rule
is what needs amending — see §7.

**Not a defect:** the `letter-spacing` at `Header.svelte:819,826`. Those style
the literal strings `DETACHED HEAD` and `MERGING`, so tracking on them is
correct typography, and `FRONTEND.md` §8 already records the badge-vs-chip-label
difference as intentional.

### 6.5 Settings form controls

`STYLE.md:205` wants `Toggle`/`Picker` shapes; the Tauri form draws a checkbox
and a `<select>` at
[`SettingsOverlay.svelte`](../../apps/tauri-app/src/lib/views/SettingsOverlay.svelte#L222)
(and :236, :265, :275, :337, :355). `accent-color` already handles the
checkboxes. For the selects, gate on `@supports (appearance: base-select)` so
Windows gets the styled picker and Linux keeps the current control as the
baseline (§4). Do not reach for a hand-rolled dropdown to close the gap —
that trades a platform control for a maintenance burden on the one platform
that cannot use it.

### 6.6 Window chrome — decline

Change nothing. `titleBarStyle` and `trafficLightPosition` are macOS-only and
inert; materials are Windows-only with no Linux equivalent at all. The window
frame is host-owned chrome under §1's exception, so it stays as the platform
draws it. Recorded here so it is not re-opened.

## 7. `STYLE.md` amendments this plan requires

The design language is written from the native client's position and three of
its rules do not survive contact with a Windows/Linux WebView. Each needs an
explicit exception rather than a component quietly disagreeing with the doc:

1. **§Context menus (`STYLE.md:179`)** — "stock system menus … the platform owns
   the look" is right for the native client and wrong for the Tauri one, where
   the stock menu is a win32 `HMENU`. State the split: native takes the system
   menu, Tauri draws one to the same metrics.
2. **§Typography (`STYLE.md:23`)** — "avoid bold (700+)" needs the status-badge
   exception, since the native reference is 700 there.
3. **§Terminal (`STYLE.md:255`)** — "No scrollbar styling — let the platform
   draw it" currently sits in the Terminal section, but the client styles
   scrollbars globally. Say what the app-wide rule is, and that overlay
   behaviour is the engine's to give.
4. **§Section headers (`STYLE.md:210`)** — "13px semibold for in-app section
   titles (Settings categories, dialog titles)" collapses three native registers
   into one (§6.4). Split it: **15 px semibold** for sheet, dialog and failure
   titles (`.title3.semibold`), **13 px semibold** for detail-pane headings
   (`.headline`) and section labels, and pane-level empty states left to the
   system's own sizing.

## 8. On-target verifications

**The design question is settled: where the native client and this document
disagree, the native client wins** — `STYLE.md:9`'s standing rule, applied to
every choice in §6 and §7.

What remains is not preference but three facts about what the two engines
support, which no documentation answers and no decision can supply. Each is
recorded with the outcome the rule above asks for, so the test has a target
rather than only a question. Where an engine cannot reach it, the answer is to
record the divergence in `FRONTEND.md` §8 — never to redesign both platforms
down to the weaker one.

1. **Does `scrollbar-width` defeat WebKitGTK's overlay scrollbars?**
   *Target:* Linux keeps overlay scrollbars, which is the native behaviour and
   is free there. If setting the property turns them into classic ones, drop
   `scrollbar-width` on Linux and keep `scrollbar-color` alone.
2. **Is `#overlay-scrollbars` reachable through WebView2's
   `AdditionalBrowserArguments`?**
   *Target:* Windows matches Linux. If the flag is not reachable, Windows keeps
   non-overlay Fluent scrollbars and that is a recorded divergence, not a defect.
3. **Does WebKitGTK 2.52 support `accent-color` and `appearance: base-select`?**
   *Target:* both platforms get the styled control. Shared WebKit code makes
   `accent-color` near-certain and `base-select` unlikely; the
   `@supports` gate in §6.5 already handles the negative case, so this only
   decides how much of §6.5 runs on Linux.

All three are answered by building the Tauri client on a Windows and a Linux
target and looking.

## 9. Execution order

Ordered so each step is verifiable on its own and nothing depends on an
unanswered question:

1. **§7** — the four `STYLE.md` amendments, so the document and the reference
   agree before any component is moved to match either.
2. **§6.3** — the two tokens. Mechanical, 26 sites, nothing to decide.
3. **§6.4** — the conformance fixes, headings included; §7's amendment 4 is what
   unblocks them, and each heading's target is already named.
4. **§6.1** — delete the dead `::-webkit-scrollbar` block. Then run
   verifications 1 and 2 on target and record the outcome in `FRONTEND.md` §8.
5. **§6.2** — restyle `ContextMenu.svelte` to AppKit metrics. The largest single
   piece, and the one most worth checking visually against the native client
   side by side.
6. **§6.5** — Settings controls, with verification 3 deciding how much of it
   runs on Linux.

§6.6 is a decision, not a step.
