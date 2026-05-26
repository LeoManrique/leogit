# leogit — Roadmap

What's shipped lives in [DESIGN.md](DESIGN.md). This file tracks **what's next**, grouped roughly by surface and effort. Items are added as they're discovered; check a box only when it ships.

## P0 — Correctness and polish before user-facing PR work

These are visible regressions or known bugs from the existing implementation. Fix before adding new surface area.

- [ ] **CLI path normalization.** When a relative path like `.` is passed via CLI, expand to absolute before handing it to `discover_repos` / `get_status`. Tilde expansion already works but is not test-covered.
- [ ] **Live diff on file highlight.** Selecting a file in the file list should load its diff immediately; currently the activation path requires Enter on some keyboard flows. Make sure the click + arrow-key flows both fire `loadDiffForFile` without rebuilding the diff store every keystroke (memoize when the path doesn't change).
- [ ] **Word wrap stops at line-number gutter.** Long lines in the diff viewer currently push the gutter; wrapping should only apply to `.line-content`.
- [ ] **Mouse selection for file rows.** Multi-select via click + shift / cmd is missing — only single-row activation works.
- [ ] **Brighten the line-selection dot.** The current `selection-dot` is too dim against `--bg-primary`; bump alpha or use `--border-active` outline even when unselected.
- [ ] **AI generation must respect line-level selection.** Currently `getSelectedDiff` sends full file diffs for every selected file. It should use `diffApi.generatePatch(repoPath, fileDiff, selection)` per file so the AI only sees the lines actually selected. Requires propagating per-file `DiffSelection` (already in `repoState.diffSelection`) into the generate flow.
- [ ] **Commit composer auto-resize on focus.** When the user focuses the message section, grow it temporarily so summary + description are both visible. Optionally allow manual resize via `Ctrl+Shift+Arrow`.
- [ ] **AI-generated long messages scroll to start.** After Generate, scroll the description textarea to the top (currently shows the tail of long outputs). A subtle scrollbar or `…` indicator would also help.
- [ ] **Settings panel scrolling.** The Settings modal body needs `overflow-y: auto` to be reachable on smaller windows — currently it can clip below `max-height: 85vh`.
- [ ] **`q` closes Settings; double-`q` closes the app.** Wire `q` (no modifier, no input focus) as an alias for Escape inside overlays, and as a "press twice to quit" gesture at the top level.
- [ ] **Settings re-arm intervals on save.** `fetch_interval_ms` and `auto_fetch` only take effect on app restart today; saving Settings should clear/restart `fetchInterval` immediately.
- [ ] **Re-focusing the terminal.** After tabbing back into the terminal, the borders highlight but keystrokes are dropped until the user clicks. Need to call `term.focus()` from xterm.js when the container regains focus.
- [ ] **Terminal resize correctness.** FitAddon fires but the PTY size doesn't always update — verify `resize_terminal` is invoked on the most recent pid.
- [ ] **Terminal key passthrough.** Space, `Ctrl+X`, and other control sequences are intercepted by global shortcuts. Gate the global keydown handler off when the terminal is focused.
- [ ] **Internally-stashed files exclusion.** Verify that when a file is deselected (in `userDeselected`) it never makes it into the commit even if `git status` reports it as staged externally.

## P1 — Feature gaps

- [ ] **PR overview panel.** `gh.rs` already exposes list/checks/create/checkout/get-current. The frontend currently only opens a button — build a proper PR list/detail view with check status pills and a "checkout" button.
- [ ] **"Create GitHub Project" action.** Surface `gh project create` (or wrap the gh GraphQL call) as an action in the PR area.
- [ ] **Inline terminal expand hint.** When the terminal is collapsed, show a thin hint band (e.g. `Press \`` `` to open terminal`) so new users discover the feature.
- [ ] **Stash management.** Backend commands + UI: `git stash push -m`, `git stash list`, `git stash apply/pop/drop`. Surface in the sidebar.
- [ ] **Branch rename UI.** `rename_branch` exists in `git.rs` but has no UI entry point in `BranchDropdown.svelte`.
- [ ] **Remote branch deletion UI.** `delete_remote_branch` exists but only via API.
- [ ] **Merge / squash-merge UI from the branch dropdown.** `MergeOverlay.svelte` is wired in `MainLayout`, but the "start a merge" trigger isn't exposed in the branch picker yet.
- [ ] **Conflict resolution surface.** `is_merging` is checked and shown in the header as `MERGING`, but there's no UI for resolving conflicts beyond what the embedded terminal allows.
- [ ] **Force-push (with lease) action.** `push()` already supports `force_with_lease` — expose it behind a confirmation dialog when the push would be rejected.

## P2 — Larger features

- [ ] **Rebase (interactive UI).** Squash/reorder/edit/drop commits with drag-and-drop.
- [ ] **Commit search.** Filter the log by author, message regex, file path, date range.
- [ ] **Blame / annotate.** Inline blame in the diff viewer + a dedicated blame view.
- [ ] **Cherry-pick / revert.** Right-click a commit → cherry-pick into current branch / revert.
- [ ] **File history.** "Show history for this file" jump from the diff viewer.
- [ ] **Reflog browser.** Read-only view of `git reflog` for recovery flows.
- [ ] **Submodule support.** Status + nested diffs.
- [ ] **Multi-repo workspace.** Open a workspace of related repos with a single window.

## P3 — Polish / extensibility

- [ ] **Custom keybindings.** User-editable mapping in Settings, persisted to `config.toml`.
- [ ] **Plugin system.** Stable Tauri command surface + a manifest format for community extensions.
- [ ] **Light theme palette pass.** [FRONTEND.md](FRONTEND.md) describes a target Apple-system aesthetic; the shipped CSS is still GitHub Primer-flavored. Migrate component-by-component when touched.
- [ ] **Marketing site.** Section on FRONTEND.md already specifies the design language for it.
- [ ] **Localization scaffolding.** No strings catalog today.

## Infrastructure

- [ ] **App signing + notarization (macOS).** Configure in `src-tauri/tauri.conf.json` for distributable DMGs.
- [ ] **Auto-update.** Tauri's updater plugin needs a signed release feed.
- [ ] **CI build matrix.** macOS / Linux / Windows on push.
- [ ] **Crash reporting.** Opt-in only; route through a privacy-respecting endpoint.
- [ ] **Release versioning.** Currently `0.0.1`. Bump to `0.1.0` once P0 items are clear and the PR view ships.

## Recently completed

- [x] Retired stale migration docs (`INTEGRATION_CHECKLIST.md`, `POST_IMPLEMENTATION_TODO.md`, `QUICKSTART.md`); open items folded into this roadmap.
- [x] Embedded terminal with multi-session support, per-repo cleanup, dock at bottom of main content.
- [x] HEAD SHA polling for auto-refreshing the commit log on external git activity.
- [x] macOS PATH fix at startup so `claude` / `gh` / brew binaries resolve when launched from Finder.
