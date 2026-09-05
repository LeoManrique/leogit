# Plan — Re-skin the Tauri client onto the native design language

> Status: **the chrome layer §6 covered is closed; the looking then found a
> larger layer §6 was never pointed at.** All six steps in §9 have landed, and
> §6.1–§6.5 record what each one changed and why, against the source the claim
> came from. **§10 is the audit that followed** — the two clients open on one
> repository, every visible difference read back to the code that causes it.
> It is the longer list, and most of it is not what this plan was about.
> **Most of §10 has since landed**: the app's leading and the diff pane's
> geometry (§10.1, §10.3), then the header's span, the toolbar's capsules, both
> list geometries, the grouped Settings form, the composer's label and disabled
> paint, and the icon registry that replaced forty-four inline `<svg>` blocks.
> What is left in the top group is **the native client's to fix, not the Tauri
> client's** (§10.2), plus P-8/P-9/P-10 — the diff header's string, status plate
> and type register, which close together once `FileEntry` reaches `DiffViewer`.
> The design question is settled — where the native client and this document
> disagree, the native client wins (§8) — and §10.7 is down to three choices
> that rule does not decide, with the work under them waiting on those choices.
> **§8's three checks need a Windows and a Linux build**, because they ask what
> the two engines render rather than what they support; §10 was read on macOS
> and inherits that limit wherever it is engine-specific.
> §6.6 is a decision not to act, and stays one.
> Companions: [`STYLE.md`](../../STYLE.md) (the design language),
> [`FRONTEND.md`](../../FRONTEND.md) §8 (the divergences that stay),
> [`ROADMAP.md`](../../ROADMAP.md) (the control *shapes* this plan deliberately
> left alone — see §6.5).

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
[`FileList.svelte:642-655`](../../apps/tauri-app/src/lib/components/FileList.svelte#L642-L655)
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
| Lists | `List(selection:)`, `.listStyle(.inset)`, no explicit row height — rows are 2 pt (files) / 3 pt (commits) vertical padding. Both call sites also carry `.alternatingRowBackgrounds()`, which is **not** a target — §10.2 records why it is drift | `Design/ChangedFileList.swift:103-114`, `Screens/HistorySidebar.swift:95-109` |
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

**Scrollbars.** `scrollbar-width` / `scrollbar-color` landed in Chromium 121.
WebKitGTK enables `scrollbar-width` from **2.46** (`CSSScrollbarWidthEnabled`
defaults true from that release) and `scrollbar-color` from **2.52.3**
(2026-04-16), backported onto the 2.52 branch. Confirm the target's actual
package version rather than trusting a figure here — distro builds move — but
any build from 2.52.3 on has both. The two systems are **mutually exclusive**,
and WebKit's own predicate is where that is written down rather than inferred
(`Source/WebCore/rendering/style/RenderStyle+GettersInlines.h`, one identical
blob at `webkitgtk-2.52.3` and `2.52.6`):

```cpp
// ignore non-standard ::-webkit-scrollbar when standard properties are in use
usesStandardScrollbarStyle() { return scrollbarWidth() != Auto || !scrollbarColor().isAuto(); }
usesLegacyScrollbarStyle()   { return hasPseudoStyle(WebKitScrollbar) && !usesStandardScrollbarStyle(); }
```

It is an **or**, so *either* standard property alone switches the pseudo-elements
off — the `*` rule sets both, so a `::-webkit-scrollbar` block is inert twice
over. The same predicate gates custom-scrollbar creation at all three sites
(`RenderLayerScrollableArea::createScrollbar`, `RenderListBox::createScrollbar`,
and `LocalFrameView`, which additionally requires `scrollbarWidthStyle() ==
Auto` before honouring a block declared on `<body>`), and it gates overlay:
`RenderBox::canUseOverlayScrollbars()` is
`!style().usesLegacyScrollbarStyle() && ScrollbarTheme::theme().usesOverlayScrollbars()`,
and `RenderScrollbar::isOverlayScrollbar()` is hardcoded `false`. **So a
`::-webkit-scrollbar` block that is not overridden costs Linux its overlay
scrollbars and a reserved column in every pane** — which is the reason the ban
on that block is structural rather than tidiness.

What Linux does *not* get is thickness: `ScrollbarThemeAdwaita::scrollbarThickness`
branches only on `ScrollbarWidth::None` and otherwise returns a constant 21, and
nothing under `platform/adwaita/` or `platform/gtk/` reads `ScrollbarWidth::Thin`
at all. `scrollbar-color` *is* honoured there, overlay thumb included — **but
only because the app is on the Adwaita path in the first place**, which is not
the GTK3 default. `ScrollbarTheme::nativeTheme()` on `webkit2gtk-4.1` is
`ScrollbarThemeGtk`, which paints through GTK CSS gadgets and contains no
reference to `scrollbar-color` at all; every one of its entry points opens by
delegating to Adwaita `if (!m_useSystemAppearance)`. wry sets exactly that —
`context.set_use_system_appearance_for_scrollbars(false)`
(`wry-0.55.1/src/webkitgtk/mod.rs:422`) — so the thumb colour on Linux rides on
a wry call, and a wry release that changed it would drop the colour silently.

Overlay behaviour is an engine/OS property, not CSS:
WebKitGTK has painted real overlay scrollbars **by default since 2.12**, so
Linux gets the reference behaviour free. `ScrollbarThemeAdwaita::usesOverlayScrollbars()`
reads no CSS at all — only `SystemSettings`' `overlayScrolling` (defaulting
`true`) and, on GTK3, a `GTK_OVERLAY_SCROLLING=0` escape hatch — so a desktop
that has turned overlay scrolling off is the one Linux configuration that draws
classic scrollbars, and it is the user's setting rather than the app's. Chromium
ships Fluent scrollbars
**non-overlay** by default on Windows, but overlay there is a **first-class
embedder option, not a flag**: `CoreWebView2ScrollbarStyle.FluentOverlay`,
which Tauri surfaces as the `scrollBarStyle` window key (`"fluentOverlay"`) and
as `WebviewWindowBuilder::scroll_bar_style`. It needs **WebView2 Runtime
125.0.2535.41+** and silently does nothing below that; Linux and macOS accept
only `Default` and no-op; and every webview sharing a data directory must carry
the same value. Tauri also documents that **CSS scrollbar styling applies on top
of** the native appearance chosen here. CSSWG `scrollbar-style: overlay`
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
`<progress>` — **never `<select>`** — and both engines read it **only on the
native paint path**, so it colours a control the engine still draws and nothing
else. It is available on both: Blink throughout, and WebKitGTK with
`AccentColorEnabled` stable and defaulting true under `USE(THEME_ADWAITA)`
(`Source/WTF/Scripts/Preferences/UnifiedWebPreferences.yaml` at
`webkitgtk-2.52.0` and `2.52.3`).

**Neither engine devolves a checkbox or a radio**, whatever an author declares
on it. Blink's `LayoutTheme::IsControlStyled` (`branch-heads/7977`, M152)
switches on the button, progress, meter, menulist, search, textarea and
textfield parts and returns `default: false` for the rest; WebKit's
`RenderTheme::isControlStyled` (`webkitgtk-2.52.3`) has the same shape. The
declarations are discarded instead. **A `<select>` is the opposite**: an author
`background`, `border` **or `border-radius`** — the radius longhands carry
Blink's `is_border` flag — devolves it to a *styled menulist* (`kMenulistButton`
on Blink, `nativeAppearanceDisabled()` on WebKit), which is a half-native state
that keeps the engine's own arrow and its arrow padding, loses `box-shadow` to
the themed-control reset, and on WebKitGTK has its `line-height` overwritten by
`RenderThemeAdwaita::adjustMenuListStyle`. `appearance: none` is what leaves the
native path outright, on both.

**A select's popup is only stylable on one engine.** Windows is the surprise:
Chromium uses an internal popup — a real HTML document — and applies per-option
`color`, `background-color` and font; the external native popup is the macOS
path. WebKitGTK is the reverse of the folklore and builds a genuine
`GtkTreeView` in a `GtkPopover`, consuming none of the `<option>` styles WebCore
computes ([webkit.org/b/9846](https://bugs.webkit.org/show_bug.cgi?id=9846),
filed 2006, still open).

**Customizable select** ships as `appearance: base-select` from **Chrome/Edge
135** (2025-04-01) with `::picker(select)`, `::picker-icon`, `::checkmark` and
`<selectedcontent>`, so it is available on Windows. On Linux it is **absent from
2.52 by two weeks**: WebKit added the keyword 2026-02-03 and enabled it
2026-02-14, after the `webkitglib/2.52` branch was cut ~2026-01-20. It is on
`webkitglib/2.54`. Two detection traps go with it: probe the `appearance`
**value**, since an unknown value invalidates the declaration — `base` alone is
in 2.52's `CSSValueKeywords.in` and would report true while delivering nothing —
and never probe with `::picker()` or `:open`, which parse without behaving.

## 5. What this means

Three of the four goals are reachable on both platforms without leaving the
frontend. The one divergence that cannot be closed is **window materials**
(Windows only), and §6.6 declines it rather than half-building it. **Overlay
scrollbars are not among them** — free on Linux, and one config key on Windows
(§4), so the reference behaviour is reachable on both. The one scroller no rule
here reaches is the terminal's, which xterm draws itself in JS; it is already an
auto-fading overlay, so the divergence is width and colour rather than
behaviour.

**Customizable `<select>` is available on Windows only and is used on neither**
(§6.5): a control styled on one platform and not the other is a wider divergence
than the one it closes, and the gap is a WebKitGTK release rather than a
platform limit. What both platforms do get is a select taken off the native path
with `appearance: none`, which is the part that renders the same on both. Each
of these is recorded in §6 as a decision rather than left to be rediscovered.

## 6. The work

### 6.1 Scrollbars — one standard rule, and Windows asked for overlay

Done. [`app.css`](../../apps/tauri-app/src/app.css) holds the whole definition
in one `*` rule — `scrollbar-width: thin` with
`scrollbar-color: var(--border-strong) transparent` — and the client contains no
`::-webkit-scrollbar` rule anywhere. The deleted block never rendered on either
engine, because either standard property alone switches the pseudo-elements off
(§4 quotes the predicate).

**Both standard properties stay, because each is the one an engine actually
renders**, and `app.css` says so per property to stop either being "cleaned up"
as inert. `scrollbar-width: thin` is a real thickness on Chromium/WebView2 and
sets none on WebKitGTK, whose Adwaita theme branches only on `None`.
`scrollbar-color` is what Linux renders (2.52.3+), giving the neutral thumb at
GTK's own width. Neither is what keeps the legacy block at bay by itself — the
gate is an **or** and both properties are set, so it is shut twice over.

Windows is asked for overlay where such a switch belongs:
`"scrollBarStyle": "fluentOverlay"` on the `main` window in
[`tauri.conf.json`](../../apps/tauri-app/src-tauri/tauri.conf.json). The app
builds no webview in Rust, so this is a one-key config change with no call site.
The key is verified against this exact toolchain rather than assumed: `tauri`
2.11.5 / `tauri-utils` 2.9.3 parse `tauri.conf.json` at build time with
`deny_unknown_fields`, and `scrollBarStyle` is in the accepted set — a misspelled
key or a bad enum value is a compile error, not a silent no-op. (The file's
`$schema` now points at the CLI's real `config.schema.json`, so an editor
validates the key too.)

**`scrollbar-gutter: stable` stays** at
[`FileList.svelte`](../../apps/tauri-app/src/lib/components/FileList.svelte#L543).
Nothing depends on the column it reserves — the rows are `left: 0; right: 0`,
the filename cell is `flex: 1 1 0; min-width: 0; overflow: hidden`, and no
script reads a width — so it satisfies `STYLE.md`'s rule that a gutter may
narrow a column but must never be what one depends on. It costs nothing under
overlay, and the one configuration it still serves is precisely the one the
config key cannot reach: a Windows host below WebView2 125.0.2535.41, where
`fluentOverlay` is ignored and classic Fluent scrollbars would otherwise shift
the list sideways the moment it grows past the viewport.

**The terminal is outside all of this**, and `STYLE.md` and `FRONTEND.md` §8 now
say so rather than leaving the app-wide rule quietly untrue. `@xterm/xterm` 6
vendors VS Code's scrollable element and draws the scrollbar as `<div>`s, so no
CSS scrollbar property reaches it — `.xterm-viewport` still carries
`overflow-y: scroll` but is left empty and never produces a thumb. It is themed
through `ITheme` instead, and is already an auto-fading overlay slider, so the
behaviour matches; only its width (14 px) and colour (the terminal foreground at
20 %) differ from the app's hairline. Both are reachable via
`ITheme.scrollbarSlider*` if the divergence ever stops being acceptable.

Noted while mapping that DOM, for whoever next touches `Terminal.svelte`:
its `:global(.xterm-viewport) { background-color: #000 !important }` paints
nothing. The visible black comes from `Viewport.ts` setting
`backgroundColor` inline on the scrollable element from `theme.background`,
which the component already sets to `#000000`, over a `.terminal-body` that is
also black. Left in place rather than removed here because it is a background
rather than a scrollbar, so it wants its own visual check on target.

### 6.2 Context menus — keep drawing them, draw them like AppKit

Done.
[`ContextMenu.svelte`](../../apps/tauri-app/src/lib/components/ContextMenu.svelte)
draws a reproduction of a macOS 26 `NSMenu`, and every number in it is a
measurement of a real one rather than a value chosen here. Only the style block
and one label wrapper changed; the component's behaviour is untouched.

**Tauri's native context menu is not adopted.** It is feasible on both
platforms, and it is the wrong call here on five separate grounds: the Windows
menu is a classic win32 `HMENU` (§4), so it defeats goal #1 outright; there is
no theming API to bring it back; the destructive-red item would be lost;
Wayland positioning is broken; and there is no dismissal event to drive the
callers' state. [`STYLE.md`](../../STYLE.md)'s *Context menus* states the split
this rests on — the native client takes the system menu, the Tauri client draws
its own to the same metrics.

**Where the metrics came from.** Apple publishes none of them: the HIG's *Menus*
page carries no numbers for macOS at all, and `NSMenu` exposes no layout API. So
they were read out of AppKit on **macOS 26.6.2 (build 25G83)** — popping a real
`NSMenu`, walking `NSPopupMenuWindow`'s view and layer trees, and rasterizing
rows offscreen. Two independent passes agreed on every row below but one, and
that one is the row worth reading twice: the label's *view frame* sits at 14pt
and its glyphs at 16pt, and CSS padding positions glyphs. This table is the only
record in the repo of what an AppKit menu measures.

| Property | AppKit, macOS 26 | How it was read | In the component |
|---|---|---|---|
| Menu corner radius | **12pt**, `continuous` | `cornerRadius` on the menu window's glass layers | `border-radius: 12px` (CSS has no squircle) |
| Menu content inset | **5pt** top and bottom; rows full-bleed horizontally | first `NSTableRowView` at y=5; `_styleInsets` = 5/0/5/0 | `padding: 4px` + the 1px border |
| Item row height | **24pt**, rows abutting | row frames; `rowHeight` 24, `intercellSpacing` (0,0) | `height: 24px` |
| Label leading inset | **16pt** from the menu edge | glyph ink at x=16 in an offscreen raster — the `_NSMenuItemTextField` *frame* is at 14, and its own `alignmentRectInsets` add the other 2 | 1px border + 4px padding + `padding: 0 11px` |
| Label trailing inset | **16pt** | `NSMenu.size.width` is the title's width + 32pt exactly, across four different strings | the same 11px |
| Label font | **13pt regular** `.AppleSystemUIFont`, 16pt line box | `NSFont.menuFont(ofSize: 0)`; the live text field | `font-size: 13px`, `font-family: inherit`; the line box stays inherited, since pinning 16px would clip a Linux fallback face whose own box is taller and the row centres the glyphs either way |
| Highlight shape | inset rounded rect — **5pt** left and right, the row's **full 24pt** height | `NSRootMenuWindowBackgroundView` at (5, 5, 136×24) in a 146pt menu | the item box itself |
| Highlight radius | **7pt** | `cornerRadius` on that view's fill layers | `border-radius: 7px` |
| Highlight fill | the **accent**, composited through glass | fill layer reads sRGB `#1769E6` in Dark over a `CABackdropLayer`; the highlight follows the user's accent | `var(--border-active)` |
| Highlighted label | **opaque white in both appearances** | the text field's colour flips to `selectedMenuItemTextColor` | `var(--on-accent)` |
| Enabled label | `labelColor` | the live text field | `var(--text-primary)` |
| Disabled label | `tertiaryLabelColor` — 0.259 Light, 0.247 Dark | the text field on a disabled item | `var(--text-faint)` |
| Disabled row | takes no highlight | HIG: "doesn't respond to interactions" | `:not(:disabled)` on the highlight rule |
| Destructive **and** disabled | reads disabled, not red | a disabled item greys whatever its title colour | `:disabled` last, at equal specificity to `.destructive`, so source order decides it |
| Rows highlighted at once | **exactly one** | the single `NSRootMenuWindowBackgroundView` is re-framed onto the current row rather than one fill per row | the fill is keyed on `focusIdx` alone, which `mouseenter` moves, so the pointer and the keyboard cursor cannot light two rows |
| Separator row | **11pt**: 5pt, a **1pt** hairline, 5pt | the separator row's frame; its line layer at y=5, h=1 | `height: 1px; margin: 5px 11px` |
| Separator inset | **16pt** each side — aligned to the **label**, not to the highlight | that line layer at x=16, w=114 in a 146pt menu | the 11px margin + 4px padding + 1px border |
| Highlight animation | **none** | — | `transition: none`, stated so the global `button` rule cannot lend it 120ms |
| Destructive item | no `NSMenuItem` API exists; SwiftUI's `Button(role: .destructive)` is what the native client uses, and a red label goes **white** on the highlight | `ChangesSidebar.swift:276,289`, `SyncControls.swift:127`, `BranchMenu.swift:104`; an `attributedTitle` in `systemRed` measurably *stays* red on the fill, which is the trap to avoid | `.destructive` ordered ahead of the highlight rule, which outranks it |

**The surface is the one thing that cannot be reproduced.** The real menu is a
Liquid Glass window — an `NSVisualEffectView` on a private material, composited
by the WindowServer, so it cannot even be sampled — and `STYLE.md` rules
`backdrop-filter` out because these windows are opaque. The menu therefore takes
the app's standing elevated-surface treatment, the same one the branch picker
and every dialog wear: `--bg-elevated`, a 1px `--border-inactive` hairline, and
`--shadow-popover`. **That border is why the paddings are one pixel short of the
measured insets** — it stands in for the edge the glass gives the original, so
it is part of the 5px the highlight is inset by and part of the label's 16px.

**Five places the reproduction departs from the measurement on purpose**, each
because a standing rule outranks the literal rather than an approximation
slipping in — the first three are `STYLE.md`'s "token, never a hex":

- **The highlight is the flat accent**, not the glass composite. `#1769E6` is
  what the accent becomes *under* a material this client cannot have; without
  it, the accent itself is the honest reproduction, and `--border-active` is
  already what every accent fill in the app paints.
- **The separator is `--border-inactive`** (0.1 in both themes). Light matches
  the measured hairline almost exactly (0.102); Dark is lighter than AppKit's
  (0.141). One hairline token covers the whole app, and a menu-only second one
  would buy 0.04 of alpha.
- **The disabled label is `--text-faint`** (0.3 / 0.32) against AppKit's 0.259 /
  0.247 — a shade stronger, and the token `STYLE.md` already assigns to this
  register.
- **The hit area is the highlight.** AppKit's row is full-bleed and highlights
  from the menu's own edge inward; here the item box *is* the highlight, so the
  outer 5px is inert. The picture is identical; the hover target is 5px
  narrower.
- **The 200px floor outranks the 16px trailing inset.** An `NSMenu` hugs its
  widest title, so its trailing inset is 16pt and nothing more. Three of the
  four callers here have no label long enough to reach 200px — Header's two menus
  measure 174px and 184px, the commit list's 159px — so they sit on the floor
  and carry 32–118px of trailing space instead. The floor is what makes
  `Header.svelte`'s right-alignment land, so it wins; only the file list's menu
  is genuinely content-sized.

**One divergence is behaviour, so it stays.** The menu paints its first enabled
item highlighted the moment it opens, because `focusIdx` starts there — a real
`NSMenu` opens with nothing highlighted, and ↓ then takes the first item rather
than the second. An accent fill states it far louder than a 4% tint would, and
on the file list it means *Discard Changes…* is filled the moment the menu
appears. Dropping the paint alone would only hide the mismatch and leave ↓ still
skipping the first item, so the paint and the key belong to one change — a
behavioural one, which §1 puts outside this plan. **This is the thing to look at
first** when judging the reproduction, because it is the one place it does not
match.

The same index has a second consequence worth knowing before it is rediscovered:
`mouseenter` only moves it for an *enabled* item, so resting the pointer on a
greyed row — or on the menu's own padding — leaves the last enabled row lit
somewhere else, where an `NSMenu` would clear the highlight outright. Clearing
`focusIdx` there would take Return's target with it, so this is the same
behavioural change as the paragraph above rather than a second one.

**A menu item is still in the tab order**, so Tab reaches one and `app.css`'s
`button:focus-visible` draws a `--cursor-bg` halo an `NSMenu` has no counterpart
for. The halo is the only thing marking DOM focus, which `focusIdx` does not
track, so removing it would leave that state undrawn — the fix is focus
management, which is behaviour again.

**`min-width: 200px` is load-bearing across files** and the component says so:
`Header.svelte` anchors both of its menus at `rect.right - 200` so they hang
from the right edge of their chevron, and nothing else makes that land. A long
label ellipsizes instead of widening past the viewport, capped at the same 6px
margin the component's own clamp keeps.

**The two radii are in `STYLE.md`'s scale**, since a number this table measured
belongs in the design language rather than only in a plan; §7 item 5 records
where.

### 6.3 Two colour tokens, applied 24 times

Done. Two decisions, 24 sites, and both tokens are defined in **both** theme
blocks of [`app.css`](../../apps/tauri-app/src/app.css) — a token that exists in
only one theme falls back silently and is worse than the literal it replaced.

- **`--overlay-backdrop`** — `rgba(0,0,0,0.3)` light, `rgba(0,0,0,0.5)` dark,
  which is what `STYLE.md`'s *Modals / dialogs* specifies, at **14** modal and
  overlay backdrops.
  The flat `rgba(0, 0, 0, 0.4)` it replaces was wrong in both directions at
  once, so this is a visible change in both themes: the light scrim lifts, the
  dark one deepens. It is declared beside `--shadow-popover`, the other piece of
  modal chrome.
- **`--on-accent`** — `#FFFFFF` in both themes, at **10** sites, every one of
  them a `color` on a `background: var(--border-active)` fill. The same value in
  both blocks is the point rather than an oversight, and it is listed in both
  rather than hoisted to `:root` so the accent and the label that has to read on
  it stay together. `#FFFFFF` is what the native client renders: **no
  `.borderedProminent` site in `apps/swift-ui-app/` sets a label colour at all**
  — `Color.white` appears nowhere in the native sources — so AppKit paints the
  prominent label, and both accents (`#007AFF` light, `#0A84FF` dark) are dark
  enough that it resolves to white in either appearance. `STYLE.md`'s *Buttons*
  states the same rule in words.

**One white is deliberately not `--on-accent`.**
[`MainLayout.svelte:3057`](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L3057)
sets `color: #ffffff` over `background: var(--status-red)` — the terminal close
button's hover, a white glyph on a destructive fill. It is the client's only
white-on-status-fill, the native client has no counterpart to copy, and a token
named for the accent would name the wrong background, so it stays a literal.

**30 of 32 components use tokens exclusively.** The two that do not are
`Terminal.svelte` and `MainLayout.svelte`, and between them they hold only the
terminal: black (`#000000`), which `STYLE.md`'s *Terminal* explicitly allows; the
emulator's `#e5e5e5` foreground, which is the sRGB triple
`Screens/TerminalSessionView.swift:369` hands SwiftTerm, so both clients paint
the same grey on purpose.

### 6.4 Per-component conformance

Done. Each item and where it landed:

- **Focus ring — a checkbox taking an `outline` rather than the shared halo is
  not a defect.** The shared ring is a `0 0 0 2px var(--cursor-bg)` halo, which
  on a field sits around a border swapped to `var(--border-active)`. A checkbox
  can wear neither half.
  WebKitGTK erases the shadow: `RenderTheme::adjustStyle` runs
  `if (!supportsBoxShadow(style)) style.setBoxShadow(None)`, and the base
  `supportsBoxShadow` returns `false` for every themed control — `RenderThemeIOS`
  is its only override in the tree, so Adwaita inherits the `false`. A text field
  escapes it because its author border and background take it off the native
  path first, which a checkbox cannot be. And the halo alone is not a ring: over
  the selected row's `--bg-tertiary` it measures **1.24:1**, under WCAG 1.4.11's
  3:1, because elsewhere it is the solid border beside it that carries the
  contrast. An outline is exempt from the style adjuster, paints over the themed
  control, and takes the accent at full strength — the only ring these controls
  can take. `STYLE.md`'s *Spacing, radii, focus* states both shapes; §6.5 is
  where the checkbox one became a single app-wide rule.
- **Letter-spacing.** Gone from `.shell-name` in `MainLayout.svelte`. The
  client's only remaining declarations are `Header.svelte`'s two.
- **Headings.** The error-screen `h1` in `App.svelte` is **15 px**; the commit
  summary in `CommitDetail.svelte` is **13 px** and still semibold; the
  dirty-submodule title in `MainLayout.svelte` was already 15 px and is
  untouched. Three registers rather than one, which is what native draws — a
  sheet or failure title (`.title3.semibold`), a detail-pane heading
  (`.headline`), a system-sized pane empty state — and `STYLE.md`'s *Section
  headers* carries all three.
- **Dialog titles — one rule, thirteen dialogs.**
  [`app.css`](../../apps/tauri-app/src/app.css) holds
  `.modal-header h2 { font-size: 15px; font-weight: 600; color: var(--text-primary) }`
  once, and no component restates size or weight. Svelte emits a scoped rule as
  `.modal-header.svelte-hash h2:where(.svelte-hash)` — 0-2-1 against the
  unscoped rule's 0-1-1 — so `ForcePushConfirm`, `DiscardConfirm` and
  `ErrorModal` win on specificity while restating `color` alone, as does
  `ConfirmDialog`'s `.destructive` variant. `RepoPicker` is the thirteenth: its
  class sat on the `<h2>` itself, outside a descendant selector's reach, so its
  header is now a wrapper around the `<h2>` like every other dialog's.
- **Pane-level empty states.** `.diff-empty` (`MainLayout.svelte`) and
  `.empty-state` (`DiffViewer.svelte`) are **15 px**. The register belongs to
  the title, so `.diff-empty .show-anyway` is pinned to 13 px against the global
  `button { font-size: inherit }`, and the `.muted` and `.detail` sub-lines
  already set their own 12 / 11 px. A *list's* empty line is the other register
  and stays where it is — `FileList`, `CommitList`, `BranchDropdown`,
  `RepoListEmptyState`. So does `DiffViewer`'s `.binary-state`, which stands in
  for content that exists rather than announcing a pane with none.
- **Shadows are all one token.** `.terminal-link-hint` in `Terminal.svelte`
  takes `var(--shadow-popover)` like every other floating surface — it is a hint
  over the terminal, the same class of surface as `RepoTooltip`. The token is
  theme-aware, so the hint casts a light shadow in Light and a heavy one in
  Dark; no component holds a literal shadow.

**Not a defect:** `font-weight: 700` on the status plate in `FileList.svelte`.
It matches the native badge exactly (§2), and `STYLE.md`'s *Typography* names
the status plate as the one exception to *avoid bold*.

**Resolved by ✅ P-2 instead:** the `letter-spacing` in `Header.svelte` styled
two all-caps badges, `DETACHED HEAD` and `MERGING`, which the header no longer
draws. Both states now render the way the native chip renders them — inside the
branch control's own label, as `Detached at <sha>` and as a purple `· merging`
suffix (`BranchMenu.swift:160`, `:170`) — so there is no all-caps run left in
the bar and no tracking to justify.

### 6.5 Settings form controls

Done, in [`app.css`](../../apps/tauri-app/src/app.css) rather than in the four
components that draw one of these controls. Two of the three results below are
the opposite of what the platform notes predicted, and each says so with the
source that settled it.

**Every checkbox in the app now has a focus ring, and one rule draws it.** Four
of the six had none. One `:focus-visible` rule on
`input[type='checkbox'], input[type='radio']` sets
`outline: 2px solid var(--border-active)` with `outline-offset: 2px` — the
shape `STYLE.md`'s *Spacing, radii, focus* names — and no component restates
it. Six controls take it: `FileList`'s two, Settings' three
([`SettingsOverlay.svelte:238`](../../apps/tauri-app/src/lib/views/SettingsOverlay.svelte#L238),
`:267`, `:277`), and `PublishRepository.svelte:86`. The file list's two arrived
with this ring already — `FileList` stated it locally, which is the "not a
defect" §6.4 records — and the other four had `input:focus`'s `outline: none`
reaching them instead, which left **nothing** on WebKitGTK — WebKit adds the
focused state only while
`outlineStyle() == OutlineStyle::Auto`, and `RenderThemeAdwaita::supportsFocusRing`
returns true for `Checkbox` and `Radio`, so Adwaita was willing to draw one and
was never asked — and on Chromium the `--cursor-bg` halo alone, which measures
**1.24:1** over the file list's selected row, under WCAG 1.4.11's 3:1. The two
engines disagree about the halo and the disagreement is why the answer is an
outline: Blink paints an author `box-shadow` on a themed checkbox (its shadow
pass has no appearance check) and WebKit deletes it (`RenderTheme::adjustStyle`
runs `if (!supportsBoxShadow(style)) style.setBoxShadow(None)`, and neither the
base nor `RenderThemeAdwaita` returns true). An outline is the only ring both
draw.

The narrowing that stops `outline: none` reaching a checkbox is written
`input:where(:not([type='checkbox']):not([type='radio']))`, and the `:where()`
is load-bearing. Without it the selector takes its arguments' specificity and
lands at (0,2,1) — heavier than a **single-class** component rule, which Svelte
emits as a bare `.class.svelte-hash` at (0,2,0) rather than with the `:where()`
it gives a descendant (§6.4's dialog-title bullet is the descendant case, where
the component wins at 0-2-1). At (0,2,1) an app-wide default silently outranks
every field that states its own padding — the Settings number fields' room for
their steppers among them. `:where()` contributes nothing, so the
rule keeps the weight a bare `input` had and only its reach changes.

**`accent-color` is what colours these controls, and the Chromium trap this
step went looking for does not exist.** The 2020 Form Controls Refresh reads as
though an author `background`/`border` would devolve a checkbox on Blink and
leave `accent-color` inert on the wreckage. Source says otherwise:
`LayoutTheme::IsControlStyled` — read at `branch-heads/7977`, the M152 branch
WebView2 tracks — switches on button, progress, meter, menulist, search,
textarea and textfield, and `kCheckbox`/`kRadio` fall to `default: return
false`. WebKit's `RenderTheme::isControlStyled` at `webkitgtk-2.52.3` does the
same. **Neither engine devolves a checkbox**; both discard the declarations
instead (Blink's `AdjustCheckboxStyle` runs `ResetPadding` and `ResetBorder`,
which clears the radii too, and the background paint is skipped wherever the
theme painted). So `accent-color` was never at risk — and it is now stated
once, with the 14×14 size and the pointer, instead of in three components.
WebKitGTK carries it stable and default-on (`AccentColorEnabled` is true under
`USE(THEME_ADWAITA)`), which is half of the old verification 3 answered from
source rather than on target.

The narrowing stays on the field rule as well as the focus rule, even though
only the focus rule needs it: it keeps four declarations out of a control that
silently discards them, and writing the same selector in both places makes one
idea cover one set of controls rather than two sets that have to be kept in
agreement.

**The `<select>`s take `appearance: none`, and `base-select` is declined.**
Gating a styled picker on `@supports (appearance: base-select)` would have been
a Windows-only fork of a control, and three things say not to:

- **The popup is the platform's on both, and that is what the reference does.**
  The native client shows a stock `Picker`, i.e. the system popup; drawing our
  own on Windows would make the Tauri client the only one of the three with a
  dropdown of its own. It is also not available on Linux at any price —
  WebKitGTK renders the popup as a real `GtkTreeView` in a `GtkPopover` and
  consumes none of the `<option>` styles WebCore computes
  ([webkit.org/b/9846](https://bugs.webkit.org/show_bug.cgi?id=9846), open since
  2006). Chromium's popup **is** an internal HTML document and would style, so
  the fork would be exactly one platform wide.
- **The gate is temporary.** `base-select` is stable in Chromium from **135**
  (2025-04-01) and present in WebView2's channel; WebKit implemented it
  2026-02-03 and enabled it 2026-02-14, but the `webkitglib/2.52` branch was cut
  ~2026-01-20, two weeks early. It is already on `webkitglib/2.54`. So the
  one-sided gate is a fork with a known expiry, and building it now buys a
  Windows-only presentation that has to be maintained until Linux catches up and
  then reconsidered anyway.
- **The closed control was the real problem, on Linux, today.** Author
  background and border without `appearance: none` do not leave a select native:
  both engines devolve it to a *styled menulist* (Blink returns
  `kMenulistButton`; WebKit routes through `nativeAppearanceDisabled()`), which
  is the worst of the three states. The engine keeps painting its own arrow, so
  Windows and Linux showed different glyphs; it keeps reserving its own arrow
  padding; `RenderThemeAdwaita::adjustMenuListStyle` calls
  `style.setLineHeight(initialLineHeight())` unconditionally, so on Linux the
  control sat **shorter than the fields stacked with it**; and, still counting
  as a themed control, it lost the focus halo to the same `supportsBoxShadow`
  reset as the checkboxes.

`appearance: none` ends all four at once — `RenderTheme::adjustStyle`
early-returns and no control part is created — so the select now wears the same
register and the same focus ring as the fields beside it, on both engines. The
chevron `appearance: none` removes is drawn back once, as a `::after` on a
`.select-field` wrapper (a pseudo-element cannot hang off the `<select>`
itself), from two borders in `--text-secondary` — one theme-aware token, so no
asset has to be themed — and it is the same 90° V the header draws for "this
opens something", at the affordance weight rather than the label's. All four
selects wear it: `SettingsOverlay.svelte:223`, `:340`, `:360` and
[`CommitMessage.svelte:522`](../../apps/tauri-app/src/lib/components/CommitMessage.svelte#L522),
the last of which stays button-filled because it sits in the composer's button
bar rather than in a form column.

**The control *shapes* `STYLE.md` names are not part of this plan, and that is
§1's rule rather than an omission.** *Forms (Settings)* wants a `Toggle` for a
boolean and a segmented control for 2–4 exclusive options, which the native form
gets from `Toggle` and `Picker`. Reaching them here means replacing a checkbox
and three of the four selects with different controls — new markup, keyboard
and AT semantics — and §1 puts behaviour out of scope: this plan changes what a
control looks like, never what it is. The three binary selects (theme,
and the AI provider in both places) are the candidates, `DiffViewer.svelte`
already draws the segmented pattern the replacement would reuse, and `ROADMAP.md`
carries it as its own item.

### 6.6 Window chrome — decline

Change nothing. `titleBarStyle` and `trafficLightPosition` are macOS-only and
inert; materials are Windows-only with no Linux equivalent at all. The window
frame is host-owned chrome under §1's exception, so it stays as the platform
draws it. Recorded here so it is not re-opened.

## 7. `STYLE.md` amendments

All five are applied. `STYLE.md` now states the language for both clients, so
every target in §6 is the document's own rule rather than this plan's reading of
it:

1. **Context menus — `STYLE.md`'s *Context menus*.** "The native client takes the system menu;
   the Tauri client draws its own to the same metrics," because on Windows and
   Linux the stock menu is a win32 `HMENU` no theming API reaches. *Disable,
   don't hide* and the ordering rule govern both clients unchanged.
2. **Typography — `STYLE.md`'s *Typography*.** "Avoid bold (700+)" now carries exactly one
   exception, the **single-letter status plate** at 10 px mono 700, which is
   what the native `FileStatusBadge` sets. Nothing else takes bold.
3. **Scrollbars — `STYLE.md`'s *Spacing, radii, focus***, beside "the window
   never scrolls", where an app-wide rule belongs. Three bullets: the `*` rule
   (`scrollbar-width: thin` + `scrollbar-color`) is the whole definition, with
   `::-webkit-scrollbar` banned and the reason it is banned stated as structural
   rather than tidiness; overlay behaviour is the engine's to give, not CSS's,
   with overlay named as the target on every platform and Windows' config key
   named as where it is asked for; and the terminal is recorded as the one
   surface the rule cannot reach, because xterm draws its scrollbar in JS.
4. **Section headers — `STYLE.md`'s *Section headers*.** Three registers: **15 px semibold**
   for sheet, dialog and failure titles; **13 px semibold** for detail-pane
   headings and Settings section labels; **system-sized** for pane-level empty
   states.
5. **Radius scale — `STYLE.md`'s *Spacing, radii, focus***. The scale carries
   the context menu's pair — **12 px** at the menu, **7 px** on its items — and
   states that both are a measurement of a macOS 26 `NSMenu` rather than a
   choice, pointing at §6.2's table for the rest. Dropdown items sit at 6 px
   beside the inputs and buttons, which is what the branch picker's rows are,
   and the terminal pane carries no radius at all.

## 8. On-target verifications

**The design question is settled: where the native client and this document
disagree, the native client wins** — `STYLE.md`'s *the native client is the
reference* note, applied to
every choice in §6 and §7.

What remains is not preference but three questions about what the two engines
actually render, which no decision can supply and only a build on target can
settle. Each is recorded with the outcome the rule above asks for, so the test
has a target rather than only a question. Where an engine cannot reach it, the answer is to
record the divergence in `FRONTEND.md` §8 — never to redesign both platforms
down to the weaker one.

A question that source can answer is not on this list. "Does `scrollbar-width`
defeat WebKitGTK's overlay scrollbars?" was, and the answer read out of the
tagged 2.52.3/2.52.6 sources is **no, and it is the property preventing that
outcome** — §4 has the predicate and the two call sites. There is nothing left
to discover about the mechanism; what is below is what a person still has to
look at.

1. **Does the Linux build actually draw a thin, neutral, floating scroller?**
   *Target:* the thumb overlays the content rather than displacing it, and
   carries `--border-strong` in both themes at GTK's own width. Two things
   upstream source cannot settle: whether Debian or Ubuntu carry a distro patch
   over the scrollbar code, and whether the host's desktop has overlay scrolling
   switched off, which is a user setting the app does not override. Both show up
   the same way — a scroller that takes a column. Neither is a reason to change
   the CSS. A *different* symptom, a thumb in the GTK theme's colour rather than
   the app's grey, means the Adwaita path lost (§4) and is worth reporting
   against the wry version rather than adjusting here.
2. **Does `"scrollBarStyle": "fluentOverlay"` reach the target machine, and does
   it read like the reference?** *Target:* Windows matches Linux. That the key is
   spelled right and accepted by this Tauri version is settled at build time
   (§6.1), so what is left is the runtime and the look: confirm the machine's
   WebView2 Runtime clears **125.0.2535.41** — below it the key is silently
   ignored and Windows keeps classic Fluent scrollbars — and judge the Fluent
   overlay scroller beside the macOS one. If the runtime is too old, that is the
   host's to update, not a reason to redesign; the reserved gutter in the file
   list is already the fallback that keeps that case laying out correctly.
3. **Do the form controls read as one register on both engines?** *Target:* a
   checkbox is an accent-filled native control with a visible ring around it
   when it is tabbed to, and a `<select>` is the same height, the same fill and
   the same chevron as the fields it is stacked with. Support is not the
   question — §6.5 read `AccentColorEnabled` and `appearance` straight out of
   `webkitgtk-2.52.3` and the M152 branch — so what is left is what the two
   engines make of them. Four things to look at, in the order they would go
   wrong: the ring's contrast against the row fill on GTK, which is Adwaita's
   accent rendering rather than a token; whether the Settings selects now line
   up with the number fields on Linux, which is the `line-height` clobber
   `appearance: none` was adopted to end; whether any stray arrow survives
   beside the drawn chevron, which would mean `appearance: none` did not take;
   and `text-align` on the closed select, the one property whose GTK behaviour
   source did not settle. A second arrow or a misaligned row is a bug in this
   CSS; an accent hue that is Adwaita's rather than `--border-active` is the
   engine's and belongs in `FRONTEND.md` §8.

All three are answered by building the Tauri client on a Windows and a Linux
target and looking.

`appearance: base-select` is deliberately absent from this list. It is a real
gap on Linux — the keyword missed the `webkitglib/2.52` branch by two weeks and
is already on `2.54` — but §6.5 declines to gate on it, so there is nothing
about it for a build to show. It is worth revisiting when the ship target moves
to 2.54, at which point it stops being a Windows-only fork.

## 9. Execution order

Ordered so each step is verifiable on its own and nothing depends on an
unanswered question:

1. ~~**§7** — the `STYLE.md` amendments.~~ **Done** — the document and the
   reference agree; §7 records where each rule landed. Step 5 added the fifth,
   which is where a measured number and the written scale first disagreed.
2. ~~**§6.3** — the two tokens.~~ **Done** — `--overlay-backdrop` and
   `--on-accent` are in both theme blocks and applied at 24 sites; §6.3 records
   the values, the counts and the one white that is not accent-on-fill.
3. ~~**§6.4** — the conformance fixes, headings included.~~ **Done** — §6.4
   records each item's resolution, the one shared rule all thirteen dialog
   titles take their size from, and the one item that turned out not to be a
   defect. It also hands §6.5 a focus-ring gap in the checkboxes it owns.
4. ~~**§6.1** — the scrollbar rule and the window's `scrollBarStyle`.~~ **Done** —
   §6.1 records the single `*` rule, why `scrollbar-width` stays despite Adwaita
   ignoring `thin`, the build-time proof that `"scrollBarStyle": "fluentOverlay"`
   is a real key in this Tauri version, and why the file list keeps its gutter.
   It also hands §4 the WebKit predicate that answered the old verification 1
   from source, and records the terminal as the one surface the app-wide rule
   cannot reach. `FRONTEND.md` §8 carries both divergences; §8's remaining two
   scrollbar checks are what a build on target still has to show.
5. ~~**§6.2** — restyle `ContextMenu.svelte` to AppKit metrics.~~ **Done** —
   §6.2 records the reproduction and tabulates every metric against the live
   `NSMenu` it was read from, which is the only place in the repo those numbers
   exist. It also names the five places a standing rule outranks the
   measurement, the surface the reproduction cannot have, and the three
   divergences that are behaviour rather than paint and so wait for a
   behavioural change. §7 carries the radius-scale correction it turned up.
6. ~~**§6.5** — Settings controls.~~ **Done** — §6.5 records one app-wide
   checkbox shape and focus ring where three components held pieces of one, the
   `accent-color` trap that source says is not a trap, and why the `<select>`s
   take `appearance: none` and a drawn chevron rather than the `@supports
   (appearance: base-select)` gate this section originally called for. It hands
   §8 a replacement for verification 3 — support was answerable from source, the
   rendering is not — and `ROADMAP.md` the one thing it deliberately did not do:
   the `Toggle` and segmented-control *shapes*, which replace controls rather
   than re-skin them.

§6.6 is a decision, not a step.

## 10. Parity audit — what the side-by-side actually shows

§6 closed the chrome layer it was written against. This section is what the two
clients look like when they are put beside each other on one repository anyway,
read from the code rather than from the screenshot: every entry names the
mechanism and the file that causes it.

The short answer the audit gave was **no, they do not read as one product**, and
the reasons were largely not the ones §6 was about. Several of the loudest were
one declaration each. One is a decision this plan cannot make. **Two of the top
five belong to the native client**, which is the case §8's rule did not
anticipate: "the native client wins" answers *which look is correct*, not
*which client is wrong*.

Entries marked **✅ Landed** describe the code as it stands and why it is that
way; the rest are still findings. The app's leading (P-2) and the diff pane's
body geometry (P-11 – P-15) have landed together, which was the largest single
gain available without a decision from §10.7.

Read on macOS, so anything engine-specific inherits §8's limit. Items marked
**⚠ on-target** cannot be settled without a Windows or a Linux build.

### 10.1 The two that do most of the damage

**P-1 — the Tauri header does not span the window. ✅ Landed.** Natively the repo
chip sits at the window's leading edge, immediately after the traffic lights,
because the toolbar is the window's and spans the whole frame
([`LeoGitApp.swift:47`](../../apps/swift-ui-app/Sources/LeoGit/App/LeoGitApp.swift#L47)
`.windowToolbarStyle(.unified)`, split at `ContentView.swift:250`). The Tauri
header was a child of `.main-layout`'s **third column**, so the chips began
where the sidebar ended — roughly 320 px in — and `TabBar.svelte`'s hardcoded
`height: 40px` was the tell: the tab strip was standing in for the part of the
header that was missing.

`.main-layout` now carries `grid-template-rows: auto 1fr` and the header is its
first row, spanning all three tracks; the sidebar, the resize handle and the
detail pane auto-place into row 2. The composer's height cap needed no change —
it measures `.tab-panes` with `bind:clientHeight` rather than deriving from
`100vh` — and the overlays are `position: fixed`, so neither followed the header
up. It also settles the second consequence: the bar no longer changes span
between the pre-main phases and the main view, which is what `STYLE.md`'s
*chrome does not move* rule asks.

What is **not** matched is total chrome height, and that is `FRONTEND.md` §8's
to carry rather than this section's: the native toolbar is also the title bar,
Tauri's sits below whatever title bar the host draws, and closing that gap means
`decorations: false` plus hand-drawn window controls on two platforms — which
§6.6 declines. The error banner stays in the detail column, where it names a
failure the detail pane is showing the consequences of.

**P-2 — the app's leading. ✅ Landed.** `:root` now sets `line-height: normal`
at [`app.css`](../../apps/tauri-app/src/app.css), and STYLE.md's *Typography*
carries the rule. It was `1.5`, one declaration, and it was the whole of
"the Tauri text looks bigger". It is not the font and not the size: measured in
a real `WKWebView` against AppKit, SF Mono 12 pt advance is **7.41796875 in
both**, and `-apple-system` and `Font.system` resolve to the same `.SF NS` file
through the same `CTFontCreateUIFontForLanguage` entry point, at the same
clamped `opsz` of 17 and the same `trak` tracking of −12/2048 em at 13.
Families match, sizes match, advances match. Only the leading differs:

| Nominal | Native | `line-height: normal` | `line-height: 1.5` (was) |
|---|---|---|---|
| UI 13 | 16.0 | 16 | 19 (+19 %) |
| Diff mono 12 | 15.0 | 15 | 18 (+20 %) |
| Gutter mono 11 | 13.0 | 13 | 16.5 |

`normal` is exact because WebKit computes it as `lround(ascent) +
lround(descent) + lround(lineGap)` over the same font metrics AppKit reads
(1980 / −432 / 0 at 2048 upem). Two components already knew the target and say
so — `CommitList.svelte` pins a 16 px badge and `ContextMenu.svelte` reasons
about "AppKit's own box is 16pt" while deliberately inheriting. The root
declaration had been contradicting work this plan already did.

**`normal`, not a fixed ratio**, and the reason is Linux rather than macOS.
Noto Sans carries an ascent-plus-descent ratio of ~1.362 against SF's 1.178, so
any ratio tuned to SF clips it — which is the exact defect a verification pass
caught in §6.2 when `line-height: 16px` was tried. Leading is a property of the
font the host gave you, not of the design, so §1's *one look* rule does not
reach it: `normal` is what AppKit does, and it is the only value that cannot
clip a face we do not choose. Most local `line-height` declarations that remain
are wrapped prose or a pinned box, which is what `STYLE.md`'s rule allows; two
are ratios on single-line labels (`Terminal.svelte`'s hint,
`CommitDetail.svelte`'s mono metadata row) and are the rule's only outstanding
violations.

**⚠ on-target, and found while proving the above:** the chrome stack is
`-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", system-ui,
sans-serif`. On macOS the first entry wins and the next three are dead —
`'SF Pro Text'` names no installed family, because the file's family name is
`.SF NS`, hidden behind its leading dot. **On Linux the first three are all
no-ops and `"Helvetica Neue"` is reached before `system-ui`**, where fontconfig
commonly aliases Helvetica to Nimbus Sans. If that alias is live, the Linux
build has been rendering its entire UI in Nimbus rather than the system face,
which would dwarf every other item here. A Linux build settles it in one look.
The same applies to `'SF Mono'` in the mono stack: only `ui-monospace` reaches
the real face, and it is already first.

### 10.2 Where the native client is the one that drifted

`STYLE.md`'s rule is that the native client wins **a disagreement about what the
look should be**. It is not a claim that the native client is correct — it
reaches the aesthetic by using stock controls, so where it takes a stock
control's *default* that nothing chose, the default is not a target.

**P-3 — the file and commit lists paint phantom alternating rows.**
`.alternatingRowBackgrounds()` at
[`ChangedFileList.swift:114`](../../apps/swift-ui-app/Sources/LeoGit/Design/ChangedFileList.swift#L114)
and [`HistorySidebar.swift:109`](../../apps/swift-ui-app/Sources/LeoGit/Screens/HistorySidebar.swift#L109).
It maps to `NSTableView`'s striping, whose drawing hook is
`drawBackground(inClipRect:)` — it paints the **clip rect**, not the occupied
rows, so a two-file repository gets a column of empty rounded plates running
down to the composer. Under `.listStyle(.inset)` each stripe takes the inset
style's rounded geometry, which is why they read as placeholder rows.

There is no API to scope it: Apple's own doc says the fill is painted "on top of
the overall list or table background", and `listRowBackground` — the only
per-row override — by construction cannot reach space with no row in it. The
AppKit remedy is subclassing `NSTableView`.

**✅ Landed, in the other direction.** This section had proposed
`.alternatingRowBackgrounds(.disabled)` — treating the striping as a scaffold
default nothing chose. The call went the other way: the reference ships
striping, so the Tauri list stripes too, and the native line stays.

The Tauri implementation is the better half of the pair, which is the part worth
keeping in mind if the native side is ever revisited. It stripes the **row
element**, keyed on the file's index in the model, so it is structurally
incapable of painting the phantom plates below the last row that
`drawBackground(inClipRect:)` produces. Keying on the model index rather than
DOM position is not a preference either — these rows are virtualized and
absolutely positioned, so `:nth-child` sees only the handful near the viewport
and would restripe the list on every scroll. The stripe sits at about half
`--surface-hover` rather than at AppKit's measured ~4.7 % white, because this
list has a hover state to stay distinguishable from and the native one does not.

If the native side is ever brought level, `listRowBackground` keyed on index
parity is the supported route, and its inability to paint empty space is the
feature.

**P-4 — the repo chip is named from a different source in each client, and the
native client disagrees with itself.** The chip reads `Tesis` natively and
`Tesis-docs` in Tauri for one repository.
[`RepoSwitcher.swift:37`](../../apps/swift-ui-app/Sources/LeoGit/Screens/RepoSwitcher.swift#L37)
takes `RepoDirectoryStore.displayName`, a plain `lastPathComponent`;
[`Header.svelte:81-86`](../../apps/tauri-app/src/lib/components/Header.svelte#L81-L86)
takes `repoIdentifier?.name ?? basename(path)` off core's `get_repo_identifier`.
`STYLE.md`'s *Repository switcher* and `DESIGN.md` both say a repository is named
by its remote's repository name where it has one — so Tauri is compliant.

The native client already owns the compliant answer: `RepoIdentifierStore.label(of:)`
exists and its **own picker rows use it** (`RepoPickerList.swift:209`, `:292`).
So opening the switcher today shows a popover row reading `Tesis-docs` under a
chip reading `Tesis`. The switcher already receives `identifiers`; it forwards
them to the list without consulting them. Small.

This also resolves the duplicate-name half of the title-strip question: the
Tauri window title is `basename(path)` (`App.svelte:224`) while the chip below is
the remote name, so one window states two names for one repository.

**P-5 — the native client paints no pane tint at all.** Neither column takes a
`.background(...)`; the only surface fills in the whole Screens+Design tree are
the terminal bar's `.bar`, the sync banner's `.regularMaterial` and the
terminal's black. The lists use `.listStyle(.inset)`, not `.sidebar`, so there
is no vibrancy either. The Tauri sidebar is `--bg-secondary`
(`MainLayout.svelte:2665`), which is what `STYLE.md`'s token table specifies.
Two-tone versus flat is very visible. Decision in §10.7.

**P-6 — gutter numbers.** Native is `.quaternary`
(`DiffLineRow.swift:156`), roughly 0.10–0.25 alpha; Tauri is `--text-muted` at
0.45/0.48, which is what `STYLE.md`'s *Diff viewer* asks for. Tauri is the
compliant one; `.secondary` is the SwiftUI level nearest the token.

**P-7 — the summary field's bezel.** The native composer stacks a square-bezelled
`NSTextField` (`WheelScrollableTextField.swift:23`) directly above a 6 pt
`RoundedRectangle` description editor (`CommitComposer.swift:300`) — it
disagrees with itself, and `STYLE.md` specifies the rounded form. `.roundedBezel`
is not a drop-in; it changes height and inset.

### 10.3 The diff pane

**P-8 — the header consumes a different string in each client.** Native renders
`FileEntry.display_name`, which core derives from `git status --porcelain=2 -z`
(`git.rs`) — `-z` output is never quoted. Tauri renders `fileDiff.new_path`
([`DiffViewer.svelte`](../../apps/tauri-app/src/lib/components/DiffViewer.svelte)),
parsed out of the patch's `+++` line.

**The string itself is now correct in both.** The defect underneath this was not
cosmetic and was not this plan's — it destroyed syntax highlighting in *both*
clients for any non-ASCII path — and it is fixed in core as **D-25** in
[`cross-client-feature-parity.md`](cross-client-feature-parity.md) §3.1, so the
pane no longer shows `"b/01 Plan Trab Investigaci\303\263n.md"`.

What is left is the parity half, and it is unchanged by that fix: the header
still renders a whole path where the native header renders a **filename** with
the directory as a caption under it. `DiffViewer` should take the `FileEntry`
the file list already has — which carries `display_name`, `display_dir` and the
status — rather than scraping the patch header for a path and deriving the rest.
That one prop closes P-8, P-9 and P-10 together, and none of them closes
convincingly alone.

**P-9 — no status plate in the Tauri diff header.** Native leads with the 18×18
tinted letter (`DiffView.swift:205`). `DiffViewer`'s props carry only
`origPath` (`DiffViewer.svelte:12-40`), so it cannot draw one. The plate already
exists at pixel parity in `FileList.svelte:642` — this is an extraction and a
prop, not new design, and it lands with P-8.

**P-10 — the header's type register is wrong.** Native is a 13 pt semibold sans
title over a muted caption carrying the directory or the rename pair
(`DiffView.swift:206-210`). Tauri is one 12 px **mono** line
(`DiffViewer.svelte:469`). This violates `STYLE.md` twice: its 13 px semibold
register names the diff-header filename explicitly, and its *Diff viewer*
section requires a rename to read `from → to` **under** its own filename —
Tauri renders the destination alone as the title, which is the failure that
bullet exists to prevent.

**P-11 — the header strip. ✅ Landed.** It fills `--bg-primary` with a bottom
hairline, which is the native shape: fill nothing, take a `Divider()`.
`STYLE.md` assigns `--bg-primary` to the diff canvas and does not list this
header among `--bg-secondary`'s uses. The dead `position: sticky` went with it —
the header is a sibling of the scroll container, not a child, so it never stuck
to anything.

**P-12 — the gutter's vertical rules. ✅ Landed.** `.line-number` no longer
carries a `border-right`, and STYLE.md's *Diff viewer* now states the rule
positively: the pane draws no vertical rules, and its only divider is the
side-by-side separator. A full-height hairline beside each number column was
what turned a page of code into a table, and after the header it was the most
visible body-chrome difference.

**P-13 — the number column's width. ✅ Landed.** Each column is now 44 px — 40
of right-aligned digits and a 4 px gap — which is `DiffLineCell`'s own
`.frame(width: 40).padding(.trailing, 4)`. It was `width: 3em`, 33 px at 11 px,
leaving a ~20 px content box after padding and the rule: about three glyphs.
With `flex-shrink: 0` and `justify-content: flex-end` a four-digit number spilled
**left**, out of its box, instead of clipping, so any file over 999 lines
overlapped its own digits. 40 px is chosen for the pointer rather than the text —
the gutter is the line handle — and five digits fit inside it.

**P-14 — context lines. ✅ Landed.** The `--text-secondary` dimming is gone and
context takes full strength, which is what native does (no `foregroundStyle` at
all) and what `STYLE.md` now says. On prose, where most tokens map to no syntax
class and inherit, the dimming reached nearly every line.

**P-15 — small metrics. ✅ Landed, except the blank line.** The hunk band is
`DiffHunkBand`'s box: 11 px with `padding: 5px 12px` and **no borders**. All
three go together — 11 px alone left the band 8 px *shorter* than the native one
with its text flush against two hairlines native does not draw. The size is also
what keeps the `@@` row on one line: git puts the enclosing heading there, so on
prose it carries a whole sentence, and at 12 px it wrapped. The prefix column is 16 px at regular weight
against the old 18 px at 500 — it is chrome, and native gives it
`.frame(width: 16)` with no weight. `.line-content` has lost the 8 px of left
padding native does not have; it was compensating for P-13's narrower gutter, and
with the gutter at its real width it only pushed the body right. Rows carry
`padding: 1px 0`, unified and split alike, since natively both arrangements are
the same cell.

The band's colours stay documented rather than native — `--text-muted` against
`.tertiary`, `--bg-secondary` against a translucent `.quaternary.opacity(0.5)` —
because `STYLE.md`'s *Diff viewer* names both tokens, so Tauri is the compliant
one and this is P-6's pattern. Its box is native's, because nothing documents a
box.

**Still open in this pane:** `STYLE.md` asks for a blank line before each hunk,
which **neither** client draws. `DiffView.swift` puts `.padding(.vertical, 4)`
on the rows' stack with no counterpart on `.diff-body`. And `NoNewline` maps to
`diff-context` (`DiffViewer.svelte`), so the `\ No newline at end of file`
marker renders as a full row with a 104 px gutter, where native draws a bare
11 pt line with 12 pt of side padding and no gutter at all.

### 10.4 Chrome and controls

**P-16 — the chips are one capsule natively and two boxes in Tauri.** Adjacent
`ToolbarItem`s with no spacer share one glass background, which
`ContentView.swift:342-347` states outright and `BranchMenu.swift:75` supports by
hiding the menu indicator so the pair reads as one family. Tauri draws two
`.chip-button`s with a 12 px gap. **The capsule half has landed** — every
control in the bar now takes `--toolbar-radius`, half its own height, which is
the macOS 26 toolbar shape and the one radius exception `STYLE.md` grants. What
is still two things where the native is one is the **shared background**: the
pair needs a single grouped surface with the 12 px gap closed, and
`.split-button` is the pattern to copy for it.

**P-17 — the terminal dock is the surface §6 skipped.** Height 26 + 1 against
28 + 1, where `STYLE.md` says 28. `--bg-secondary` where `STYLE.md` assigns
`--bg-tertiary` and native takes the `.bar` material. The expanded chevron is a
minus rather than `chevron.down`. The icon is a bare `>_` where the native
`apple.terminal` mark carries an enclosing rounded-rect frame — one `<rect rx="3">`
apart. The label is a plain button where native's is a `Toggle` carrying the
panel's state. And the label is hand-styled to 10 px `--text-muted`
(`MainLayout.svelte:3006`) where native sets no font and no foreground and takes
the accessory bar's own typography — which `STYLE.md` forbids in as many words,
and is why the two bars read differently more than the icon is.

**P-18 — the commit button says a different thing. ✅ Landed.** The label is
`CommitComposer.swift:314-323`'s ladder — `Commit` / `Commit 1 File` /
`Commit N Files`, and `Amend Commit` / `Amending…` while amending — derived from
the same included count `handleCommit` resolves, so the face cannot advertise a
count the write then drops. The disabled paint followed: a disabled Commit takes
`--bg-tertiary` with a muted label instead of a half-strength accent, because an
accent fill at any opacity reads as a button that can be pressed. The *gates*
were already equivalent and are untouched. A spinner joined the row rather than
a "Committing…" string, which is what the native does — it keeps the count label
and shows `ProgressView().controlSize(.small)`.

**P-19 — the include-all row reads as another file row. ✅ Landed.** Native is
`.caption` (10 pt regular, `.secondary`) with its own 12×6 padding and a
full-bleed `Divider()`; Tauri was 12 px / weight 500 reusing `.file-row`'s box.
It now takes the caption register and its own box, and the rule under it is
full-bleed and flush rather than inset and floated — an inset rule reads as an
underline belonging to the text above it rather than as a boundary between two
regions of the pane. `STYLE.md`'s 11 px was the wrong target for it and now says
so: 10 px is macOS's Caption 1, and anything the native sets in `.caption` takes
it. Tauri's row-wide click target stays — an extra affordance, not a defect.

**P-20 — the split behaves differently even at equal defaults.** Both start at
320. But `HSplitView` gives extra window width to the sidebar
(`ContentView.swift:255`) while `grid-template-columns: var(--sidebar-width) 1px 1fr`
gives all of it to the right column, so the two agree at exactly one window
width. Tauri also persists `leogit:sidebarWidth` where native persists only the
composer height — tracked in `ROADMAP.md`. Separately the History inner split is
280 / 180–600 in Tauri against native's 240 / 200–360 and `STYLE.md`'s ~240
capped ~360, which is squarely Tauri drift: three constants.

### 10.5 Latent — these agree today by luck

Neither is biting now, both are one line, and both fail silently the moment a
user changes a setting.

**P-21 — the theme source.** Native follows `@Environment(\.colorScheme)`, i.e.
the system appearance. The Tauri client drives `data-theme` purely from
`config.theme` with **no `prefers-color-scheme` path at all**
([`config.ts:118`](../../apps/tauri-app/src/lib/stores/config.ts#L118),
default dark at `core/src/config.rs:242`). A machine set to Light with a config
saying `dark` renders the two clients in different themes. `FRONTEND.md` §8
explains why the native client ignores the config field; nothing explains why
the WebView ignores the system.

**P-22 — the accent colour.** The native client ships no `Assets.xcassets`, so
`.tint` resolves to the **user's** system accent. `--border-active` is the
hardcoded `#0a84ff` / `#007aff`. Set the system accent to graphite and only one
client follows. This one has a real tension behind it — `STYLE.md`'s token table
states the accent as a value — so it is listed in §10.7 rather than as a fix.

### 10.6 Not parity — correctness defects found on the way

These are behaviour, which §1 puts out of scope, so they live in
[`cross-client-feature-parity.md`](cross-client-feature-parity.md) and are named
here only so this audit is not read as their home. **D-25** — git's path quoting
reaching the syntax highlighter in both clients — is fixed, in §3.1. **D-26** —
a `maxlength="200"` on the Tauri summary field that silently truncates pasted
and AI-generated messages — is open, in §3.2. P-13's gutter overflow stayed
here, because its fix was one CSS value in the surface this section owns.

### 10.7 Decisions this audit cannot make

`STYLE.md`'s "the native client wins" settles a disagreement about a control's
look. None of these three is that.

1. **P-5, the sidebar tint.** `STYLE.md` mandates the two-tone, the native
   client paints flat, and the rule says the reference wins — but here the
   reference is *declining to paint*, which is the P-3 pattern again. Either
   `STYLE.md` loses its two-tone or the native client gains a background.
2. **P-22, the accent.** Follow the system accent in both (native already does,
   Tauri would need `AccentColor` on Windows and a Linux answer that may not
   exist), or hardcode blue in both and have the native client stop following
   the user. `STYLE.md` currently implies the second and the native client does
   the first.
3. **The 13 px register's weight.** `STYLE.md` calls it semibold; the native
   client reaches it through `.headline`, and Apple's macOS type table gives
   Headline as **Bold** (emphasized Heavy). Under §8 the reference wins and the
   register becomes bold — which would widen `STYLE.md`'s single bold exception
   (§7.2) beyond the status plate. Alternatively the native headings move to
   `.system(size: 13, weight: .semibold)` and the document stands.

Three that were on this list are now decided, each in the same direction — the
reference wins and the Tauri client moves:

- **P-1, the header's span — matched.** The toolbar is the layout grid's first
  row, spanning every track, so it sits over the sidebar as well as the diff.
  What is *not* matched is the total chrome height: the native's toolbar is
  also its title bar, while Tauri's sits under whatever title bar the host
  draws, and closing that would mean `decorations: false` and hand-drawn window
  controls on two platforms — which §6.6 declines. `FRONTEND.md` §8 carries the
  remainder as the divergence a host title bar forces.
- **P-3, alternating row backgrounds — reproduced, not disabled.** This section
  previously argued the native was the drifter here and the fix was
  `.alternatingRowBackgrounds(.disabled)`. The call went the other way: the
  reference ships striping, so the Tauri list stripes too. Two details are the
  Tauri client's own and are improvements rather than compromises — only real
  rows are painted (`NSTableView` fills the clip rect, so a short changeset
  gets a column of empty plates), and the stripe sits at about half
  `--surface-hover` because this list has a hover state to stay distinct from
  and the native one does not.
- **The `Summary (required)` placeholder — the native string wins.**
  `STYLE.md`'s *Commit message composer* now carries it, along with the
  reasoning that the disabled Commit button states the requirement a second
  way rather than instead.

One documentation defect, independent of the above: `STYLE.md` states the
no-count-capsule rule as an absolute in its anti-patterns while its own toolbar
paragraph and `FRONTEND.md` §8 both record the Tauri count capsules as
intentional.

The sync button's `Publish branch` against the native's `Publish Branch` was
listed here too, as a platform divergence nothing wrote down. It is not one:
**P-26** shows it is a single instance of an app-wide capitalisation mismatch
spanning four categories, with Apple stating the rule outright for two of them.
It is an ordinary parity item, and it is fixed where the rest of the category
is.

### 10.8 Checked and matching — do not re-audit

Recorded so the negative results are not rediscovered. **The diff body's colour
is genuinely in sync**: `--diff-add-bg` / `--diff-remove-bg` are byte-identical
to `DiffPalette`'s values in both themes, both clients bleed the fill full-width
including the gutter, both draw intra-line word highlights at matching alphas,
and all sixteen `--syn-*` values equal `DiffPalette.color(for:)` including the
markup classes and their weight and style intents. The status plate is at pixel
parity (18×18, radius 4, 10 px mono 700, 15 % tint). The tab strip's count badge
matches in size, weight, tabular numerics, radius and semantics. The composer's
geometry constants are identical (180–600, 220 default, 16 step, 80 floor), as
are the auto-summary derivation, the tri-state include-all semantics and its
count string. The sync ladder matches down to which states earn a chevron and
what the chevron menu holds. The mono stack, the diff font size, and — per P-2 —
the resolved font file, optical size and tracking are the same on both sides.
`FileList` row selection's two-tier split, the count-capsule placement, the
transfer-progress surface and the layout toggle's shape are all already recorded
in `FRONTEND.md` §8 and are not defects.

### 10.9 Second pass — what the first tranche left

Found by putting the two clients side by side again *after* §10.1–§10.5's items
landed. Two of the three are that pass's own loose ends rather than new ground,
which is the reason to write them where the work that caused them is recorded.

**P-23 — the commit list does not alternate its row backgrounds, and the file
list does. ✅ Landed.** `.alternatingRowBackgrounds()` is called at **two** sites —
[`ChangedFileList.swift:114`](../../apps/swift-ui-app/Sources/LeoGit/Design/ChangedFileList.swift#L114)
and
[`HistorySidebar.swift:109`](../../apps/swift-ui-app/Sources/LeoGit/Screens/HistorySidebar.swift#L109)
— and P-3 named both. Only the file list was wired, so the two Tauri lists now
disagree with each other as well as the History list disagreeing with its
reference; the History sidebar is visibly flat beside the native one.

The fix is `FileList.svelte`'s exactly: `class:striped` keyed on the commit's
index in the model, `--surface-stripe`, declared *before* `:hover`,
`.commit-row.selected` and the active row so source order — which is the whole
of the cascade between four equal-specificity single-class rules — leaves the
stripe underneath the states. Keying on DOM position is wrong for the same
reason it was wrong there: `CommitList` virtualizes and absolutely positions its
rows, so `:nth-child` sees only the slice near the viewport and would restripe
the list on every scroll. Row 0 stays plain.

**P-24 — the Settings glyph reads as a sunburst, not a gear. ✅ Landed.**
`Icon.svelte`'s
`gear` is a `ring(8, 8, 2.35)` plus eight detached radial segments. That cannot
resolve as a gear at any size, and the size is not what is wrong with it: a gear
is read from a **closed outer contour whose edge is interrupted by teeth**,
around a hole. With no rim, eight strokes radiating from a small circle are a
sun, a sparkle or an asterisk — which is what the header renders at 14 px.

The reasoning that produced it was sound and the conclusion was not: scalloped
`gearshape`-style outlines genuinely do turn to mush below 14 px, so the teeth
were dropped to spokes. The answer is to keep a rim and make the teeth coarse —
six or eight trapezoidal bumps on a closed path, an inner hole large enough to
survive at 12 px — rather than to remove the shape that carries the meaning.
This is one of the two glyphs with **no** SF Symbol counterpart at all (the
native client has no settings icon; macOS puts Settings in the app menu, and a
Tauri window has no app menu to put it in), so there is nothing to match and the
only test is whether it reads as a gear at 12, 14 and 16 px.

**P-25 — no pane-level empty state in the Tauri client has an icon, and the
native draws thirteen. ✅ Landed.** Not one missed case: of the **13**
`ContentUnavailableView` sites in the native client, **0** have a glyph in their
Tauri counterpart. The cause is legible in the history — the icon pass was a
*replacement* sweep over 44 inline `<svg>` blocks, and an empty state never had
an `<svg>` to replace, so nothing matched and none was reached. Six registry
symbols (`doc`, `doc-text`, `doc-zipper`, `doc-text-magnifyingglass`,
`arrow-turn-down-right`, `exclamationmark-triangle`) have **no call site
anywhere in the client**; they were drawn for these states and never wired to
them. **Every symbol needed already exists — nothing has to be drawn.**

| Native | Symbol | Tauri |
|---|---|---|
| `DiffView.swift:359` Submodule Changes | `arrow.turn.down.right` | `MainLayout.svelte:2292` |
| `DiffView.swift:372` Couldn't Load Diff | `exclamationmark.triangle` | `:2287`, `:2378` |
| `DiffView.swift:378` Binary File | `doc.zipper` | `DiffViewer.svelte:303` |
| `DiffView.swift:386` Large Diff | `doc.text.magnifyingglass` | `:2315`, `:2401` |
| `DiffView.swift:404` No/Whitespace/No Textual Changes | `doc` | `:2330`, `:2411` |
| `ChangesDetailPane.swift:21` No Changes | `checkmark.circle` | `:2338` (`files.length === 0` branch only) |
| `ChangesDetailPane.swift:27`, `HistoryDetailPane.swift:119` No File Selected | `doc.text` | `:2343`, `:2419` |
| `HistoryDetailPane.swift:25,29,35` history states | `clock` | `:2430`, `:2435` |

**Apple publishes no metrics for `ContentUnavailableView` — none, on any
platform.** No size, colour, gap or width, and macOS 26 adds no API (there is no
`ContentUnavailableViewStyle`). The numbers below were extracted from the
shipping SwiftUI binary on this machine (7.5.3, macOS 26.6.2) by resolving the
compiled constants, so they are **observed rather than documented and Apple may
change them in any update** — which is itself the reason to keep them in one
component rather than spread across call sites:

- content capped at `maxWidth: 400`, `padding(20)` all round, centred, text
  centre-aligned;
- outer stack spacing **12** (label block → description), description → actions
  **12**, between action buttons **6**;
- icon `HierarchicalShapeStyle.tertiary`; title `.bold`; description
  `Color.secondary`.

Two complete metric sets sit behind one size-class branch, and the binary work
could not prove which a normal Mac window takes. The screenshots settle it:
the branch giving **icon 36, icon→title 22, title `.largeTitle` (26 pt),
description `.body` (13 pt)** is the one where the heading is about twice the
description's size, which is what the native pane visibly shows. The other
branch puts the title at `.headline` (13 pt) against an 11 pt description —
near-identical sizes, which is not what renders. Confirm it against a build
before treating the four numbers as settled.

**The Tauri side draws this pattern 13 times across 2 files with 4 CSS
definitions that disagree** — `.diff-empty` (15 px, `--text-secondary`, weight
400), `.submodule-title` (15 px, `--text-primary`, weight **600**),
`DiffViewer`'s `.empty-state` (15 px, `--text-faint`) and `.binary-state` (**13
px**, `--text-faint`). The native draws all thirteen with one stock view. So the
fix is **extract `PaneEmptyState.svelte` first, then use it** — not "add an icon
in eight places": the icon is a glyph *plus* its own colour tier *plus* a
non-uniform gap that the current uniform `gap: 6px` cannot express, and the
metrics above are unpublished and therefore likely to be revised, which is an
argument for having exactly one place to revise them. This repo has made the
same call three times already (`RepoListEmptyState`, `ConfirmDialog`,
`SeamlessDiffPane`), each time after the duplicates had drifted.

Two smaller divergences fall out of the same audit. `FileList.svelte`'s empty
line is `--text-muted` where `CommitList.svelte`'s is `--text-faint`, though
both stand in for the same `EmptyListPlaceholder`'s `.tertiary`; `--text-faint`
is the closer match and `STYLE.md` independently assigns it to empty-state
hints. And `RepoListEmptyState`'s action is `.buttonStyle(.link)` natively —
blue link text — against Tauri's bordered `--bg-tertiary` button.

`PaneEmptyState.svelte` is that component and all thirteen sites now use it,
each with the symbol its native counterpart draws. Two sites gained the
heading they were missing rather than only a glyph — `Select a file to view its
diff` was one line where the native has `No File Selected` over `Select a file
to see its changes.`, and the binary state had no heading at all against the
native's `Binary File`. One wrapper survives the extraction and is worth
knowing about: the two loading-gated panes withhold the state itself under a
150 ms threshold, so something must still hold the pane open and paint it, and
that is `.diff-empty-hold` — `flex`, a column, and `--bg-primary`, nothing
else. Dropping it would flash a transparent pane on every quick file switch,
which is the same defect the threshold exists to prevent, from the other side.

**Three native states have no Tauri counterpart at all**, and those are
behaviour rather than paint, so they belong to
[`cross-client-feature-parity.md`](cross-client-feature-parity.md) and not
here: `Loading History…` (Tauri renders nothing while `!loaded`, though the prop
is already passed), `Couldn't Load Commit` (`MainLayout.svelte:897` is a bare
`catch {}` — the failure is swallowed), and `No Changed Files` on an empty or
merge commit (Tauri shows "select a file" beside an empty list).

**P-26 — the two clients capitalise user-visible text differently, and it is a
convention rather than a set of typos. ✅ Landed**, on both sides, with the
table now written into `STYLE.md`'s *Typography* so it stops being an unwritten
practice that only a census could recover. Found by pulling the thread on
`Large Diff` / `Large diff`. A census of both clients puts the disagreement in
**four categories out of eleven** — the other seven already agree exactly:

| Category | Native | Tauri |
|---|---|---|
| Buttons | Title, 14/17 | Sentence, 14/19 |
| Dialog / sheet / alert titles | **Conditional, 12/12** | Sentence, 10/14 |
| Pane-level empty-state headings | Title, **13/13** | Sentence, **13/13** |
| Context / menu-bar items | Title, 28/32 | No convention — 11 Title / 9 Sentence, split by *file* |
| Field labels, placeholders, tooltips, footers, body copy, progress labels, list-empty lines, Settings and branch section headings | *agree* | *agree* |

Each client's exceptions are almost all deliberate cross-copies: Tauri's five
title-cased strings are the ones ported from the native (`Commit N Files`,
`Amend Commit`, `Stop Amending`, `Clone Repository…`, `Squash & Merge`), and
`CommitMessage.svelte` carries a comment saying so.

**Apple states the rule outright for two of the four** — "As with all button
titles, use title-style capitalization and no ending punctuation" (*Alerts*) and
"To be consistent with platform experiences, use title-style capitalization"
(*Menus*) — and says **nothing at all** about sheet titles, tab labels, section
headings or empty states, which is exactly the case §8's "the native client
wins" exists to settle. The current HIG has no capitalisation table and no
per-element list; its only general statement is "choose a style for each UI
element type and use it consistently". The archived OS X HIG table that used to
carry one now redirects and must not be cited.

**The native's title rule is worth naming because it is already right, 12 of
12.** A fragment takes title case and no ending punctuation (`Clone Repository`,
`Discard Changes?`, `Force Push with Lease?`); a complete sentence takes
sentence case and its own punctuation (`Create a repository here?`, `Commit
nested repository as a link?`). That is Apple's alerts rule verbatim, followed
without ever having been written down.

Three things this turns up that are **not** casing and must not be fixed as if
they were:

- **`Merge into “main”…` is already correct in both clients.** `into` is a
  four-letter preposition and stays lowercase in title style. It looks
  sentence-cased and is not; "fixing" it to `Merge Into` would be the error.
- **`Checkout commit?` is two defects.** `Checkout` as one word is a noun; the
  verb is `check out`, so casing it alone yields the ungrammatical `Checkout
  Commit?`. The native's `Check Out This Commit?` / `Check Out` is the string to
  take wholesale.
- **`Select a file to view its diff` is a structural gap, not a casing one.**
  The native answers that state with a heading *and* a body sentence
  (`No File Selected` + `Select a file to see its changes.`); Tauri collapses
  both into one line. It belongs with P-25, not here.

`STYLE.md` currently contradicts itself on this, which is why nothing caught it:
`Create Branch` appears at line 87 and `Create branch` at lines 197 and 308.
Whatever lands has to reconcile those three plus the branch-menu list at line
245. This also closes §10.7's second documentation defect — the sync button's
`Publish branch` against the native's `Publish Branch` is one instance of the
category, not a special case.

**Decided: all four categories move, on both sides.** The Tauri client takes
title case for buttons, context-menu items and empty-state headings, and the
conditional rule for dialog titles. The **native** changes too, in the six
strings where it disagrees with its own 14/17, 28/32 and 13/13 pattern —
`Create repository`, `Commit as link`, `Choose folders to search`
(`InitRepoSheet.swift:67`, `ChangesSidebar.swift:145`,
`RepoListEmptyState.swift:58`) and three of the four update-chip menu items
(`UpdateChip.swift:30,33,40`; `Download from GitHub` is already correct, since
`from` is a four-letter preposition and stays lowercase). That is not the
reference bending to the copy: it is the reference agreeing with itself, and
those three menu items break Apple's stated *Menus* rule outright. Leaving them
would have been the worse option in a way worth recording — Tauri currently
*matches* five of the six, so title-casing only the Tauri side would have
repaired forty strings and broken five that already agreed.

### 10.10 Third pass — the two pickers, and one line in the sidebar

Found with the clients side by side a third time, after §10.9 landed, this time
with the repository switcher and the branch menu open in both. The pickers had
not been looked at by any earlier pass — §10.8's "checked and matching" list
never named them — and they turn out to carry the largest single divergence
left in the chrome, plus one real layout defect that the screenshot shows
directly and that no reading of the CSS would have predicted.

**P-27 — both pickers float in the middle of the window; the native hangs each
one under its chip, and `STYLE.md` already says so. ✅ Landed.** `MainLayout.svelte:3064`'s
`.overlay-backdrop` is a fixed, window-filling flex box with
`justify-content: center` and `padding-top: 60px`, painted with
`--overlay-backdrop` — the *modal* backdrop — and both `<RepoDropdown>` (`:2545`)
and `<BranchDropdown>` (`:2573`) mount inside it. So each opens centred across
the window, sixty pixels down, over a dimmed repository, with nothing
connecting it to the chip that was clicked. The native repo chip presents an
`NSPopover` (`RepoSwitcher.swift:44`, `.popover(arrowEdge: .bottom)`): centred
on the chip, an arrow pointing back at it, no dimming. The native branch chip
is a pull-down `Menu` (`BranchMenu.swift:56`), which AppKit hangs from the
control's leading edge — no arrow, no dimming. `STYLE.md`'s *Branch picker*
section opens with "Popover anchored to the branch button in the header", so
this is the Tauri client contradicting its own stylesheet rather than a
decision nobody made. The repo popover is also 340 × ≤420 where the native
declares 320 × 440 (`RepoSwitcher.swift:68`).

The fix is geometry, not a new surface: the chip hands its rect to the opener,
the backdrop becomes a transparent click-catcher (it still dismisses, and the
overlay stack's Escape handling is untouched), and the content is placed at the
chip's bottom edge — centred on it for the repo popover, with an arrow that
stays on the chip when the viewport clamps the box, and leading-aligned for the
branch popover, which is standing in for a menu. FRONTEND.md §8 already records
the branch surface's *shape* as a deliberate divergence (popover with a filter
and footer against a stock menu); its *placement* was never part of that
divergence and now matches. The arrow and the popover's own metrics are the
part nothing publishes, and are recorded at the end of this section.

**P-28 — the picker rows collapse to their text height the moment the list
overflows, which is the half-height rows in the screenshot. ✅ Landed.** Not a density
choice: `RepoDropdown.svelte:376`'s `.repo-item` is `height: 24px`, and the
screenshot shows about sixteen. `.repo-list` (`:336`) is a flex column of
*definite* height (`flex: 1; min-height: 0`), and a flex item's automatic
minimum size is its content size capped at its specified size (CSS Flexbox
§4.5) — with a specified 24 px and a 13 px line inside, the floor is the line.
So once the rows' total exceeds the column's height, the default
`flex-shrink: 1` takes every row toward its text *before* the column has
anything to scroll. (The column's own `overflow-y: auto` plays no part in
that; it is the definite height that forces the shrink.) The effect is invisible with a
handful of repositories and total with forty, which is why the list read fine
on the machine it was written on. `BranchDropdown.svelte:461/:486` has the same
structure and fails the same way the first time a repository has more branches
than fit; the Welcome picker (`RepoPicker.svelte:280`) escapes only because its
rows have no `height` and so no basis to shrink from. The fix is
`flex-shrink: 0` on every fixed-height row and heading. Turning the lists into
block scrollers instead would lose the stretch that lets `RepoListEmptyState`
fill the popover, which is the native's `placeholderMaxHeight`.

**P-29 — inside the popovers, the native draws six things differently, and
`STYLE.md` had written three of the Tauri versions down as rules. ✅ Landed,
all seven rows, in both pickers and the Welcome list.**

| | Native | Tauri | `STYLE.md` |
|---|---|---|---|
| Current item | a fixed 14 pt checkmark column on every row, visible on the open one (`RepoPickerList.swift:404-407`); the menu's own ✓ for the branch (`Picker(.inline)`). No fill, no weight change | repo: `--bg-tertiary` fill + weight 500, no glyph (`RepoDropdown.svelte:394,426`); branch: 6 px accent dot + fill + 500 (`BranchDropdown.svelte:507-513,543`) | documents the dot (*Branch picker*) |
| Keyboard cursor / hover | `.selection` at 35 % / 15 % (`RepoPickerList.swift:449-450`) — two alphas of one colour | inset 1.5 px `--border-active` ring / neutral `--surface-hover` (`:400`, `:521`) | documents the ring as the web's stand-in (*Repo pickers*), reasoned from composing with the current row's fill |
| Dirty dot | `Circle().fill(.tint)` — the accent (`:466-467`) | `--text-muted`, brightening on hover (`:468-479`) | documents the grey (*Status indicators*) |
| Row height | `padding(.vertical, 5)` around the 13 pt line (`:421`) = 26 | 24 (`:381`, `:491`) | 24 / 22–24 |
| Sync badges | `.caption.monospacedDigit()` in `.secondary` — 10 pt regular (`:480`) | 11 px **600** `--text-muted` (`:445-455`) | — |
| Remote branch rows | plain `Button`s, primary (`BranchMenu.swift:326`) | `--text-muted` (`:515`) | documents the muting |
| Filter placeholder | `Filter repositories` (`:128`) | `Filter repositories…` (`:207`), `Filter branches…` (`:289`) | writes the ellipsis in |

Under §8 the reference wins on a control's look, and each row above is that.
Two are worth a sentence beyond "match it". The ring's documented reason was
that it composes with the current row's fill; with the fill gone — the native
marks the open item with the checkmark alone — there is nothing left to
compose with, and the native's own device is available: one colour at two
alphas, `color-mix(in srgb, var(--border-active) 35%, transparent)` for the
cursor and 15 % for hover, which `color-mix` already does at five sites in the
client. And the placeholder's ellipsis is not a typo in one string but a
category error: `…` means "this opens something" (*Typography*), and a
placeholder is a hint inside a field, which opens nothing. The "one marker,
never a dot *and* a bar" rule survives the checkmark unchanged.

**P-30 — the Changes sidebar's empty line sits at the top of the list; the
native centres it. ✅ Landed.** `EmptyListPlaceholder.swift:15` claims the whole slot
(`maxWidth: .infinity, maxHeight: .infinity`). `FileList.svelte:609`'s
`.empty-state` asks for the same with `flex: 1` — but its parent
`.rows-viewport` (`:587`) is a block scroller, not a flex container, so the
declaration is inert and the box is as tall as its one line.
`CommitList.svelte:474` draws the same placeholder in the same structure with
`height: 100%` and centres, and `STYLE.md` already says "one faint **centred**
line". `height: 100%` it is; the viewport is a definite-height flex item so the
percentage resolves.

**P-31 — the toolbar's glyphs are about two-thirds the size of the native's,
and it is the whole bar, not the two chips. ✅ Landed.** `Icon` defaults to
`size = 12` (`Icon.svelte:391`); neither chip passes one (`Header.svelte:511`,
`:526`), nor do the sync button's six glyphs, and Settings and Help pass 14. A
macOS toolbar renders a `Label`'s symbol at the **large symbol scale of the
13 pt text beside it**, not at the text's size — the configuration is
unpublished (see the metrics note), but the rendered ink is measurable, and on
this machine `folder` inks 18.5 × 15, `arrow.triangle.branch` 14.5 × 15.5,
`arrow.triangle.2.circlepath` 20.5 × 16.5, `arrow.up.circle` and
`questionmark.circle` 16.5, `gearshape` 17.5, the plain arrows 12.5 × 15.5.
The registry's glyphs ink at roughly twelve of their sixteen grid units, so one
size, **21**, puts each within about two pixels of its counterpart; it is one
constant in `Header.svelte` on every `<Icon>` in the bar, and `Icon`'s default
stays 12, which is right for the affordance glyphs it was chosen for. What 21
cannot fix is P-33.

**P-33 — the registry draws every glyph near-square, and SF Symbols are not.**
A symbol's point size is typographic: the scales are defined against the text's
cap height, so symbols share a cap height and a baseline, *not* a bounding box,
and a wide symbol is genuinely wider than tall — `folder` 18.5 × 15,
`arrow.triangle.2.circlepath` 20.5 × 16.5 at the toolbar's scale. `Icon.svelte`
draws its `folder` 11.4 × 9.4 units and its sync loop as a 10.3-unit circle,
so at any one size the wide symbols come out narrower than the native's while
the round ones match. The stroke follows: a glyph filling less of its box needs
a bigger `size` for the same ink, and `Icon` scales the stroke with `size`, so
the toolbar's strokes run about 1.6 px where the native's are about 1.35. The
fix is a normalisation pass over the registry — draw each glyph to SF's
proportions with a shared cap band, which the documented template puts at
0.705 of the point size — after which the toolbar size drops back toward the
text's and the strokes fall in line. Not this pass: it touches all 33 glyphs
and is worth its own before-and-after.

**P-32 — the two Tauri repo lists are two copies, and the native's are one
view.** `STYLE.md`'s *Repo pickers* says "anything shown in both is shared
code" and names the empty state, the row labels and the footer — and those
are. The *rows* are not: `RepoDropdown.svelte:376-479` and
`RepoPicker.svelte:280-319` each draw their own, at different heights, with
indicators in one and not the other, a current marker in one and not the
other, and two copies of the sort, the core-ranked filter and the keyboard
cursor above them. `RepoPickerList.swift` is one view for both native surfaces,
parameterised by `activePath` and `RepoPickerHeight`. Every row in P-29 has to
be applied twice until that is true here too. **Deferred, deliberately**: it is
a structural change with its own behaviour to verify (the Welcome list gaining
the indicators the native already shows there), and it does not belong in a
paint pass. This pass hardens the Welcome copy against P-28 and gives it the
same cursor so the two do not drift further; extracting `RepoList.svelte` is
the next structural item.

**Metrics nothing publishes.** As with `ContentUnavailableView` in P-25, Apple
documents neither `NSPopover`'s geometry nor the toolbar's symbol
configuration — a research pass over the current HIG, the AppKit and SwiftUI
references and the WWDC sessions found the arrow's *existence* documented and
not one of its numbers, and Apple stopped publishing AppKit release notes after
macOS 14. So the numbers were read off this machine (macOS 26.6) and are kept
in one place each:

- **The popover.** A live `NSPopover` reports `anchorSize` **27.5 × 13**, its
  window centred on the anchor to the point, the arrow's tip one point off the
  anchor's edge, and the content inset 13 on every side — the arrow region,
  as `hasFullSizeContent`'s documentation says. The body's corner radius is
  drawn by the `NSGlassView` inside the frame and exposes no value, so the
  popover keeps `STYLE.md`'s 10. All of it lives in `MainLayout.svelte`'s
  anchoring constants; the arrow is a turned square, so its base is 26 for a
  height of 13. One simplification, stated: the web popover draws the arrow
  and nothing else of that 13-point inset — on the other three sides it is
  the native's shadow margin, which `box-shadow` supplies without a box.
- **The toolbar symbols.** The ink of each symbol at 13 pt / `.large` is the
  table in P-31, and the size chosen from it is one constant in
  `Header.svelte`. That the toolbar uses the large scale is inferred from
  three agreeing observations — the native chip's glyph in the side-by-side
  is about 1.6 × the Tauri 12 px one, a published 2× measurement of real
  toolbars found ~20 pt symbol boxes, and 13 pt / `.large` is the one
  configuration that produces both — not from any statement of Apple's.
- **The row.** The one documented number: the macOS type table gives the 13 pt
  body a **16 pt line height**, so the row is 5 + 16 + 5 = 26. One caveat
  recorded rather than acted on: SwiftUI lays text out at the font's natural
  line height, which a forum measurement puts about 1.5 pt under the table's
  figure at 17 pt, so the native row may render a point or so under 26.
