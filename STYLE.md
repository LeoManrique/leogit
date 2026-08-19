# Frontend Design Principles

leogit's interface is a desktop Git client. The design language is the calm, restrained, system-feeling aesthetic that Apple ships in macOS Tahoe — Finder, Xcode, Settings — built on system blue, semantic surfaces, and the OS font stack. Two themes are supported, Light and Dark.

> **Note on current state.** The shipped CSS is GitHub Primer–flavored (`#0d1117`, `#58a6ff`, `#3fb950`, etc.). This document describes the target the UI should migrate toward, not what's currently in [tauri-app/src/app.css](apps/tauri-app/src/app.css). When a component is touched, prefer to move it in this direction rather than match neighboring code.

> **The native client is the reference.** The macOS SwiftUI client ([apps/swift-ui-app](apps/swift-ui-app)) reaches this aesthetic by construction rather than by imitation — it uses stock `List`/`Table`/toolbar and the system font stack, so it inherits the macOS 26 look (including Liquid Glass chrome) for free. It deliberately adopts no custom glass or colour treatments: anything it can get from a standard control, it gets from the standard control. Where this document and the native app disagree about a control's look, the native app wins, and this document should be updated to match. The Tauri frontend is to be re-skinned onto it rather than the two evolving in parallel.

## Intent

If a designer at Apple would not ship it in macOS Settings, Finder, or Xcode, it doesn't belong here. The recurring failure modes we deliberately avoid: GitHub-style colored pill badges on every row, status banners with tinted backgrounds, icon-in-tinted-square section headers, all-caps tracked section labels, chunky pill buttons with leading icons, two-line radio-card pickers with marketing-style descriptions, custom mono fonts (JetBrains Mono, Fira Code) for diff content, accent-color glow shadows. leogit is a tool the user keeps open all day — it should disappear into the OS, not announce itself.

## Typography

- **Font stack:** `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", system-ui, sans-serif` for everything chrome-related (sidebar, header, file lists, commit messages, settings, dialogs).
- **Mono stack:** `ui-monospace, "SF Mono", Menlo, Monaco, "Cascadia Mono", monospace` — reserved for diff content, terminal output, SHAs, file paths in diff headers, and code-shaped strings. Never for labels, captions, button text, or counts.
- **No imported web fonts.** Apple's system stack is doing the work.
- **Default body size: 13px** in app chrome and dense UI. File lists, commit lists, settings, dialogs all stay at 13px.
- **Smaller scale:** 11px for compact labels, badges, captions, keyboard-shortcut hints, ahead/behind counts, status footers. 10px for SHA short-hash metadata. 12px for secondary lines (commit author/date under a summary).
- **Diff content: 12–13px mono**, the same scale as the rest of the chrome — don't blow it up.
- **Weights:** regular for body, medium (500) for labels/active states/file names with changes, semibold (600) for headings and the active tab. Avoid bold (700+).
- **No letter-spacing on lowercase text.** Reserve `tracking-wider` for nothing — not even section labels in Settings.
- **Tabular numerics** for any column of digits: ahead/behind counts, line numbers in the diff gutter, commit dates, file change counts, `+N -N` deltas. They must align across rows.

## Color

Each theme defines a parallel set of tokens. Components consume tokens — never hardcoded hex values — so the same component renders correctly in both themes. The current variable names (`--bg-primary`, `--text-primary`, `--status-green`, etc. in [app.css](apps/tauri-app/src/app.css#L7-L53)) stay; only the values move toward the palette below.

### Light

| Token | Value | Use |
|---|---|---|
| `--bg-primary` | `#FFFFFF` | Window background, diff viewer canvas |
| `--bg-secondary` | `#F5F5F7` | Sidebar (file list + commit composer), tab bar, settings band |
| `--bg-tertiary` | `#ECECEF` | Hover row, selected commit row, terminal header |
| `--bg-elevated` | `#FFFFFF` | Dropdowns, modals, branch picker popover |
| `--border-inactive` | `rgba(0,0,0,0.1)` | Default 1px borders between panes |
| `--border-strong` | `rgba(0,0,0,0.18)` | Input borders, focused pane outline |
| `--surface-hover` | `rgba(0,0,0,0.04)` | Hover tint on file rows, commit rows, buttons |
| `--text-primary` | `#1D1D1F` | Body text, file names, commit summaries |
| `--text-secondary` | `rgba(0,0,0,0.62)` | Form labels, dialog body copy, captions |
| `--text-muted` | `rgba(0,0,0,0.45)` | Author/date, ahead-behind labels, SHA, paths |
| `--text-faint` | `rgba(0,0,0,0.3)` | Empty-state hints ("Working tree is clean") |
| `--border-active` (accent) | `#007AFF` | Active tab indicator, focus ring, primary CTA fill |
| `--accent-secondary` | `#0051D5` | Hover/pressed state on primary CTA |
| `--status-green` | `#16A34A` | Added lines (fg), "synced" indicator, success |
| `--status-red` | `#DC2626` | Deleted lines (fg), conflict, error |
| `--status-yellow` | `#D97706` | Modified marker, behind-by-N indicator, warning |
| `--status-blue` | `#2563EB` | Info, untracked file marker |
| `--diff-add-bg` | `rgba(22,163,74,0.10)` | Added-line row background in diff |
| `--diff-remove-bg` | `rgba(220,38,38,0.10)` | Deleted-line row background in diff |
| `--cursor-bg` | `rgba(0,122,255,0.18)` | Focus ring tint, selected diff line |

### Dark

| Token | Value | Use |
|---|---|---|
| `--bg-primary` | `#1E1E1E` | Window background, diff viewer canvas |
| `--bg-secondary` | `#252525` | Sidebar, tab bar, settings band |
| `--bg-tertiary` | `#2C2C2E` | Hover row, selected commit row, terminal header |
| `--bg-elevated` | `#3A3A3C` | Dropdowns, modals, branch picker popover |
| `--border-inactive` | `rgba(255,255,255,0.1)` | Default 1px borders |
| `--border-strong` | `rgba(255,255,255,0.18)` | Input borders, focused pane outline |
| `--surface-hover` | `rgba(255,255,255,0.06)` | Hover tint |
| `--text-primary` | `#FFFFFF` | Body text, file names, commit summaries |
| `--text-secondary` | `rgba(255,255,255,0.65)` | Form labels, dialog body copy, captions |
| `--text-muted` | `rgba(255,255,255,0.48)` | Author/date, ahead-behind labels, SHA, paths |
| `--text-faint` | `rgba(255,255,255,0.32)` | Empty-state hints |
| `--border-active` (accent) | `#0A84FF` | Active tab indicator, focus ring, primary CTA fill |
| `--accent-secondary` | `#0066CC` | Hover/pressed state on primary CTA |
| `--status-green` | `#22C55E` | Added lines, "synced" indicator, success |
| `--status-red` | `#EF4444` | Deleted lines, conflict, error |
| `--status-yellow` | `#F59E0B` | Modified marker, behind-by-N, warning |
| `--status-blue` | `#3B82F6` | Info, untracked file marker |
| `--diff-add-bg` | `rgba(34,197,94,0.12)` | Added-line row background |
| `--diff-remove-bg` | `rgba(239,68,68,0.12)` | Deleted-line row background |
| `--cursor-bg` | `rgba(10,132,255,0.22)` | Focus ring tint, selected diff line |

### Usage rules

- Tokens carry semantic meaning. Use `--text-muted` for "secondary information", not "this gray". Reach for hex only inside the theme definition.
- **The accent is reserved for affordances the user can act on**: primary CTA fill ("Commit", "Create Branch"), active tab indicator, focus ring, the selected file in the file list. Never use accent as decoration — no accent borders on the sidebar, no accent text in headings.
- **Status colors are reserved for status.** Don't tint a "Delete branch" button red unless the action is destructive *and* unrecoverable (force push, branch delete with unmerged commits, discard changes). A regular "Delete" button is just a secondary button.
- **Diff add/remove backgrounds are the exception** to the "no tinted backgrounds" rule. Changed-line tinting is information, not decoration. Keep the alpha low (≤12%) so the line stays readable; the foreground color of the `+`/`-` marker carries most of the signal.
- Frosted-glass / `backdrop-filter` effects: skip everywhere. Tauri windows are opaque; the effect is inert and just adds GPU cost.

## Spacing, radii, focus

- **Radius scale:** 6px for inputs, buttons, file rows when selected; 8px for menu items, dropdown items, terminal pane; 10px for modals and the branch picker; 12px for the app icon. Avoid larger than 12px in chrome — corners shouldn't shout.
- **Padding for controls:** ~4×8px (inputs), ~3×12px (buttons), ~6×10px (dropdown items). System-native controls are surprisingly tight.
- **Row height in dense lists:** ~22–24px effective. Vertical padding of 4px with 13px text matches macOS Finder list view. This applies to the file list and the branch list. The commit list is the deliberate exception — its rows are 50px because each commit stacks a summary line and an author/date line (see Commit list).
- **Pane gutters:** 1px borders, never wider. The sidebar/content separator and tab-bar underline are both 1px `--border-inactive`. No 2px divider, no double rule. A resize handle's *grab zone* may be wider (~7px) — its rule stays 1px.
- **The window never scrolls.** `html`/`body`/`#app` are locked to the viewport (`height: 100%`, `overflow: hidden`, `overscroll-behavior: none`) so only inner panes scroll and a trackpad drag at an edge can't rubber-band the whole app. New top-level layout must fit the viewport, not extend it.
- **Focus ring:** 2px ring at accent color with low alpha (~0.2, i.e. `--cursor-bg`). No glowing shadows, no `0 0 20px` halos. Current CSS uses `box-shadow: 0 0 0 3px var(--cursor-bg)` ([app.css:95](apps/tauri-app/src/app.css#L95)) — that's the right shape; tighten to 2px and ensure alpha stays low in both themes.
- **Shadows:** subtle. `0 4px 12px rgba(0,0,0,0.4)` for popovers/dropdowns/modals in dark; lower-alpha (`0 4px 12px rgba(0,0,0,0.12)`) in light. Never combine shadow + gradient.

## Component patterns

### App chrome / layout

- **Two-column layout**: one permanent split. The left column is the **sidebar** — the tab bar, then the tab's list (changed files or commit history), then on Changes the commit composer — resizable from 280 to 640px and 320 by default; the right column is the **main content** — the tab's detail over the terminal dock. The split sits above the tabs, so switching tabs or emptying a list swaps only what's *inside* each column: the divider never moves or disappears, the composer stays on a clean tree, and the diff always keeps the majority of the window. A nested split inside the detail (History's commit files ‖ diff) is the detail's own.
- **The commit composer is resizable from its top edge**: the 1px divider above it is the handle (~7px grab zone, system row-resize pointer on hover). Range 180–600px, default 220, remembered per client. A taller composer is more description, never more chrome — the summary field and the button row keep their height and the description editor absorbs the rest. Both the drag and the rendered height are capped so the list above keeps a floor (~80px) in a short window.
- Sidebar background: `--bg-secondary`. Main content background: `--bg-primary`. The 1px right border on the sidebar is `--border-inactive`.
- **Terminal pane** docks at the bottom of the main content — under the diff, never under the sidebar — ~280px tall, separated by a 1px border. Its own background can stay slightly darker than `--bg-primary` (use `#000` or `--bg-primary` — never an arbitrary off-color).
- The header strip at the top of the main content (Tauri; the native client carries the same three controls in the window toolbar) carries repo name, branch dropdown, and the **adaptive sync button** — one control whose face shows Fetch, Pull, Push, Publish branch, or Publish depending on where the branch stands relative to its remote, GitHub-Desktop style. There is no separate refresh button. Keep the strip ~36–40px tall.

### Tab bar (Changes / History)

- Plain text tabs, left-aligned, with a 2px accent underline under the active tab. No filled pill backgrounds, no rounded corners on tabs.
- Active tab: 13px semibold `--text-primary` + accent underline. Inactive: 13px regular `--text-muted`. Hover on inactive: `--text-secondary`.
- The whole tab bar sits on `--bg-secondary` with a 1px bottom border in `--border-inactive`. It is the top of the sidebar column, so it spans the sidebar only — never the main content.

### File list (staging)

- One row per changed file. 22–24px row height, 4px vertical padding, 12px horizontal padding.
- Layout: `[checkbox] [status marker] [filename] ····  [+N -N]` — checkbox for staging, single-character status marker (`M` / `+` / `−` / `R` / `!`) in the appropriate status color, filename in `--text-primary` medium-weight, line-delta on the right in 11px mono `--text-muted` with tabular nums.
- **Status as a colored letter, not a pill.** No background, no border, no rounded-full chrome. The letter *is* the badge.
- Hover row: `--surface-hover` tint, nothing more. No left-edge accent bar, no scale transform.
- Selected (active diff) row: `--bg-tertiary` background, 6px radius, no border. The selection state, not the staging state, gets visual emphasis.
- Checkbox: native-feeling square, 14px, accent fill when checked. Mixed-state (some hunks staged) uses an em-dash glyph, not a "minus" pill.
- **Nested repos & submodules** swap the status letter for a ↪ link glyph: blue `--status-blue` for an embedded repo (commits as a gitlink), muted `--text-muted` for a **dirty submodule** that can't be staged from the parent. The dirty submodule also mutes its filename and **disables its checkbox** (40% opacity, `not-allowed` cursor) with a tooltip explaining the change must be committed inside the submodule — the same "inactive but still selectable to view" treatment a read-only row gets. Its diff pane shows a centered "Submodule changes" message rather than a raw subproject-commit line.
- **Path truncation prefers the filename.** When a row is too narrow, the muted directory shrinks to a trailing `…/` bridge but never below a first-letter `b…/` hint, so a nested file can't be mistaken for a root file; the bright filename middle-truncates only once the hint plus the full filename can't fit. Directory characters are never styled as filename or vice versa.
- **Renames** render `[from] → [to]` where both sides share the same filename-first truncation so a deep `from` path can't crowd the `to` out of view. The `from` side is fully muted (filename included); the `to` side is the normal filename treatment. Both flex to equal width.

### Diff viewer

- Two-column gutter (old line number, new line number), then content. Gutter numbers in 11px mono `--text-muted` with tabular nums. Right-align within the gutter.
- Add lines: `--diff-add-bg` row background + `+` prefix in `--status-green`. Remove lines: `--diff-remove-bg` + `−` in `--status-red`. Context lines: no background, prefix is a single space.
- Hunk headers (`@@ -X,Y +A,B @@`): full-width subtle band at `--bg-secondary`, mono text in `--text-muted`. No accent color. One blank line of breathing room before each hunk.
- Side-by-side mode: same row heights, same colors, just split. A single 1px vertical `--border-inactive` between panes.
- Syntax highlighting: muted theme. The highlighter's accent should not compete with the add/remove background. Comments at ~`--text-muted` alpha, keywords at full `--text-primary`, strings/numbers slightly tinted but never saturated. The diff color is the primary signal; syntax is secondary.
- Markdown / prose diffs lean on **weight and style, not saturation** — headings bold, bold/italic/strikethrough carry their real font styling, links underlined, blockquotes muted italic. Colors stay in the same restrained Primer range as code tokens (`--syn-heading`, `--syn-link`, `--syn-raw`, `--syn-quote`), so a `.md` diff reads like the rendered document without shouting over the add/remove tint.
- Selected line (for line-level staging): `--cursor-bg` background overlaying the add/remove tint. Range selection: same tint extends contiguously.

### Commit list (History)

- Two-line rows, 50px tall (taller than the 22–24px file/branch rows because each commit needs a summary line and a metadata line). One commit per row.
- Layout, stacked:
  - Line 1 — `[summary] ········ [tag pill] [↑ push indicator]`. The summary grows to fill the row and ellipsizes; indicators are right-aligned. Tag pill: `--badge-bg` / `--badge-fg`, 5px radius, 10.5px mono; a `+N` companion pill when a commit has multiple tags.
  - Line 2 — `[author] · [relative date]` in 11.5px `--text-muted`, tabular nums on the date. The author ellipsizes first; the date never truncates.
- No SHA in the row — it's copied via the right-click menu's "Copy SHA" and shown in the detail card.
- Selected row: `--bg-tertiary`, 6px radius. Hover: `--surface-hover`.
- The right pane (commit detail + changed files) gets its own thin 1px left border. Below the detail card the geometry matches the changes view: changed-file list on the left, per-file diff on the right, the file column independently resizable — and narrower than the outer column (~240px, capped ~360px), since these rows carry only a path and this diff should dominate its split too.

### Commit message composer

- Lives at the bottom of the sidebar. Two stacked fields: a single-line title input (72-char soft limit, shown as a 11px `--text-muted` counter that flips to `--status-yellow` past the limit) and a multi-line description textarea. The title overflows horizontally with no scrollbar; a wheel/trackpad delta is mapped onto `scrollLeft` so a long summary can be swiped through (native single-line inputs only scroll via the caret).
- Inputs use `--bg-primary` (recessed against the `--bg-secondary` sidebar), 1px `--border-strong`, 6px radius, 2px accent focus ring.
- **No label above the inputs.** The placeholder ("Summary", "Description") is enough. Reserve labels for forms with mixed field types.
- Action row at the bottom: `[Commit]` primary (accent fill, right-aligned), `[AI ✨]` ghost-icon button to its left if AI commit messages are enabled. No leading icon on Commit itself.
- **Description is *not* monospace.** The current implementation may render it in mono — switch it to the system font. Reserve mono for content that benefits from fixed-width alignment.
- **Amend notice** — while the composer is rewriting the last commit, a caption-sized band sits above the summary field: "Your changes will modify your **most recent commit**." with a link-styled *Stop Amending* on the trailing edge. Marked by a 2px `--status-yellow` rule on its leading edge, not a filled block — the composer is already a dense stack of bordered fields, and a tinted panel here reads as another one. It never names the commit: the message it seeded is in the fields directly below.

### Context menus

- Row context menus are **stock system menus** — plain items, `Divider()` between groups, the destructive role on the one item that destroys work. Nothing here is hand-drawn or re-themed; the platform owns the look, the same rule that governs the toolbar and the terminal's accessory bar.
- **Disable, don't hide.** An action that doesn't apply to this row (reveal a deleted file, amend a commit that isn't HEAD) stays in place greyed out, so the menu has one shape and its items keep one position.
- Order runs destructive → repository-changing → copy → hand-off to the OS, separated by dividers, so the item that can lose work is never adjacent to the one people click most.

### Buttons

- **Primary action:** solid accent fill (`--border-active`), white text, no leading icon, ~3×12 padding, 6px radius. One per dialog/section: "Commit", "Create branch", "Merge", "Confirm force push".
- **Secondary action:** `--bg-elevated` background + 1px `--border-strong`, no fill. Use for cancel, "Browse…", "Open in Finder…".
- **Ghost / tertiary:** transparent, text-only, `--surface-hover` on hover. Use for in-row icon buttons (the X to close the terminal, the AI sparkle), inline links.
- **Destructive:** secondary button styling with `--status-red` text. Reserve for genuinely unrecoverable actions (force push, branch delete with unmerged work, discard hunk). A plain "Delete branch" with safe local-only deletion stays secondary.
- **Order in dialogs:** Cancel on the left, primary on the right, right-aligned at the bottom. The reverse reads as web app.
- **Ellipsis convention:** trailing `…` for actions that open further UI ("Browse…", "Open repo…", "Switch branch…").

### Forms (Settings)

- Label-on-left layout with right-aligned labels at a fixed column width (~140px for Settings — slightly wider than LeoSync's 96–112px because leogit's labels run longer: "Auto-fetch interval", "Syntax highlighting"). Inputs flex into the remaining width.
- Inputs use `--bg-primary` (recessed against `--bg-secondary` settings container), 1px `--border-strong`, 6px radius, 2px accent focus ring (no glow).
- Group related fields without dividers — vertical rhythm carries the structure. Use a single section header + thin 1px `--border-inactive` rule only between top-level sections (Appearance / Diff / Terminal / Git / AI).
- Don't use floating labels, helper text under every input, or asterisks for required fields. A single 11px `--text-muted` hint on its own full-width line under a control is allowed only where the option's scope isn't self-evident (e.g. the shell picker: "Applies to new terminal sessions") — never as a routine caption.
- **Toggles** for booleans (auto-fetch, hide whitespace, syntax highlighting): flat solid accent fill when on, white knob, no gradient.
- **Segmented controls** for 2–4 mutually exclusive options (theme: System/Light/Dark; AI provider: Claude/Ollama). A single rounded container with sub-buttons; selected segment gets `--bg-elevated` fill + `--text-primary`, unselected segments stay transparent + `--text-muted`.
- Numeric inputs (fetch interval, tab size, context lines) are plain inputs with mono input value and tabular nums. No stepper arrows.

### Section headers

- Plain text. No icon-in-tinted-square. No subtitle below the title unless it adds non-obvious information.
- 13px semibold for in-app section titles (Settings categories, dialog titles).
- If a section needs a description, render it once at the top with `--text-muted`, not as a subtitle under every header.

### Modals / dialogs

- Small. 360–420px width. Tight padding (16–20px). Header + body, no card-within-card chrome.
- Backdrop: `rgba(0,0,0,0.3)` light, `rgba(0,0,0,0.5)` dark. No backdrop blur (skip in the desktop app where opaque windows make the effect inert).
- Close affordance: a tiny X in the header corner. `Escape` dismisses. Backdrop click dismisses for non-destructive dialogs (Settings, Help, Branches); destructive dialogs (Force push, Discard changes) require an explicit button.
- One concern per modal. Prefer in-page sections (sidebar tabs, settings categories) over spawning a modal.
- **Error modal** ([ErrorModal.svelte](apps/tauri-app/src/lib/components/ErrorModal.svelte)): title in `--status-red` semibold, body in `--text-primary`, single `[OK]` button right-aligned. No icon-in-tinted-square next to the title.

### Branch picker

- Popover anchored to the branch button in the header. ~280–320px wide.
- Top: a single search input (placeholder "Filter branches…"). No label.
- List below: 22–24px rows, branch name in `--text-primary`, ahead/behind indicator in 11px mono `--text-muted` on the right, current branch marked with a thin accent left bar OR a small accent dot to the left of the name — pick one, not both.
- Footer row: "Create branch…" ghost button. Trailing `…` because it opens an input.

### Terminal

- Background `#000` (or `--bg-primary` in light, but real terminals are dark — `#000` is fine on both themes). Mono font, 12–13px.
- The emulator sits on a small gutter — ~8px horizontal, ~6px vertical — painted the same black, so it reads as part of the terminal. Terminal emulators draw edge to edge, which leaves the first column touching the header strip above and running into the window's rounded bottom corner, where it gets clipped.
- Header strip ~28px on a toolbar material, with a 1px rule above it. It is an **accessory bar** — the recessed scope-bar control class macOS uses for Finder's search scopes and Safari's favourites bar (SwiftUI's `.accessoryBar` button style, at `.controlSize(.small)`). The shell name leads on the left; New Session `+`, the expand/collapse chevron, and the session-killing `✕` sit icon-only on the right, with `✕` disabled while no session exists.
- **Never hand-style the strip's controls.** The accessory bar draws hover, press, and selected states itself; a strip of plain/borderless buttons with a forced secondary foreground renders as inert glyphs that give no feedback on mouse-over, which is exactly what reads as non-native. Same rule as the toolbar: use the system control, don't imitate it.
- **Active shell name** is the label of a button-styled toggle beside a terminal glyph (`apple.terminal` — the current-era symbol, not the older generic `terminal`), so the strip carries the panel's open state the way a scope bar carries a selected scope. It reports what actually launched, which can differ from the configured preference when that shell isn't installed, and reads `Terminal` until a session names itself.
- Icon-only buttons are still built from labels, so every one has a tooltip and an accessibility name. There is no bottom-toolbar placement on macOS (`.bottomBar` is unavailable there), so this strip is hand-assembled by necessity — but out of stock controls.
- No scrollbar styling — let the platform draw it.

### Empty states

- **An empty state that a setting can fix must carry the fix.** "No repositories found" alone is a dead end; name what was searched, then offer the action. Pattern: a plain title in `--text-faint`, an 11px explanatory line inheriting it, the searched values as an 11px mono list in `--text-muted` (paths are data — mono), then a single tertiary button. Left-aligned text would fight the centred modal, so the whole stack stays centred.
- Don't stack more than one action. The contextual button and the persistent header control are enough; a third route reads as uncertainty about which one works.
- **A sidebar list with no rows shows one faint centred line** (`No changes`, `No commits`), never the icon-and-headline treatment: that is sized for a pane, and the pane-sized story ("The working tree is clean.") is the detail column's to tell. The composer stays put underneath — the placeholder claims exactly the list's slot.
- **An empty state that replaces only part of a pane must still claim the whole pane.** Sized to its own content, it leaves the pane's stack shorter than the slot it sits in, so the layout centres the stack — and a header meant to sit at the top drifts to the middle, where it reads as oversized chrome rather than as a short body. The header stays pinned; the empty state takes the rest and centres inside it. This is what the binary-file and "no textual changes" diff bodies do.

### Repo-less chrome

- The header bar is **the same component** in every phase, not a reduced copy. When no repository is open it simply drops everything that acts on one (repo/branch chips, status area, the sync button) and keeps Settings, Help, and the update chip. Never build a second, simpler header — the two would drift, and the user would notice the app "changing shape" between launch and main view.
- App-level chrome (Settings, Help, update availability) is reachable in *every* phase. A phase that can't be escaped without restarting the app is a bug, not a layout.

### Status indicators

- **Connection / sync status** (ahead/behind): standalone muted text beside the sync action — `↑N ↓N`, tabular numbers, secondary foreground, no background, no border, no capsule. It is status, not a control: it must never look clickable. Hidden entirely when in sync; the spelled-out counts ("2 commits to push") live in a tooltip.
- **Counts in tabs** (the Changes tab): a small stadium pill after the label — 16px tall, ≥18px min-width, 11px medium tabular number, muted foreground on `--badge-bg`. Hidden at zero, never capped. Never a bare parenthesized number — "(12)" in a control face reads as an afterthought, not an indicator. Toolbar action buttons (Pull/Push) carry **no** count: a macOS system toolbar control can't host a custom view, so the counts stand as separate status text beside the button (see *Connection / sync status* above) — don't imitate the system control to smuggle a pill in.
- **Dirty dot (repo switcher rows):** a 6px `--text-muted` circle between the repo name and its `↓/↑` badges, brightening to `--text-secondary` with the row like the badges do. It signals "has uncommitted changes" without a count — the Changes tab carries the number.
- **Update chip (header, left of the action cluster):** informational, not an action the user owes us — so it's *tinted*, not solid: `--status-blue` text on a `color-mix` 12% wash of the same token with a 40% border (20% on hover), sitting a step below Pull/Push in the bar's hierarchy while still reading as new. Same 28px height and 6px radius as its neighbours, an up-arrow glyph, and no count badge. The "copied" confirmation swaps that glyph for a checkmark and leaves the label alone — swapping the *text* would resize the chip and shove the whole action cluster sideways for the duration. Never a banner, modal, or toast — an update is never urgent enough to take space from the repo.
- **Transfer progress (push / pull / clone):** progress lives *inside the control that started it*, GitHub-Desktop style — no toasts, banners, or floating progress windows. The Pull/Push button gets a full-height `--surface-hover` fill scaled with `transform: scaleX(fraction)` (`transform-origin: left`, 0.3 s ease-out so git's phase jumps glide instead of teleporting); button content layers above via `position: relative`, and the usual `:disabled` dimming is cancelled while the fill shows (the spinner already signals "busy" — graying the progress out would defeat it). Git's raw progress line renders in the header status area in 11px mono `--text-muted`, ellipsized. The Clone dialog uses a 4px rounded bar (`--border-active` fill on `--bg-secondary`) above the same mono line.

### Icons

- SF Symbols–style line icons at 12–14px in chrome (header buttons, dropdown delete, modal close), 14–16px for standalone toolbar buttons. Stroke icons over filled icons for chrome. Filled icons for status only (the green sync dot, the conflict triangle).
- **Inline SVGs**, not unicode glyphs. `⎇`, `↑`, `↓`, `⟳`, `↻`, `⚙`, `?`, `✕`, `●` and similar TUI-flavored characters render at OS-default weight in whatever font happens to be available, look misaligned in dense rows, and are inconsistent across themes. Hand-rolled SVG paths with `stroke-width="1.4–1.6"` and `stroke-linecap="round"` match the macOS line-icon weight and inherit `currentColor` from the parent button. Reusable patterns currently inlined: branch glyph (header), up/down arrows (Pull/Push, ahead/behind), upload-over-baseline (Publish branch) and cloud-up (Publish repo), refresh swirl (spins via `@keyframes spin` when an op is in flight), gear (Settings), circled-question (Help), close X (modals + delete), checkmark (copied state), clipboard (copy SHA).
- **Don't put icons in tinted square backgrounds** anywhere — not for section headers, not for the app logo block, not for the AI button.
- **Don't lead buttons with icons** by default. "Commit", "Create branch", "Merge" are text-only. The sync button is the principled exception: the arrow direction *is* the affordance, and it sits next to a count that needs an anchor. Ghost icon-only buttons (close X, gear, sparkle, refresh) are fine — those are *icon* buttons, not icon-led text buttons.

## Anti-patterns (don't ship these)

- GitHub-style colored pill badges on file rows (`[Modified]`, `[Conflict]` in a rounded-full tinted chip). Use a single colored letter instead.
- Gradient mesh / aurora backgrounds anywhere in the app.
- Accent-color glow shadows around buttons, inputs, or cards.
- Icon-in-tinted-rounded-square next to every section title.
- Active state communicated *only* by gradient or glow.
- All-caps `tracking-wider` section labels.
- Helper subtitles under every form input in Settings.
- Two-line "radio card" pickers for theme or AI provider when a segmented control fits.
- Custom mono fonts (JetBrains Mono, Fira Code, etc.) for diff content or terminal output — system mono is fine and faster.
- Backwards button order (primary on the left of Cancel).
- Force-push / Delete-branch buttons styled as full-width destructive banners. A red secondary button inside the dialog is enough.
- Tinted full-row backgrounds in the file list to "color-code" file type or status — the status letter carries the signal.

## Marketing site (future)

leogit currently has no marketing site. When one is built, the same design language applies — same font stack, same color tokens, same restraint — with these adjustments:

- Body copy may scale up to 15–17px on hero/landing sections; chrome-scale elements (download buttons, footer, nav) stay at 13px.
- Section rhythm uses generous vertical padding (64–96px between sections), but components *inside* a section follow the tight scale used in the app.
- Horizontal nav, transparent over hero, switches to a subtle `--bg-secondary` background on scroll. 4–6 nav items max, right-aligned.
- Hero CTA: single primary button (accent fill, "Download for macOS") + a quiet secondary text link ("View on GitHub"). No leading icon, no trailing icon, no glow, no gradient.
- Feature cards: subtle `--surface-hover` fill, no border, no hover lift, no gradient. Numeric or icon up top, 13–15px label below.
- Screenshots of the app are the marketing material. Don't dress them up — let the calm UI speak.
