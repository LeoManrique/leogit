# LeoGIT — Go Implementation Plan

## Overview

A GUI alternative for Git, built in Go. Wraps system `git` and `gh` CLI tools instead of bundling them. Focused on power-user workflows with AI commit message generation.

Binary: `leogit`

## Tech Stack

- **Go** — single static binary, cross-platform
- **Bubbletea** — TUI framework (Elm architecture)
- **Lipgloss / Bubbles** — styling and pre-built components
- **Chroma** — syntax highlighting for diffs
- **System `git`** — all git operations
- **System `gh`** — GitHub API, auth, PRs, issues
- **Mouse support** — enabled via bubbletea, compatible with GPM (no DE required)

## Layout

Clean layout, no panel density.
Vim-style keybindings (`j/k/h/l`, space, enter).

### Pane Focus Shortcuts

Numbers are positional (left-to-right, top-to-bottom) within the current tab.
`Tab` switches between Changes and History tabs.

| Key   | Changes Tab      | History Tab      |
|-------|------------------|------------------|
| `1`   | Changed Files    | Commit List      |
| `2`   | Diff Viewer      | Changed Files    |
| `3`   | Commit Message   | Diff Viewer      |
| `` ` ``| Terminal/Log    | Terminal/Log     |
| `Tab` | → History        | → Changes        |

### Header Bar (always visible)

```
┌──────────────────────────────────────────────┐
│ ⎇ leogit   │ ᚠ leo-fork   │ ↻ Fetch (28m)   │
└──────────────────────────────────────────────┘
```

- **Repository**: current repo name, dropdown to switch or clone
- **Branch**: current branch, dropdown to switch/create
- **Quick Action**: context-aware
  - No divergence → Fetch
  - Behind only → Pull
  - Ahead only → Push
  - Ahead + Behind → dropdown: Pull, Push, Force Push, Pull --rebase

### Changes Tab

```
┌─────────────────────┬──────────────────────────────────┐
│ [Changes] History   │                                  │
├─────────────────────┤         Diff Viewer              │
│                     │    (syntax highlighted,           │
│   Changed Files     │     unified or side-by-side)     │
│   [checkbox list]   │                                  │
│                     ├──────────────────────────────────┤
├─────────────────────┤                                  │
│ Commit Message      │  [Terminal] Log  (collapsible)   │
│ [summary]           │   $ interactive shell            │
│ [description]       │   — or —                         │
│ [AI] [Commit]       │   git status, git diff, ...      │
└─────────────────────┴──────────────────────────────────┘
```

- **Changed Files**: list of working directory changes with checkboxes for staging
  - `space` to toggle staging, `a` to stage/unstage all
  - Supports hunk-level and line-level staging
  - Status icons: `[M]` modified, `[+]` added, `[-]` deleted, `[R]` renamed, `[!]` conflicted
  - `j/k` to navigate, `enter` to view diff
- **Diff Viewer**: syntax-highlighted diff of the selected file
  - Toggle unified / side-by-side with `s`
  - Scroll with `j/k` or mouse
- **Commit Message**: summary + description fields
  - `AI` button triggers Claude or Ollama commit generation
  - `Commit` button creates the commit with staged files
  - Co-author support
- **Terminal / Log** (collapsible, toggle with `` ` ``):
  - **Terminal tab**: interactive shell subprocess at repo root
  - **Log tab**: scrollable log of every git/gh command the app runs

### History Tab

```
┌──────────────────┬──────────────────┬──────────────────┐
│ Changes [History]│ Commit Details                      │
├──────────────────┤ summary, author, date, hash         │
│                  ├──────────────────┬──────────────────┤
│  Commit List     │ Changed Files    │   Diff Viewer    │
│  (scrollable)    │  file1.go  [M]   │  (selected file, │
│                  │  file2.go  [+]   │   syntax         │
│                  │  file3.go  [-]   │   highlighted)   │
│                  │                  │                  │
└──────────────────┴──────────────────┴──────────────────┘
```

- **Commit List**: scrollable log of commits on current branch
  - Lazy-loaded in batches as you scroll
  - Shows SHA, author, date, summary
  - `j/k` to navigate, `enter` to select
  - Advanced operations (cherry-pick, revert, amend) via embedded terminal
- **Commit Details**: summary, full message, author, date, hash of selected commit
- **Changed Files**: files modified in the selected commit
  - `j/k` to navigate, `enter` to view diff
  - Status icons same as Changes tab
- **Diff Viewer**: syntax-highlighted diff of the selected file in the selected commit

## Git Command Reference

Exact commands for each operation. All use `-z` (NUL-separated) where possible
for safe path handling. Environment: `TERM=dumb` always set.

### Status

```
git --no-optional-locks status --untracked-files=all --branch --porcelain=2 -z
```

Porcelain v2 gives 2-char status codes (`XY`) mapped to user-friendly labels:

| Raw code | Label        | Icon  |
|----------|-------------|-------|
| `??`     | New          | `[+]` |
| `.M`     | Modified     | `[M]` |
| `.D`     | Deleted      | `[-]` |
| `R.`     | Renamed      | `[R]` |
| `UU` etc | Conflicted   | `[!]` |

### Diff

```bash
# Working tree changes (tracked file)
git diff --no-ext-diff --patch-with-raw -z --no-color HEAD -- <path>

# Working tree changes (untracked file)
git diff --no-ext-diff --patch-with-raw -z --no-color --no-index -- /dev/null <path>

# Staged changes
git diff --no-ext-diff --patch-with-raw --no-color --staged [<commitish>]

# Single commit diff
git log <sha> -m -1 --first-parent --patch-with-raw --format= -z --no-color -- <path>

# Commit range diff
git diff <oldest>^ <latest> --patch-with-raw --format= -z --no-color -- <path>

# Whitespace-ignored: add -w flag to any of the above
```

### Staging & Unstaging

```bash
# Stage entire files (3-step process)
git update-index --add --remove --force-remove --replace -z --stdin  # renamed old paths
git update-index --add --remove --replace -z --stdin                 # normal files
git update-index --add --remove --force-remove --replace -z --stdin  # deleted files

# Stage partial (hunk/line selection) — via constructed patch
git apply --cached --unidiff-zero --whitespace=nowarn -  # stdin: patch

# Unstage files
git reset HEAD -- <paths>

# Discard working tree changes — via inverse patch
git apply --unidiff-zero --whitespace=nowarn -  # stdin: reverse patch (no --cached)
```

### Commit

```bash
# Standard commit (message via stdin)
git commit -F -

# Amend
git commit -F - --amend

# Merge commit (no editor)
git commit --no-edit --cleanup=strip
```

### Log / History

```bash
git log [<range>] --date=raw --max-count=<N> --skip=<N> \
  --format='%H%n%h%n%s%n%b%n%an <%ae> %ad%n%cn <%ce> %cd%n%P%n%(trailers:unfold,only)%n%D' \
  --no-show-signature --no-color --
```

Fields: SHA, short SHA, summary, body, author+date, committer+date, parents, trailers, refs.
Lazy-loaded in batches.

### Branch

```bash
git branch <name> [<start-point>] [--no-track]   # create
git checkout [--progress] <branch> --             # switch (local)
git checkout [--progress] <branch> -b <local> --  # switch (remote → local)
git branch -D <name>                              # delete local
git push <remote> :<branch>                       # delete remote
git branch --format='%(refname:short)'            # list
```

### Fetch / Push / Pull

```bash
# Fetch
git fetch [--progress] --prune [--recurse-submodules=on-demand] <remote>

# Push
git push [--progress] <remote> [<local>:<remote>] [--set-upstream] [--force-with-lease]

# Pull
git pull [--ff] [--recurse-submodules] [--progress] <remote>

# Ahead/behind count
git rev-list --left-right --count <range> --
```

### Clone

```bash
git -c init.defaultBranch=<branch> clone --recursive [--progress] [-b <branch>] -- <url> <path>
```

### Merge

```bash
git merge [--squash] <branch>
git merge --abort
git merge-base <commitish-a> <commitish-b>
```

## Hunk & Line-Level Staging

The most complex feature. Pipeline:

### 1. Parse diff into structured data

Parse unified diff output (`git diff --patch-with-raw`) into:

```
DiffHunk {
    Header    { OldStart, OldCount, NewStart, NewCount }
    Lines[]   { Text, Type(Context|Add|Delete|Hunk), OldLineNo, NewLineNo }
}
```

Hunk header regex: `@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@`

### 2. Track selection state

```
DiffSelection {
    DefaultState   All | None          // initial: all lines selected
    DivergingLines Set<int>            // lines that differ from default
    SelectableLines Set<int>           // only Add/Delete lines are selectable
}
```

Key operations:
- `WithLineSelection(line, selected)` — toggle single line
- `WithRangeSelection(from, count, selected)` — toggle hunk/range
- `IsSelected(line)` — check if line is in staging selection

### 3. Generate partial patch from selection

For each hunk, iterate lines and build a new patch:
- **Context lines**: always include (oldCount++, newCount++)
- **Selected Add lines**: include as-is (newCount++)
- **Selected Delete lines**: include as-is (oldCount++)
- **Unselected Add lines**: skip entirely
- **Unselected Delete lines**: convert to context — change `-` prefix to ` ` (oldCount++, newCount++)

Recalculate hunk header with adjusted counts.

### 4. Apply patch to index

```bash
git apply --cached --unidiff-zero --whitespace=nowarn -  # stdin: generated patch
```

### 5. Discard selected changes (inverse patch)

Inverse logic: selected Adds → Deletes, selected Deletes → Adds. Apply without `--cached`:

```bash
git apply --unidiff-zero --whitespace=nowarn -  # stdin: inverse patch
```

### Staging workflow summary

| Selection type | Method |
|---------------|--------|
| Entire file   | `git update-index --add --remove` |
| Partial file  | `git apply --cached` with constructed patch |
| No lines      | Skip file |

## AI Commit Message Generation

Two providers, same interface. Lives in `internal/ai/`.

### Provider Interface

```go
type CommitMessageProvider interface {
    ID() string                                      // "claude" or "ollama"
    DisplayName() string
    IsAvailable() (bool, error)
    GenerateCommitMessage(diff string) (*CommitMessage, error)
}

type CommitMessage struct {
    Title       string   // ≤50 chars, imperative mood
    Description string   // what changed and why
}
```

### Prompt Template (shared by both providers)

```
You are a Git commit message generator. Analyze the provided git diff
and generate a commit message.

Return ONLY valid JSON in this exact format:
{"title": "≤50 char summary in imperative mood", "description": "what changed and why"}

Rules:
- Title MUST be ≤50 characters, imperative mood ("Add", "Fix", "Update")
- Description explains what and why, not how
- Return ONLY the JSON object

Git diff:
```diff
{staged diff from: git diff --no-ext-diff --patch-with-raw --no-color --staged}
`` `
```

**Input**: full staged diff only. No commit history, no file list, no branch info.

### Claude CLI Provider

```bash
claude --print --output-format json --model <model>
# stdin: full prompt
# default model: "haiku"
# timeout: 120s
# max diff size: 20MB
```

Response: `{"type":"result","result":"```json\n{...}\n```"}` — extract `.result`,
strip markdown fences, parse JSON. Field normalization: accepts `title`/`summary`/
`subject`/`message` for title, `description`/`body`/`details` for description.

Binary lookup order (Unix): `~/.local/share/pnpm/claude`, `~/.local/bin/claude`,
`/usr/local/bin/claude`, `/usr/bin/claude`, then `$SHELL -l -c 'which claude'`.

### Ollama HTTP Provider

```
POST {server_url}/api/generate
{
  "model": "<model>",
  "prompt": "<full prompt>",
  "stream": false,
  "format": "json"
}

# default model: "tavernari/git-commit-message:latest"
# default server: "http://localhost:11434"
# timeout: 120s
# max diff size: 50MB
# availability check: GET {server_url}/api/tags (5s timeout)
```

Response: `{"response":"..."}` — extract `.response`, strip fences, parse JSON,
normalize fields same as Claude.

### Error Handling

| Code | Retryable | Meaning |
|------|-----------|---------|
| `EMPTY_DIFF` | no | Nothing staged |
| `DIFF_TOO_LARGE` | no | Exceeds size limit |
| `TIMEOUT` | yes | Provider didn't respond in time |
| `CLI_ERROR` | yes | Claude CLI non-zero exit |
| `CONNECTION_ERROR` | yes | Can't reach Ollama server |
| `MODEL_NOT_FOUND` | no | Ollama model not pulled |
| `API_ERROR` | yes | Ollama HTTP 500+ |
| `INVALID_RESPONSE` | yes | Unparseable JSON |

## Configuration

TOML file at `~/.config/leogit/config.toml`.

```toml
# ── Appearance ────────────────────────────────────
[appearance]
theme = "dark"                    # "light", "dark", "system"

# ── Diff Display ──────────────────────────────────
[diff]
side_by_side = false              # true = split view, false = unified
hide_whitespace = false           # ignore whitespace-only changes
tab_size = 4                      # tab width in diff viewer
context_lines = 3                 # lines of context around changes

# ── AI Commit Messages ────────────────────────────
[ai.claude]
model = "haiku"                   # claude model name
timeout = 120                     # seconds
max_diff_size = 20971520          # 20MB

[ai.ollama]
model = "tavernari/git-commit-message:latest"
server_url = "http://localhost:11434"
timeout = 120                     # seconds
max_diff_size = 52428800          # 50MB

# ── Git Behavior ──────────────────────────────────
[git]
fetch_interval = 300              # auto-fetch every N seconds (0 = disabled)

# ── Confirmations ─────────────────────────────────
[confirmations]
discard_changes = true
force_push = true
branch_delete = true

# ── Repository Discovery ──────────────────────────
[repos]
mode = "folders"                  # "folders" = auto-discover, "manual" = explicit list

# Folders mode: scan these directories for git repos (non-recursive by default)
scan_paths = ["~/Dev", "~/Projects"]
scan_depth = 1                    # how many levels deep to look for .git dirs

# Manual mode: explicitly listed repos
# manual_paths = ["/home/leo/my-project", "/home/leo/other-repo"]

# State stored separately in ~/.config/leogit/repos-state.json
# Tracks last_opened timestamps, per-repo branch, etc. — not user-edited.
```


## GH CLI Command Reference

All PR commands use `--json` for machine-parseable output. Authentication is
mandatory — the app checks on startup.

### Authentication

```bash
gh auth status
# Exit code 0 = logged in, exit code 4 = not logged in
# On exit code 4, show fullscreen message:
#   "GitHub authentication required. Run: gh auth login"
```

### Pull Request Commands

```bash
# List PRs (JSON output for TUI rendering)
gh pr list --state <open|closed|merged|all> --author <login> --limit <N> \
  --json number,title,author,baseRefName,headRefName,state,isDraft,createdAt,url

# View PR details
gh pr view <number> --json number,title,body,author,state,reviews,comments,commits,files

# Create PR
gh pr create --title <title> --body <body> --base <branch> [--draft] [--fill]

# Checkout PR branch locally
gh pr checkout <number> [--branch <local-name>] [--force]

# PR status (current branch + authored + review-requested)
gh pr status --json number,title,state,mergeable

# CI checks for a PR
gh pr checks <number> --json name,state,bucket
# bucket values: pass, fail, pending, skipping, cancel
```

### Key JSON Fields

Available across `gh pr list/view/status`:
`number`, `title`, `body`, `author`, `state`, `isDraft`, `baseRefName`,
`headRefName`, `mergeable`, `reviewDecision`, `statusCheckRollup`,
`additions`, `deletions`, `changedFiles`, `createdAt`, `updatedAt`, `url`

## Startup Flow

```
1. Check `gh auth status` → exit code 4?
   └─ YES → fullscreen: "GitHub login required. Run: gh auth login"
            Block all functionality. Re-check on any keypress.
   └─ NO  → continue

2. CLI arg provided? (`leogit /path/to/repo`)
   └─ YES → open that repo directly
   └─ NO  → check repos-state.json for last_opened
            └─ found → open last repo
            └─ not found → show repo picker (scan_paths or manual list)

3. Load config.toml, discover repos, run initial git status, render UI
```

## Focus & Input Model

Panes have two states: **navigable** (receives pane-level shortcuts) and
**focused** (captures all keystrokes, blocking global shortcuts).

### Navigable mode (default)

Global and pane shortcuts work normally (`1`/`2`/`3`, `Tab`, `q`, `?`, etc.).
`j/k` navigate within the highlighted pane. `enter` or typing in an input
field switches to focused mode.

### Focused mode

All keystrokes go to the active pane. Global shortcuts are blocked.
`Esc` returns to navigable mode.

Panes that enter focused mode:
- **Commit Message** — summary/description text input
- **Terminal** — PTY subprocess receives all input
- **Branch dropdown** — search/filter input
- **Repo picker** — search/filter input

### Embedded Terminal

Uses `creack/pty` (v1.1.24) to spawn an interactive shell subprocess at repo root.

- Toggle visibility: `` ` `` key
- Focus terminal: `` ` `` when already visible, or `enter` when highlighted
- Unfocus: `Esc` → returns to navigable mode
- Resize while focused: `Ctrl+Shift+Up` / `Ctrl+Shift+Down` to grow/shrink by 1 row
  - Minimum height: 3 rows
  - Maximum height: 80% of terminal height
- All keystrokes (including `Ctrl+C`, `Ctrl+D`) go to the PTY while focused
- Terminal pane re-renders on PTY output via bubbletea `Cmd`

### Error Display

Errors show as a **centered modal dialog** overlaying the current view.
- Title bar with error category (e.g. "Git Error", "Network Error", "AI Error")
- Error message body
- For retryable errors: `[Retry]` and `[Dismiss]` buttons
- For non-retryable: `[OK]` button only
- `Esc` or `enter` dismisses
- Background fetch notifications use the same modal pattern

## Refresh Strategy

No file watcher. Poll `git status` on a timer and on re-focus.

- **Active polling**: run `git status --no-optional-locks --porcelain=2 -z` every
  2 seconds while the app is in the foreground
- **Re-focus refresh**: if the terminal supports focus events (most modern terminals
  do via `\x1b[?1004h`), trigger a full refresh on focus-in
- **Post-action refresh**: always refresh after any git command (commit, stage,
  checkout, pull, etc.)
- **Background fetch**: runs on a separate goroutine per `fetch_interval` config.
  Sends a `FetchCompleteMsg` to bubbletea. If ahead/behind changed, show modal
  notification.

## Keybinding Reference

### Global (navigable mode)

| Key | Action |
|-----|--------|
| `1` / `2` / `3` | Focus pane (positional, per tab) |
| `` ` `` | Toggle terminal pane |
| `Tab` | Switch Changes / History tab |
| `q` | Quit |
| `?` | Show help overlay |
| `Esc` | Unfocus pane / dismiss modal |
| `Ctrl+C` | Quit (fallback) |

### Changed Files pane

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate up/down |
| `space` | Toggle staging for selected file |
| `a` | Stage/unstage all files |
| `enter` | View diff of selected file |

### Diff Viewer pane

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down/up |
| `s` | Toggle unified / side-by-side |

### Commit Message pane

| Key | Action |
|-----|--------|
| `enter` | Focus summary field (enters focused mode) |
| `Tab` | Switch between summary and description (while focused) |
| `Ctrl+Enter` | Create commit |
| `Esc` | Unfocus (back to navigable) |

### Commit List pane (History tab)

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate commits |
| `enter` | Select commit (show details + files) |

### Terminal pane (focused mode)

| Key | Action |
|-----|--------|
| All keys | Sent to PTY subprocess |
| `Esc` | Unfocus terminal (back to navigable) |
| `Ctrl+Shift+Up` | Grow terminal by 1 row |
| `Ctrl+Shift+Down` | Shrink terminal by 1 row |

## Core Features (TUI-native)

- Repository management (open, switch, recent, clone)
- Git status, staging (hunk/line-level), commit
- Branch management (create, switch, delete, merge)
- Push / pull / fetch
- Background auto-fetch with configurable interval
- Visual diffs (unified + side-by-side, syntax highlighted)
- Image diffs (Kitty/iTerm2/Sixel protocols, fallback to external)
- Commit history log with diff viewer
- Pull request workflows (create, list, view, checkout)
- AI commit messages (Claude CLI, Ollama HTTP)
- Config via TOML file

## Terminal-Deferred Features

Advanced git operations are **not built into the TUI** — users drop into the
embedded terminal pane for anything beyond the core workflow:

- Stash (`git stash`, `git stash pop`, etc.)
- Rebase (`git rebase -i`, continue, abort)
- Cherry-pick, squash, reorder commits
- Revert, amend, undo commits
- Tags (`git tag`, `git tag -d`, etc.)
- Conflict resolution (manual via editor)
- .gitignore editing
- Submodules, LFS
- CI/CD (`gh run list`, `gh run view`, `gh run rerun`)
- Issue management (`gh issue list`, `gh issue create`)
- PR reviews (`gh pr review`, `gh pr comment`)

This keeps the TUI focused and lean. The embedded terminal + command log
ensures nothing is out of reach.

## Project Structure

```
cmd/
  leogit/main.go       — TUI entry point
  leogit-desktop/main.go      — Wails entry point (future)
internal/
  core/          — shared interfaces and types
  git/           — git command wrappers (UI-agnostic)
  gh/            — gh CLI wrappers (UI-agnostic)
  diff/          — diff parsing and staging logic (UI-agnostic)
  ai/            — claude + ollama providers (UI-agnostic)
  config/        — TOML config loading (UI-agnostic)
  tui/
    app/         — bubbletea model, update, view
    views/       — TUI screens (repos, changes, history, commit)
    render/      — lipgloss diff rendering, image protocols
```

Core packages (`git/`, `gh/`, `diff/`, `ai/`, `config/`) have zero UI
dependencies. This allows a future Wails GUI to import the same core
and only replace the presentation layer.

## Dependencies

Module: `github.com/LeoManrique/leogit`

```
github.com/charmbracelet/bubbletea   v2.0.2    — TUI framework (v2, stable)
github.com/charmbracelet/bubbles     v2.0.0    — pre-built components (v2 for bubbletea v2)
github.com/charmbracelet/lipgloss    v1.1.0    — styling (v1, stable; v2 still beta)
github.com/alecthomas/chroma/v2     v2.15.0    — syntax highlighting
github.com/creack/pty                v1.1.24   — PTY for embedded terminal
github.com/BurntSushi/toml           v1.6.0    — config parsing
```

## Build & Install

```bash
go install github.com/LeoManrique/leogit/cmd/leogit@latest

# or from source
go build -o leogit ./cmd/leogit
```

Prerequisites: `git` and `gh` must be in `$PATH`.
