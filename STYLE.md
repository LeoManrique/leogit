# Frontend Design Principles

leogit looks like macOS Tahoe's own apps: system blue, semantic surfaces, the OS font stack. Two themes, Light and Dark.

- **One look on every platform.** Tauri wears this language on Windows and Linux too. Only chrome the host owns (window controls, menu bar, frame) stays the platform's.
- **The native client is the reference.** SwiftUI takes whatever a standard control gives it, with no custom glass or colour. If this doc and native disagree on a control's look, native wins and this doc changes. Tauri follows native.

## Intent

If a designer at Apple wouldn't ship it in macOS Settings, Finder or Xcode, it doesn't belong. An all-day tool should disappear into the OS.

## Typography

- **UI stack:** `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", system-ui, sans-serif`. No web fonts.
- **Mono stack:** `ui-monospace, "SF Mono", Menlo, Monaco, "Cascadia Mono", monospace`. For diffs, terminal (which drops `"Cascadia Mono"`), SHAs, paths as data, git's own text. Never labels, buttons or counts.
- **Sizes:** 13px body (lists, settings, dialog copy). 12px dialog buttons, strip text, dialog notes, picker footers, diff mono. 11px compact chrome (badges, hints, gutter). 10px is whatever native sets in `.caption`, which macOS draws at 10pt.
- **Leading rule:** `:root` is `line-height: normal`, never a ratio. `normal` lands on AppKit's leading, and a ratio tuned to SF clips Noto Sans on Linux. Two local overrides only: a wrapped paragraph that wants air (1.4–1.6), and a pinned box where leading is geometry (`0`, `1`, or a px matching a fixed height). A single-line label is neither.
- **Weights:** regular body, 500 labels and included filenames, 600 headings and active tab. 700 only for the 10px status letter and the 26px pane empty-state title, both from native.
- **Capitalisation** is by element kind, on every platform. Title case is Apple's: lowercase articles, coordinating conjunctions, infinitive `to`, `as`, and prepositions up to four letters (`Merge into "main"…`, `Commit as Link` are correct).
  - Title case: buttons, menu items, tabs, Settings and branch-list headings, pane empty-state headings.
  - Sentence case: field labels, placeholders, tooltips, footers, dialog body, progress labels, a list's empty line.
  - Dialog titles follow grammar: fragment gets title case (`Discard Changes?`), full sentence gets sentence case (`Create a repository here?`).
- No letter-spacing. **Tabular numerics** for any column of digits.

## Color

Components consume tokens, never hex. [app.css](apps/tauri-app/src/app.css) defines both themes, plus the `--syn-*` syntax palette (GitHub Light and Dark).

### Light

| Token | Value | Use |
|---|---|---|
| `--bg-primary` | `#FFFFFF` | Window background, diff canvas |
| `--bg-secondary` | `#F5F5F7` | Sidebar, tab bar, settings band |
| `--bg-tertiary` | `#ECECEF` | Plate of the row whose detail is showing |
| `--bg-elevated` | `#FFFFFF` | Dropdowns, modals, popovers, secondary buttons |
| `--border-inactive` | `rgba(0,0,0,0.1)` | Default 1px borders between panes |
| `--border-strong` | `rgba(0,0,0,0.18)` | Input and button borders, scrollbar thumb |
| `--surface-hover` | `rgba(0,0,0,0.04)` | Hover tint, multi-row selection |
| `--surface-stripe` | `rgba(0,0,0,0.022)` | Alternate file list rows |
| `--selection-cursor` | accent at 35% | Picker keyboard cursor |
| `--selection-hover` | accent at 15% | Picker hover |
| `--text-primary` | `#1D1D1F` | Body text, file names, commit summaries |
| `--text-secondary` | `rgba(0,0,0,0.62)` | Form labels, dialog body copy, captions |
| `--text-muted` | `rgba(0,0,0,0.45)` | Secondary information: SHA, paths, gutter numbers |
| `--text-faint` | `rgba(0,0,0,0.3)` | A list's own empty line ("No changes"), disabled context-menu items |
| `--border-active` (accent) | `#007AFF` | Active tab indicator, focus, primary CTA fill, context-menu highlight |
| `--accent-secondary` | `#0051D5` | Hover/pressed state on primary CTA |
| `--on-accent` | `#FFFFFF` | Label on an accent fill (~3.7:1, what macOS ships, leave it) |
| `--status-green` | `#16A34A` | Added marker |
| `--status-red` | `#DC2626` | Deleted marker, errors, destructive text |
| `--status-yellow` | `#D97706` | Warnings: strip, amend rule, failed walk |
| `--status-orange` | `#FF8D28` | Modified marker, over-limit summary counter |
| `--status-blue` | `#2563EB` | Renamed marker, nested-repo glyph, update chip |
| `--status-purple` | `#9333EA` | Conflicted marker, operation in progress |
| `--diff-add-fg` | `#16A34A` | `+` prefix and totals |
| `--diff-remove-fg` | `#DC2626` | `−` prefix and totals |
| `--diff-add-bg` | `rgba(22,163,74,0.10)` | Added-line row background |
| `--diff-remove-bg` | `rgba(220,38,38,0.10)` | Deleted-line row background |
| `--diff-intra-add-bg` | `rgba(22,163,74,0.28)` | Changed characters within an added line |
| `--diff-intra-remove-bg` | `rgba(220,38,38,0.30)` | Changed characters within a deleted line |
| `--cursor-bg` | `rgba(0,122,255,0.18)` | Focus ring halo |
| `--shadow-popover` | `0 4px 12px rgba(0,0,0,0.12)` | Popovers, dropdowns, modals |
| `--overlay-backdrop` | `rgba(0,0,0,0.3)` | Dialog scrim |
| `--badge-bg` | `rgba(0,0,0,0.08)` | Neutral badge fill |
| `--badge-fg` | `rgba(0,0,0,0.6)` | Neutral badge text |

### Dark

| Token | Value | Use |
|---|---|---|
| `--bg-primary` | `#1E1E1E` | Window background, diff canvas |
| `--bg-secondary` | `#252525` | Sidebar, tab bar, settings band |
| `--bg-tertiary` | `#2C2C2E` | Plate of the row whose detail is showing |
| `--bg-elevated` | `#3A3A3C` | Dropdowns, modals, popovers, secondary buttons |
| `--border-inactive` | `rgba(255,255,255,0.1)` | Default 1px borders |
| `--border-strong` | `rgba(255,255,255,0.18)` | Input and button borders, scrollbar thumb |
| `--surface-hover` | `rgba(255,255,255,0.06)` | Hover tint, multi-row selection |
| `--surface-stripe` | `rgba(255,255,255,0.03)` | Alternate file list rows |
| `--selection-cursor` | accent at 35% | Picker keyboard cursor |
| `--selection-hover` | accent at 15% | Picker hover |
| `--text-primary` | `#FFFFFF` | Body text, file names, commit summaries |
| `--text-secondary` | `rgba(255,255,255,0.65)` | Form labels, dialog body copy, captions |
| `--text-muted` | `rgba(255,255,255,0.48)` | Secondary information: SHA, paths, gutter numbers |
| `--text-faint` | `rgba(255,255,255,0.32)` | A list's own empty line, disabled context-menu items |
| `--border-active` (accent) | `#0A84FF` | Active tab indicator, focus, primary CTA fill, context-menu highlight |
| `--accent-secondary` | `#0066CC` | Hover/pressed state on primary CTA |
| `--on-accent` | `#FFFFFF` | Label on an accent fill |
| `--status-green` | `#22C55E` | Added marker |
| `--status-red` | `#EF4444` | Deleted marker, errors, destructive text |
| `--status-yellow` | `#F59E0B` | Warnings: strip, amend rule, failed walk |
| `--status-orange` | `#FF9230` | Modified marker, over-limit summary counter |
| `--status-blue` | `#3B82F6` | Renamed marker, nested-repo glyph, update chip |
| `--status-purple` | `#A855F7` | Conflicted marker, operation in progress |
| `--diff-add-fg` | `#22C55E` | `+` prefix and totals |
| `--diff-remove-fg` | `#EF4444` | `−` prefix and totals |
| `--diff-add-bg` | `rgba(34,197,94,0.12)` | Added-line row background |
| `--diff-remove-bg` | `rgba(239,68,68,0.12)` | Deleted-line row background |
| `--diff-intra-add-bg` | `rgba(34,197,94,0.32)` | Changed characters within an added line |
| `--diff-intra-remove-bg` | `rgba(239,68,68,0.34)` | Changed characters within a deleted line |
| `--cursor-bg` | `rgba(10,132,255,0.22)` | Focus ring halo |
| `--shadow-popover` | `0 4px 12px rgba(0,0,0,0.4)` | Popovers, dropdowns, modals |
| `--overlay-backdrop` | `rgba(0,0,0,0.5)` | Dialog scrim |
| `--badge-bg` | `rgba(255,255,255,0.12)` | Neutral badge fill |
| `--badge-fg` | `rgba(255,255,255,0.72)` | Neutral badge text |

### Usage rules

- Tokens carry meaning: `--text-muted` is "secondary information", not "this gray".
- **The accent is for affordances the user can act on**: primary CTA, active tab, focus ring, whatever the keys are moving. Never decoration.
- **Status colors are for status.** Red text on a button only when the action is destructive and unrecoverable.
- **Diff add/remove backgrounds** are the one allowed tinted row, since they're information. Alpha stays ≤12%.
- **No `backdrop-filter` or frosted glass.** Tauri windows are opaque, so it's inert and costs GPU.

## Spacing, radii, focus

- **Radii:** 6px is the chrome radius (inputs, buttons, selected rows, Settings cards). 7px menu items and 12px the menu (macOS 26's `NSMenu` pair). 10px modals and the branch picker. 12px is the ceiling.
- **Toolbar capsules are the one exception.** `--toolbar-radius`, declared on `.header`, is exactly half the 28px control height. Never `999px`, an anti-pattern: a number with a reason fails visibly if the height moves.
- **App icon:** native source stays a flat full-bleed square, because macOS adds the rounding and shadow.
- **Padding:** ~4×8 inputs, ~3×12 buttons, ~6×10 dropdown items.
- **Row heights are derived:** file list 30px, commit list 44px, picker rows 26px. One constant per list (`--row-height`), since a second copy tears a virtualized scroller.
- **Pane gutters:** 1px `--border-inactive`. A resize grab zone may be ~7px, its rule stays 1px.
- **The window never scrolls.** `html`/`body`/`#app` lock to the viewport, so only inner panes scroll.
- **Scrollbars:** `scrollbar-width: thin` and `scrollbar-color: var(--border-strong) transparent`, once on `*`. Each engine reads a different one, so both stay. Never add `::-webkit-scrollbar`. xterm draws its own, out of reach of this rule.
- **Overlay is the target everywhere**, but it's the engine's to give: free on macOS and WebKitGTK, requested on Windows via `tauri.conf.json`. `scrollbar-gutter: stable` (the file list's) may narrow a column, but no width may depend on it.
- **Focus ring:** `outline: none` plus `box-shadow: 0 0 0 2px var(--cursor-bg)`, once in app.css. Fields also swap their border to `--border-active`, since the halo alone is too faint on a filled row. No glows.
- **Checkbox and radio ring:** an `outline` at full accent, `outline-offset: 2px`. Author borders can't reach a checkbox, and WebKit deletes the halo while Chromium paints it.
- **Checked colour is `accent-color` only.** Engines discard an author background or border there.
- **Write every `user-select` twice, `-webkit-user-select` first.** Shipping WebKit reads only the prefix and nothing adds it at build. It inherits, so put `none` on the element, not a container of copyable text.
- **A `<select>` takes `appearance: none`** before any styling, or it stays half native with the wrong arrow and no halo. The chevron returns as a `::after` on a wrapper. The open list stays the platform's.

## Component patterns

### App chrome / layout

- **Two columns, one permanent split above the tabs.** Sidebar (tab bar, list, composer) is 280–640px, default 320, on `--bg-secondary`. Main is the detail over the terminal dock, on `--bg-primary`. Tabs never move the divider.
- **Composer** resizes from its top edge, 180–600px, default 220. Height goes to the description, and the list keeps a ~80px floor. The description sets no minimum of its own, or it overflows its slot.
- **Header strip:** 40px, spanning the window. Where-am-I controls lead, the one action trails. No refresh button.
- **Repo chip:** the bar's only semibold, repo name alone, deliberately not owner-qualified. Disambiguation belongs to picker rows.

### Tab bar (Changes / History)

Both clients: plain left-aligned text, 36px on `--bg-secondary`, 1px bottom border, sidebar width only. Active is 13px semibold `--text-primary` with a 2px accent underline. Inactive is 13px regular `--text-muted`, `--text-secondary` on hover. No pills.

### File list (staging)

- **Row:** `[checkbox] [status plate] [path]`, 30px, 12px leading room, 10px gaps. No per-file line counts.
- **Stripes:** `--surface-stripe` on odd real rows, keyed on model index, never `:nth-child` (virtualized).
- **Header:** tri-state select-all plus `3 of 12 files included`. Not drawn when empty.
- **Inclusion cue:** medium weight on the filename only.
- **Status letters** are git's: `A` `M` `D` `R` `U`, from core's `file_status_styles`. Only colour is per platform.
- **Status plate:** 18×18, 4px radius, 15% of the status colour, the letter 10px mono bold. No word chips.
- **Conflicted is `--status-purple`, not red,** since red means Deleted. The branch chip's `· merging` suffix and the operation notice share that hue, one token per client.
- **Selection is a set** (shift-click, shift-arrow). Every selected row takes `--surface-hover`, and the row whose diff is showing keeps `--bg-tertiary` at 6px radius. Native uses AppKit's drawing.
- **First file opens itself** on two conditions only: nothing is open, or what was open left the list. A chosen row is never overridden. The trigger is the set of paths changing.
- **Checkbox:** 14px, native, mixed state is the engine's `indeterminate`.
- **Nested repos** swap the letter for ↪: `--status-blue` embedded, `--text-muted` dirty submodule. A dirty submodule also mutes its filename and disables its checkbox (40%, `not-allowed`, tooltip), inactive but still selectable to view.
- **Paths:** directory `--text-secondary`, filename `--text-primary`. The directory shrinks first, never below a `b…/` hint, then the filename middle-truncates.
- **Renames:** `[from] → [to]`, each side only as wide as its path, half each when short of room. `from` fully muted.

### Diff viewer

- **Geometry:** two 44px number columns (40px digits, 4px gap), a 16px prefix, then content with no left padding and 12px trailing. Same in both layouts.
- **No vertical rules** except the one 1px `--border-inactive` between side-by-side panes.
- **Lines:** `--diff-add-bg` with `+`, `--diff-remove-bg` with `−`. **The minus is U+2212, never a hyphen,** in prefixes and `+N −N` totals. Context has no background and full-strength text, or prose files go dim.
- **Hunk header:** a band on `--bg-secondary`, 11px mono `--text-muted`, one line. Selectable text, no focus ring, no tab stop until per-hunk staging is wired.
- **File header:** canvas with a hairline, no fill. A rename reads `[from] → [to]`, muted, truncated from the head. A zero total is omitted.
- **Gutter and prefix are chrome**, out of the text selection. Selectability belongs to the content, not the row.
- **Line handle (native):** a gutter-picked run takes low-alpha accent over the tint and under the text. Per-line staging needs its own control, not a second wash.
- **Slow load** dims the diff with a spinner near the top, never blanks it.
- **Failed read:** title, then git's words in selectable mono on `--bg-secondary`, in the pane.
- **Unified ⇄ side-by-side:** *Segmented controls* at icon size, header's trailing edge.
- **Side-by-side:** the empty side is a filler cell in `--surface-hover`. Hunk headers span both.
- **Syntax** stays muted under the add/remove signal.

### Commit list (History)

- **Row:** 44px. Summary, then right-aligned indicators. Then `author · relative date`, 10px `--text-secondary`, one text run. No SHA. Selection matches the file list.
- **Tag pill:** neutral `--badge-bg`/`--badge-fg`, 5px radius, 10.5px mono. Never accent, which would outshout the unpushed arrow.
- **Reorder line:** 2px accent with round ends on the join between rows. Its key caption is an elevated plate near the list foot, 160ms fade.
- **Detail card** shows the message once, since `%b` already holds the trailers.

### Commit message composer

- **Fields:** `--bg-primary`, no labels, only placeholders (`Summary (required)`, `Description`). Description is never mono.
- **Counter:** 72 chars, advisory. 10px `--text-muted`, `--status-orange` past it, beside the field. Nothing truncates.
- **Commit button:** accent, no icon, names its count or state (`Commit 12 Files`, `Continue Rebase`). Disabled loses its fill.
- **Amend and operation notices:** caption band above the summary with a 2px leading rule, never filled. `--status-yellow` for amend (trailing *Stop Amending* link), `--status-purple` for an operation.
- **Status strip:** caption block above the action row, 2px `--status-red` rule. Last failure in red, standing reason in `--text-secondary`.
- **A remedy follows its sentence,** one word space away, and replaces the failure. Only the command is clickable: `--bg-elevated` chip, 4px radius, mono.
- **An inline label never sets its own size.** Inherit, and step mono to ~0.92em. Natively `.controlSize(.small)` overrides the caption, so state the font.

### Context menus

- Native takes the stock menu. Tauri draws to AppKit's metrics, because Windows' `HMENU` can't be themed.
- **Disable, don't hide.** The item keeps its place. A drawn menu names a state reason in its `title`, with `aria-disabled` per the picker's rule. A system menu has no tooltip to give.
- **Order:** destructive, repository-changing, copy, OS hand-off, with dividers, so the item that can lose work never sits beside the one people click most.
- **Toolbar:** only the two chips have menus. The stock *Icon and Text* menu is off.

### Buttons

- **Primary:** accent fill, `--on-accent`, no icon, 6px. One per section.
- **Secondary:** `--bg-elevated`, 1px `--border-strong`.
- **Ghost:** transparent, `--surface-hover` on hover.
- **Destructive:** secondary with `--status-red` text, unrecoverable actions only.
- Cancel left, primary right. Trailing `…` on anything opening further UI.

### Forms (Settings)

- **No Save button,** no dirty marks. A rejected write is one 12px `--status-red` line above the sections, and the control reverts.
- **Rows:** leading label, trailing control (`FormStyle.grouped`).
- **Field register:** recessed `--bg-primary`, 1px `--border-strong`, 6px radius, 4×8 padding, shared focus ring.
- **Sections are cards:** `--bg-elevated`, 6px, 1px hairlines between rows, on `--bg-secondary`. Header above the card, footer below in 10px `--text-muted`. Never uppercase. Order: Appearance, Git, Diff, Repository Discovery, Terminal, AI Commit Messages.
- No floating labels, routine helper text or required asterisks.
- **A destructive text field locks behind Edit ▸ Done.** Scan paths use it, in mono.
- **Toggles:** trailing mini switch, flat accent fill when on, white knob, no gradient. Built on a real zero-opacity `<input type="checkbox">`, with the focus ring redrawn on the track.
- **Segmented controls:** one rounded container of sub-buttons for 2–4 options. Selected is `--bg-elevated` with `--text-primary`, the rest transparent with `--text-muted`. Unified ⇄ side-by-side is the one in use.
- **Pop-ups:** Theme and AI provider are styled `<select>`s, which is what a macOS `Picker` is.
- **Numbers:** right-aligned, tabular, with a stepper driving `stepUp`/`stepDown`. Unit beside the value, not in the label.

### Section headers

Plain text, no icon squares.

- **15px semibold:** sheet, dialog and failure titles.
- **13px semibold:** detail-pane headings and Settings section labels.
- **26px bold:** pane empty-state titles only.

### Modals / dialogs

- **Size:** confirmations to 420px, editors and overlays to 480px, the startup picker 520px. Padding 16–20px, no card-within-card.
- **Dismiss:** Escape reaches the frontmost surface only. Backdrop click only for non-destructive dialogs, because declining a destructive one has to be deliberate. Nothing dismisses mid-operation.
- **Focus:** takes it on mount (`use:autofocus`), container ring suppressed.
- **Confirmations share [ConfirmDialog.svelte](apps/tauri-app/src/lib/components/ConfirmDialog.svelte)** and hold open under a busy label.
- **Blocked:** button disabled, plus *Another operation is still running.* in 12px `--text-secondary`. No spinner.
- **Squash and reorder** buttons use the accent, not red, since both are undoable.
- **Uncancellable operation:** one `<fieldset disabled>`, not per-control flags, with `min-inline-size: 0`, dimmed ~0.55. Progress and errors stay outside.
- **Error modal:** `--status-red` semibold title, git's text in selectable mono on `--bg-secondary`, Retry (secondary, optional) and Dismiss (primary). Retry closes first.

### Branch picker

- **Hangs from the chip:** leading edges aligned, no arrow, no scrim, ~280–320px.
- **Rows:** 26px, `--text-primary` local and remote. Current gets a checkmark in a 14px column, nothing else. Headings are muted 11px.
- **`flex-shrink: 0` on fixed-height rows,** or a scrolling flex column squashes them.
- **Footer:** 26px ghost action rows outside the list. Abort is hidden unless an operation runs, the one exception to disable-don't-hide.
- **Picking mode** borrows the list, with a back chevron. Destructive picks tint rows `--status-red`.

### Repo pickers (switcher popover + startup modal)

- **One component family,** sharing the empty state, row labels and footer. Native shares one view (`RepoPickerList`).
- **Rows:** 26px, name middle-ellipsized, muted `owner/` prefix only when another row shares the name, full path in the tooltip.
- **Cursor vs hover:** `--selection-cursor` and `--selection-hover`, never a ring. Open repo gets a checkmark.
- **The picker's rule:** a row that can't be picked stays visible, dimmed, and says why on hover. Never `disabled`, since a disabled control takes no pointer events and its tooltip never appears. Use `aria-disabled` and refuse in code.
- **Failed walk:** one caption row under the filter, `--status-yellow` triangle, trailing **Retry**.
- **Footer:** `Clone Repository…`, 12px ghost text, outside the list so it survives empty states. The list gets `min-height: 0`.
- **Switcher:** centred on its chip with an arrow, 320px, at most 440px tall, no scrim. Startup is a centred modal.

### Terminal

- `#000` on both themes, 12px mono, ~8×6px gutter. 280px means the emulator, with the header above it.
- **Header:** ~26–28px accessory bar. Native controls are stock `.accessoryBar`, never hand-styled, or they lose hover feedback.
- **Link hint** tooltip is app chrome in the UI font, not terminal palette.

### Empty states

- **Pane-level:** glyph, heading, sentence, always `PaneEmptyState.svelte`, mirroring `ContentUnavailableView`. 400px cap, 36px glyph, 26px bold, 13px sentence. No hand-built variants, since these metrics are unpublished.
- **Claim the whole pane:** `flex: 1` on an explicit `--bg-primary`, or the layout drags a pinned header to the middle.
- **List-level:** one faint centred line in `--text-faint`.
- **Answer the pane's own question,** and say which emptiness: still looking, nothing found (with searched folders in 11px mono), nothing matched.
- **A setting-fixable empty state carries the fix:** one tertiary button.

### Repo-less chrome

Same header component in every phase, dropping what needs a repo. Settings, Help and the update chip stay reachable.

### Status indicators

- **Ahead/behind:** both directions when non-zero. Tauri: capsules on the sync button's trailing edge. Native: plain `↑N ↓N` text left of it, since a toolbar control can't host a custom view.
- **Tab counts:** stadium pill, 16px tall, ≥18px wide, 11px medium, `--badge-bg`. Hidden at zero, never "(12)".
- **Dirty dot:** 6px accent circle in picker rows, beside 10px `--text-secondary` badges.
- **Update chip:** Tauri tints it (`--status-blue`, 12% wash, 40% border). Native is stock. "Copied" swaps the glyph, never the text. Never a banner.
- **The strip:** the only banner shape (`StatusStrip`). 12px line, bottom hairline, 13px glyph, sentence, git's text in 11px mono `--text-muted`. Warning tone is a `--status-yellow` triangle on a 12% wash. Done tone is an outlined check, no wash. Exactly three conditions, stacked:
  - Repository unreadable: warning, no dismiss.
  - OS hand-off failed: warning, ✕.
  - History action undoable: done, **Undo** text link in `--border-active`, ✕, never times out.
- **Not a banner:** a failure the user is waiting on goes in the modal. One raised inside a dialog stays there, input intact, as git's text: mono, selectable, capped in height and scrollable (`RefusalText`).
- **Transfer progress** lives in the control that started it (a strip under the toolbar natively). `--surface-hover` fill, `scaleX(fraction)`, 0.3s ease-out. Sweeps with no percentage, since a frozen 0% reads as stuck. No toasts.
- **Blocked sync button:** the picker's rule, 0.5 opacity, `not-allowed`. Natively enabled at 0.55, inert, same tooltip.

### Icons

- **SVG paths, never unicode.** One registry, `Icon.svelte` with a typed `IconName`, no inline `<svg>`.
- **Sizes:** 12–14px in chrome, 21 for every toolbar glyph.
- **Stroke:** ~0.08em, ~1.1px at 13px, via `Icon`'s `weight`. Round caps, 16-unit grid, optically centred. Fill for status only.
- **Own glyphs only.** The SF Symbols licence forbids shipping or tracing them off Apple platforms.
- **Coarsen detail, don't remove what carries meaning.** A cog needs its contour.
- **A parent can't style what `Icon` renders:** Svelte scoping silently drops a bare `.icon` rule. Tint via `color:`, or `:global()` under a parent class.
- **No leading icons on text buttons,** except sync, where direction is the affordance. The Force Push face (`arrow.up.to.line`) stays neutral, and its confirmation states the danger.

## Anti-patterns (don't ship these)

- Colored pill badges on file rows, or tinted full rows for status.
- Tinted banners for anything unasked.
- Icon-in-tinted-square anywhere.
- Chunky pill buttons, `999px` radii, gradient or aurora backgrounds.
- Accent glow shadows, or active state shown only by glow.
- All-caps tracked section labels.
- Two-line radio-card pickers where a pop-up or segmented control fits.
- Custom mono fonts (JetBrains Mono, Fira Code).
- Full-width destructive banners.
