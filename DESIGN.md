# leogit — Functional Design

Visual / interaction design language lives in [FRONTEND.md](FRONTEND.md). This document covers user-facing **features and flows** — what leogit does, in what order, and how the pieces fit together.

## Product intent

leogit is a desktop Git client for developers who already know git on the command line but want a calm visual surface for the workflows they repeat dozens of times a day: staging, committing, browsing history, switching branches, opening PRs. It stays out of the way: every git operation maps to a real `git` invocation against the on-disk repo, so the user can shell out at any moment and the two surfaces stay in sync.

The embedded terminal is part of that contract — it runs in the repo directory and shares the same working tree as the UI.

## Top-level flows

### 1. Launch and repo discovery

1. App starts and renders a loading screen while the backend reads `~/.config/leogit/config.toml` (creating defaults on first run) and `~/.config/leogit/repos-state.json`.
2. `gh auth status` is probed in the background so the PR button can gate itself; failure is non-fatal.
3. The configured `scan_paths` are expanded (`~` → home) and walked up to `scan_depth` levels (default 3). The first `.git` found in a subtree stops descent.
4. Resolution:
   - If `last_opened_repo` is still in the discovered list, jump straight into the main view.
   - If exactly one repo was discovered, open it and persist that as `last_opened_repo`.
   - Otherwise show the **Repo Picker** modal with a fuzzy-filter search input.

Defaults fall back to `~/Dev`, `~/dev`, `~/code`, `~/Code`, `~/Projects`, `~/src` when the user has not configured scan paths.

### 2. Main view

Two-column layout: a resizable sidebar on the left and a content area on the right, with the embedded terminal docked at the bottom of the content column. All three split widths/heights persist in `localStorage` (`leogit:sidebarWidth`, `leogit:commitHeight`, `leogit:commitFilesWidth`).

**Sidebar** carries a tab bar (`Changes` / `History`) and changes shape per tab:
- *Changes tab:* file list + commit composer.
- *History tab:* commit list (virtualized).

**Main content** mirrors the tab:
- *Changes tab:* diff viewer for the active file, or an empty state ("Working tree is clean" / "Select a file to view its diff").
- *History tab:* commit detail card + a nested two-pane view (commit files on the left, per-file diff on the right). The commit files pane is independently resizable.

**Header** sits on top of the main content: repo switcher trigger, branch dropdown trigger, ahead/behind indicator, merge state badge, and the action cluster (Pull / Push / PRs / Refresh / Settings / Help). The repo switcher opens a popover identical in shape to the branch dropdown — a filter input plus the discovered-repos list (basename + muted path, current repo first and dotted) — and selecting one resets the in-memory repo state, persists `last_opened_repo`, and refreshes status / branches / log / auto-fetch for the new repo without restarting the app.

### 3. Staging and committing

- File list shows every change reported by `git status --porcelain=v2 -z`, including untracked files. Each row carries a colored single-letter status marker (`A` / `M` / `D` / `R` / `U`) plus a checkbox. A header row ("N changed files" with an indeterminate-aware master checkbox) sits above the list and toggles every file's inclusion in one click.
- Selection is opt-out: every detected change starts staged. The set of paths the user explicitly deselected is remembered so that fresh `git status` polls don't re-stage them. When a file disappears from the working tree the deselection forgets it too.
- Clicking a row opens its diff in the right pane and seats the row-anchor at that file. Shift+clicking another row extends a visual multi-row highlight from the anchor through the clicked row (and still activates the clicked row's diff). Pressing **Space** on a focused row toggles that row's checkbox; when the row is part of a multi-row selection, Space bulk-toggles every selected row's inclusion (include all if any are excluded, else exclude all). Shift+clicking a *checkbox* (separate anchor) sets every checkbox in the range to whatever the clicked checkbox is about to become — Finder / Gmail semantics — so the two gestures stay independent.
- The diff viewer renders unified diffs with line numbers in both gutters, optional Shiki syntax highlighting, and an optional side-by-side layout. The `tab_size` and `hide_whitespace` settings are honored live (changing `hide_whitespace` re-fetches the current file's diff). Binary files (where git can't produce a line-by-line diff) keep their file-header but the body is replaced with a "This binary file has changed." stand-in.
- When a hunk has the same number of consecutive delete and add lines, the backend pairs them up and annotates each line with the character range that actually changed (longest-common-prefix / common-suffix). The viewer overlays a brighter backplate on just that range on top of the existing Shiki tokens, so `Relay` → `Metrics` inside an otherwise identical line gets called out without losing the syntax colour. Lines longer than 1024 characters skip the annotation to avoid noisy diffs.
- Commit composer at the bottom of the sidebar has a single-line **Summary** input with a 72-character soft counter (turns yellow past the limit) and a multi-line **Description** textarea.
- **Commit** runs `git reset HEAD` first to clear the index, then re-stages only the user's selected paths (`update-index` handles renames, deletes, and normal modifications differently), then pipes the formatted message to `git commit -F -`. The message is built via the backend's `format_commit_message` so co-author trailers stay consistent.
- **Amend last commit** is reachable by right-clicking the topmost commit in History → "Amend last commit…". The composer enters amend mode: summary and description are pre-filled from the commit (Co-Authored-By trailers are extracted and re-applied via `format_commit_message`), a notice band reads "Your changes will modify your most recent commit. [Stop amending]", the primary button label swaps to "Amend commit", and the must-select-files gate is relaxed (message-only amends are allowed). On commit, the backend runs `git commit -F - --amend`. After amending a commit that was already pushed, the header naturally surfaces `↑ 1 ↓ 1` and the user can complete the workflow via the Force push (with lease) item in the push button dropdown.

### 4. AI commit message generation

- The composer has a provider dropdown (Claude / Ollama) and a **Generate** button.
- Generate sends the unified diff of the currently selected files only (untracked files are diffed via `--no-index`). The diff goes to the chosen provider with a strict JSON prompt.
- **Claude**: shells out to the `claude` CLI (`--print --output-format json --model <model>`). Model defaults to `sonnet`. Diff size is capped at 20 MB.
- **Ollama**: posts to `http://localhost:11434/api/generate` (or `ollama_server_url`). Model defaults to `tavernari/git-commit-message:latest`. Diff size is capped at 50 MB.
- The JSON response is parsed flexibly (accepts `title`/`summary`/`subject`/`message` and `description`/`body`/`details`). If parsing fails, the first line becomes the title and the rest becomes the description.
- Both providers run with a 120-second timeout. Errors surface inline below the textarea.

### 5. History browsing

- Commit list is virtualized (50 px rows) and pages 50 commits at a time, prefetching when the user is within 200 px of the bottom.
- Each row is two lines. First line: the commit summary plus right-aligned indicators — a tag pill (e.g. `v0.1.0`, with a `+N` pill when a commit has multiple tags) and the "not yet pushed" arrow. Second line: the author name · a humanized relative date (`just now` / `Nm ago` / `Nh ago` / `Nd ago` / locale date past a week). The SHA is not shown in the list (it's a row away from useful and the detail card carries it); the right-click menu's "Copy SHA" covers the copy case.
- Selecting a commit loads its detail card (full message, author, committer date, trailers, SHA with copy-to-clipboard) and its file list. The first file's diff opens automatically.
- Per-file commit diffs use `git log -1 -p --first-parent` to produce a proper unified diff (not file contents).
- Right-clicking a commit opens a context menu: "Amend last commit…" and "Undo last commit…" (both enabled only on the topmost commit, with Undo further gated on the commit being unpushed), then "Copy SHA" (full hash) and "Copy Tag" (enabled only when the commit is tagged; space-joins multiple tags). The same menu primitive will host future actions (Reset / Revert / Cherry-pick / Create branch from).

### 6. Branches

- The header branch button opens the **Branch Dropdown** overlay: local branches first, then remote branches, sorted by most recent committer date.
- Switching is a `git checkout <name> --`. Status and branch list refresh automatically.
- **Create branch**: inline input in the dropdown; new branch is created from the current HEAD and immediately switched to.
- **Delete branch**: row-level X icon on local branches (only non-current). Confirmation step before `git branch -D`.
- Other supported commands (no UI yet): rename, delete remote branch, merge, squash merge, merge abort, conflict detection (`MERGE_HEAD` presence).

### 7. Pull requests

- The PRs button is disabled until `gh auth status` succeeds in the background.
- The backend exposes: list PRs (with state filter), get checks for a PR, create PR (with title/body/base/draft), create PR via `gh pr create --fill`, checkout PR by number, and lookup the PR for the current branch.
- The frontend currently surfaces `gh pr list` and check status — a richer PR view is planned (see ROADMAP).

### 8. Remote synchronization

- **Header buttons**: Pull (`git pull --ff --recurse-submodules`) and Push (`git push --progress`, auto-adds `--set-upstream` when no upstream is configured). Each shows a small numeric badge with the behind/ahead count (`Pull (2)`, `Push (3)`) and a spinner while busy.
- The Push button is a split button: the main face triggers a plain push; a chevron on the right opens a small dropdown with "Push" and "Force push (with lease)…". The force-push item is enabled whenever there's an upstream and opens a confirmation dialog explaining that `--force-with-lease` will overwrite the remote branch but will refuse the push if someone else has pushed since the last fetch. leogit never uses bare `--force`.
- **Auto-fetch**: when `auto_fetch` is true, a background timer runs `git fetch --prune --recurse-submodules=on-demand` against the first remote every `fetch_interval_ms` (default 30 s). Fetches are skipped while the user is typing in an input or while the window is hidden.
- **Status polling**: every 2 s the frontend re-runs `get_status` silently and polls `git rev-parse HEAD`. If the SHA changed (commit/checkout/external `git` action) the commit log refreshes automatically. Polling and re-fetch also run on `visibilitychange` and window focus.
- Ahead/behind counts and merge state come from the porcelain v2 branch headers; the header shows `↑ N ↓ N` or "up to date", and a `MERGING` chip when `MERGE_HEAD` exists.

### 9. Embedded terminal

- Docked at the bottom of the main content area, ~280 px tall when expanded.
- Toggle with the backtick key (`` ` ``) or the header chevron. A separate `+` button spawns additional sessions; the X button kills the current PTY.
- Spawns the user's `$SHELL` (`/bin/zsh` fallback on Unix, `cmd.exe` on Windows) with the repo path as cwd. Parent env is forwarded plus `TERM=xterm-256color`.
- xterm.js handles rendering; FitAddon syncs cols/rows to the backend on resize. Output is streamed back via per-pid Tauri events (`terminal-output-<pid>`); `terminal-closed-<pid>` fires when the shell exits.
- Switching repos kills the active terminal session so we never leak shells from a prior repo.

### 10. Settings

A modal overlay grouped into Appearance / Diff / Git / AI / Repository discovery. Saves to `~/.config/leogit/config.toml`. Theme changes apply immediately on save; diff settings (`hide_whitespace`, `syntax_highlighting`, `side_by_side_diff`, `tab_size`) re-render the active diff; `fetch_interval_ms` and `auto_fetch` are read on startup (see ROADMAP — live re-arming).

## Keyboard surface

The full list lives in [HelpOverlay.svelte](tauri-app/src/lib/views/HelpOverlay.svelte). Single-key shortcuts only fire when no input/textarea has focus; meta-modifier shortcuts work everywhere.

| Key | Action | Scope |
|---|---|---|
| `Ctrl/Cmd + Enter` | Commit selected files (or "Amend commit" when in amend mode) | Commit composer focused |
| `Ctrl/Cmd + G` | Generate commit message with AI | Commit composer focused |
| `Ctrl/Cmd + P` | Cycle AI provider (Claude ↔ Ollama) | Commit composer focused |
| `Ctrl/Cmd + R` | Refresh status | Global |
| `Ctrl/Cmd + L` | Toggle Changes / History tab | Global |
| `B` | Open branch picker | Global, no input focused |
| `,` | Open Settings | Global, no input focused |
| `?` | Open Help | Global, no input focused |
| `` ` `` | Toggle terminal (collapsed ↔ expanded) | Global, no input focused |
| `Space` | Toggle file selection on focused row | File list focused |
| `↑` / `↓` | Move active file (loads its diff) | File list focused |
| `Home` / `End` (or `Cmd+↑` / `Cmd+↓`) | Jump to first / last file | File list focused |
| `Escape` | Close any open overlay / dismiss error | Global |

## Persistence model

- `~/.config/leogit/config.toml` — user settings (theme, AI, scan paths, diff prefs, fetch interval).
- `~/.config/leogit/repos-state.json` — last-opened repo only.
- `localStorage` — UI splitter sizes (`leogit:sidebarWidth`, `leogit:commitHeight`, `leogit:commitFilesWidth`).
- Per-session in-memory — user-deselected files, active file / commit / diff, terminal PTY ids.

Nothing about the user's code is sent anywhere except the diff that is explicitly passed to the chosen AI provider when **Generate** is clicked.
