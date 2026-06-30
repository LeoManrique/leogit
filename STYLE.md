# Frontend Design Principles

leogit's interface is a desktop Git client. The design language is the calm, restrained, system-feeling aesthetic that Apple ships in macOS Tahoe — Finder, Xcode, Settings — built on system blue, semantic surfaces, and the OS font stack. Two themes are supported, Light and Dark.

> **Note on current state.** The shipped CSS is GitHub Primer–flavored (`#0d1117`, `#58a6ff`, `#3fb950`, etc.). This document describes the target the UI should migrate toward, not what's currently in [tauri-app/src/app.css](tauri-app/src/app.css). When a component is touched, prefer to move it in this direction rather than match neighboring code.

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

Each theme defines a parallel set of tokens. Components consume tokens — never hardcoded hex values — so the same component renders correctly in both themes. The current variable names (`--bg-primary`, `--text-primary`, `--status-green`, etc. in [app.css](tauri-app/src/app.css#L7-L53)) stay; only the values move toward the palette below.

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
- **Pane gutters:** 1px borders, never wider. The sidebar/content separator and tab-bar underline are both 1px `--border-inactive`. No 2px divider, no double rule.
- **The window never scrolls.** `html`/`body`/`#app` are locked to the viewport (`height: 100%`, `overflow: hidden`, `overscroll-behavior: none`) so only inner panes scroll and a trackpad drag at an edge can't rubber-band the whole app. New top-level layout must fit the viewport, not extend it.
- **Focus ring:** 2px ring at accent color with low alpha (~0.2, i.e. `--cursor-bg`). No glowing shadows, no `0 0 20px` halos. Current CSS uses `box-shadow: 0 0 0 3px var(--cursor-bg)` ([app.css:95](tauri-app/src/app.css#L95)) — that's the right shape; tighten to 2px and ensure alpha stays low in both themes.
- **Shadows:** subtle. `0 4px 12px rgba(0,0,0,0.4)` for popovers/dropdowns/modals in dark; lower-alpha (`0 4px 12px rgba(0,0,0,0.12)`) in light. Never combine shadow + gradient.

## Component patterns

### App chrome / layout

- **Two-column layout**: 320px sidebar on the left (file list + commit composer), flexible main content on the right ([MainLayout.svelte:490](tauri-app/src/lib/views/MainLayout.svelte#L490)).
- Sidebar background: `--bg-secondary`. Main content background: `--bg-primary`. The 1px right border on the sidebar is `--border-inactive`.
- **Terminal pane** docks at the bottom of the main content, ~280px tall, separated by a 1px border. Its own background can stay slightly darker than `--bg-primary` (use `#000` or `--bg-primary` — never an arbitrary off-color).
- The header strip at the top of the main content carries repo name, branch dropdown, and the quick-action button (Pull / Push, or Publish branch / Publish when the branch or repo isn't on a remote yet). Keep it ~36–40px tall.

### Tab bar (Changes / History)

- Plain text tabs, left-aligned, with a 2px accent underline under the active tab. No filled pill backgrounds, no rounded corners on tabs.
- Active tab: 13px semibold `--text-primary` + accent underline. Inactive: 13px regular `--text-muted`. Hover on inactive: `--text-secondary`.
- The whole tab bar sits on `--bg-secondary` with a 1px bottom border in `--border-inactive`.

### File list (staging)

- One row per changed file. 22–24px row height, 4px vertical padding, 12px horizontal padding.
- Layout: `[checkbox] [status marker] [filename] ····  [+N -N]` — checkbox for staging, single-character status marker (`M` / `+` / `−` / `R` / `!`) in the appropriate status color, filename in `--text-primary` medium-weight, line-delta on the right in 11px mono `--text-muted` with tabular nums.
- **Status as a colored letter, not a pill.** No background, no border, no rounded-full chrome. The letter *is* the badge.
- Hover row: `--surface-hover` tint, nothing more. No left-edge accent bar, no scale transform.
- Selected (active diff) row: `--bg-tertiary` background, 6px radius, no border. The selection state, not the staging state, gets visual emphasis.
- Checkbox: native-feeling square, 14px, accent fill when checked. Mixed-state (some hunks staged) uses an em-dash glyph, not a "minus" pill.
- **Nested repos & submodules** swap the status letter for a ↪ link glyph: blue `--status-blue` for an embedded repo (commits as a gitlink), muted `--text-muted` for a **dirty submodule** that can't be staged from the parent. The dirty submodule also mutes its filename and **disables its checkbox** (40% opacity, `not-allowed` cursor) with a tooltip explaining the change must be committed inside the submodule — the same "inactive but still selectable to view" treatment a read-only row gets. Its diff pane shows a centered "Submodule changes" message rather than a raw subproject-commit line.
- **Renames** render `[from] → [to]` where both sides share the same middle-truncation (collapse the directory to `…`, always keep the filename) so a deep `from` path can't crowd the `to` out of view. The `from` side is fully muted (filename included); the `to` side is the normal filename treatment. Both flex to equal width.

### Diff viewer

- Two-column gutter (old line number, new line number), then content. Gutter numbers in 11px mono `--text-muted` with tabular nums. Right-align within the gutter.
- Add lines: `--diff-add-bg` row background + `+` prefix in `--status-green`. Remove lines: `--diff-remove-bg` + `−` in `--status-red`. Context lines: no background, prefix is a single space.
- Hunk headers (`@@ -X,Y +A,B @@`): full-width subtle band at `--bg-secondary`, mono text in `--text-muted`. No accent color. One blank line of breathing room before each hunk.
- Side-by-side mode: same row heights, same colors, just split. A single 1px vertical `--border-inactive` between panes.
- Syntax highlighting: muted theme. The highlighter's accent should not compete with the add/remove background. Comments at ~`--text-muted` alpha, keywords at full `--text-primary`, strings/numbers slightly tinted but never saturated. The diff color is the primary signal; syntax is secondary.
- Selected line (for line-level staging): `--cursor-bg` background overlaying the add/remove tint. Range selection: same tint extends contiguously.

### Commit list (History)

- Two-line rows, 50px tall (taller than the 22–24px file/branch rows because each commit needs a summary line and a metadata line). One commit per row.
- Layout, stacked:
  - Line 1 — `[summary] ········ [tag pill] [↑ push indicator]`. The summary grows to fill the row and ellipsizes; indicators are right-aligned. Tag pill: `--badge-bg` / `--badge-fg`, 5px radius, 10.5px mono; a `+N` companion pill when a commit has multiple tags.
  - Line 2 — `[author] · [relative date]` in 11.5px `--text-muted`, tabular nums on the date. The author ellipsizes first; the date never truncates.
- No SHA in the row — it's copied via the right-click menu's "Copy SHA" and shown in the detail card.
- Selected row: `--bg-tertiary`, 6px radius. Hover: `--surface-hover`.
- The right pane (commit detail + changed files) gets its own thin 1px left border. Layout matches the changes view: file list on top, diff below. Consistent geometry across tabs.

### Commit message composer

- Lives at the bottom of the sidebar. Two stacked fields: a single-line title input (72-char soft limit, shown as a 11px `--text-muted` counter that flips to `--status-yellow` past the limit) and a multi-line description textarea. The title overflows horizontally with no scrollbar; a wheel/trackpad delta is mapped onto `scrollLeft` so a long summary can be swiped through (native single-line inputs only scroll via the caret).
- Inputs use `--bg-primary` (recessed against the `--bg-secondary` sidebar), 1px `--border-strong`, 6px radius, 2px accent focus ring.
- **No label above the inputs.** The placeholder ("Summary", "Description") is enough. Reserve labels for forms with mixed field types.
- Action row at the bottom: `[Commit]` primary (accent fill, right-aligned), `[AI ✨]` ghost-icon button to its left if AI commit messages are enabled. No leading icon on Commit itself.
- **Description is *not* monospace.** The current implementation may render it in mono — switch it to the system font. Reserve mono for content that benefits from fixed-width alignment.

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
- Group related fields without dividers — vertical rhythm carries the structure. Use a single section header + thin 1px `--border-inactive` rule only between top-level sections (Appearance / Diff / AI / Git).
- Don't use floating labels, helper text under every input, or asterisks for required fields.
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
- **Error modal** ([ErrorModal.svelte](tauri-app/src/lib/components/ErrorModal.svelte)): title in `--status-red` semibold, body in `--text-primary`, single `[OK]` button right-aligned. No icon-in-tinted-square next to the title.

### Branch picker

- Popover anchored to the branch button in the header. ~280–320px wide.
- Top: a single search input (placeholder "Filter branches…"). No label.
- List below: 22–24px rows, branch name in `--text-primary`, ahead/behind indicator in 11px mono `--text-muted` on the right, current branch marked with a thin accent left bar OR a small accent dot to the left of the name — pick one, not both.
- Footer row: "Create branch…" ghost button. Trailing `…` because it opens an input.

### Terminal

- Background `#000` (or `--bg-primary` in light, but real terminals are dark — `#000` is fine on both themes). Mono font, 12–13px.
- Header strip ~28px: `Terminal — /path/to/repo` in 11px `--text-secondary` on `--bg-secondary`, X close button on the right.
- No scrollbar styling — let the platform draw it.

### Status indicators

- **Connection / sync status** (ahead/behind in header): tabular-num count + a small word (`ahead 3`, `behind 1`), color from `--status-*` tokens, no background pill. A single 6px dot in `--status-green` for "in sync" — no pulse unless a fetch is in flight, in which case a brief opacity pulse is acceptable.
- **Inline counts in tabs / nav** (e.g. "Changes (12)"): 10–11px mono number, slightly muted, NOT in a pill background. The parens carry the bracketing.

### Icons

- SF Symbols–style line icons at 12–14px in chrome (header buttons, dropdown delete, modal close), 14–16px for standalone toolbar buttons. Stroke icons over filled icons for chrome. Filled icons for status only (the green sync dot, the conflict triangle).
- **Inline SVGs**, not unicode glyphs. `⎇`, `↑`, `↓`, `⟳`, `↻`, `⚙`, `?`, `✕`, `●` and similar TUI-flavored characters render at OS-default weight in whatever font happens to be available, look misaligned in dense rows, and are inconsistent across themes. Hand-rolled SVG paths with `stroke-width="1.4–1.6"` and `stroke-linecap="round"` match the macOS line-icon weight and inherit `currentColor` from the parent button. Reusable patterns currently inlined: branch glyph (header), up/down arrows (Pull/Push, ahead/behind), upload-over-baseline (Publish branch) and cloud-up (Publish repo), refresh swirl (spins via `@keyframes spin` when an op is in flight), gear (Settings), circled-question (Help), close X (modals + delete), checkmark (copied state), clipboard (copy SHA).
- **Don't put icons in tinted square backgrounds** anywhere — not for section headers, not for the app logo block, not for the AI button.
- **Don't lead buttons with icons** by default. "Commit", "Create branch", "Merge" are text-only. The Pull/Push buttons are the principled exception: the arrow direction *is* the affordance, and they sit next to count badges that need an anchor. Ghost icon-only buttons (close X, gear, sparkle, refresh) are fine — those are *icon* buttons, not icon-led text buttons.

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
