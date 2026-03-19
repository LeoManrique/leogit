# LeoGIT — Implementation Guide

A GUI Git client built in Go — suitable for beginner and advanced visual Git users.
Binary name: `leogit`. Module: `github.com/LeoManrique/leogit`.

Sequential phases following the user journey from launch to push, then additional features.
Each phase tells you exactly what to build, which files to create, and what the code should do.
This document is fully self-contained — everything you need to implement is here.

---

## Phase 0 — Project Setup

This phase creates the Go module, installs dependencies, and scaffolds the directory structure.
Nothing runs yet — this is just the skeleton.

### 0.1 Create the Project Directory & Go Module

Create a new directory for the project and initialize the Go module:

```bash
mkdir leogit && cd leogit
go mod init github.com/LeoManrique/leogit
```

> **Go version requirement**: This project uses the built-in `min()` and `max()` functions
> introduced in **Go 1.21**. Make sure you have Go 1.21 or later installed (`go version`).
> After `go mod init`, your `go.mod` will contain a `go` directive matching your installed
> version — verify it says `go 1.21` or higher.

### 0.2 Create the Directory Structure

Create every directory you will need. Do this all at once so you have the full picture:

```
cmd/
  leogit/          ← the binary entry point lives here
internal/
  core/                    ← shared types and interfaces (used by everything)
  git/                     ← wrappers around system `git` commands
  gh/                      ← wrappers around system `gh` commands
  diff/                    ← diff parsing and hunk/line staging logic
  ai/                      ← AI commit message providers (Claude, Ollama)
  config/                  ← TOML config file loading and defaults
  tui/
    app/                   ← main bubbletea model (ties everything together)
    views/                 ← individual screens: repo picker, changes, history
    components/            ← reusable TUI components: modal, header, file list
    render/                ← lipgloss styling, diff coloring, syntax highlighting
```

Create all of these:

```bash
mkdir -p cmd/leogit
mkdir -p internal/{core,git,gh,diff,ai,config}
mkdir -p internal/tui/{app,views,components,render}
```

> **Note**: The `{core,git,...}` syntax is **brace expansion** — it works in bash and zsh.
> If you are using a different shell (e.g. plain `sh`), create each directory separately:
> `mkdir -p internal/core internal/git internal/gh` ... and so on.

### 0.3 Install Dependencies

```bash
go get charm.land/bubbletea/v2@v2.0.2
go get charm.land/bubbles/v2@v2.0.0
go get charm.land/lipgloss/v2@v2.0.2
go get github.com/alecthomas/chroma/v2@v2.15.0
go get github.com/creack/pty@v1.1.24
go get github.com/BurntSushi/toml@v1.6.0
```

### 0.4 Verify

After running the `go get` commands, your `go.mod` should list the module name and all six
dependencies (in the `require` block). You can open `go.mod` in a text editor to confirm.

> **Important**: Do NOT run `go mod tidy` at this point. Since there are no `.go` files yet,
> `go mod tidy` would *remove* all the dependencies you just installed (it strips anything
> that is not imported by actual Go source code). You will run `go mod tidy` later in Phase 1
> after creating the first `.go` files.

At this point you have: a Go module, all directories, all dependencies. No `.go` files yet.

---

## Phase 1 — Application Bootstrap

**Goal**: Create a program that starts, parses CLI arguments, loads a config file, and launches
an empty Bubbletea TUI that shows "Hello" and quits when you press `q`. This proves the entire
pipeline works end-to-end before you build any real features.

### 1.1 CLI Entry Point & Argument Parsing

**File**: `cmd/leogit/main.go`

This is the only file with `package main`. It reads an optional repo path from the CLI,
loads the config, and starts the Bubbletea program.

```go
package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/tui/app"
)

func main() {
	// Optional first argument: path to a git repository.
	// os.Args[0] is always the program name (e.g. "./leogit"),
	// so os.Args[1] is the first user-provided argument.
	var repoPath string
	if len(os.Args) > 1 {
		repoPath = os.Args[1]
	}

	// Load configuration (missing file is fine — defaults are used)
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error loading config: %v\n", err)
		os.Exit(1)
	}

	// Create and run the TUI
	// Note: AltScreen and mouse mode are set declaratively in View()
	// via tea.View fields (Bubbletea v2 pattern) — no program options needed.
	model := app.New(cfg, repoPath)
	p := tea.NewProgram(model)

	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "Error running app: %v\n", err)
		os.Exit(1)
	}
}
```

> **Note on import aliases**: In the code above, `tea "charm.land/bubbletea/v2"` creates an
> import alias. Instead of typing `bubbletea.NewProgram()`, you write `tea.NewProgram()`.
> This is standard Go — you will see `tea` used as the alias for Bubbletea in all phases.

### 1.2 Configuration File Loading

**File**: `internal/config/config.go`

Defines the `Config` struct and a `Load()` function that reads
the config file from the OS-appropriate config directory. If the file does not
exist, it returns defaults (no error — missing config is fine). TOML decode merges
into the default struct, so any field the user omits keeps its default value.

The config file location depends on the operating system (via Go's `os.UserConfigDir()`):

| OS      | Config directory                          | Full path                                                  |
|---------|-------------------------------------------|------------------------------------------------------------|
| macOS   | `~/Library/Application Support/leogit/`   | `~/Library/Application Support/leogit/config.toml`         |
| Linux   | `$XDG_CONFIG_HOME/leogit/` (or `~/.config/leogit/`) | `~/.config/leogit/config.toml`              |
| Windows | `%APPDATA%\leogit\`                       | `C:\Users\<you>\AppData\Roaming\leogit\config.toml`        |

> **Beginner note on struct tags**: In the code below, you will see backtick annotations like
> `` `toml:"appearance"` `` after struct fields. These are called **struct tags** -- they tell
> the TOML library which section name in the config file maps to which Go struct field.
> For example, `` `toml:"appearance"` `` means the `[appearance]` section in the TOML file
> fills that field. You do not need to fully understand them now -- just copy them exactly.

```go
package config

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

type Config struct {
	Appearance    AppearanceConfig    `toml:"appearance"`
	Diff          DiffConfig          `toml:"diff"`
	AI            AIConfig            `toml:"ai"`
	Git           GitConfig           `toml:"git"`
	Confirmations ConfirmationsConfig `toml:"confirmations"`
	Repos         ReposConfig         `toml:"repos"`
}

type AppearanceConfig struct {
	Theme string `toml:"theme"`
}

type DiffConfig struct {
	SideBySide     bool `toml:"side_by_side"`
	HideWhitespace bool `toml:"hide_whitespace"`
	TabSize        int  `toml:"tab_size"`
	ContextLines   int  `toml:"context_lines"`
}

type AIConfig struct {
	Claude ClaudeConfig `toml:"claude"`
	Ollama OllamaConfig `toml:"ollama"`
}

type ClaudeConfig struct {
	Model       string `toml:"model"`
	Timeout     int    `toml:"timeout"`
	MaxDiffSize int    `toml:"max_diff_size"`
}

type OllamaConfig struct {
	Model       string `toml:"model"`
	ServerURL   string `toml:"server_url"`
	Timeout     int    `toml:"timeout"`
	MaxDiffSize int    `toml:"max_diff_size"`
}

type GitConfig struct {
	FetchInterval int `toml:"fetch_interval"`
}

type ConfirmationsConfig struct {
	DiscardChanges bool `toml:"discard_changes"`
	ForcePush      bool `toml:"force_push"`
	BranchDelete   bool `toml:"branch_delete"`
}

type ReposConfig struct {
	Mode        string   `toml:"mode"`
	ScanPaths   []string `toml:"scan_paths"`
	ScanDepth   int      `toml:"scan_depth"`
	ManualPaths []string `toml:"manual_paths"`
}

// newDefaultConfig returns a Config with every field set to its default value.
// `*Config` means "pointer to Config" -- the `&` below creates the struct and
// returns a pointer to it, so callers share the same instance (not a copy).
func newDefaultConfig() *Config {
	return &Config{
		Appearance: AppearanceConfig{
			Theme: "dark",
		},
		Diff: DiffConfig{
			SideBySide:     false,
			HideWhitespace: false,
			TabSize:        4,
			ContextLines:   3,
		},
		AI: AIConfig{
			Claude: ClaudeConfig{
				Model:       "sonnet",
				Timeout:     120,
				MaxDiffSize: 20_971_520, // 20MB
			},
			Ollama: OllamaConfig{
				Model:       "qwen2.5-coder",
				ServerURL:   "http://localhost:11434",
				Timeout:     120,
				MaxDiffSize: 52_428_800, // 50MB
			},
		},
		Git: GitConfig{
			FetchInterval: 300,
		},
		Confirmations: ConfirmationsConfig{
			DiscardChanges: true,
			ForcePush:      true,
			BranchDelete:   true,
		},
		Repos: ReposConfig{
			Mode:      "folders",
			ScanDepth: 1,
		},
	}
}

// configDir returns the leogit config directory using the OS-appropriate
// location: ~/Library/Application Support/leogit on macOS,
// %APPDATA%\leogit on Windows, $XDG_CONFIG_HOME/leogit on Linux.
func configDir() (string, error) {
	base, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, "leogit"), nil
}

// Load reads the config file and merges it into defaults.
// If the file does not exist, it creates the config directory and writes
// a default config file so the user can find and edit it later.
func Load() (*Config, error) {
	cfg := newDefaultConfig()

	dir, err := configDir()
	if err != nil {
		return nil, err
	}

	path := filepath.Join(dir, "config.toml")

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			// Create the config directory and write a default config file
			// so the user knows where to find it and can edit it.
			if mkErr := os.MkdirAll(dir, 0o755); mkErr != nil {
				return cfg, nil // silently use defaults if we can't create the dir
			}
			if writeErr := writeDefaultConfig(path, cfg); writeErr != nil {
				return cfg, nil // silently use defaults if we can't write
			}
			return cfg, nil
		}
		return nil, err
	}

	// Unmarshal decodes the TOML data into `cfg`. Because `cfg` already has
	// defaults, only the fields present in the file get overwritten.
	if err := toml.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	return cfg, nil
}

// writeDefaultConfig writes a commented default config file so users
// can discover all available options without reading documentation.
func writeDefaultConfig(path string, cfg *Config) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return toml.NewEncoder(f).Encode(cfg)
}
```

**Full config file format** (located in the OS config directory — see table above):

Every field is optional — if missing, the default from `newDefaultConfig()` is used.
This is what the user edits:

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
model = "sonnet"                  # claude model name
timeout = 120                     # seconds
max_diff_size = 20971520          # 20MB

[ai.ollama]
model = "qwen2.5-coder"
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

# Folders mode: scan these directories for git repos
scan_paths = ["~/Dev", "~/Projects"]
scan_depth = 1                    # how many levels deep to look for .git dirs

# Manual mode: explicitly listed repos
# manual_paths = ["/home/leo/my-project", "/home/leo/other-repo"]
```

**Repo state file** (`repos-state.json` in the same OS config directory as `config.toml`):

This is a separate JSON file that the app reads and writes automatically — the user
never edits it. It tracks which repo was last opened and per-repo state:

```json
{
  "last_opened": "/home/leo/Dev/my-project",
  "repos": {
    "/home/leo/Dev/my-project": {
      "last_opened_at": "2026-03-19T10:30:00Z",
      "last_branch": "main"
    }
  }
}
```

You do NOT need to implement repos-state.json in this phase — just know it exists.
It will be created in Phase 3.

### 1.3 Terminal Setup & Bubbletea Initialization

**File**: `internal/tui/app/app.go`

This is the root Bubbletea model. For now it is minimal — just enough to prove the
framework works. You will expand it in every future phase.

**How Bubbletea works**:

Bubbletea uses the Elm architecture. You define a `Model` with three methods:
- `Init()` — returns the model and the first command to run (or `nil` for nothing)
- `Update(msg)` — receives messages (keypresses, window resize, custom messages),
  returns the updated model and an optional command
- `View()` — returns a `tea.View` with the content to display (Bubbletea v2 uses
  `tea.View` instead of a plain string, so it can also carry terminal settings)

The framework calls `View()` after every `Update()` to re-render. You never print
directly — you build a string and wrap it in a `tea.View`.

```go
package app

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
)

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	repoPath string // from CLI arg, may be empty
	width    int    // terminal width (updated by WindowSizeMsg)
	height   int    // terminal height (updated by WindowSizeMsg)
	quitting bool
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:   cfg,
		repoPath: repoPath,
	}
}

// Init is called once when the program starts. No initial command needed yet.
// The `(m Model)` before the function name makes this a "method" on Model
// (like a class method in other languages). Returning nil means "no command".
func (m Model) Init() tea.Cmd {
	return nil
}

// Update handles all incoming messages (key presses, window resize, etc).
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	// "Type switch": a Go pattern for checking what concrete type an interface
	// value holds. `msg` arrives as tea.Msg (an interface), and we check if
	// it is a WindowSizeMsg, KeyPressMsg, etc.
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch msg.String() {
		case "q", "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	}

	return m, nil
}

// View renders the current state to the terminal.
//
// Returns tea.View (Bubbletea v2 API). The View struct lets us
// declaratively set terminal features like AltScreen and MouseMode.
func (m Model) View() tea.View {
	var content string

	if !m.quitting {
		// Build the info text
		repoDisplay := m.repoPath
		if repoDisplay == "" {
			repoDisplay = "(none)"
		}

		lines := []string{
			"leogit",
			"",
			fmt.Sprintf("Config loaded: %s", m.config.Appearance.Theme),
			fmt.Sprintf("Repo path: %s", repoDisplay),
			fmt.Sprintf("Terminal: %dx%d", m.width, m.height),
			"",
			"Press q to quit",
		}
		text := strings.Join(lines, "\n")

		// Center the content on screen
		style := lipgloss.NewStyle().
			Width(m.width).
			Height(m.height).
			Align(lipgloss.Center, lipgloss.Center)

		content = style.Render(text)
	}

	v := tea.NewView(content)
	v.AltScreen = true              // takes over the full terminal; restores previous content on exit
	v.MouseMode = tea.MouseModeCellMotion // enables mouse click/scroll events
	return v
}
```

### 1.4 Run `go mod tidy`

After creating all three `.go` files above (`main.go`, `config.go`, and `app.go`),
run `go mod tidy` from the project root. This cleans up `go.mod` and `go.sum` to match
your actual imports. You should do this before trying to build.

```bash
go mod tidy
```

### 1.5 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit
```

You should see a fullscreen terminal with the centered text. Press `q` to quit.

> **Tip — use `go run` for faster iterations**: Throughout this guide, test sections
> show `go build` + `./leogit` as two steps. For quicker feedback during development,
> you can combine them into a single command:
> ```bash
> go run ./cmd/leogit
> # or with arguments:
> go run ./cmd/leogit /path/to/repo
> ```
> `go run` compiles and runs in one step without leaving a binary on disk. Go compiles
> fast, so the turnaround is just a couple of seconds. Use `go build` when you want a
> standalone binary to keep around.

Test with a repo path argument:

```bash
./leogit /path/to/some/repo
```

The "Repo path" line should show the path you passed.

The app automatically creates the config directory and a default `config.toml` on
first run. Check the "Config loaded" line — it should say "dark" (the default theme).

To test config loading, edit the auto-created config file and change the theme:

```bash
# Find your config file (OS-dependent):
#   macOS:   ~/Library/Application Support/leogit/config.toml
#   Linux:   ~/.config/leogit/config.toml
#   Windows: %APPDATA%\leogit\config.toml

# Example (macOS):
sed -i '' 's/theme = "dark"/theme = "light"/' ~/Library/Application\ Support/leogit/config.toml

# Example (Linux):
sed -i 's/theme = "dark"/theme = "light"/' ~/.config/leogit/config.toml
```

Run again — the "Config loaded" line should now say "light" instead of "dark".

**Files created in Phase 1** (3 total):

1. `cmd/leogit/main.go` -- entry point
2. `internal/config/config.go` -- config loading
3. `internal/tui/app/app.go` -- root Bubbletea model

**Phase 1 is complete when**: the app starts fullscreen, shows config values and terminal
dimensions, accepts a repo path argument, and quits cleanly with `q`.

## Phase 2 — Authentication Gate

**Goal**: Before the app does anything useful, it must check that the user is logged into
GitHub via the `gh` CLI. If they are not, the app shows a fullscreen blocker message and
re-checks every time the user presses a key. No other functionality is accessible until
auth succeeds.

This is step 1 of the startup flow:
1. **Auth gate** (this phase) — blocks everything if not logged in
2. Repo selection (Phase 3)
3. Load config, discover repos, render UI (Phase 4+)

### 2.1 GH CLI Auth Check

**File**: `internal/gh/auth.go` (new file — you need to create the `internal/gh/` directory first: `mkdir -p internal/gh`)

This file wraps the `gh auth status` command. It runs the command and checks the exit code:
- **Exit code 0** → user is logged in
- **Exit code 4** → user is NOT logged in (or `gh` is not installed)
- **Any other error** → treat as not logged in (safe fallback)

```go
package gh

import (
	"os/exec"
)

// CheckAuth runs `gh auth status` and returns true if the user is logged in.
// Exit code 0 means logged in. Exit code 4 (or any error) means not logged in.
func CheckAuth() bool {
	cmd := exec.Command("gh", "auth", "status")

	// We don't need stdout/stderr — just the exit code
	err := cmd.Run()
	return err == nil
}
```

That's the entire file. The function returns `true` if authenticated, `false` otherwise.

**Why it's so simple**: `gh auth status` does all the heavy lifting. Exit code 0 means
the user has a valid token. Any non-zero exit code (including 4 for "not logged in" or
127 for "gh not found") means we can't proceed, so we return `false` for all of them.

### 2.2 Auth Failure Fullscreen Blocker

**File**: `internal/tui/app/app.go` (modify the existing file from Phase 1)

Now you need to modify the root model to:
1. Add an `authenticated` field to track auth state
2. On `Init()`, run the auth check as a Bubbletea command
3. If not authenticated, show a fullscreen blocker instead of the normal UI
4. When the user presses any key while the blocker is showing, re-check auth
5. Once auth succeeds, proceed to the normal app (for now, the Phase 1 "Hello" screen)

Replace the entire contents of `internal/tui/app/app.go` with:

```go
package app

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/gh"
)

// authResultMsg carries the result of an auth check back to the Update loop.
type authResultMsg struct {
	authenticated bool
}

// checkAuthCmd runs the auth check in a goroutine and returns the result as a message.
// This is a Bubbletea Cmd — it runs asynchronously so it doesn't block the UI.
//
// Note: a tea.Cmd is defined as `func() tea.Msg`. This function has exactly that
// signature (no parameters, returns tea.Msg), so it can be used anywhere a tea.Cmd
// is expected — for example, returned from Init() or Update().
func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config        *config.Config
	repoPath      string // from CLI arg, may be empty
	width         int    // terminal width (updated by WindowSizeMsg)
	height        int    // terminal height (updated by WindowSizeMsg)
	quitting      bool
	authenticated bool // true once gh auth check passes
	authChecking  bool // true while an auth check is in progress (prevents spamming)
	authChecked   bool // true after the first auth check completes (hides loading flicker)
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		repoPath:     repoPath,
		authChecking: true,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// Update handles all incoming messages (key presses, window resize, etc).
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	// Type switch: `msg := msg.(type)` tests what concrete type the msg is.
	// The outer `msg` is the interface (tea.Msg); the inner `msg` becomes the
	// concrete type (e.g. tea.KeyPressMsg) — Go lets you shadow the variable.
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case authResultMsg:
		m.authenticated = msg.authenticated
		m.authChecking = false
		m.authChecked = true
		return m, nil

	case tea.KeyPressMsg:
		// If not authenticated, any keypress triggers a re-check
		if !m.authenticated && m.authChecked {
			// Always allow quitting, even from the blocker
			if msg.String() == "ctrl+c" {
				m.quitting = true
				return m, tea.Quit
			}

			// Don't spam auth checks — without this guard, every rapid keypress
			// would spawn a new goroutine running `gh auth status`. authChecking
			// acts as a lock: set true here, reset to false when authResultMsg arrives.
			if !m.authChecking {
				m.authChecking = true
				return m, checkAuthCmd
			}
			return m, nil
		}

		// Normal key handling (only reached when authenticated)
		switch msg.String() {
		case "q", "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	}

	return m, nil
}

// View renders the current state to the terminal.
//
// Returns tea.View (Bubbletea v2 API). The View struct lets us
// declaratively set terminal features like AltScreen and MouseMode.
func (m Model) View() tea.View {
	var content string

	if m.quitting {
		// empty content
	} else if !m.authChecked {
		// While waiting for the first auth check, show nothing (avoids flicker)
	} else if !m.authenticated {
		content = m.viewAuthBlocker()
	} else {
		content = m.viewMain()
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	return v
}

// viewAuthBlocker renders a fullscreen centered message telling the user to log in.
func (m Model) viewAuthBlocker() string {
	// ── Styles ──────────────────────────────────────────
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	// ── Content ─────────────────────────────────────────
	title := titleStyle.Render("Authentication Required")

	message := messageStyle.Render(
		"GitHub authentication is required to use this app.\n\nRun the following command in your terminal:",
	)

	command := commandStyle.Render("gh auth login")

	hint := hintStyle.Render("Press any key to retry • Ctrl+C to quit")

	// ── Box ─────────────────────────────────────────────
	boxContent := strings.Join([]string{
		title,
		"",
		message,
		"",
		"  " + command,
		"",
		hint,
	}, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center)

	box := boxStyle.Render(boxContent)

	// ── Center on screen ────────────────────────────────
	fullscreen := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return fullscreen.Render(box)
}

// viewMain renders the normal app content (expanded in later phases).
func (m Model) viewMain() string {
	repoDisplay := m.repoPath
	if repoDisplay == "" {
		repoDisplay = "(none)"
	}

	lines := []string{
		"leogit",
		"",
		fmt.Sprintf("Config loaded: %s", m.config.Appearance.Theme),
		fmt.Sprintf("Repo path: %s", repoDisplay),
		fmt.Sprintf("Terminal: %dx%d", m.width, m.height),
		"",
		"Press q to quit",
	}
	content := strings.Join(lines, "\n")

	style := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return style.Render(content)
}
```

**What changed from Phase 1**:

1. **New import**: `gh` package for the auth check
2. **New message type**: `authResultMsg` — carries auth results back to `Update()`
3. **New command**: `checkAuthCmd` — runs `gh.CheckAuth()` and returns a message
4. **New fields on `Model`**: `authenticated`, `authChecking`, `authChecked`
5. **`Init()`** now returns `checkAuthCmd` instead of `nil` — the auth check runs
   immediately on startup
6. **`Update()`** handles `authResultMsg` and re-checks auth on any keypress when blocked
7. **`View()`** now branches: if not authenticated → show blocker, otherwise → show main
8. **Two new methods**: `viewAuthBlocker()` and `viewMain()` (the old `View()` body)

**How the auth flow works step by step**:

1. App starts → `Init()` fires `checkAuthCmd`
2. Screen is blank while `authChecked` is `false` (brief moment)
3. `authResultMsg` arrives → sets `authenticated` to `true` or `false`
4. If `false` → `View()` shows the red-bordered blocker with "gh auth login" instructions
5. User presses any key → `Update()` fires another `checkAuthCmd`
6. If the user ran `gh auth login` in another terminal, the next check returns `true`
7. `authenticated` flips to `true` → `View()` shows the normal app
8. `Ctrl+C` always works, even from the blocker

### 2.3 Test It

**Test 1 — Not authenticated** (if you're currently logged in, log out first):

> **Heads up**: `gh auth logout` will log you out for real — you will need to run `gh auth login` again afterward to restore access for all tools that use `gh`.

```bash
gh auth logout
go build -o leogit ./cmd/leogit && ./leogit
# or: go run ./cmd/leogit
```

You should see a fullscreen blocker with a red border saying "Authentication Required"
and instructions to run `gh auth login`. Pressing any key should briefly re-check
(the screen may flicker for a moment) and then show the blocker again.

`Ctrl+C` should quit the app cleanly.

**Test 2 — Authenticate while blocked**:

1. Start the app (still logged out) — you see the blocker
2. In another terminal, run `gh auth login` and complete the login
3. Back in the app, press any key
4. The blocker should disappear and you should see the normal "leogit" screen

**Test 3 — Already authenticated**:

```bash
gh auth status  # should show "Logged in to github.com"
./leogit
```

The app should skip the blocker entirely and go straight to the normal screen.

**Test 4 — gh not installed** (optional, if you want to verify the fallback):

Temporarily hide `gh` from your shell so `exec.Command("gh", ...)` fails:

- **macOS (Apple Silicon)**: `gh` is typically at `/opt/homebrew/bin/gh`.
  ```bash
  sudo mv /opt/homebrew/bin/gh /opt/homebrew/bin/gh.bak
  # test the app — it should show the blocker
  sudo mv /opt/homebrew/bin/gh.bak /opt/homebrew/bin/gh
  ```
- **Linux**: `gh` is typically at `/usr/bin/gh` or `/usr/local/bin/gh`.
  ```bash
  sudo mv $(which gh) $(which gh).bak
  # test the app — it should show the blocker
  sudo mv $(which gh).bak $(which gh)
  ```

The app should show the blocker (since `exec.Command("gh", ...)` will fail, returning an error → `false`).

**Phase 2 is complete when**: the app blocks all functionality when `gh auth status`
fails, shows the auth blocker, re-checks on keypress, and proceeds normally once
authenticated.

## Phase 3 — Repository Selection

**Goal**: After authentication passes, the app needs to figure out which git repository
to open. There are three ways, tried in order:

1. **CLI argument** — the user ran `leogit /path/to/repo` → open it directly
2. **Last opened** — read `repos-state.json` for the most recently opened repo → open it
3. **Repo picker** — discover repos from config and show a filterable list → user picks one

This phase creates: repo validation, state file persistence, folder-based repo discovery,
manual repo list support, and a fullscreen repo picker UI.

### 3.1 Repository Validation

**File**: `internal/git/repo.go`

Before opening any repo (from CLI arg, last opened, or picker), you need to verify
it's actually a git repository. This function checks that a `.git` directory (or file,
for worktrees) exists at the given path.

```go
package git

import (
	"os"
	"path/filepath"
)

// IsGitRepo checks if the given path is a git repository.
// It looks for a .git directory or file (worktrees use a .git file).
func IsGitRepo(path string) bool {
	info, err := os.Stat(filepath.Join(path, ".git"))
	if err != nil {
		return false
	}
	// .git can be a directory (normal repo) or a file (worktree/submodule)
	return info.IsDir() || info.Mode().IsRegular()
}

// RepoName extracts a display name from a repo path.
// It returns the last component of the path (e.g., "/home/leo/Dev/my-project" → "my-project").
func RepoName(path string) string {
	return filepath.Base(path)
}
```

### 3.2 State File (repos-state.json)

**File**: `internal/config/state.go`

This file manages the `repos-state.json` file that tracks which repo was last opened
and per-repo state. The file lives in the same OS config directory as `config.toml`
(the `configDir()` helper from `config.go` is reused here since both files are in the
same `config` package).

The JSON format:

```json
{
  "last_opened": "/home/leo/Dev/my-project",
  "repos": {
    "/home/leo/Dev/my-project": {
      "last_opened_at": "2026-03-19T10:30:00Z",
      "last_branch": "main"
    }
  }
}
```

- `last_opened` — the absolute path of the most recently opened repo
- `repos` — a map of repo paths to their individual state
- `last_opened_at` — RFC3339 timestamp of when the repo was last opened
- `last_branch` — the branch that was active when the repo was last used

```go
package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"time"
)

// ReposState is the top-level structure of repos-state.json.
type ReposState struct {
	LastOpened string                `json:"last_opened"`
	Repos      map[string]RepoState `json:"repos"`
}

// RepoState tracks per-repo persistent state.
type RepoState struct {
	LastOpenedAt time.Time `json:"last_opened_at"`
	LastBranch   string    `json:"last_branch"`
}

// statePath returns the path to repos-state.json.
// Uses configDir() from config.go (same package) to get the OS-appropriate directory.
func statePath() (string, error) {
	dir, err := configDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "repos-state.json"), nil
}

// LoadState reads repos-state.json. If the file does not exist, returns an empty state.
func LoadState() (*ReposState, error) {
	path, err := statePath()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return &ReposState{Repos: make(map[string]RepoState)}, nil
		}
		return nil, err
	}

	var state ReposState
	if err := json.Unmarshal(data, &state); err != nil {
		// If the file is corrupt, start fresh rather than crashing
		return &ReposState{Repos: make(map[string]RepoState)}, nil
	}

	if state.Repos == nil {
		state.Repos = make(map[string]RepoState)
	}

	return &state, nil
}

// SaveState writes repos-state.json. Creates the config directory if needed.
func SaveState(state *ReposState) error {
	path, err := statePath()
	if err != nil {
		return err
	}

	// Ensure the config directory exists
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}

	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(path, data, 0o644)
}

// SetLastOpened updates the state to record that a repo was opened now.
func (s *ReposState) SetLastOpened(repoPath string) {
	s.LastOpened = repoPath
	s.Repos[repoPath] = RepoState{
		LastOpenedAt: time.Now().UTC(),
		LastBranch:   s.Repos[repoPath].LastBranch, // preserve existing branch if any
	}
}
```

### 3.3 Folder-Based Repository Discovery

**File**: `internal/git/discover.go`

When the config `repos.mode` is `"folders"` (the default), the app scans directories
listed in `repos.scan_paths` to find git repositories. It walks each path up to
`repos.scan_depth` levels deep, looking for directories that contain a `.git` entry.

- `scan_depth = 1` (default) means: look at immediate children of each scan path.
  For example, if `scan_paths = ["~/Dev"]` and `~/Dev` contains `my-project/` and
  `other-repo/`, it checks `~/Dev/my-project/.git` and `~/Dev/other-repo/.git`.
- `scan_depth = 2` would also check `~/Dev/github/my-project/.git` etc.
- Tilde (`~`) in scan paths is expanded to the user's home directory.
- Hidden directories (starting with `.`) are skipped during scanning.
- Symbolic links are followed.

```go
package git

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// DiscoverRepos scans the given paths for git repositories up to maxDepth levels deep.
// Returns a sorted list of absolute paths to discovered repos.
func DiscoverRepos(scanPaths []string, maxDepth int) []string {
	var repos []string
	seen := make(map[string]bool)

	for _, scanPath := range scanPaths {
		// Expand tilde to home directory
		expanded := ExpandTilde(scanPath)

		// Resolve to absolute path
		abs, err := filepath.Abs(expanded)
		if err != nil {
			continue
		}

		// Check if the scan path itself exists
		info, err := os.Stat(abs)
		if err != nil || !info.IsDir() {
			continue
		}

		// Walk the directory tree up to maxDepth
		scanForRepos(abs, abs, maxDepth, seen, &repos)
	}

	sort.Strings(repos)
	return repos
}

// scanForRepos recursively searches for git repos starting at dir.
// root is the original scan path, used to calculate current depth.
func scanForRepos(dir, root string, maxDepth int, seen map[string]bool, repos *[]string) {
	// Calculate current depth relative to root
	rel, err := filepath.Rel(root, dir)
	if err != nil {
		return
	}

	depth := 0
	if rel != "." {
		depth = strings.Count(rel, string(filepath.Separator)) + 1
	}

	// Don't go deeper than maxDepth
	if depth > maxDepth {
		return
	}

	// Check if this directory is a git repo (depth > 0 skips the scan root itself)
	if depth > 0 && IsGitRepo(dir) {
		absPath, err := filepath.Abs(dir)
		if err != nil {
			return
		}
		if !seen[absPath] {
			seen[absPath] = true
			*repos = append(*repos, absPath)
		}
		return // Don't scan inside a git repo for nested repos
	}

	// Read directory entries
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}

	for _, entry := range entries {
		name := entry.Name()

		// Skip hidden directories
		if strings.HasPrefix(name, ".") {
			continue
		}

		// Follow symlinks: resolve the entry to check if it's a directory
		fullPath := filepath.Join(dir, name)
		info, err := os.Stat(fullPath) // os.Stat follows symlinks
		if err != nil || !info.IsDir() {
			continue
		}

		scanForRepos(fullPath, root, maxDepth, seen, repos)
	}
}

// ExpandTilde replaces a leading ~ with the user's home directory.
func ExpandTilde(path string) string {
	if !strings.HasPrefix(path, "~") {
		return path
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return path
	}

	if path == "~" {
		return home
	}

	if strings.HasPrefix(path, "~/") {
		return filepath.Join(home, path[2:])
	}

	return path
}
```

**How manual mode works**: When `repos.mode` is `"manual"`, the app skips scanning
entirely and uses the `repos.manual_paths` list from the config. Each path is expanded
(tilde), resolved to absolute, and validated with `IsGitRepo()`. Invalid paths are
silently excluded from the picker.

No separate file is needed for manual mode — the picker will handle both modes by
receiving a `[]string` of repo paths regardless of how they were discovered.

### 3.4 Repository Picker UI

**File**: `internal/tui/views/repopicker.go`

When neither a CLI argument nor a last-opened repo is available (or if those paths
are no longer valid git repos), the app shows a fullscreen repo picker.

The picker is a filterable list of discovered repositories:
- Type to filter the list (fuzzy match on repo name and path)
- `j`/`k` or `Up`/`Down` to navigate
- `Enter` to open the selected repo
- `Esc` or `q` to quit the app (there's nothing else to go back to)

The picker is its own Bubbletea model, embedded inside the root app model. In the code below, `func() tea.Msg { return SomeMsg{} }` is how you create a Bubbletea command -- it is a function that Bubbletea calls and feeds the returned message back into `Update()`.

```go
package views

import (
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// RepoSelectedMsg is sent when the user picks a repo from the list.
type RepoSelectedMsg struct {
	Path string
}

// RepoPickerModel is the fullscreen repo picker view.
type RepoPickerModel struct {
	allRepos     []string // all discovered repo paths
	filtered     []string // repos matching the current filter
	filter       string   // current search text
	cursor       int      // index in filtered list
	width        int
	height       int
}

// NewRepoPicker creates a new picker with the given list of repo paths.
func NewRepoPicker(repos []string) RepoPickerModel {
	return RepoPickerModel{
		allRepos: repos,
		filtered: repos,
	}
}

// Init does nothing — the picker is ready immediately.
func (m RepoPickerModel) Init() (RepoPickerModel, tea.Cmd) {
	return m, nil
}

// Update handles input for the repo picker.
func (m RepoPickerModel) Update(msg tea.Msg) (RepoPickerModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch msg.String() {
		case "ctrl+c":
			return m, tea.Quit

		case "escape":
			// If there's a filter, clear it first
			if m.filter != "" {
				m.filter = ""
				m.applyFilter()
				return m, nil
			}
			return m, tea.Quit

		case "enter":
			if len(m.filtered) > 0 {
				selected := m.filtered[m.cursor]
				return m, func() tea.Msg {
					return RepoSelectedMsg{Path: selected}
				}
			}
			return m, nil

		case "up", "k":
			if m.cursor > 0 {
				m.cursor--
			}
			return m, nil

		case "down", "j":
			if m.cursor < len(m.filtered)-1 {
				m.cursor++
			}
			return m, nil

		case "backspace":
			if len(m.filter) > 0 {
				m.filter = m.filter[:len(m.filter)-1]
				m.applyFilter()
			}
			return m, nil

		default:
			// Single printable character → add to filter
			if len(msg.String()) == 1 {
				char := msg.String()
				// Only accept printable characters (letters, numbers, symbols)
				if char >= " " && char <= "~" {
					m.filter += char
					m.applyFilter()
				}
			}
			return m, nil
		}
	}

	return m, nil
}

// applyFilter updates the filtered list based on the current filter text.
// Matches against both the repo name and the full path (case-insensitive).
func (m *RepoPickerModel) applyFilter() {
	if m.filter == "" {
		m.filtered = m.allRepos
		m.cursor = 0
		return
	}

	query := strings.ToLower(m.filter)
	var matches []string
	for _, repo := range m.allRepos {
		name := strings.ToLower(filepath.Base(repo))
		path := strings.ToLower(repo)
		if strings.Contains(name, query) || strings.Contains(path, query) {
			matches = append(matches, repo)
		}
	}
	m.filtered = matches

	// Keep cursor in bounds
	if m.cursor >= len(m.filtered) {
		m.cursor = max(0, len(m.filtered)-1)
	}
}

// View renders the repo picker.
func (m RepoPickerModel) View() string {
	// ── Styles ──────────────────────────────────────────
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	filterStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#00AAFF")).
		Bold(true)

	selectedStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#000000")).
		Background(lipgloss.Color("#00AAFF")).
		Bold(true)

	normalStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#CCCCCC"))

	pathStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	// ── Title ───────────────────────────────────────────
	title := titleStyle.Render("Select a Repository")

	// ── Filter input ────────────────────────────────────
	filterLine := "Filter: "
	if m.filter != "" {
		filterLine += filterStyle.Render(m.filter)
	}
	filterLine += filterStyle.Render("█") // cursor block

	// ── Repo list ───────────────────────────────────────
	var listLines []string

	if len(m.filtered) == 0 {
		if len(m.allRepos) == 0 {
			listLines = append(listLines, normalStyle.Render("No repositories found."))
			listLines = append(listLines, "")
			listLines = append(listLines, normalStyle.Render("Configure scan_paths in your config.toml (see --help for path)"))
			listLines = append(listLines, normalStyle.Render("or use mode = \"manual\" with manual_paths."))
		} else {
			listLines = append(listLines, normalStyle.Render("No matches for \""+m.filter+"\""))
		}
	} else {
		// Show a window of repos that fits the terminal height
		// Reserve lines for: title, blank, filter, blank, list..., blank, hint
		maxVisible := m.height - 6
		if maxVisible < 3 {
			maxVisible = 3
		}

		// Calculate window start so the cursor is always visible
		start := 0
		if m.cursor >= maxVisible {
			start = m.cursor - maxVisible + 1
		}
		end := start + maxVisible
		if end > len(m.filtered) {
			end = len(m.filtered)
		}

		for i := start; i < end; i++ {
			repo := m.filtered[i]
			name := filepath.Base(repo)
			dir := filepath.Dir(repo)

			if i == m.cursor {
				line := selectedStyle.Render(" " + name + " ")
				line += " " + pathStyle.Render(dir)
				listLines = append(listLines, line)
			} else {
				line := normalStyle.Render("  "+name+" ") + " " + pathStyle.Render(dir)
				listLines = append(listLines, line)
			}
		}

		// Show scroll indicator if list is truncated
		if len(m.filtered) > maxVisible {
			indicator := pathStyle.Render(
				"  (" + strings.Itoa(m.cursor+1) + "/" + strings.Itoa(len(m.filtered)) + ")",
			)
			listLines = append(listLines, indicator)
		}
	}

	// ── Hint ────────────────────────────────────────────
	hint := hintStyle.Render("Type to filter • ↑/↓ or j/k to navigate • Enter to open • Esc to quit")

	// ── Assemble ────────────────────────────────────────
	sections := []string{
		title,
		"",
		filterLine,
		"",
	}
	sections = append(sections, listLines...)
	sections = append(sections, "", hint)

	content := strings.Join(sections, "\n")

	// ── Box ─────────────────────────────────────────────
	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#00AAFF")).
		Padding(1, 3).
		MaxWidth(m.width - 4)

	box := boxStyle.Render(content)

	// ── Center on screen ────────────────────────────────
	fullscreen := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return fullscreen.Render(box)
}
```

### 3.5 Startup Flow & Integration

**File**: `internal/tui/app/app.go` (modify the existing file from Phase 2)

Now you need to wire the startup flow into the root model. After authentication
succeeds, the app determines which repo to open using this priority:

1. **CLI argument** — if `repoPath` was passed on the command line and it's a valid
   git repo, open it immediately
2. **Last opened** — load `repos-state.json`, check `last_opened`, verify it's still
   a valid git repo, and open it
3. **Repo picker** — discover repos (folder scan or manual list, depending on config),
   show the picker, let the user choose

Replace the entire contents of `internal/tui/app/app.go` with:

```go
package app

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

// authResultMsg carries the result of an auth check back to the Update loop.
type authResultMsg struct {
	authenticated bool
}

// repoResolvedMsg is sent when the startup flow has determined which repo to open.
// If path is empty, no repo could be resolved (need to show picker).
type repoResolvedMsg struct {
	path string
}

// reposDiscoveredMsg carries the list of discovered repos for the picker.
type reposDiscoveredMsg struct {
	repos []string
}

// ── Commands ────────────────────────────────────────────

// checkAuthCmd runs the auth check asynchronously.
func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

// resolveRepoCmd tries CLI arg → last opened → gives up (empty string).
func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		// 1. CLI argument
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}

		// 2. Last opened from state file
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}

		// 3. No repo found — the picker will be shown
		return repoResolvedMsg{path: ""}
	}
}

// discoverReposCmd scans for repos based on the config.
func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string

		switch cfg.Repos.Mode {
		case "manual":
			// Use manually listed paths, validate each one
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default: // "folders" or any unrecognized mode
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}

		return reposDiscoveredMsg{repos: repos}
	}
}

// ── App State ───────────────────────────────────────────

// appState tracks which screen the app is on.
type appState int

const (
	stateAuthChecking    appState = iota // waiting for first auth check
	stateAuthBlocked                     // auth failed, showing blocker
	stateResolvingRepo                   // trying CLI arg / last opened
	stateDiscoveringRepos                // scanning for repos
	stateRepoPicker                      // showing the repo picker
	stateMain                            // repo is open, normal UI
)

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	cliPath  string // original CLI arg (may be empty)
	repoPath string // resolved repo path (set once a repo is chosen)
	width    int
	height   int
	quitting bool

	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// Update handles all incoming messages.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to picker if active
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			// Auth passed → resolve repo
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			// We have a repo — open it
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			return m, nil
		}
		// No repo found — discover repos for the picker
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		// Forward the current window size to the picker
		m.repoPicker, _ = m.repoPicker.Update(tea.WindowSizeMsg{
			Width: m.width, Height: m.height,
		})
		m.state = stateRepoPicker
		return m, nil

	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		return m, nil

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// handleKey processes key messages based on current state.
// Returns (Model, tea.Cmd) not (tea.Model, tea.Cmd) — Go auto-converts because Model satisfies tea.Model.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.state {

	case stateAuthBlocked:
		if msg.String() == "ctrl+c" {
			m.quitting = true
			return m, tea.Quit
		}
		// Any other key → re-check auth
		if !m.authChecking {
			m.authChecking = true
			return m, checkAuthCmd
		}
		return m, nil

	case stateRepoPicker:
		var cmd tea.Cmd
		m.repoPicker, cmd = m.repoPicker.Update(msg)
		return m, cmd

	case stateMain:
		switch msg.String() {
		case "q", "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	}

	return m, nil
}

// saveRepoState records the opened repo in repos-state.json.
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return // silently ignore — state persistence is best-effort
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state) // best-effort
}

// View renders the current state to the terminal.
//
// Returns tea.View (Bubbletea v2 API). The View struct lets us
// declaratively set terminal features like AltScreen and MouseMode.
func (m Model) View() tea.View {
	var content string

	if !m.quitting {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			// brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			content = m.viewMain()
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	return v
}

// viewAuthBlocker renders a fullscreen centered message telling the user to log in.
func (m Model) viewAuthBlocker() string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	title := titleStyle.Render("Authentication Required")

	message := messageStyle.Render(
		"GitHub authentication is required to use this app.\n\nRun the following command in your terminal:",
	)

	command := commandStyle.Render("gh auth login")

	hint := hintStyle.Render("Press any key to retry • Ctrl+C to quit")

	boxContent := strings.Join([]string{
		title,
		"",
		message,
		"",
		"  " + command,
		"",
		hint,
	}, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center)

	box := boxStyle.Render(boxContent)

	fullscreen := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return fullscreen.Render(box)
}

// viewMain renders the normal app content (expanded in later phases).
func (m Model) viewMain() string {
	repoDisplay := m.repoPath
	if repoDisplay == "" {
		repoDisplay = "(none)"
	}

	lines := []string{
		"leogit",
		"",
		fmt.Sprintf("Config loaded: %s", m.config.Appearance.Theme),
		fmt.Sprintf("Repo: %s (%s)", git.RepoName(m.repoPath), m.repoPath),
		fmt.Sprintf("Terminal: %dx%d", m.width, m.height),
		"",
		"Press q to quit",
	}
	content := strings.Join(lines, "\n")

	style := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return style.Render(content)
}
```

**What changed from Phase 2**:

1. **New imports**: `git`, `views` packages for repo validation and picker
2. **New message types**: `repoResolvedMsg`, `reposDiscoveredMsg` — for async repo resolution
3. **New commands**: `resolveRepoCmd()`, `discoverReposCmd()` — async startup steps
4. **`appState` enum**: replaces the three booleans (`authenticated`, `authChecking`,
   `authChecked`) with a clear state machine: `stateAuthChecking` → `stateAuthBlocked`
   or → `stateResolvingRepo` → `stateDiscoveringRepos` → `stateRepoPicker` or → `stateMain`
5. **`Model` fields**: `cliPath` (original arg), `repoPath` (resolved), `state`, `repoPicker`
6. **`Update()`**: handles the full startup chain — auth → resolve → discover → picker → main
7. **`handleKey()`**: extracted to keep `Update()` readable; dispatches to blocker,
   picker, or main based on state
8. **`saveRepoState()`**: persists to `repos-state.json` when a repo is opened
9. **`View()`**: uses a switch on `state` instead of nested if/else
10. **`viewMain()`**: now shows the repo name and path instead of the raw CLI arg

**How the startup flow works step by step**:

1. App starts → `Init()` fires `checkAuthCmd` → state is `stateAuthChecking`
2. `authResultMsg{true}` arrives → state becomes `stateResolvingRepo`, fires `resolveRepoCmd`
3. `resolveRepoCmd` checks CLI arg, then last opened:
   - If found → `repoResolvedMsg{path: "/path/to/repo"}` → state becomes `stateMain`
   - If not found → `repoResolvedMsg{path: ""}` → state becomes `stateDiscoveringRepos`,
     fires `discoverReposCmd`
4. `discoverReposCmd` scans folders or reads manual list → `reposDiscoveredMsg{repos: [...]}`
5. `reposDiscoveredMsg` arrives → creates picker model, state becomes `stateRepoPicker`
6. User types to filter, navigates with j/k, presses Enter → `RepoSelectedMsg`
7. `RepoSelectedMsg` arrives → `repoPath` is set, state becomes `stateMain`,
   `repos-state.json` is updated

### 3.6 Test It

Build (or use `go run ./cmd/leogit` for quick iterations):

```bash
go build -o leogit ./cmd/leogit
```

**Test 1 — CLI argument** (direct repo open):

```bash
./leogit /path/to/any/git/repo
```

The app should skip the picker and go straight to the main screen showing
`Repo: repo-name (/path/to/any/git/repo)`. Check `repos-state.json` in your config
directory — it should now contain the repo path under `last_opened`.

> **Cross-platform note for all tests below**: The shell commands use Linux paths
> (`~/.config/leogit/`). On **macOS**, replace with `~/Library/Application\ Support/leogit/`.
> On **Windows**, use `%APPDATA%\leogit\` (or `$env:APPDATA\leogit\` in PowerShell).

**Test 2 — Last opened** (no CLI arg, state file exists):

```bash
./leogit
```

Since Test 1 saved the repo to `repos-state.json`, the app should automatically
open the same repo without showing the picker.

**Test 3 — Repo picker** (no CLI arg, no last opened):

```bash
# Clear the state file to force the picker
rm ~/.config/leogit/repos-state.json

# Make sure you have scan_paths configured:
cat ~/.config/leogit/config.toml
# Should have: scan_paths = ["~/Dev"] (or wherever your repos are)

./leogit
```

You should see the repo picker with a blue border, listing discovered repos.
Type to filter, use `j`/`k` to navigate, press `Enter` to select a repo.
After selecting, the main screen should show the repo name and path.

**Test 4 — Manual mode**:

Edit your `config.toml`:

```toml
[repos]
mode = "manual"
manual_paths = ["/path/to/repo1", "/path/to/repo2"]
```

```bash
rm ~/.config/leogit/repos-state.json
./leogit
```

The picker should show only the manually listed repos (excluding any invalid paths).

**Test 5 — No repos found**:

```toml
[repos]
mode = "folders"
scan_paths = ["/nonexistent/path"]
```

```bash
rm ~/.config/leogit/repos-state.json
./leogit
```

The picker should show "No repositories found" with instructions to configure
`scan_paths` or use manual mode. `Esc` should quit.

**Test 6 — Invalid CLI arg falls through**:

```bash
rm ~/.config/leogit/repos-state.json
./leogit /tmp
```

`/tmp` is not a git repo, and there's no last opened — should show the picker.

**Phase 3 is complete when**: the app follows the CLI arg → last opened → picker
priority, discovers repos from scan paths or manual list, shows a filterable picker,
persists the selection to `repos-state.json`, and opens the chosen repo.

> **Beginner notes for Phase 3**: (1) `handleKey` returns `(Model, tea.Cmd)` not `(tea.Model, tea.Cmd)` -- Go auto-converts because Model satisfies the interface. (2) `saveRepoState` and `applyFilter` use pointer receivers (`*Model`, `*RepoPickerModel`) for mutation/side-effects, while Bubbletea methods use value receivers. (3) `func() tea.Msg { return Msg{} }` is a Bubbletea command -- Bubbletea runs it and feeds the result to `Update()`.

## Phase 4 — Main Layout Shell

**Goal**: Replace the placeholder `viewMain()` with the actual layout —
a header bar, tab indicator, sidebar + main area split, and a collapsible terminal region.
Add a focus model (navigable vs focused states), wire up global keybindings, create a help
overlay, and build a reusable error modal system.

After this phase, the app renders the full layout skeleton with labeled placeholder panes.
No pane has real content yet — that starts in Phase 5.

### 4.1 Layout Skeleton (Sidebar + Main Area + Terminal)

A modern graphical layout, adapted for the terminal:

```
┌──────────────────────────────────────────────────────────┐
│ ⎇ repo-name  │ ᚠ branch  │ ↻ Fetch                      │  ← Header (1 row)
├──────────────────────────────────────────────────────────┤
│ [Changes] History                                        │  ← Tab bar (1 row)
├───────────────────┬──────────────────────────────────────┤
│                   │                                      │
│  Pane 1           │         Pane 2                       │
│  (Changed Files)  │         (Diff Viewer)                │
│                   │                                      │
├───────────────────┤──────────────────────────────────────┤
│  Pane 3           │                                      │
│  (Commit Msg)     │  Terminal / Log (collapsible)        │
│                   │                                      │
└───────────────────┴──────────────────────────────────────┘
```

Pane focus shortcuts are **positional** — numbers 1/2/3 correspond to left-to-right,
top-to-bottom within the current tab:

| Key   | Changes Tab      | History Tab      |
|-------|------------------|------------------|
| `1`   | Changed Files    | Commit List      |
| `2`   | Diff Viewer      | Changed Files    |
| `3`   | Commit Message   | Diff Viewer      |
| `` ` ``| Terminal/Log   | Terminal/Log     |
| `Tab` | → History        | → Changes        |

The sidebar is 30% of terminal width (minimum 25 characters, maximum 50%). The main area
fills the rest. The sidebar splits vertically: file list on top (flexible height) and commit
message on bottom (8 rows fixed). The main area splits vertically: diff viewer on top
(flexible) and terminal on bottom (collapsible, default 10 rows when open).

This section creates three new files: shared focus/pane types, layout math, and header/tab
bar rendering.

**File**: `internal/core/focus.go` (new file)

Shared types for tracking focus, tabs, and panes. Lives in `core/` so it's accessible from
both the app model and the view components.

> **Note for beginners**: The constants below use Go's `iota` keyword, which auto-assigns
> incrementing integers starting from 0. So `Navigable = 0`, `Focused = 1`, `ChangesTab = 0`,
> `HistoryTab = 1`, and so on. This is Go's idiomatic way to define enumerations.

```go
package core

// FocusMode determines whether global shortcuts are active.
type FocusMode int

const (
	// Navigable is the default mode. Global shortcuts (q, ?, Tab, 1/2/3) work.
	// j/k navigate within the active pane.
	Navigable FocusMode = iota

	// Focused means all keystrokes go to the active pane. Global shortcuts
	// are blocked. Esc returns to Navigable.
	Focused
)

// Tab identifies which tab is shown.
type Tab int

const (
	ChangesTab Tab = iota
	HistoryTab
)

// Pane identifies a UI pane by its positional number.
type Pane int

const (
	PaneNone     Pane = iota
	Pane1                    // Changes: Changed Files  | History: Commit List
	Pane2                    // Changes: Diff Viewer     | History: Changed Files
	Pane3                    // Changes: Commit Message  | History: Diff Viewer
	PaneTerminal             // Terminal/Log pane (both tabs)
)

// PaneName returns the display name for a pane in the given tab.
func PaneName(p Pane, t Tab) string {
	if p == PaneTerminal {
		return "Terminal"
	}
	if t == ChangesTab {
		switch p {
		case Pane1:
			return "Changed Files"
		case Pane2:
			return "Diff Viewer"
		case Pane3:
			return "Commit Message"
		}
	} else {
		switch p {
		case Pane1:
			return "Commit List"
		case Pane2:
			return "Changed Files"
		case Pane3:
			return "Diff Viewer"
		}
	}
	return ""
}
```

**File**: `internal/tui/layout/layout.go` (new file — create `internal/tui/layout/` directory first: `mkdir -p internal/tui/layout`)

Pure math — calculates the size of every layout region from the terminal dimensions. No
rendering, no Bubbletea dependency. The `Calculate()` function returns a `Dimensions` struct
that the view uses to size each pane.

```go
package layout

// Dimensions holds the calculated sizes for all layout regions.
// All values are in terminal cells (characters wide, rows tall).
type Dimensions struct {
	Width  int // total terminal width
	Height int // total terminal height

	HeaderHeight int // always 1 row
	TabBarHeight int // always 1 row

	ContentTop    int // row where content area starts
	ContentHeight int // rows available for panes

	// Sidebar (left column)
	SidebarWidth    int
	FileListHeight  int // top of sidebar (flexible)
	CommitMsgHeight int // bottom of sidebar (8 rows fixed)

	// Main area (right column)
	MainWidth  int
	DiffHeight int // top of main (flexible)

	// Terminal (bottom of main, collapsible)
	TerminalHeight int // 0 when collapsed
}

const (
	headerRows      = 1
	tabBarRows      = 1
	minSidebarWidth = 25
	sidebarRatio    = 0.30
	commitMsgRows   = 8
	defaultTermRows = 10
	minTermRows     = 3
	minPaneRows     = 3
)

// Calculate computes layout dimensions from terminal size.
// terminalOpen/terminalRows control the collapsible terminal pane.
func Calculate(width, height int, terminalOpen bool, terminalRows int) Dimensions {
	d := Dimensions{
		Width:        width,
		Height:       height,
		HeaderHeight: headerRows,
		TabBarHeight: tabBarRows,
	}

	d.ContentTop = headerRows + tabBarRows
	d.ContentHeight = height - d.ContentTop
	if d.ContentHeight < 1 {
		d.ContentHeight = 1
	}

	// ── Horizontal: sidebar | main ──
	d.SidebarWidth = int(float64(width) * sidebarRatio)
	if d.SidebarWidth < minSidebarWidth {
		d.SidebarWidth = minSidebarWidth
	}
	if d.SidebarWidth > width/2 {
		d.SidebarWidth = width / 2
	}
	d.MainWidth = width - d.SidebarWidth
	if d.MainWidth < 1 {
		d.MainWidth = 1
	}

	// ── Terminal ──
	if terminalOpen {
		d.TerminalHeight = terminalRows
		if d.TerminalHeight < minTermRows {
			d.TerminalHeight = minTermRows
		}
		maxTerm := d.ContentHeight * 80 / 100 // 80% of content area, using integer math
		if d.TerminalHeight > maxTerm {
			d.TerminalHeight = maxTerm
		}
	}

	// ── Sidebar vertical: file list (top) + commit msg (bottom) ──
	d.CommitMsgHeight = commitMsgRows
	d.FileListHeight = d.ContentHeight - d.CommitMsgHeight
	if d.FileListHeight < minPaneRows {
		d.FileListHeight = minPaneRows
		d.CommitMsgHeight = d.ContentHeight - d.FileListHeight
	}

	// ── Main vertical: diff (top) + terminal (bottom) ──
	d.DiffHeight = d.ContentHeight - d.TerminalHeight
	if d.DiffHeight < minPaneRows {
		d.DiffHeight = minPaneRows
	}

	return d
}

// DefaultTerminalRows returns the default terminal height when first opened.
func DefaultTerminalRows() int {
	return defaultTermRows
}
```

**File**: `internal/tui/views/header.go` (new file)

Renders the header bar and tab bar. Both span the full terminal width. The header shows the
repo name, branch name, and a quick action button. The tab bar highlights the active tab.

In Phase 4, the branch name is passed as an empty string — Phase 5 fills it in with the
real branch from `git branch --show-current`.

```go
package views

import (
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/core"
)

// RenderHeader renders the top header bar with repo, branch, and action.
func RenderHeader(repoName, branchName string, width int) string {
	if branchName == "" {
		branchName = "(loading...)"
	}

	bg := lipgloss.Color("#1E1E1E")

	repoStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(bg).
		Padding(0, 1)

	branchStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Background(bg).
		Padding(0, 1)

	actionStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(bg).
		Padding(0, 1)

	sep := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Background(bg).
		Render(" │ ")

	left := repoStyle.Render("⎇ "+repoName) +
		sep +
		branchStyle.Render("ᚠ "+branchName) +
		sep +
		actionStyle.Render("↻ Fetch")

	return lipgloss.NewStyle().Width(width).Background(bg).Render(left)
}

// RenderTabBar renders the tab indicator showing Changes and History.
func RenderTabBar(activeTab core.Tab, width int) string {
	bg := lipgloss.Color("#161B22")

	activeStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(bg).
		Padding(0, 1).
		Underline(true)

	inactiveStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(bg).
		Padding(0, 1)

	var tabs string
	if activeTab == core.ChangesTab {
		tabs = activeStyle.Render("Changes") + " " + inactiveStyle.Render("History")
	} else {
		tabs = inactiveStyle.Render("Changes") + " " + activeStyle.Render("History")
	}

	return lipgloss.NewStyle().Width(width).Background(bg).Render(tabs)
}
```

### 4.2 Focus & Input Model (Navigable vs Focused States)

Panes have two input modes, tracked by `core.FocusMode`:

**Navigable mode** (default):
- Global shortcuts work: `q`, `?`, `Tab`, `1`/`2`/`3`, `` ` ``, `Esc`, `Ctrl+C`
- `j`/`k` navigate within the active pane
- `Enter` or typing in an input field switches to focused mode

**Focused mode**:
- All keystrokes go directly to the active pane
- Global shortcuts are **blocked** — pressing `q` doesn't quit, it types "q"
- `Esc` is the only way back to navigable mode

Panes that enter focused mode (implemented in later phases):

| Pane | Enters Focused When | Phase |
|------|---------------------|-------|
| Commit Message | `Enter` on summary/description field | 9 |
| Terminal | `` ` `` when visible, or `Enter` when highlighted | 12 |
| Branch dropdown | Opening the branch picker | 15 |

No new file is needed for this section — the focus model is integrated into `app.go` in
section 4.5 below. The `core.FocusMode`, `core.Pane`, and `core.Tab` types from section 4.1
carry the state.

### 4.3 Global Keybindings (q, ?, Tab Navigation)

These keybindings work in **navigable mode only** (when no pane is focused). `Ctrl+C` is the
exception — it always quits, even in focused mode.

| Key | Action |
|-----|--------|
| `1` / `2` / `3` | Focus pane by position (left-to-right, top-to-bottom) |
| `` ` `` | Toggle terminal pane visibility |
| `Tab` | Switch between Changes and History tabs |
| `q` | Quit the application |
| `?` | Toggle the help overlay |
| `Esc` | Unfocus current pane / dismiss modal |
| `Ctrl+C` | Quit (always works, even in focused mode) |

Pressing `?` opens a centered help overlay that lists all keybindings. Press `?` or `Esc`
again to close it.

**File**: `internal/tui/views/helpoverlay.go` (new file)

```go
package views

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// RenderHelpOverlay renders a centered help screen showing all keybindings.
func RenderHelpOverlay(width, height int) string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	sectionStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#D2A8FF"))

	keyStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF7B72")).
		Width(12)

	descStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	row := func(key, desc string) string {
		return keyStyle.Render(key) + descStyle.Render(desc)
	}

	lines := []string{
		titleStyle.Render("Keyboard Shortcuts"),
		"",
		sectionStyle.Render("Global (navigable mode)"),
		row("1 / 2 / 3", "Focus pane (positional, per tab)"),
		row("`", "Toggle terminal pane"),
		row("Tab", "Switch Changes / History tab"),
		row("q", "Quit"),
		row("?", "Toggle this help"),
		row("Esc", "Unfocus pane / dismiss modal"),
		row("Ctrl+C", "Quit (always works)"),
		"",
		sectionStyle.Render("Pane Navigation (navigable)"),
		row("j / k", "Navigate up / down"),
		row("Enter", "Focus pane / select item"),
		"",
		sectionStyle.Render("Focused Mode"),
		descStyle.Render("  All keys go to the active pane."),
		row("Esc", "Return to navigable mode"),
		"",
		hintStyle.Render("Press ? or Esc to close"),
	}

	content := strings.Join(lines, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3)

	box := boxStyle.Render(content)

	return lipgloss.NewStyle().
		Width(width).
		Height(height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
```

### 4.4 Error Modal System

Errors display as a **centered modal dialog** that takes over the screen. The modal has:
- A title bar with the error category (e.g., "Git Error", "Network Error", "AI Error")
- The error message body
- For **retryable** errors: `[Retry]` and `[Dismiss]` buttons — `Tab` switches between them,
  `Enter` activates the focused button
- For **non-retryable** errors: `[OK]` button only
- `Esc` or `Enter` dismisses the modal
- Background fetch notifications reuse the same modal pattern

**File**: `internal/tui/views/errormodal.go` (new file)

```go
package views

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// ErrorDismissedMsg is sent when the user dismisses the error modal.
// This is a Bubbletea "message" — a plain struct used as a signal. When the modal
// is dismissed, it returns a tea.Cmd (a function that returns this message), which
// Bubbletea will execute and deliver back to the root model's Update method.
type ErrorDismissedMsg struct{}

// ErrorModalModel is a centered modal that displays errors.
type ErrorModalModel struct {
	Title     string  // e.g., "Git Error", "Network Error"
	Message   string  // the error message
	Retryable bool    // true = show Retry + Dismiss, false = show OK only
	RetryCmd  tea.Cmd // command to run when user clicks Retry
	Visible   bool

	retryFocused bool // which button is highlighted (true = Retry)
	width        int
	height       int
}

// Update handles input when the modal is visible.
func (m ErrorModalModel) Update(msg tea.Msg) (ErrorModalModel, tea.Cmd) {
	if !m.Visible {
		return m, nil
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch msg.String() {
		case "escape", "q":
			m.Visible = false
			return m, func() tea.Msg { return ErrorDismissedMsg{} }

		case "enter":
			if m.Retryable && m.retryFocused && m.RetryCmd != nil {
				m.Visible = false
				return m, m.RetryCmd
			}
			m.Visible = false
			return m, func() tea.Msg { return ErrorDismissedMsg{} }

		case "tab":
			if m.Retryable {
				m.retryFocused = !m.retryFocused
			}
			return m, nil
		}
	}

	return m, nil
}

// View renders the error modal centered on screen.
func (m ErrorModalModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#F85149")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9")).
		Align(lipgloss.Center)

	activeBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#58A6FF")).
		Padding(0, 2)

	inactiveBtnStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(lipgloss.Color("#21262D")).
		Padding(0, 2)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	title := titleStyle.Render(m.Title)
	message := messageStyle.Render(m.Message)

	var buttons string
	if m.Retryable {
		retryBtn := inactiveBtnStyle.Render("Retry")
		dismissBtn := inactiveBtnStyle.Render("Dismiss")
		if m.retryFocused {
			retryBtn = activeBtnStyle.Render("Retry")
		} else {
			dismissBtn = activeBtnStyle.Render("Dismiss")
		}
		buttons = retryBtn + "  " + dismissBtn
	} else {
		buttons = activeBtnStyle.Render("OK")
	}

	hint := hintStyle.Render("Enter to confirm • Esc to dismiss")

	content := strings.Join([]string{title, "", message, "", buttons, "", hint}, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#F85149")).
		Padding(1, 3).
		Align(lipgloss.Center).
		MaxWidth(60)

	box := boxStyle.Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// ShowError creates a visible error modal ready to display.
func ShowError(title, message string, retryable bool, retryCmd tea.Cmd, width, height int) ErrorModalModel {
	return ErrorModalModel{
		Title:        title,
		Message:      message,
		Retryable:    retryable,
		RetryCmd:     retryCmd,
		Visible:      true,
		retryFocused: true,
		width:        width,
		height:       height,
	}
}
```

To trigger an error modal from anywhere in the app (used in later phases):

```go
m.errorModal = views.ShowError(
    "Git Error",
    "Failed to run git status: exit code 128",
    true,           // retryable
    someRetryCmd,   // command to retry on [Retry]
    m.width, m.height,
)
```

### 4.5 Wiring It All Together

**File**: `internal/tui/app/app.go` (replace entire file)

This is a complete rewrite of `app.go` from Phase 3. The startup flow (auth → resolve repo →
picker) is preserved unchanged. What's new is the `stateMain` rendering and input handling:
the layout, focus model, global keybindings, help overlay, and error modal integration.

> **Important**: "Replace entire file" means you should delete all existing content from
> `app.go` and paste the code below as the new file contents. Do not try to merge old and new
> code — this version already includes everything from Phase 3 that needs to be kept.

```go
package app

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/core"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/layout"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

type authResultMsg struct{ authenticated bool }
type repoResolvedMsg struct{ path string }
type reposDiscoveredMsg struct{ repos []string }

// ── Commands ────────────────────────────────────────────

func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}
		return repoResolvedMsg{path: ""}
	}
}

func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string
		switch cfg.Repos.Mode {
		case "manual":
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default:
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}
		return reposDiscoveredMsg{repos: repos}
	}
}

// ── App State ───────────────────────────────────────────

type appState int

const (
	stateAuthChecking     appState = iota
	stateAuthBlocked
	stateResolvingRepo
	stateDiscoveringRepos
	stateRepoPicker
	stateMain
)

// ── Model ───────────────────────────────────────────────

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	cliPath  string // original CLI arg (may be empty)
	repoPath string // resolved repo path (set once a repo is chosen)
	width    int
	height   int
	quitting bool

	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel

	// Layout & focus
	activeTab      core.Tab
	activePane     core.Pane
	focusMode      core.FocusMode
	terminalOpen   bool
	terminalHeight int
	showHelp       bool
	errorModal     views.ErrorModalModel
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// ── Update ──────────────────────────────────────────────

// Update handles all incoming messages.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to picker if active
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		// Keep error modal dimensions in sync
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			return m, nil
		}
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		m.repoPicker, _ = m.repoPicker.Update(tea.WindowSizeMsg{
			Width: m.width, Height: m.height,
		})
		m.state = stateRepoPicker
		return m, nil

	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		return m, nil

	case views.ErrorDismissedMsg:
		// Modal already hidden by the modal's own Update — nothing to do
		return m, nil

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// ── Key Handling ────────────────────────────────────────

// handleKey processes key messages based on current state.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// Ctrl+C always quits, regardless of state or focus mode
	if msg.String() == "ctrl+c" {
		m.quitting = true
		return m, tea.Quit
	}

	switch m.state {

	case stateAuthBlocked:
		// Any key → re-check auth
		if !m.authChecking {
			m.authChecking = true
			return m, checkAuthCmd
		}
		return m, nil

	case stateRepoPicker:
		var cmd tea.Cmd
		m.repoPicker, cmd = m.repoPicker.Update(msg)
		return m, cmd

	case stateMain:
		return m.handleMainKey(msg)
	}

	return m, nil
}

// handleMainKey processes keys when the app is in the main layout state.
// Priority: error modal → help overlay → focused mode → navigable keybindings.
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "escape" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			return m, nil
		}
		// TODO: forward to active pane (commit msg, terminal)
		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1 // reset to first pane on tab switch
		return m, nil

	case "1":
		m.activePane = core.Pane1
		return m, nil

	case "2":
		m.activePane = core.Pane2
		return m, nil

	case "3":
		m.activePane = core.Pane3
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
		} else if m.activePane == core.PaneTerminal {
			m.activePane = core.Pane1
		}
		return m, nil

	case "escape":
		// Already navigable — Esc is a no-op
		return m, nil
	}

	return m, nil
}

// ── State Persistence ───────────────────────────────────

// saveRepoState records the opened repo in repos-state.json (best-effort).
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state)
}

// ── View ────────────────────────────────────────────────

// View renders the current state to the terminal.
//
// Returns tea.View (Bubbletea v2 API). The View struct lets us
// declaratively set terminal features like AltScreen and MouseMode.
func (m Model) View() tea.View {
	var content string

	if !m.quitting {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			// brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			// Overlays take over the full screen when active
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else {
				content = m.viewMain()
			}
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	return v
}

// viewAuthBlocker renders the fullscreen auth message
func (m Model) viewAuthBlocker() string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	boxContent := strings.Join([]string{
		titleStyle.Render("Authentication Required"),
		"",
		messageStyle.Render("GitHub authentication is required to use this app.\n\nRun the following command in your terminal:"),
		"",
		"  " + commandStyle.Render("gh auth login"),
		"",
		hintStyle.Render("Press any key to retry • Ctrl+C to quit"),
	}, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// viewMain renders the full layout with header, tab bar, and panes.
func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar ──
	header := views.RenderHeader(git.RepoName(m.repoPath), "", dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	pane1 := renderPane(
		core.PaneName(core.Pane1, m.activeTab),
		"(file list)",
		dim.SidebarWidth, dim.FileListHeight,
		m.activePane == core.Pane1,
	)
	pane3 := renderPane(
		core.PaneName(core.Pane3, m.activeTab),
		"(commit message)",
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	pane2 := renderPane(
		core.PaneName(core.Pane2, m.activeTab),
		"(diff viewer)",
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
	mainCol := pane2
	if m.terminalOpen {
		termPane := renderPane(
			"Terminal",
			"(terminal)",
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// renderPane draws a bordered box with a title and placeholder content.
// Active panes get a blue border; inactive panes get a gray border.
func renderPane(title, placeholder string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	// Border adds 2 to width and 2 to height, so subtract to hit target outer size
	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)
	body := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")).Render(placeholder)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + body)
}
```

**What changed from Phase 3**:

1. **New imports**: `core`, `layout` packages for focus types and dimension calculations
2. **Removed import**: `fmt` — no longer needed (was used by the old placeholder `viewMain`)
3. **New model fields**: `activeTab`, `activePane`, `focusMode`, `terminalOpen`,
   `terminalHeight`, `showHelp`, `errorModal`
4. **`New()` defaults**: initializes `activeTab = ChangesTab`, `activePane = Pane1`,
   `focusMode = Navigable`
5. **`Update()` additions**: handles `ErrorDismissedMsg`, forwards `WindowSizeMsg` to error
   modal when visible
6. **`handleKey()` refactored**: the `stateMain` case now calls `handleMainKey()` — the old
   3-line quit handler is replaced with the full focus-aware key dispatch
7. **`handleMainKey()`** (new): layered key handling with clear priority — error modal → help
   overlay → focused mode → navigable mode keybindings
8. **`View()` change**: `stateMain` case checks for overlays (error modal, help) before
   rendering the layout
9. **`viewMain()` replaced**: the old centered debug text is gone — now renders the header,
   tab bar, and a 2-column pane layout using `lipgloss.JoinVertical` / `JoinHorizontal`
10. **`renderPane()`** (new): draws a bordered box with a title for each pane; border color
    indicates focus state (blue = active, gray = inactive)

### 4.6 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Layout renders**:

You should see:
- A dark header bar at the top: `⎇ repo-name │ ᚠ (loading...) │ ↻ Fetch`
- A tab bar below it with `Changes` underlined and `History` dimmed
- Two panes on the left (Changed Files on top, Commit Message on bottom)
- One pane on the right (Diff Viewer)
- The first pane (Changed Files) should have a blue border — it's the active pane
- All other panes have gray borders

**Test 2 — Pane focus with 1/2/3**:

Press `2` — the Diff Viewer pane's border should turn blue, Changed Files turns gray.
Press `3` — Commit Message pane turns blue.
Press `1` — back to Changed Files.

**Test 3 — Tab switching**:

Press `Tab` — the tab bar should switch to `History` underlined, and pane labels change:
- Pane 1: "Commit List" (instead of "Changed Files")
- Pane 2: "Changed Files" (instead of "Diff Viewer")
- Pane 3: "Diff Viewer" (instead of "Commit Message")

Press `Tab` again to return to the Changes tab.

**Test 4 — Terminal toggle**:

Press `` ` `` — a Terminal pane should appear at the bottom right with a blue border (it
gets focus automatically). The Diff Viewer above it shrinks to make room.

Press `` ` `` again — the Terminal disappears and the Diff Viewer expands back.

**Test 5 — Help overlay**:

Press `?` — the screen should show a centered help box listing all keybindings.
Press `?` or `Esc` — the help box closes and the layout returns.

**Test 6 — Quit**:

Press `q` — the app should exit cleanly.

**Test 7 — Error modal** (manual trigger for testing):

To verify the error modal works, temporarily add this line inside `app.go`'s
`repoResolvedMsg` handler (look for the `case repoResolvedMsg:` switch case in the `Update`
method), right after `m.saveRepoState()` and before `return m, nil` in the
`if msg.path != ""` block:

```go
m.errorModal = views.ShowError("Test Error", "This is a test error message.", true, nil, m.width, m.height)
```

Run the app — the error modal should appear immediately over the layout.
Press `Tab` to switch between Retry and Dismiss buttons. Press `Esc` to close.
**Remove this test line after verifying** — it's not part of the real app.

**Phase 4 is complete when**: the app shows the full layout with header, tab bar, and
bordered panes; pane focus changes with 1/2/3; Tab switches between Changes and History;
backtick toggles the terminal pane; `?` shows the help overlay; `q` quits; and the error
modal renders correctly when triggered.

## Phase 5 — Current Branch & Status

**Goal**: Run `git status` every 2 seconds to get the current branch name, ahead/behind
counts, and number of changed files. Display the branch name and a context-aware action
button (Fetch / Pull / Push) in the header bar. Also refresh on terminal focus-in events
so the UI updates instantly when you switch back from another window.

This phase introduces three important patterns:
1. **Polling with `tea.Tick`** — a repeating timer that fires every 2 seconds
2. **Async git commands** — long-running commands wrapped in `tea.Cmd` functions
3. **Focus-in refresh** — using terminal focus events to trigger immediate updates

It also migrates `View()` from returning `string` to returning `tea.View` (the proper
Bubbletea v2 API). Previous phases used the `string` shorthand, but `tea.View` is
required to enable focus reporting and other terminal features declaratively.

### 5.1 Git Status Command

**File**: `internal/git/status.go` (new file)

This file runs `git status --porcelain=2 --branch -z` and extracts branch info from
the header lines. The porcelain v2 format includes structured header lines that give us
branch name, upstream tracking info, and ahead/behind counts — all in a single command.

The header lines in porcelain v2 output look like this:

```
# branch.oid abc123def456...
# branch.head main
# branch.upstream origin/main
# branch.ab +3 -1
```

- `# branch.head` — the current branch name (or `(detached)` for detached HEAD)
- `# branch.upstream` — the upstream tracking branch (only present if one is set)
- `# branch.ab` — ahead/behind counts: `+<ahead> -<behind>` (only present if upstream exists)
- `# branch.oid` — the current commit SHA

After the header lines, the command outputs changed file entries (NUL-separated when
using `-z`). Phase 5 stores the raw output for Phase 6 to parse later — we only extract
branch metadata here.

```go
package git

import (
	"os/exec"
	"strconv"
	"strings"
)

// RepoStatus holds the branch metadata extracted from git status --porcelain=2 --branch.
// File entries are NOT parsed here — RawOutput is parsed separately for the file list.
type RepoStatus struct {
	Branch      string // current branch name (empty string if detached HEAD)
	OID         string // current commit SHA
	Upstream    string // upstream tracking branch (e.g. "origin/main"), empty if none
	Ahead       int    // commits ahead of upstream (0 if no upstream)
	Behind      int    // commits behind upstream (0 if no upstream)
	HasUpstream bool   // true if an upstream tracking branch is configured
	RawOutput string // full command output, stored for file parsing
}

// GetStatus runs `git status --porcelain=2 --branch -z` and parses the branch header
// lines. This is the primary polling command — it runs every 2 seconds.
//
// The --no-optional-locks flag prevents git from taking any optional locks, which is
// important because this command runs frequently and we don't want it to block other
// git operations. TERM=dumb prevents any color/pager behavior.
func GetStatus(repoPath string) (RepoStatus, error) {
	cmd := exec.Command("git",
		"--no-optional-locks",
		"status",
		"--untracked-files=all",
		"--branch",
		"--porcelain=2",
		"-z",
	)
	cmd.Dir = repoPath
	// cmd.Environ() copies all current environment variables (PATH, HOME, etc.)
	// so git can still find its config. We append TERM=dumb to suppress colors/pager.
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	// cmd.Output() runs the command, waits for it to finish, and returns stdout.
	out, err := cmd.Output()
	if err != nil {
		return RepoStatus{}, err
	}

	output := string(out)
	result := RepoStatus{RawOutput: output}

	// Header lines are always newline-terminated, even with -z.
	// File entries after the headers are NUL-terminated.
	for _, line := range strings.Split(output, "\n") {
		// switch{} with no expression acts like if/else-if: each case is
		// evaluated top-to-bottom, and the first true case runs.
		switch {
		case strings.HasPrefix(line, "# branch.head "):
			result.Branch = strings.TrimPrefix(line, "# branch.head ")
			if result.Branch == "(detached)" {
				result.Branch = "" // empty string signals detached HEAD
			}

		case strings.HasPrefix(line, "# branch.oid "):
			result.OID = strings.TrimPrefix(line, "# branch.oid ")

		case strings.HasPrefix(line, "# branch.upstream "):
			result.Upstream = strings.TrimPrefix(line, "# branch.upstream ")
			result.HasUpstream = true

		case strings.HasPrefix(line, "# branch.ab "):
			// Format: "+<ahead> -<behind>" (e.g., "+3 -1")
			ab := strings.TrimPrefix(line, "# branch.ab ")
			parts := strings.Fields(ab)
			if len(parts) == 2 {
				// strconv.Atoi converts a string to int. The _ discards
				// the error — if parsing fails, the value defaults to 0.
				result.Ahead, _ = strconv.Atoi(strings.TrimPrefix(parts[0], "+"))
				result.Behind, _ = strconv.Atoi(strings.TrimPrefix(parts[1], "-"))
			}
		}
	}

	return result, nil
}
```

**How the porcelain v2 parsing works**:

The `git status --porcelain=2 --branch -z` output has two sections:

1. **Header lines** (newline-separated, always present with `--branch`):
   - `# branch.oid <sha>` — always present
   - `# branch.head <name>` — always present (`(detached)` when not on a branch)
   - `# branch.upstream <name>` — only if upstream is configured
   - `# branch.ab +N -M` — only if upstream is configured

2. **File entries** (NUL-separated when using `-z`):
   - `1 <XY> ...` — ordinary changed entry
   - `2 <XY> ...` — rename/copy entry
   - `u <XY> ...` — unmerged entry
   - `? <path>` — untracked file
   - `! <path>` — ignored file

We split by `\n` and only look at lines starting with `# branch.`. The file entries
contain NUL bytes which show up as noise in the newline split, but since they never
start with `# branch.`, they're safely ignored. Phase 6 will re-parse `RawOutput`
using NUL splitting to extract the actual file list.

### 5.2 Polling Infrastructure & Focus-In Refresh

**File**: `cmd/leogit/main.go` (unchanged from Phase 1)

Since Phase 1, `main.go` already uses `tea.NewProgram(model)` with no program options — terminal
features (AltScreen, MouseMode, ReportFocus) are set declaratively on `tea.View` in `View()`,
which is the proper Bubbletea v2 pattern. This phase adds `ReportFocus = true` to the `View()`
return value. No changes needed to `main.go`:

```go
package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/tui/app"
)

func main() {
	// Optional first argument: path to a git repository
	var repoPath string
	if len(os.Args) > 1 {
		repoPath = os.Args[1]
	}

	// Load configuration (missing file is fine — defaults are used)
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error loading config: %v\n", err)
		os.Exit(1)
	}

	// Create and run the TUI
	// AltScreen, mouse mode, and focus reporting are set declaratively
	// in View() via tea.View fields (Bubbletea v2 pattern).
	model := app.New(cfg, repoPath)
	p := tea.NewProgram(model)

	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "Error running app: %v\n", err)
		os.Exit(1)
	}
}
```

**What changed from Phase 1**: `main.go` is unchanged — `tea.NewProgram(model)` was already
correct. The main change is in `app.go` below, which now adds `ReportFocus` to the `tea.View`
and wires up the status polling system.

### 5.3 Wiring It All Together

**File**: `internal/tui/views/header.go` (modify — update `RenderHeader` signature)

The header's quick action button is context-aware based on ahead/behind state:

| Ahead | Behind | Action Label |
|-------|--------|-------------|
| 0     | 0      | `↻ Fetch`   |
| 0     | > 0    | `↓ Pull`    |
| > 0   | 0      | `↑ Push`    |
| > 0   | > 0    | `↕ Pull / Push` |

When there's no upstream tracking branch, the action is always `↻ Fetch`.

The ahead/behind counts are shown next to the branch name when non-zero:
- Ahead only: `↑3` in green
- Behind only: `↓2` in red
- Both: `↑3 ↓2` in yellow

Replace the entire file:

```go
package views

import (
	"fmt"

	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/core"
)

// HeaderData holds the information needed to render the header bar.
type HeaderData struct {
	RepoName    string
	BranchName  string
	Ahead       int
	Behind      int
	HasUpstream bool
}

// RenderHeader renders the top header bar with repo name, branch, ahead/behind
// indicators, and a context-aware action button.
func RenderHeader(data HeaderData, width int) string {
	branchDisplay := data.BranchName
	if branchDisplay == "" {
		branchDisplay = "(detached)"
	}

	bg := lipgloss.Color("#1E1E1E")

	repoStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(bg).
		Padding(0, 1)

	branchStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Background(bg).
		Padding(0, 1)

	actionStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(bg).
		Padding(0, 1)

	sep := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Background(bg).
		Render(" │ ")

	// ── Branch name + ahead/behind indicators ──
	branchText := "ᚠ " + branchDisplay

	if data.HasUpstream && (data.Ahead > 0 || data.Behind > 0) {
		branchText += " "
		if data.Ahead > 0 && data.Behind > 0 {
			// Both ahead and behind — yellow indicator
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#D29922")).
				Background(bg).
				Render(fmt.Sprintf("↑%d ↓%d", data.Ahead, data.Behind))
		} else if data.Ahead > 0 {
			// Ahead only — green indicator
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#3FB950")).
				Background(bg).
				Render(fmt.Sprintf("↑%d", data.Ahead))
		} else {
			// Behind only — red indicator
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#F85149")).
				Background(bg).
				Render(fmt.Sprintf("↓%d", data.Behind))
		}
	}

	// ── Action button (context-aware) ──
	action := actionLabel(data)

	left := repoStyle.Render("⎇ "+data.RepoName) +
		sep +
		branchStyle.Render(branchText) +
		sep +
		actionStyle.Render(action)

	return lipgloss.NewStyle().Width(width).Background(bg).Render(left)
}

// actionLabel returns the quick action text based on ahead/behind state.
func actionLabel(data HeaderData) string {
	if !data.HasUpstream {
		return "↻ Fetch"
	}

	switch {
	case data.Ahead > 0 && data.Behind > 0:
		return "↕ Pull / Push"
	case data.Behind > 0:
		return "↓ Pull"
	case data.Ahead > 0:
		return "↑ Push"
	default:
		return "↻ Fetch"
	}
}

// RenderTabBar renders the tab indicator showing Changes and History.
func RenderTabBar(activeTab core.Tab, width int) string {
	bg := lipgloss.Color("#161B22")

	activeStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(bg).
		Padding(0, 1).
		Underline(true)

	inactiveStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(bg).
		Padding(0, 1)

	var tabs string
	if activeTab == core.ChangesTab {
		tabs = activeStyle.Render("Changes") + " " + inactiveStyle.Render("History")
	} else {
		tabs = inactiveStyle.Render("Changes") + " " + activeStyle.Render("History")
	}

	return lipgloss.NewStyle().Width(width).Background(bg).Render(tabs)
}
```

**What changed from Phase 4**:

1. **New `HeaderData` struct**: replaces the old `(repoName, branchName string, width int)`
   signature with a struct that carries all header info — cleaner than adding more parameters
2. **Ahead/behind indicators**: colored arrows next to the branch name (green ↑, red ↓,
   yellow ↑↓ when both)
3. **Context-aware action button**: `actionLabel()` returns the appropriate action based on
   the ahead/behind state — Fetch (default), Pull (behind), Push (ahead), or Pull / Push (both)
4. **`RenderTabBar` is unchanged** — included here because the entire file is replaced

> **Beginner note on command patterns**: In the code below, notice `checkAuthCmd` has the signature `func() tea.Msg`, which IS a `tea.Cmd` (since `tea.Cmd` is `type Cmd func() Msg`). That is why `Init()` returns `checkAuthCmd` without parentheses -- it passes the function itself. The other commands like `refreshStatusCmd(repoPath)` need parameters, so they are wrapper functions that RETURN a `tea.Cmd` closure. Both patterns are valid.

**File**: `internal/tui/app/app.go` (replace entire file)

This is the complete rewrite of `app.go` for Phase 5. Everything from Phase 4 is preserved.
What's new:

1. **`View()` returns `tea.View`** — sets `AltScreen`, `MouseMode`, and `ReportFocus` declaratively
2. **Status polling**: a `statusTickMsg` fires every 2 seconds, triggering `refreshStatusCmd`
3. **Focus-in refresh**: `tea.FocusMsg` triggers an immediate status refresh
4. **New model fields**: `branchName`, `ahead`, `behind`, `hasUpstream` — populated from git status
5. **Header receives real data**: `viewMain()` builds a `HeaderData` struct from model fields

```go
package app

import (
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/core"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/layout"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

type authResultMsg struct{ authenticated bool }
type repoResolvedMsg struct{ path string }
type reposDiscoveredMsg struct{ repos []string }

// statusTickMsg is sent every 2 seconds by the polling timer.
type statusTickMsg struct{}

// statusResultMsg carries the result of a git status command back to Update.
type statusResultMsg struct {
	status git.RepoStatus
	err    error
}

// ── Commands ────────────────────────────────────────────

func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}
		return repoResolvedMsg{path: ""}
	}
}

func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string
		switch cfg.Repos.Mode {
		case "manual":
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default:
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}
		return reposDiscoveredMsg{repos: repos}
	}
}

// refreshStatusCmd runs git status asynchronously and returns the result.
func refreshStatusCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		status, err := git.GetStatus(repoPath)
		return statusResultMsg{status: status, err: err}
	}
}

// startTickCmd starts the 2-second polling timer. When it fires, it sends a
// statusTickMsg which triggers a status refresh. The timer is restarted after
// each refresh completes, creating a continuous polling loop.
func startTickCmd() tea.Cmd {
	// tea.Tick waits for the given duration, then calls the callback.
	// The callback receives the current time (t) and must return a tea.Msg.
	return tea.Tick(2*time.Second, func(t time.Time) tea.Msg {
		return statusTickMsg{}
	})
}

// ── App State ───────────────────────────────────────────

type appState int

const (
	stateAuthChecking     appState = iota
	stateAuthBlocked
	stateResolvingRepo
	stateDiscoveringRepos
	stateRepoPicker
	stateMain
)

// ── Model ───────────────────────────────────────────────

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	cliPath  string // original CLI arg (may be empty)
	repoPath string // resolved repo path (set once a repo is chosen)
	width    int
	height   int
	quitting bool

	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel

	// Layout & focus
	activeTab      core.Tab
	activePane     core.Pane
	focusMode      core.FocusMode
	terminalOpen   bool
	terminalHeight int
	showHelp       bool
	errorModal     views.ErrorModalModel

	// Branch & status
	branchName  string // current branch from git status
	ahead       int    // commits ahead of upstream
	behind      int    // commits behind upstream
	hasUpstream bool   // whether an upstream tracking branch is configured
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// ── Update ──────────────────────────────────────────────

// Update handles all incoming messages.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to picker if active
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		// Keep error modal dimensions in sync
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			// Start polling: fetch initial status + start the 2s tick timer
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
		}
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		m.repoPicker, _ = m.repoPicker.Update(tea.WindowSizeMsg{
			Width: m.width, Height: m.height,
		})
		m.state = stateRepoPicker
		return m, nil

	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		// Start polling: fetch initial status + start the 2s tick timer
		return m, tea.Batch(
			refreshStatusCmd(m.repoPath),
			startTickCmd(),
		)

	case views.ErrorDismissedMsg:
		return m, nil

	case statusTickMsg:
		// Timer fired — refresh status and restart the timer.
		// Only poll when in the main state (don't poll during auth/picker).
		if m.state == stateMain {
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
		}
		return m, nil

	case statusResultMsg:
		if msg.err != nil {
			// Status command failed — show error but keep polling.
			// This can happen if the repo is deleted or git is broken.
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Update model with fresh status data
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		return m, nil

	case tea.FocusMsg:
		// Terminal gained focus (user switched back to this window).
		// Trigger an immediate status refresh for instant feedback.
		if m.state == stateMain {
			return m, refreshStatusCmd(m.repoPath)
		}
		return m, nil

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// ── Key Handling ────────────────────────────────────────

// handleKey processes key messages based on current state.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// Ctrl+C always quits, regardless of state or focus mode
	if msg.String() == "ctrl+c" {
		m.quitting = true
		return m, tea.Quit
	}

	switch m.state {

	case stateAuthBlocked:
		// Any key → re-check auth
		if !m.authChecking {
			m.authChecking = true
			return m, checkAuthCmd
		}
		return m, nil

	case stateRepoPicker:
		var cmd tea.Cmd
		m.repoPicker, cmd = m.repoPicker.Update(msg)
		return m, cmd

	case stateMain:
		return m.handleMainKey(msg)
	}

	return m, nil
}

// handleMainKey processes keys when the app is in the main layout state.
// Priority: error modal → help overlay → focused mode → navigable keybindings.
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "escape" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			return m, nil
		}
		// TODO: forward to active pane (commit msg, terminal)
		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1 // reset to first pane on tab switch
		return m, nil

	case "1":
		m.activePane = core.Pane1
		return m, nil

	case "2":
		m.activePane = core.Pane2
		return m, nil

	case "3":
		m.activePane = core.Pane3
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
		} else if m.activePane == core.PaneTerminal {
			m.activePane = core.Pane1
		}
		return m, nil

	case "escape":
		// Already navigable — Esc is a no-op
		return m, nil
	}

	return m, nil
}

// ── State Persistence ───────────────────────────────────

// saveRepoState records the opened repo in repos-state.json (best-effort).
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state)
}

// ── View ────────────────────────────────────────────────

// View renders the current state to the terminal.
//
// Returns tea.View (not string) — this is the proper Bubbletea v2 API.
// The View struct lets us declaratively set terminal features:
// - AltScreen: use the alternate screen buffer (fullscreen mode)
// - MouseMode: enable mouse tracking for click/scroll events
// - ReportFocus: receive tea.FocusMsg / tea.BlurMsg when the terminal gains/loses focus
func (m Model) View() tea.View {
	var content string

	if m.quitting {
		content = ""
	} else {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			content = "" // brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			// Overlays take over the full screen when active
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else {
				content = m.viewMain()
			}
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	v.ReportFocus = true // enables tea.FocusMsg / tea.BlurMsg
	return v
}

// viewAuthBlocker renders the fullscreen auth message
func (m Model) viewAuthBlocker() string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	boxContent := strings.Join([]string{
		titleStyle.Render("Authentication Required"),
		"",
		messageStyle.Render("GitHub authentication is required to use this app.\n\nRun the following command in your terminal:"),
		"",
		"  " + commandStyle.Render("gh auth login"),
		"",
		hintStyle.Render("Press any key to retry • Ctrl+C to quit"),
	}, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// viewMain renders the full layout with header, tab bar, and panes.
func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar ──
	headerData := views.HeaderData{
		RepoName:    git.RepoName(m.repoPath),
		BranchName:  m.branchName,
		Ahead:       m.ahead,
		Behind:      m.behind,
		HasUpstream: m.hasUpstream,
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	pane1 := renderPane(
		core.PaneName(core.Pane1, m.activeTab),
		"(file list)",
		dim.SidebarWidth, dim.FileListHeight,
		m.activePane == core.Pane1,
	)
	pane3 := renderPane(
		core.PaneName(core.Pane3, m.activeTab),
		"(commit message)",
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	pane2 := renderPane(
		core.PaneName(core.Pane2, m.activeTab),
		"(diff viewer)",
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
	mainCol := pane2
	if m.terminalOpen {
		termPane := renderPane(
			"Terminal",
			"(terminal)",
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// renderPane draws a bordered box with a title and placeholder content.
// Active panes get a blue border; inactive panes get a gray border.
func renderPane(title, placeholder string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	// Border adds 2 to width and 2 to height, so subtract to hit target outer size
	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)
	body := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")).Render(placeholder)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + body)
}
```

**What changed from Phase 4**:

1. **New import**: `time` — for the 2-second tick duration
2. **New message types**: `statusTickMsg` (timer fire), `statusResultMsg` (git status result)
3. **New commands**: `refreshStatusCmd()` (async git status), `startTickCmd()` (2s timer)
4. **New model fields**: `branchName`, `ahead`, `behind`, `hasUpstream` — populated from
   git status results
5. **`repoResolvedMsg` handler updated**: now uses `tea.Batch` to start both the initial
   status refresh and the polling timer simultaneously when entering `stateMain`
6. **`RepoSelectedMsg` handler updated**: same `tea.Batch` pattern — polling starts when
   picking a repo from the list too
7. **New `statusTickMsg` handler**: when the timer fires, batch a status refresh + restart
   the timer (continuous polling loop)
8. **New `statusResultMsg` handler**: updates model fields from the git status result;
   shows error modal if the command failed
9. **New `tea.FocusMsg` handler**: triggers an immediate status refresh when the terminal
   regains focus (user switched back from another app)
10. **`View()` now sets `ReportFocus = true`**: enables `tea.FocusMsg` / `tea.BlurMsg`
    events when the terminal gains/loses focus. `AltScreen` and `MouseMode` were already
    set in prior phases
11. **`viewMain()` updated**: builds a `views.HeaderData` struct and passes it to the
    updated `RenderHeader()` function, which now shows real branch/ahead/behind data

**How the polling loop works**:

```
stateMain entered
  ├── refreshStatusCmd()  → runs git status → sends statusResultMsg
  └── startTickCmd()      → waits 2s        → sends statusTickMsg
                                                   │
                                                   ├── refreshStatusCmd() → ...
                                                   └── startTickCmd()     → waits 2s → ...
```

The timer and the status command are independent — `tea.Batch` runs them concurrently.
When the tick fires, it triggers both a new refresh and a new tick, creating a continuous
2-second polling loop. The status command runs in its own goroutine (all `tea.Cmd`
functions do), so it never blocks the UI.

**Why `tea.Batch` and not sequential?** Both commands need to start at the same time.
`tea.Batch` runs multiple commands concurrently — the status fetch starts immediately
while the timer counts down independently. If the status fetch takes 500ms, the next
tick still fires exactly 2s after the previous one (not 2.5s).

**Focus-in events**: When `ReportFocus = true` is set on `tea.View`, Bubbletea sends
terminal escape sequences (`\x1b[?1004h`) that tell the terminal emulator to report
focus changes. When the user switches back to the terminal window, the emulator sends
a focus-in sequence, which Bubbletea decodes as a `tea.FocusMsg`. Most modern terminals
support this (iTerm2, WezTerm, kitty, Windows Terminal, GNOME Terminal, etc.).

### 5.4 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Branch name appears in header**:

The header bar should show the current branch name instead of `(loading...)`. For example:
```
⎇ my-project │ ᚠ main │ ↻ Fetch
```

If the repo has no upstream tracking branch, the action button always shows `↻ Fetch`.
If you're on a detached HEAD, the branch shows `(detached)`.

**Test 2 — Ahead/behind indicators**:

Create some test conditions:

```bash
# Make the repo ahead by 1 commit:
cd /path/to/repo
echo "test" >> testfile.txt && git add . && git commit -m "test ahead"
# Don't push — now you're ahead of upstream
```

The header should update within 2 seconds to show:
```
⎇ my-project │ ᚠ main ↑1 │ ↑ Push
```

The `↑1` should be green and the action should change from "Fetch" to "Push".

To test "behind": fetch without pulling:
```bash
# On another machine or clone, push a commit to the same branch.
# Then in your repo:
git fetch
```

The header should show:
```
⎇ my-project │ ᚠ main ↓2 │ ↓ Pull
```

The `↓2` should be red and the action should say "Pull".

To test both ahead and behind:
```
⎇ my-project │ ᚠ main ↑1 ↓2 │ ↕ Pull / Push
```

The indicators should be yellow when both ahead and behind.

**Test 3 — 2-second polling**:

While the app is running, open a second terminal and make changes to the repo:

```bash
# In a separate terminal:
cd /path/to/repo
touch new-file.txt
```

Wait 2 seconds — the app should automatically detect the change. You won't see the file
list yet (that's Phase 6), but the polling infrastructure is running. You can verify by
adding a temporary debug print in `statusResultMsg` handler.

**Test 4 — Focus-in refresh**:

1. Run the app
2. Switch to another terminal window
3. Make a git change (e.g., `git checkout -b new-branch`)
4. Switch back to the app

The branch name in the header should update immediately (without waiting for the 2s timer),
because the focus-in event triggers an instant refresh.

**Note**: Focus-in events depend on your terminal emulator supporting focus reporting.
If your terminal doesn't support it, the 2s polling will still catch changes — the focus
refresh is just a bonus for instant feedback.

**Test 5 — No upstream**:

```bash
git checkout -b no-upstream-test
```

The header should show `ᚠ no-upstream-test` with no ahead/behind indicators and the
action should say `↻ Fetch` (since there's no upstream to push to or pull from).

**Test 6 — Error recovery**:

Test what happens when git status fails:

```bash
# While the app is running, temporarily break the repo:
cd /path/to/repo && mv .git .git-backup
# Wait 2 seconds — the app should show an error modal
# Then restore it:
mv .git-backup .git
# Press Retry in the error modal — it should recover
```

**Phase 5 is complete when**: the header shows the real branch name and ahead/behind
indicators; the action button changes based on divergence state; status polls every
2 seconds; focus-in events trigger immediate refreshes; and git status errors show
the error modal with a retry option.

### Additional Beginner Notes for Phase 5

- **`tea.Batch(cmd1, cmd2)`**: runs multiple commands concurrently so both start at the same time. Used in `repoResolvedMsg` to start the status refresh and tick timer simultaneously.
- **`tea.FocusMsg`**: a built-in Bubbletea message. When `ReportFocus = true` is set on `tea.View`, Bubbletea sends this automatically when the terminal gains focus. You do not define it yourself.
- **Why `Branch` can be empty**: on a detached HEAD, git reports `(detached)`. `GetStatus` converts this to `""` so code can check `if branchName == ""` instead of comparing against a magic string.

## Phase 6 — Changed Files List

**Goal**: Parse the file entries from `git status --porcelain=2 -z` output (stored in
`RepoStatus.RawOutput` from Phase 5), display them as a scrollable list in the Changes
sidebar (Pane 1), and let the user navigate with `j`/`k` and select files with `Enter`.

This phase introduces:
1. **Porcelain v2 file entry parsing** — extracting paths and statuses from NUL-separated entries
2. **File status classification** — mapping XY codes to human-friendly labels and icons
3. **A custom scrollable list component** — lightweight file list with status icons and staging indicators
4. **Pane-aware keyboard routing** — `j`/`k`/`Enter` only affect the file list when Pane 1 is focused

After this phase, changed files appear in the sidebar with colored status icons, the user can
scroll through them, and pressing `Enter` emits a selection message (consumed by Phase 7 for
diff viewing). Staging toggle (`space`/`a`) is wired as a no-op — Phase 8 implements it.

### 6.1 Parse `git status --porcelain=2 -z`

**File**: `internal/git/files.go` (new file)

Phase 5 stored the full `git status` output in `RepoStatus.RawOutput`. Now we parse the
file entries from that output. The porcelain v2 format with `-z` has two sections:

1. **Header lines** (newline-terminated, start with `#`) — already parsed in Phase 5
2. **File entries** (NUL-terminated) — parsed here

The entry formats (with `-z` flag) are:

| Type | Prefix | Format | Description |
|------|--------|--------|-------------|
| Ordinary | `1` | `1 XY sub mH mI mW hH hI <path>\0` | Normal changed file |
| Rename | `2` | `2 XY sub mH mI mW hH hI Xscore <newpath>\0<oldpath>\0` | Renamed or copied file |
| Unmerged | `u` | `u XY sub m1 m2 m3 mW h1 h2 h3 <path>\0` | Merge conflict |
| Untracked | `?` | `? <path>\0` | New untracked file |

Key details:
- **Header lines use `\n`** even with `-z`. Only file entries are NUL-separated.
  This means the raw output mixes two delimiters: header lines (starting with `#`)
  end with newlines, while file entries use `\0` (NUL) as separators. The parser
  below handles this by first skipping `\n`-terminated `#` lines, then splitting
  the remainder by `\0`.
- **Rename entries consume two NUL-separated segments**: the new path and the old path.
- **XY** is a 2-character status code: `X` = index (staging area) status, `Y` = worktree status.
- **`sub`** is a 4-character submodule state (`N...` for non-submodule).
- **`mH`/`mI`/`mW`** are octal file modes (HEAD, index, worktree).
- **`hH`/`hI`** are object hashes (HEAD, index).
- **`Xscore`** for renames is a letter + score, e.g., `R100` means 100% rename match.
- Fields are space-separated, but the path can contain spaces — use `SplitN` with the
  correct field count to handle this.

```go
package git

import (
	"path/filepath"
	"strings"
)

// FileStatus represents the user-facing status of a changed file.
type FileStatus int

const (
	StatusNew        FileStatus = iota // untracked or newly added
	StatusModified                     // modified in worktree or index
	StatusDeleted                      // deleted
	StatusRenamed                      // renamed or moved
	StatusConflicted                   // unmerged / merge conflict
)

// Icon returns the short icon string for display in the file list.
func (s FileStatus) Icon() string {
	switch s {
	case StatusNew:
		return "[+]"
	case StatusModified:
		return "[M]"
	case StatusDeleted:
		return "[-]"
	case StatusRenamed:
		return "[R]"
	case StatusConflicted:
		return "[!]"
	default:
		return "[?]"
	}
}

// Label returns the full human-readable label.
func (s FileStatus) Label() string {
	switch s {
	case StatusNew:
		return "New"
	case StatusModified:
		return "Modified"
	case StatusDeleted:
		return "Deleted"
	case StatusRenamed:
		return "Renamed"
	case StatusConflicted:
		return "Conflicted"
	default:
		return "Unknown"
	}
}

// FileEntry represents a single changed file from git status.
type FileEntry struct {
	Path     string     // file path relative to repo root
	OrigPath string     // original path (only set for renames, empty otherwise)
	Status   FileStatus // user-facing status category
	Staged   bool       // true if the file has changes in the index (staging area)
	XY       string     // raw 2-character status code from porcelain v2
}

// DisplayName returns the filename (last path component) for display.
func (f FileEntry) DisplayName() string {
	return filepath.Base(f.Path)
}

// DisplayDir returns the directory portion of the path, or empty string if at repo root.
func (f FileEntry) DisplayDir() string {
	dir := filepath.Dir(f.Path)
	if dir == "." {
		return ""
	}
	return dir + "/"
}

// ParseFiles extracts FileEntry items from the RawOutput of a git status command.
// The input must be from `git status --porcelain=2 --branch -z`.
//
// Parsing strategy:
// 1. Skip header lines (start with "# ", newline-terminated)
// 2. Split the remaining content by NUL (\x00) to get entry segments
// 3. Parse each segment based on its type prefix (1, 2, u, ?)
// 4. For rename entries (type 2), consume the NEXT segment as the original path
func ParseFiles(rawOutput string) []FileEntry {
	if rawOutput == "" {
		return nil
	}

	// Skip header lines. They start with "# " and are newline-terminated.
	// Everything after the last header line is file entries.
	rest := rawOutput
	for {
		nl := strings.Index(rest, "\n")
		if nl == -1 {
			break
		}
		line := rest[:nl]
		if !strings.HasPrefix(line, "# ") {
			break
		}
		rest = rest[nl+1:]
	}

	if rest == "" {
		return nil
	}

	// Split by NUL to get entry segments.
	// Each entry is NUL-terminated, so the last element after split is empty.
	segments := strings.Split(rest, "\x00")

	var entries []FileEntry
	i := 0
	for i < len(segments) {
		seg := segments[i]
		if seg == "" {
			i++
			continue
		}

		switch {
		case strings.HasPrefix(seg, "1 "):
			// Type 1: ordinary changed entry
			if e := parseOrdinaryEntry(seg); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "2 "):
			// Type 2: rename/copy entry
			// The NEXT NUL-separated segment is the original (old) path
			origPath := ""
			if i+1 < len(segments) {
				origPath = segments[i+1]
				// Why i++ here? Rename entries are unique: they produce TWO
				// NUL-separated segments (newpath\0oldpath\0) instead of one.
				// This extra increment skips past the oldpath segment so the
				// outer loop's own i++ doesn't try to parse it as a new entry.
				i++ // consume the extra segment
			}
			if e := parseRenameEntry(seg, origPath); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "u "):
			// Type u: unmerged (conflict) entry
			if e := parseUnmergedEntry(seg); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "? "):
			// Type ?: untracked file (just a path, no metadata)
			path := strings.TrimPrefix(seg, "? ")
			entries = append(entries, FileEntry{
				Path:   path,
				Status: StatusNew,
				Staged: false,
				XY:     "??",
			})
		}

		i++
	}

	return entries
}

// parseOrdinaryEntry parses a type-1 (ordinary changed) entry.
// Format: "1 XY sub mH mI mW hH hI <path>"
// 9 fields total — use SplitN to handle paths with spaces.
func parseOrdinaryEntry(line string) *FileEntry {
	// SplitN splits into at most 9 pieces, so the 9th piece captures everything
	// remaining — including spaces. For example:
	//   Input:  "1 M. N... 100644 100644 100644 abc123 def456 my cool file.go"
	//   fields[0..7] get the first 8 space-separated tokens, and fields[8]
	//   becomes "my cool file.go" (unsplit) — exactly the file path we need.
	fields := strings.SplitN(line, " ", 9)
	if len(fields) < 9 {
		return nil
	}

	xy := fields[1]
	path := fields[8]

	return &FileEntry{
		Path:   path,
		Status: statusFromXY(xy),
		Staged: isStagedXY(xy),
		XY:     xy,
	}
}

// parseRenameEntry parses a type-2 (rename/copy) entry.
// Format: "2 XY sub mH mI mW hH hI Xscore <newpath>"
// 10 fields total. origPath is the next NUL-separated segment.
func parseRenameEntry(line string, origPath string) *FileEntry {
	fields := strings.SplitN(line, " ", 10)
	if len(fields) < 10 {
		return nil
	}

	xy := fields[1]
	path := fields[9]

	return &FileEntry{
		Path:     path,
		OrigPath: origPath,
		Status:   StatusRenamed,
		Staged:   isStagedXY(xy),
		XY:       xy,
	}
}

// parseUnmergedEntry parses a type-u (unmerged/conflict) entry.
// Format: "u XY sub m1 m2 m3 mW h1 h2 h3 <path>"
// 11 fields total.
func parseUnmergedEntry(line string) *FileEntry {
	fields := strings.SplitN(line, " ", 11)
	if len(fields) < 11 {
		return nil
	}

	xy := fields[1]
	path := fields[10]

	return &FileEntry{
		Path:   path,
		Status: StatusConflicted,
		Staged: false, // conflicts are not considered "staged"
		XY:     xy,
	}
}

// statusFromXY maps the 2-character XY status code to a FileStatus.
//
// X = status in the index (staging area)
// Y = status in the worktree
//
// Priority order: Conflicted → New → Renamed → Deleted → Modified
func statusFromXY(xy string) FileStatus {
	if len(xy) != 2 {
		return StatusModified
	}
	x, y := xy[0], xy[1]

	// Unmerged states: any U, or both added (AA), or both deleted (DD)
	if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
		return StatusConflicted
	}

	// Untracked
	if x == '?' {
		return StatusNew
	}

	// Added to index (new file)
	if x == 'A' {
		return StatusNew
	}

	// Renamed in index
	if x == 'R' {
		return StatusRenamed
	}

	// Deleted (either in index or worktree)
	if x == 'D' || y == 'D' {
		return StatusDeleted
	}

	// Modified (M in either position, C for copied, or any other combination)
	return StatusModified
}

// isStagedXY returns true if the file has changes in the index (staging area).
// X represents the index status:
//   - '.' means unmodified in the index
//   - '?' means untracked
//   - '!' means ignored
//
// Any other X value means the file has staged changes.
func isStagedXY(xy string) bool {
	if len(xy) != 2 {
		return false
	}
	x := xy[0]
	return x != '.' && x != '?' && x != '!'
}
```

### 6.2 File Status Labels (New, Modified, Deleted, Renamed, Conflicted)

The `statusFromXY()` function in `files.go` maps the raw 2-character XY codes to five
user-facing statuses. Here is the complete mapping:

| XY Code | X (Index) | Y (Worktree) | Status | Icon | Staged? |
|---------|-----------|-------------|--------|------|---------|
| `??` | `?` | `?` | New | `[+]` | No |
| `A.` | `A` (added) | `.` (unchanged) | New | `[+]` | Yes |
| `AM` | `A` (added) | `M` (modified) | New | `[+]` | Yes |
| `.M` | `.` (unchanged) | `M` (modified) | Modified | `[M]` | No |
| `M.` | `M` (modified) | `.` (unchanged) | Modified | `[M]` | Yes |
| `MM` | `M` (modified) | `M` (modified) | Modified | `[M]` | Yes |
| `.D` | `.` (unchanged) | `D` (deleted) | Deleted | `[-]` | No |
| `D.` | `D` (deleted) | `.` (unchanged) | Deleted | `[-]` | Yes |
| `R.` | `R` (renamed) | `.` (unchanged) | Renamed | `[R]` | Yes |
| `RM` | `R` (renamed) | `M` (modified) | Renamed | `[R]` | Yes |
| `UU` | `U` (unmerged) | `U` (unmerged) | Conflicted | `[!]` | No |
| `AA` | `A` (both added) | `A` (both added) | Conflicted | `[!]` | No |
| `DD` | `D` (both deleted) | `D` (both deleted) | Conflicted | `[!]` | No |
| `AU` | `A` (added by us) | `U` (unmerged) | Conflicted | `[!]` | No |
| `UA` | `U` (unmerged) | `A` (added by them) | Conflicted | `[!]` | No |

**The "Staged?" column** is determined by `isStagedXY()`: if `X` is anything other than
`.`, `?`, or `!`, the file has changes in the staging area. This is a simplification —
a file can have BOTH staged and unstaged changes (e.g., `MM` means modified in index AND
modified in worktree). Phase 8 will handle the nuance of partial staging.

**The `Icon()` and `Label()` methods** on `FileStatus` provide the display strings used
in the file list component (section 6.3 below).

### 6.3 Changed Files Sidebar List UI

**File**: `internal/tui/components/filelist.go` (new file)

This is a custom scrollable list component — simpler and lighter than the `bubbles/list`
widget, which includes features we don't need (fuzzy filtering, pagination, status bar).
Our component renders a compact list optimized for the sidebar width.

Each line in the list shows:
```
 ● [M] filename.go  src/lib/
 ○ [+] new_file.go
```

- **`●`** (green) = file has staged changes, **`○`** (gray) = unstaged only
- **`[M]`** = status icon with color (green for new, yellow for modified, red for deleted/conflicted, blue for renamed)
- **`filename.go`** = file name (bright)
- **`src/lib/`** = directory path (dim, omitted if file is at repo root)
- For renames: **`← old_name.go`** appended in dim text

The cursor row gets a blue highlight background. The list scrolls when it exceeds the
visible height, keeping the cursor always visible.

```go
package components

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// FileSelectedMsg is sent when the user presses Enter on a file in the list.
// This is used to show the diff for the selected file.
type FileSelectedMsg struct {
	Index int
	File  git.FileEntry
}

// FileListModel displays a scrollable list of changed files with status icons.
type FileListModel struct {
	Files  []git.FileEntry // current list of changed files
	cursor int             // index of the highlighted file
	offset int             // scroll offset (first visible index)
	width  int             // available width for rendering (inner, excluding borders)
	height int             // available height in rows (inner, excluding borders and title)
}

// NewFileList creates an empty file list. Files are set via SetFiles().
func NewFileList() FileListModel {
	return FileListModel{}
}

// SetFiles replaces the file list contents and resets the cursor if out of bounds.
// Called every time a statusResultMsg arrives with fresh git status data.
func (m *FileListModel) SetFiles(files []git.FileEntry) {
	m.Files = files
	if m.cursor >= len(m.Files) {
		m.cursor = max(0, len(m.Files)-1)
	}
	m.clampOffset()
}

// SetSize updates the available rendering dimensions.
// Called when the terminal resizes or the layout changes.
func (m *FileListModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.clampOffset()
}

// SelectedFile returns the currently highlighted file, or nil if the list is empty.
func (m FileListModel) SelectedFile() *git.FileEntry {
	if len(m.Files) == 0 || m.cursor >= len(m.Files) {
		return nil
	}
	return &m.Files[m.cursor]
}

// Update handles navigation keys when the file list pane is focused.
// Only called when the file list pane (Pane 1 on Changes tab) is the active pane.
func (m FileListModel) Update(msg tea.Msg) (FileListModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < len(m.Files)-1 {
				m.cursor++
				m.clampOffset()
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.clampOffset()
			}
			return m, nil

		case "g":
			// Jump to top of the list
			m.cursor = 0
			m.clampOffset()
			return m, nil

		case "G":
			// Jump to bottom of the list
			if len(m.Files) > 0 {
				m.cursor = len(m.Files) - 1
				m.clampOffset()
			}
			return m, nil

		case "enter":
			// Select the current file → triggers diff view 
			if len(m.Files) > 0 && m.cursor < len(m.Files) {
				file := m.Files[m.cursor]
				// This is how Bubbletea commands work: we return a function
				// (tea.Cmd) that produces a tea.Msg. Bubbletea calls this
				// function asynchronously, and when it returns, the resulting
				// message is fed back into Update(). This pattern lets you
				// run work in the background without blocking the UI.
				return m, func() tea.Msg {
					return FileSelectedMsg{Index: m.cursor, File: file}
				}
			}
			return m, nil

		// space and 'a' are reserved for staging.
		// Handling them here as no-ops prevents them from bubbling up
		// to the global key handler (where 'a' might conflict with
		// future shortcuts).
		case " ":
			// toggle staging for the selected file
			return m, nil

		case "a":
			// stage/unstage all files
			return m, nil
		}
	}

	return m, nil
}

// View renders the file list as a string that fits within the configured width/height.
func (m FileListModel) View() string {
	if len(m.Files) == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Italic(true).
			Render("No changed files")
	}

	// Calculate the visible window based on scroll offset
	visibleHeight := m.height
	if visibleHeight <= 0 {
		visibleHeight = 10 // fallback if size not yet set
	}

	end := m.offset + visibleHeight
	if end > len(m.Files) {
		end = len(m.Files)
	}

	// ── Styles ──────────────────────────────────────────
	cursorStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#264F78"))

	stagedStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))
	unstagedStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58"))

	statusColors := map[git.FileStatus]lipgloss.Color{
		git.StatusNew:        lipgloss.Color("#3FB950"), // green
		git.StatusModified:   lipgloss.Color("#D29922"), // yellow
		git.StatusDeleted:    lipgloss.Color("#F85149"), // red
		git.StatusRenamed:    lipgloss.Color("#58A6FF"), // blue
		git.StatusConflicted: lipgloss.Color("#F85149"), // red
	}

	dirStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58"))

	// ── Render each visible row ─────────────────────────
	var lines []string
	for i := m.offset; i < end; i++ {
		file := m.Files[i]

		// Staging indicator: ● staged, ○ unstaged
		staging := unstagedStyle.Render("○")
		if file.Staged {
			staging = stagedStyle.Render("●")
		}

		// Status icon [M], [+], [-], [R], [!] with color
		iconColor := statusColors[file.Status]
		icon := lipgloss.NewStyle().
			Foreground(iconColor).
			Bold(true).
			Render(file.Status.Icon())

		// File name (bright) + directory path (dim)
		name := file.DisplayName()
		dir := file.DisplayDir()
		dirText := ""
		if dir != "" {
			dirText = " " + dirStyle.Render(dir)
		}

		// Rename arrow: ← old_path
		rename := ""
		if file.OrigPath != "" {
			rename = " " + dirStyle.Render("← "+file.OrigPath)
		}

		// Assemble the line: " ● [M] filename  dir/  ← oldname"
		line := fmt.Sprintf(" %s %s %s%s%s", staging, icon, name, dirText, rename)

		if i == m.cursor {
			// Highlight the entire row with a blue background
			// Pad to full width for a solid highlight bar
			padded := line + strings.Repeat(" ", max(0, m.width-lipgloss.Width(line)))
			lines = append(lines, cursorStyle.Render(padded))
		} else {
			lines = append(lines, line)
		}
	}

	return strings.Join(lines, "\n")
}

// clampOffset adjusts the scroll offset to keep the cursor visible within the viewport.
//
// Virtual scrolling concept: the full file list may have 100 items, but the
// screen only shows (say) 20 rows. We maintain a "window" defined by m.offset
// (the index of the first visible item) and visibleHeight (how many items fit
// on screen). As the cursor moves, we slide this window so the cursor is always
// inside it — scrolling down when the cursor passes the bottom edge, and
// scrolling up when it passes the top edge.
func (m *FileListModel) clampOffset() {
	visibleHeight := m.height
	if visibleHeight <= 0 {
		visibleHeight = 10
	}

	// Scroll down if cursor is below the visible window
	if m.cursor >= m.offset+visibleHeight {
		m.offset = m.cursor - visibleHeight + 1
	}

	// Scroll up if cursor is above the visible window
	if m.cursor < m.offset {
		m.offset = m.cursor
	}

	// Clamp to valid range
	if m.offset < 0 {
		m.offset = 0
	}
	maxOffset := len(m.Files) - visibleHeight
	if maxOffset < 0 {
		maxOffset = 0
	}
	if m.offset > maxOffset {
		m.offset = maxOffset
	}
}
```

### 6.4 File Selection & Navigation

**File**: `internal/tui/app/app.go` (replace entire file)

This wires the file list component into the main app model. The key changes from Phase 5:

1. **New import**: `components` package for the file list, `fmt` for file count formatting
2. **New model field**: `fileList components.FileListModel` — the changed files sidebar component
3. **`statusResultMsg` handler**: after updating branch info, parses `RawOutput` into file
   entries and updates the file list via `SetFiles()`
4. **`WindowSizeMsg` handler**: recalculates layout and calls `fileList.SetSize()` so the
   list knows how many rows are visible
5. **`handleMainKey()` default case**: any key not consumed by global shortcuts gets forwarded
   to `handlePaneKey()` — this routes `j`/`k`/`Enter`/`space`/`a`/`g`/`G` to the file list
   when Pane 1 is active
6. **New `handlePaneKey()` method**: dispatches key events to the active pane's component.
   Only Pane 1 on the Changes tab has a component (file list) — other panes are still
   placeholders. Future phases add Pane 2 (diff viewer), Pane 3 (commit message), etc.
7. **New `FileSelectedMsg` handler**: receives the selection event from the file list.
   Phase 7 will use this to show the diff — for now it's a no-op acknowledgment.
8. **`viewMain()` updated**: Pane 1 shows the file list view (with file count in the title)
   instead of a placeholder string
9. **`renderPane()` updated**: the body content is no longer wrapped in a gray foreground
   style — each component manages its own styling. The parameter name changes from
   `placeholder` to `content` to reflect this.

```go
package app

import (
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/core"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/components"
	"github.com/LeoManrique/leogit/internal/tui/layout"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

type authResultMsg struct{ authenticated bool }
type repoResolvedMsg struct{ path string }
type reposDiscoveredMsg struct{ repos []string }

// statusTickMsg is sent every 2 seconds by the polling timer.
type statusTickMsg struct{}

// statusResultMsg carries the result of a git status command back to Update.
type statusResultMsg struct {
	status git.RepoStatus
	err    error
}

// ── Commands ────────────────────────────────────────────

func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}
		return repoResolvedMsg{path: ""}
	}
}

func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string
		switch cfg.Repos.Mode {
		case "manual":
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default:
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}
		return reposDiscoveredMsg{repos: repos}
	}
}

// refreshStatusCmd runs git status asynchronously and returns the result.
func refreshStatusCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		status, err := git.GetStatus(repoPath)
		return statusResultMsg{status: status, err: err}
	}
}

// startTickCmd starts the 2-second polling timer.
func startTickCmd() tea.Cmd {
	return tea.Tick(2*time.Second, func(t time.Time) tea.Msg {
		return statusTickMsg{}
	})
}

// ── App State ───────────────────────────────────────────

type appState int

const (
	stateAuthChecking     appState = iota
	stateAuthBlocked
	stateResolvingRepo
	stateDiscoveringRepos
	stateRepoPicker
	stateMain
)

// ── Model ───────────────────────────────────────────────

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	cliPath  string // original CLI arg (may be empty)
	repoPath string // resolved repo path (set once a repo is chosen)
	width    int
	height   int
	quitting bool

	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel

	// Layout & focus
	activeTab      core.Tab
	activePane     core.Pane
	focusMode      core.FocusMode
	terminalOpen   bool
	terminalHeight int
	showHelp       bool
	errorModal     views.ErrorModalModel

	// Branch & status
	branchName  string // current branch from git status
	ahead       int    // commits ahead of upstream
	behind      int    // commits behind upstream
	hasUpstream bool   // whether an upstream tracking branch is configured

	// Changed files
	fileList components.FileListModel
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
		fileList:     components.NewFileList(),
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// ── Update ──────────────────────────────────────────────

// Update handles all incoming messages.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to picker if active
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		// Keep error modal dimensions in sync
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		// Update file list dimensions when in main state
		if m.state == stateMain {
			m.updateFileListSize()
		}
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			// Start polling: fetch initial status + start the 2s tick timer
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
		}
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		m.repoPicker, _ = m.repoPicker.Update(tea.WindowSizeMsg{
			Width: m.width, Height: m.height,
		})
		m.state = stateRepoPicker
		return m, nil

	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		// Start polling: fetch initial status + start the 2s tick timer
		return m, tea.Batch(
			refreshStatusCmd(m.repoPath),
			startTickCmd(),
		)

	case views.ErrorDismissedMsg:
		return m, nil

	case statusTickMsg:
		// Timer fired — refresh status and restart the timer.
		if m.state == stateMain {
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
		}
		return m, nil

	case statusResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Update branch info
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		// Parse and update changed files
		files := git.ParseFiles(msg.status.RawOutput)
		m.fileList.SetFiles(files)
		m.updateFileListSize()
		return m, nil

	case tea.FocusMsg:
		// Terminal gained focus — trigger immediate refresh
		if m.state == stateMain {
			return m, refreshStatusCmd(m.repoPath)
		}
		return m, nil

	case components.FileSelectedMsg:
		// A file was selected from the list.
		// This is used to display the diff in Pane 2.
		// For now, just acknowledge the selection (no-op).
		return m, nil

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// updateFileListSize recalculates and sets the file list dimensions from the current layout.
// Inner dimensions = pane dimensions minus borders (2) and title line (1).
//
// Why -2 for width? The border draws 1 character on the left and 1 on the right,
// so the usable inner width is SidebarWidth minus 2.
//
// Why -3 for height? The border takes 2 rows (top + bottom) and the pane title
// takes 1 row, leaving FileListHeight minus 3 rows for actual file entries.
func (m *Model) updateFileListSize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	m.fileList.SetSize(dim.SidebarWidth-2, dim.FileListHeight-3)
}

// ── Key Handling ────────────────────────────────────────

// handleKey processes key messages based on current state.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// Ctrl+C always quits, regardless of state or focus mode
	if msg.String() == "ctrl+c" {
		m.quitting = true
		return m, tea.Quit
	}

	switch m.state {

	case stateAuthBlocked:
		// Any key → re-check auth
		if !m.authChecking {
			m.authChecking = true
			return m, checkAuthCmd
		}
		return m, nil

	case stateRepoPicker:
		var cmd tea.Cmd
		m.repoPicker, cmd = m.repoPicker.Update(msg)
		return m, cmd

	case stateMain:
		return m.handleMainKey(msg)
	}

	return m, nil
}

// handleMainKey processes keys when the app is in the main layout state.
// Priority: error modal → help overlay → focused mode → navigable keybindings → pane keys.
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "escape" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			return m, nil
		}
		// TODO: forward to active pane (commit msg, terminal)
		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1 // reset to first pane on tab switch
		return m, nil

	case "1":
		m.activePane = core.Pane1
		return m, nil

	case "2":
		m.activePane = core.Pane2
		return m, nil

	case "3":
		m.activePane = core.Pane3
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
		} else if m.activePane == core.PaneTerminal {
			m.activePane = core.Pane1
		}
		// Recalculate file list size since terminal toggle changes available height
		m.updateFileListSize()
		return m, nil

	case "escape":
		// Already navigable — Esc is a no-op
		return m, nil
	}

	// ── Forward unhandled keys to the active pane ──
	// This routes j/k/enter/space/a/g/G to the correct pane component.
	// Keys the pane doesn't handle are silently dropped (no-op).
	return m.handlePaneKey(msg)
}

// handlePaneKey forwards key events to the component that owns the active pane.
// Each pane's component handles its own navigation and selection keys.
func (m Model) handlePaneKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.activePane {

	case core.Pane1:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 1 = Changed Files list
			var cmd tea.Cmd
			m.fileList, cmd = m.fileList.Update(msg)
			return m, cmd
		}
		// History tab → Pane 1 = Commit List
		return m, nil

	case core.Pane2:
		// Changes tab → Diff Viewer
		// History tab → Changed Files in commit
		return m, nil

	case core.Pane3:
		// Changes tab → Commit Message
		// History tab → Diff Viewer
		return m, nil

	case core.PaneTerminal:
		// Terminal pane
		return m, nil
	}

	return m, nil
}

// ── State Persistence ───────────────────────────────────

// saveRepoState records the opened repo in repos-state.json (best-effort).
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state)
}

// ── View ────────────────────────────────────────────────

// View renders the current state to the terminal.
func (m Model) View() tea.View {
	var content string

	if m.quitting {
		content = ""
	} else {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			content = "" // brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			// Overlays take over the full screen when active
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else {
				content = m.viewMain()
			}
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	v.ReportFocus = true // enables tea.FocusMsg / tea.BlurMsg
	return v
}

// viewAuthBlocker renders the fullscreen auth message
func (m Model) viewAuthBlocker() string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	boxContent := strings.Join([]string{
		titleStyle.Render("Authentication Required"),
		"",
		messageStyle.Render("GitHub authentication is required to use this app.\n\nRun the following command in your terminal:"),
		"",
		"  " + commandStyle.Render("gh auth login"),
		"",
		hintStyle.Render("Press any key to retry • Ctrl+C to quit"),
	}, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// viewMain renders the full layout with header, tab bar, and panes.
func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar ──
	headerData := views.HeaderData{
		RepoName:    git.RepoName(m.repoPath),
		BranchName:  m.branchName,
		Ahead:       m.ahead,
		Behind:      m.behind,
		HasUpstream: m.hasUpstream,
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	var pane1Content string
	var pane1Title string
	if m.activeTab == core.ChangesTab {
		// Changes tab → Pane 1 = Changed Files list with file count in title
		fileCount := len(m.fileList.Files)
		if fileCount > 0 {
			pane1Title = fmt.Sprintf("Changed Files (%d)", fileCount)
		} else {
			pane1Title = "Changed Files"
		}
		pane1Content = m.fileList.View()
	} else {
		// History tab → Pane 1 = Commit List
		pane1Title = core.PaneName(core.Pane1, m.activeTab)
		pane1Content = "(commit list)"
	}

	pane1 := renderPane(
		pane1Title,
		pane1Content,
		dim.SidebarWidth, dim.FileListHeight,
		m.activePane == core.Pane1,
	)
	pane3 := renderPane(
		core.PaneName(core.Pane3, m.activeTab),
		"(commit message)",
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	pane2 := renderPane(
		core.PaneName(core.Pane2, m.activeTab),
		"(diff viewer)",
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
	mainCol := pane2
	if m.terminalOpen {
		termPane := renderPane(
			"Terminal",
			"(terminal)",
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// renderPane draws a bordered box with a title and content.
// Active panes get a blue border; inactive panes get a gray border.
// The content string manages its own styling — renderPane does NOT
// apply any foreground color to it (unlike the earlier version which grayed out placeholders).
func renderPane(title, content string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	// Border adds 2 to width and 2 to height, so subtract to hit target outer size
	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}
```

**What changed from Phase 5**:

1. **New imports**: `fmt` (for file count formatting), `components` (file list component)
2. **New model field**: `fileList components.FileListModel` — embedded file list component
3. **`New()` updated**: initializes `fileList` with `components.NewFileList()`
4. **`WindowSizeMsg` handler**: calls `updateFileListSize()` when in `stateMain` so the
   file list knows how many rows are visible after a terminal resize
5. **`statusResultMsg` handler**: after updating branch info, calls `git.ParseFiles()` to
   extract file entries from `RawOutput`, then calls `fileList.SetFiles()` and
   `updateFileListSize()` to update the sidebar
6. **New `FileSelectedMsg` handler**: receives file selection events from the file list
   component (consumed by Phase 7 for diff viewing — currently a no-op)
7. **New `updateFileListSize()` helper**: recalculates layout dimensions and calls
   `fileList.SetSize()` — called from multiple places (resize, status update, terminal toggle)
8. **`handleMainKey()` updated**: added a `default` case that forwards unhandled keys to
   `handlePaneKey()` — this routes `j`/`k`/`Enter`/`space`/`a`/`g`/`G` to the file list
   instead of dropping them
9. **New `handlePaneKey()` method**: dispatches keys to the active pane's component. Currently
   only Pane 1 on the Changes tab has a component (file list). Other panes return no-op —
   they'll be connected in Phases 7, 9, 12, and 16
10. **Terminal toggle (`` ` ``)**: now calls `updateFileListSize()` because toggling the
    terminal changes the available content height
11. **`viewMain()` updated**: Pane 1 renders the file list view with a dynamic title that
    shows the file count (e.g., "Changed Files (5)"). On the History tab, Pane 1 remains
    a placeholder for Phase 16
12. **`renderPane()` updated**: the body content is no longer wrapped in
    `lipgloss.NewStyle().Foreground(gray)` — each component manages its own styling. The
    parameter name changes from `placeholder` to `content`

### 6.5 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Changed files appear in sidebar**:

Navigate to a repo with some changes (modified, untracked, or staged files). The "Changed
Files" pane (top-left, Pane 1) should show a list of files with status icons:

```
Changed Files (3)
 ○ [M] main.go  src/
 ○ [+] newfile.txt
 ○ [-] removed.go  old/
```

- `[M]` should be yellow (modified)
- `[+]` should be green (new/untracked)
- `[-]` should be red (deleted)
- The first file should have a blue highlight bar (cursor)

If there are no changes, the pane should show "No changed files" in gray italic.

**Test 2 — Navigation with j/k**:

Make sure Pane 1 is the active pane (press `1` if needed). Then:

- Press `j` or `↓` — cursor moves down
- Press `k` or `↑` — cursor moves up
- Press `g` — cursor jumps to the top
- Press `G` — cursor jumps to the bottom

If you have more files than the pane height, the list should scroll to keep the cursor visible.

**Test 3 — File selection with Enter**:

Press `Enter` on a file — nothing visible happens yet (Phase 7 will show the diff). But
the `FileSelectedMsg` is sent and handled (you can verify by temporarily adding a debug
print in the `FileSelectedMsg` handler in `app.go`).

**Test 4 — Staged files show ● indicator**:

```bash
# Stage a file:
cd /path/to/repo
git add somefile.go
```

Wait 2 seconds for the status poll. The staged file should show a green `●` instead of
the gray `○`. Files with only unstaged changes show `○`.

```
Changed Files (2)
 ● [M] somefile.go  src/     ← staged (green dot)
 ○ [M] otherfile.go  src/   ← unstaged (gray dot)
```

**Test 5 — Rename detection**:

```bash
git mv oldname.go newname.go
```

The file list should show:

```
 ● [R] newname.go  ← oldname.go
```

The `[R]` icon should be blue, and the old name appears dimmed after an arrow.

**Test 6 — Pane focus routing**:

- Press `1` to focus Pane 1 (Changed Files) — `j`/`k` navigate the file list
- Press `2` to focus Pane 2 (Diff Viewer) — `j`/`k` do nothing (Pane 2 has no component yet)
- Press `Tab` to switch to History tab — Pane 1 now says "Commit List" with a placeholder
- Press `Tab` again to return to Changes tab — file list is back

**Test 7 — Real-time updates**:

While the app is running with Pane 1 showing files:

```bash
# In a separate terminal:
cd /path/to/repo
touch brand_new_file.txt
```

Wait 2 seconds — `brand_new_file.txt` should appear in the file list as `[+]` (green, new).

```bash
rm brand_new_file.txt
```

Wait 2 seconds — the file should disappear from the list. The file count in the title
should update accordingly.

**Test 8 — Scroll with many files**:

Create a repo with many changed files:

```bash
cd /tmp && mkdir scroll-test && cd scroll-test && git init
for i in $(seq 1 50); do echo "content" > "file_$i.txt"; done
```

```bash
./leogit /tmp/scroll-test
```

The file list should show as many files as fit in the pane height, with the cursor on the
first file. Press `j` repeatedly — the list scrolls when the cursor reaches the bottom edge.
Press `G` to jump to the last file, `g` to jump back to the top.

**Test 9 — Space and 'a' are reserved**:

With Pane 1 focused, press `space` and `a`. Nothing should happen (no-op), and importantly
the app should NOT quit or show the help overlay — these keys are consumed by the file list
component even though their real functionality comes in Phase 8.

**Phase 6 is complete when**: the sidebar shows changed files with colored status icons and
staging indicators; files update every 2 seconds from git status polling; `j`/`k` navigate
the list; `g`/`G` jump to top/bottom; `Enter` emits a selection message; `space` and `a`
are reserved no-ops; the file count appears in the pane title; and pane focus routing
sends keys only to the active pane's component.

## Phase 7 — Diff Viewer

**Goal**: When the user selects a file from the Changed Files list (Pane 1), show a
syntax-highlighted, scrollable diff in the Diff Viewer pane (Pane 2). This is the first
pane that renders real git content on the right side of the layout.

This phase introduces:
1. **Git diff commands** — running `git diff` for working-tree and staged changes, plus
   `git diff --no-index` for untracked files
2. **Diff parsing** — extracting file headers, hunk headers, and individual lines from
   unified diff output into structured Go types
3. **Syntax highlighting** — using Chroma to colorize code within diff lines based on
   the file's language
4. **A scrollable diff component** — a Bubbletea sub-component with `j`/`k` scrolling,
   `g`/`G` jump-to-top/bottom, and `s` to toggle unified/side-by-side view mode (side-by-side
   rendering is a stub — Phase 7 implements unified only)

After this phase, selecting a file shows its color-coded diff in Pane 2. Added lines are
green, deleted lines are red, hunk headers are cyan, and code tokens are syntax-highlighted
within each line. The user can scroll the diff with `j`/`k` or `g`/`G`. Pressing `s`
toggles a flag for future side-by-side support (Phase 7 always renders unified).

### 7.1 Generate Diff (`git diff` / `git diff --cached`)

**File**: `internal/git/diff.go` (new file)

This file runs the appropriate `git diff` command based on the file's status. There are
three cases:

| File status | Command | Why |
|-------------|---------|-----|
| Tracked, unstaged changes | `git diff HEAD -- <path>` | Shows working tree vs HEAD |
| Tracked, staged changes | `git diff --cached -- <path>` | Shows index vs HEAD |
| Untracked (new file) | `git diff --no-index -- /dev/null <path>` | Treats the entire file as added |

All commands use `--no-ext-diff` (disable external diff tools), `--no-color` (we do our own
coloring with Chroma), and `--patch-with-raw` (includes raw diff stat header for rename
detection). `TERM=dumb` is set in the environment to prevent any pager behavior.

> **Note**: The design doc's Git Command Reference shows `-z` (NUL-separated output) in the
> diff commands. We intentionally omit `-z` here because the diff parser in 7.2 splits output
> on newlines (`\n`), which is the natural format for unified diffs. The `-z` flag is used
> in other commands (like `git status`) where file paths may contain special characters.

For untracked files, `git diff --no-index` exits with code 1 when differences are found
(which is always, since we're comparing against `/dev/null`). This is expected — we must
not treat exit code 1 as an error for this command.

```go
package git

import (
	"os/exec"
	"strings"
)

// GetDiff runs the appropriate git diff command for a file and returns the raw
// NOTE: FileEntry (with Status and Staged fields) is defined in internal/git/files.go.
// unified diff output. The command chosen depends on the file's status:
//   - Staged files use --cached to diff the index against HEAD
//   - Untracked files use --no-index against /dev/null to show the full file as added
//   - All other tracked files diff the working tree against HEAD
//
// The returned string is the full unified diff output, ready for parsing.
func GetDiff(repoPath string, file FileEntry) (string, error) {
	var args []string

	switch {
	case file.Status == StatusNew:
		// Untracked file: compare /dev/null with the file to show it as entirely new.
		// --no-index tells git to diff two paths outside the index.
		args = []string{
			"diff",
			"--no-ext-diff",
			"--patch-with-raw",
			"--no-color",
			"--no-index",
			"--", "/dev/null", file.Path,
		}

	case file.Staged:
		// Staged (indexed) changes: diff the index against HEAD.
		args = []string{
			"diff",
			"--no-ext-diff",
			"--patch-with-raw",
			"--no-color",
			"--cached",
			"--", file.Path,
		}

	default:
		// Tracked file with working-tree changes: diff working tree against HEAD.
		args = []string{
			"diff",
			"--no-ext-diff",
			"--patch-with-raw",
			"--no-color",
			"HEAD",
			"--", file.Path,
		}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		// git diff --no-index exits with code 1 when differences exist.
		// This is expected for untracked files — only fail if there's no output.
		if file.Status == StatusNew && len(out) > 0 {
			return string(out), nil
		}
		return "", err
	}

	return string(out), nil
}

// GetDiffWhitespaceIgnored runs the same diff as GetDiff but with the -w flag
// to ignore all whitespace changes. Used when the user enables "hide whitespace".
func GetDiffWhitespaceIgnored(repoPath string, file FileEntry) (string, error) {
	// For simplicity, get the normal diff command args and inject -w.
	// We duplicate the logic rather than adding a parameter to keep GetDiff clean.
	var args []string

	switch {
	case file.Status == StatusNew:
		args = []string{
			"diff", "--no-ext-diff", "--patch-with-raw", "--no-color",
			"--no-index", "-w",
			"--", "/dev/null", file.Path,
		}
	case file.Staged:
		args = []string{
			"diff", "--no-ext-diff", "--patch-with-raw", "--no-color",
			"--cached", "-w",
			"--", file.Path,
		}
	default:
		args = []string{
			"diff", "--no-ext-diff", "--patch-with-raw", "--no-color",
			"HEAD", "-w",
			"--", file.Path,
		}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		if file.Status == StatusNew && len(out) > 0 {
			return string(out), nil
		}
		return "", err
	}

	return string(out), nil
}

// GetCommitDiff returns the diff for a specific file in a specific commit.
// Used by the History tab to show what changed in a selected commit.
func GetCommitDiff(repoPath, sha, filePath string) (string, error) {
	cmd := exec.Command("git",
		"log", sha,
		"-m", "-1", "--first-parent",
		"--patch-with-raw",
		"--format=",
		"--no-color",
		"-z",
		"--", filePath,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return string(out), nil
}
```

**How it works**:

- `GetDiff` is the primary function called when a user selects a file in the Changes tab.
  The `FileEntry.Status` and `FileEntry.Staged` fields (from Phase 6) determine which
  variant of `git diff` to run.
- `GetDiffWhitespaceIgnored` is identical but adds `-w` for the "hide whitespace" config
  option (`[diff] hide_whitespace` in config.toml).
- `GetCommitDiff` is included now but won't be called until Phase 16 (History tab). It
  uses `git log --patch-with-raw` to get the diff for a single file within a single commit.
- All three functions return the raw unified diff string. Phase 7.2 parses this into
  structured types.

### 7.2 Diff Parsing (Hunks, Lines, Headers)

**File**: `internal/diff/parse.go` (new file)

This is the diff parser. It takes the raw unified diff string from `GetDiff()` and breaks
it into structured Go types: a `FileDiff` containing a list of `Hunk`s, each containing
a list of `Line`s. These types live in `internal/diff/` (not `internal/git/`) because
the diff package is UI-agnostic and will also be used by the staging logic in Phase 8.

A unified diff looks like this:

```
diff --git a/main.go b/main.go
index abc1234..def5678 100644
--- a/main.go
+++ b/main.go
@@ -10,6 +10,8 @@ func main() {
     existing line
     existing line
+    added line
+    another added line
     existing line
-    removed line
     existing line
@@ -30,4 +32,4 @@ func helper() {
     context
-    old version
+    new version
     context
```

Key parsing rules:
- Lines starting with `diff --git` begin a new file diff
- Lines starting with `---` and `+++` are the old/new file paths
- Lines starting with `@@` are hunk headers with line numbers
- Lines starting with `+` are additions (green)
- Lines starting with `-` are deletions (red)
- Lines starting with ` ` (space) are context (unchanged)
- Lines starting with `\` are "no newline at end of file" markers
- Everything before the first `@@` is the file header (metadata)

The hunk header `@@ -10,6 +10,8 @@` means: starting at line 10 in the old file (6 lines),
starting at line 10 in the new file (8 lines). The count is optional — `@@ -1 +1 @@` means
a single line in both.

```go
package diff

import (
	"regexp"
	"strconv"
	"strings"
)

// LineType classifies a line within a diff hunk.
type LineType int

const (
	LineContext  LineType = iota // unchanged line (prefix: " ")
	LineAdd                     // added line (prefix: "+")
	LineDelete                  // deleted line (prefix: "-")
	LineHunk                    // hunk header line (prefix: "@@")
	LineNoNewline               // "\ No newline at end of file"
)

// Line represents a single line within a diff hunk.
type Line struct {
	Text      string   // the full line text including the +/-/space prefix
	Content   string   // the line text WITHOUT the +/-/space prefix (for syntax highlighting)
	Type      LineType // context, add, delete, hunk header, or no-newline marker
	OldLineNo int      // line number in the old file (0 if not applicable, e.g., added lines)
	NewLineNo int      // line number in the new file (0 if not applicable, e.g., deleted lines)
}

// HunkHeader holds the parsed line numbers from a @@ hunk header.
type HunkHeader struct {
	OldStart int // start line in old file
	OldCount int // number of lines in old file (default 1 if omitted)
	NewStart int // start line in new file
	NewCount int // number of lines in new file (default 1 if omitted)
}

// Hunk represents a single hunk within a file diff — a contiguous group of changes
// surrounded by context lines.
type Hunk struct {
	Header HunkHeader // parsed @@ line numbers
	Lines  []Line     // all lines in this hunk, including the @@ header line itself
}

// FileDiff represents the complete parsed diff for a single file.
type FileDiff struct {
	OldPath    string // path in the old version (from "--- a/...")
	NewPath    string // path in the new version (from "+++ b/...")
	FileHeader string // raw metadata lines before the first hunk (diff --git, index, ---, +++)
	Hunks      []Hunk // parsed hunks
}

// TotalLines returns the total number of displayable lines across all hunks.
// This is used to calculate scroll height.
func (d *FileDiff) TotalLines() int {
	total := 0
	for _, h := range d.Hunks {
		total += len(h.Lines)
	}
	return total
}

// AllLines returns a flat slice of all lines across all hunks, in order.
// This is used by the diff viewer for rendering and scrolling.
func (d *FileDiff) AllLines() []Line {
	var lines []Line
	for _, h := range d.Hunks {
		lines = append(lines, h.Lines...)
	}
	return lines
}

// hunkHeaderRegex matches the @@ line in a unified diff.
// Format: @@ -oldStart[,oldCount] +newStart[,newCount] @@[ optional section heading]
var hunkHeaderRegex = regexp.MustCompile(
	`^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$`,
)

// parseHunkHeader extracts line numbers from a @@ header string.
func parseHunkHeader(line string) (HunkHeader, bool) {
	matches := hunkHeaderRegex.FindStringSubmatch(line)
	if matches == nil {
		return HunkHeader{}, false
	}

	h := HunkHeader{
		OldStart: atoi(matches[1]),
		OldCount: 1, // default if omitted
		NewStart: atoi(matches[3]),
		NewCount: 1, // default if omitted
	}

	if matches[2] != "" {
		h.OldCount = atoi(matches[2])
	}
	if matches[4] != "" {
		h.NewCount = atoi(matches[4])
	}

	return h, true
}

// atoi converts a string to int, returning 0 on failure.
func atoi(s string) int {
	n, _ := strconv.Atoi(s)
	return n
}

// Parse takes raw unified diff output (from git diff) and parses it into a FileDiff.
// Returns nil if the input is empty or contains no hunks.
func Parse(raw string) *FileDiff {
	if strings.TrimSpace(raw) == "" {
		return nil
	}

	lines := strings.Split(raw, "\n")

	result := &FileDiff{}
	var headerLines []string
	var currentHunk *Hunk
	oldLine, newLine := 0, 0
	inHeader := true

	for _, line := range lines {
		// ── Hunk header ──
		if strings.HasPrefix(line, "@@") {
			inHeader = false

			header, ok := parseHunkHeader(line)
			if !ok {
				continue
			}

			// Save previous hunk
			if currentHunk != nil {
				result.Hunks = append(result.Hunks, *currentHunk)
			}

			oldLine = header.OldStart
			newLine = header.NewStart

			currentHunk = &Hunk{
				Header: header,
				Lines: []Line{{
					Text:    line,
					Content: line,
					Type:    LineHunk,
				}},
			}
			continue
		}

		// ── File header (everything before the first @@) ──
		if inHeader {
			// Extract old/new paths from --- and +++ lines
			if strings.HasPrefix(line, "--- ") {
				path := strings.TrimPrefix(line, "--- ")
				path = strings.TrimPrefix(path, "a/") // git prefixes with a/
				result.OldPath = path
			} else if strings.HasPrefix(line, "+++ ") {
				path := strings.TrimPrefix(line, "+++ ")
				path = strings.TrimPrefix(path, "b/") // git prefixes with b/
				result.NewPath = path
			}
			headerLines = append(headerLines, line)
			continue
		}

		// ── Inside a hunk — classify each line ──
		if currentHunk == nil {
			continue
		}

		if len(line) == 0 {
			// Empty line in diff = context line with empty content.
			// This happens because strings.Split turns a blank line into "",
			// but in unified diff format it would normally be " " (space prefix).
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      " ",
				Content:   "",
				Type:      LineContext,
				OldLineNo: oldLine,
				NewLineNo: newLine,
			})
			oldLine++
			newLine++
			continue
		}

		switch line[0] {
		case '+':
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   line[1:],
				Type:      LineAdd,
				OldLineNo: 0, // added lines have no old line number
				NewLineNo: newLine,
			})
			newLine++

		case '-':
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   line[1:],
				Type:      LineDelete,
				OldLineNo: oldLine,
				NewLineNo: 0, // deleted lines have no new line number
			})
			oldLine++

		case '\\':
			// "\ No newline at end of file" — display but don't count
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:    line,
				Content: line,
				Type:    LineNoNewline,
			})

		default:
			// Context line (starts with space)
			content := line
			if len(line) > 0 && line[0] == ' ' {
				content = line[1:]
			}
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   content,
				Type:      LineContext,
				OldLineNo: oldLine,
				NewLineNo: newLine,
			})
			oldLine++
			newLine++
		}
	}

	// Save the last hunk
	if currentHunk != nil {
		result.Hunks = append(result.Hunks, *currentHunk)
	}

	result.FileHeader = strings.Join(headerLines, "\n")

	if len(result.Hunks) == 0 {
		return nil
	}

	return result
}
```

**How the parser works**:

1. Lines are scanned sequentially from top to bottom.
2. Everything before the first `@@` is collected as the file header. `---` and `+++` lines
   are parsed to extract old/new paths.
3. When a `@@` line is encountered, the regex extracts `OldStart`, `OldCount`, `NewStart`,
   and `NewCount`. A new `Hunk` is started with the `@@` line itself as the first entry
   (type `LineHunk`).
4. Inside a hunk, each line is classified by its first character (`+`, `-`, ` `, `\`).
   Line numbers are tracked independently for old and new sides:
   - **Context** (`" "`): increments both `oldLine` and `newLine`
   - **Add** (`"+"`): increments only `newLine`, sets `OldLineNo = 0`
   - **Delete** (`"-"`): increments only `oldLine`, sets `NewLineNo = 0`
   - **No-newline** (`"\"`): no line number increment
5. The `Content` field strips the `+`/`-`/` ` prefix so Chroma can highlight the pure
   source code without diff markers.
6. `AllLines()` flattens all hunks into a single slice for the scroll-based diff viewer.

### 7.3 Syntax-Highlighted Diff Display (Chroma)

**File**: `internal/tui/render/diff.go` (new file)

This is the diff renderer. It takes a parsed `FileDiff` and a visible window (scroll offset
+ height) and returns a styled string for the diff pane. It uses Chroma to syntax-highlight
the code content within each line, then wraps each line in diff-specific coloring (green for
adds, red for deletes, cyan for hunk headers).

Chroma works in three steps:
1. **Lexer** — identifies the language from the file extension (e.g., `.go` → Go lexer)
2. **Style** — a color theme (we use `"monokai"` — good on dark terminals)
3. **Formatter** — converts tokens to ANSI escape sequences (`"terminal256"` for 256-color
   terminals, which covers virtually all modern terminals)

For diff rendering, we don't highlight entire files — we highlight individual lines. Each
line's `Content` (the prefix-stripped code) is tokenized by the lexer and formatted by
Chroma, then wrapped in a diff-colored background/foreground.

The line number gutter shows both old and new line numbers side-by-side. Added lines show
only the new line number, deleted lines show only the old line number, and context lines
show both.

```go
package render

import (
	"bytes"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/alecthomas/chroma/v2"
	"github.com/alecthomas/chroma/v2/formatters"
	"github.com/alecthomas/chroma/v2/lexers"
	"github.com/alecthomas/chroma/v2/styles"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/diff"
)

// DiffColors holds the lipgloss styles for each diff line type.
var DiffColors = struct {
	Add       lipgloss.Style
	Delete    lipgloss.Style
	Hunk      lipgloss.Style
	Context   lipgloss.Style
	LineNoOld lipgloss.Style
	LineNoNew lipgloss.Style
	NoNewline lipgloss.Style
}{
	Add:       lipgloss.NewStyle().Foreground(lipgloss.Color("#2EA043")), // green
	Delete:    lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149")), // red
	Hunk:      lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF")), // cyan/blue
	Context:   lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9")), // light gray
	LineNoOld: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")), // dim gray
	LineNoNew: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")), // dim gray
	NoNewline: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")).Italic(true),
}

// chromaStyle is the Chroma color theme used for syntax highlighting within diff lines.
// "monokai" works well on dark terminal backgrounds. This is applied on top of the
// diff coloring — context lines get full Chroma colors, while add/delete lines get
// tinted green/red.
var chromaStyle = styles.Get("monokai")

// chromaFormatter is the Chroma formatter for 256-color terminal output.
// TTY256 is widely supported and maps RGB colors to the nearest 256 index.
var chromaFormatter = formatters.Get("terminal256")

// highlightLine runs Chroma syntax highlighting on a single line of code.
// The lexer is determined by the file extension. Returns the ANSI-colored string.
// If highlighting fails for any reason, returns the plain text.
func highlightLine(content string, lexer chroma.Lexer) string {
	if lexer == nil || content == "" {
		return content
	}

	iterator, err := lexer.Tokenise(nil, content)
	if err != nil {
		return content
	}

	var buf bytes.Buffer
	err = chromaFormatter.Format(&buf, chromaStyle, iterator)
	if err != nil {
		return content
	}

	// Chroma may add a trailing newline — strip it for inline use
	result := buf.String()
	result = strings.TrimRight(result, "\n")
	return result
}

// getLexer returns the Chroma lexer for a file path, or nil if none matches.
// Uses the file extension to determine the language.
func getLexer(filePath string) chroma.Lexer {
	// Try matching by filename first (handles special files like Makefile, Dockerfile)
	lexer := lexers.Match(filepath.Base(filePath))
	if lexer != nil {
		return lexer
	}
	// Fallback: try by extension
	ext := filepath.Ext(filePath)
	if ext != "" {
		lexer = lexers.Get(ext)
	}
	return lexer
}

// gutterWidth is the width of each line number column in the gutter.
const gutterWidth = 5

// formatLineNo formats a line number into a fixed-width string for the gutter.
// Line number 0 means "not applicable" — renders as blank spaces.
func formatLineNo(n int) string {
	if n == 0 {
		return strings.Repeat(" ", gutterWidth)
	}
	return fmt.Sprintf("%*d", gutterWidth, n)
}

// RenderDiffLine renders a single diff line with gutter (line numbers) and
// syntax-highlighted content. Returns a single styled string.
func RenderDiffLine(line diff.Line, lexer chroma.Lexer, width int) string {
	// ── Gutter: old line | new line | prefix ──
	oldNo := DiffColors.LineNoOld.Render(formatLineNo(line.OldLineNo))
	newNo := DiffColors.LineNoNew.Render(formatLineNo(line.NewLineNo))

	// Gutter separator
	sep := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")).Render("│")

	// ── Content ──
	var prefix string
	var styledContent string

	switch line.Type {
	case diff.LineHunk:
		// Hunk headers get full cyan styling, no syntax highlighting
		return DiffColors.Hunk.Render(line.Text)

	case diff.LineAdd:
		prefix = DiffColors.Add.Render("+")
		// For added lines, apply Chroma highlighting then tint green
		highlighted := highlightLine(line.Content, lexer)
		styledContent = DiffColors.Add.Render(highlighted)

	case diff.LineDelete:
		prefix = DiffColors.Delete.Render("-")
		highlighted := highlightLine(line.Content, lexer)
		styledContent = DiffColors.Delete.Render(highlighted)

	case diff.LineContext:
		prefix = " "
		// Context lines get full Chroma syntax highlighting
		styledContent = highlightLine(line.Content, lexer)
		if styledContent == line.Content {
			// Chroma didn't highlight — use default context color
			styledContent = DiffColors.Context.Render(line.Content)
		}

	case diff.LineNoNewline:
		return DiffColors.NoNewline.Render(line.Text)
	}

	return oldNo + sep + newNo + sep + prefix + styledContent
}

// RenderDiff renders the visible portion of a diff for the diff viewer pane.
// It takes the parsed diff, the scroll offset, the number of visible rows, and
// the pane width. Returns the rendered string.
//
// Parameters:
//   - fileDiff: the parsed diff (from diff.Parse)
//   - offset: scroll position (index of the first visible line)
//   - visibleRows: how many lines fit in the pane
//   - width: pane width in columns
//
// Returns the rendered string for the visible window.
func RenderDiff(fileDiff *diff.FileDiff, offset, visibleRows, width int) string {
	if fileDiff == nil {
		return ""
	}

	allLines := fileDiff.AllLines()
	total := len(allLines)

	if total == 0 {
		return ""
	}

	// Clamp offset
	if offset < 0 {
		offset = 0
	}
	if offset >= total {
		offset = total - 1
	}

	// Determine the visible window
	end := offset + visibleRows
	if end > total {
		end = total
	}

	// Get the lexer for this file (based on the new path, falling back to old path)
	filePath := fileDiff.NewPath
	if filePath == "" || filePath == "/dev/null" {
		filePath = fileDiff.OldPath
	}
	lexer := getLexer(filePath)

	// Render each visible line
	var rendered []string
	for i := offset; i < end; i++ {
		rendered = append(rendered, RenderDiffLine(allLines[i], lexer, width))
	}

	return strings.Join(rendered, "\n")
}
```

**How the renderer works**:

1. `getLexer()` determines the programming language from the file extension. Chroma supports
   hundreds of languages — `.go`, `.py`, `.js`, `.rs`, `.tsx`, etc. If no lexer matches,
   highlighting is skipped and plain text is shown.
2. `highlightLine()` tokenizes a single line of source code and formats it to ANSI escape
   sequences using the `terminal256` formatter and `monokai` style. This produces colored
   output that works in any 256-color terminal.
3. `RenderDiffLine()` composes the full line: a gutter with old/new line numbers (dim gray,
   5 chars each), a separator `│`, the `+`/`-`/` ` prefix, and the highlighted content.
   Hunk headers (`@@`) get full cyan styling with no gutter. "No newline" markers are
   rendered in dim italic.
4. `RenderDiff()` is the top-level function called by the diff viewer component. It takes
   the scroll offset and visible row count to render only the visible window — no need to
   render thousands of lines for a large diff.

### 7.4 Diff Scrolling & Navigation

**File**: `internal/tui/components/diffview.go` (new file)

This is the Bubbletea sub-component for the diff viewer pane (Pane 2 on the Changes tab).
It holds the currently displayed diff, manages scroll position, and handles `j`/`k`/`g`/`G`
navigation keys.

Like `FileListModel` from Phase 6, this is a sub-component — its `Update()` returns the
concrete type `(DiffViewModel, tea.Cmd)` and `View()` returns `string`. Only the main
`app.Model` needs interface-compatible signatures.

The component also holds a `sideBySide` boolean toggled by `s`. Phase 7 always renders
unified — the toggle is wired but the side-by-side rendering path is a stub that will be
implemented later.

```go
package components

import (
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/diff"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/render"
)

// DiffLoadedMsg is sent when a diff has been fetched and parsed for a file.
// The app receives this from the async command and forwards it to the diff viewer.
type DiffLoadedMsg struct {
	File     git.FileEntry // the file this diff belongs to
	FileDiff *diff.FileDiff // parsed diff (nil if the file has no diff, e.g., binary)
	Err      error          // non-nil if the git diff command failed
}

// DiffViewModel displays a scrollable, syntax-highlighted diff in a pane.
type DiffViewModel struct {
	file       git.FileEntry  // the file currently being displayed
	fileDiff   *diff.FileDiff // parsed diff for the current file
	hasContent bool           // true once a diff has been loaded

	offset     int  // scroll position: index of the first visible line
	totalLines int  // total number of lines across all hunks
	width      int  // available pane width (inner, excluding borders)
	height     int  // available pane height in rows (inner, excluding borders and title)
	sideBySide bool // true = side-by-side mode (stub), false = unified
	loading    bool // true while waiting for the diff to load
	errMsg     string // non-empty if the diff command failed
}

// NewDiffView creates an empty diff viewer. Content is set via SetDiff().
func NewDiffView() DiffViewModel {
	return DiffViewModel{}
}

// SetDiff updates the diff viewer with a new parsed diff.
// Resets the scroll position to the top.
func (m *DiffViewModel) SetDiff(file git.FileEntry, fileDiff *diff.FileDiff) {
	m.file = file
	m.fileDiff = fileDiff
	m.hasContent = true
	m.loading = false
	m.errMsg = ""
	m.offset = 0
	if fileDiff != nil {
		m.totalLines = fileDiff.TotalLines()
	} else {
		m.totalLines = 0
	}
}

// SetError sets an error state for the diff viewer.
func (m *DiffViewModel) SetError(errMsg string) {
	m.errMsg = errMsg
	m.loading = false
	m.hasContent = true
	m.fileDiff = nil
	m.totalLines = 0
	m.offset = 0
}

// SetLoading puts the diff viewer into a loading state.
func (m *DiffViewModel) SetLoading() {
	m.loading = true
	m.errMsg = ""
}

// SetSize updates the available rendering dimensions.
func (m *DiffViewModel) SetSize(width, height int) {
	m.width = width
	m.height = height
}

// IsSideBySide returns whether side-by-side mode is active.
func (m DiffViewModel) IsSideBySide() bool {
	return m.sideBySide
}

// clampOffset ensures the scroll offset stays within valid bounds.
// maxOffset = totalLines - height means the last line sits at the bottom of
// the visible area. If the diff is shorter than the pane, maxOffset is 0.
func (m *DiffViewModel) clampOffset() {
	maxOffset := m.totalLines - m.height
	if maxOffset < 0 {
		maxOffset = 0
	}
	if m.offset > maxOffset {
		m.offset = maxOffset
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

// Update handles key events for the diff viewer.
// Returns the updated model and an optional command.
func (m DiffViewModel) Update(msg tea.Msg) (DiffViewModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			m.offset++
			m.clampOffset()

		case "k", "up":
			m.offset--
			m.clampOffset()

		case "g":
			// Jump to top
			m.offset = 0

		case "G":
			// Jump to bottom
			m.offset = m.totalLines - m.height
			m.clampOffset()

		case "s":
			// Toggle unified / side-by-side
			m.sideBySide = !m.sideBySide

		case "d":
			// Page down (half screen)
			m.offset += m.height / 2
			m.clampOffset()

		case "u":
			// Page up (half screen)
			m.offset -= m.height / 2
			m.clampOffset()
		}
	}

	return m, nil
}

// View renders the diff viewer content.
func (m DiffViewModel) View() string {
	if m.loading {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Loading diff...")
	}

	if m.errMsg != "" {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#F85149")).
			Render("Error: " + m.errMsg)
	}

	if !m.hasContent {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Select a file to view its diff")
	}

	if m.fileDiff == nil || m.totalLines == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("No changes in this file")
	}

	// Render only the visible window
	return render.RenderDiff(m.fileDiff, m.offset, m.height, m.width)
}
```

**How the component works**:

- **State**: The component holds the current `FileDiff`, scroll `offset`, and dimensions.
  `hasContent` tracks whether a diff has ever been loaded (to show the "Select a file..."
  placeholder initially). `loading` shows a loading indicator while the async diff command
  runs. `errMsg` shows errors from failed diff commands.
- **Scrolling**: `j`/`k` scroll one line at a time. `g`/`G` jump to top/bottom. `d`/`u`
  do half-page scrolling (Vim-style). `clampOffset()` keeps the offset within `[0, totalLines - height]`.
- **View toggle**: `s` flips `sideBySide`. The `View()` method always calls `RenderDiff()`
  (unified) for now — a future phase can check `m.sideBySide` and call a different render
  function.
- **Rendering**: `View()` calls `render.RenderDiff()` with the current offset and height,
  so only visible lines are rendered. This keeps rendering fast even for diffs with
  thousands of lines.

### 7.5 Wire It Into the App

**File**: `internal/tui/app/app.go` (modify existing)

This section connects the diff viewer to the main app. When the user selects a file in the
file list (Pane 1), an async command runs `git diff`, parses the output, and sends the
result back as a `DiffLoadedMsg`. The diff viewer component receives the parsed diff and
renders it in Pane 2.

**Changes to the Model struct** — add the diff viewer field:

```go
	// Changed files
	fileList components.FileListModel

	// Diff viewer
	diffView components.DiffViewModel
```

**Changes to `New()`** — initialize the diff viewer:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
		fileList:     components.NewFileList(),
		diffView:     components.NewDiffView(),
	}
}
```

**New async command** — runs `git diff` in the background and sends the result:

```go
// loadDiffCmd runs git diff for a file and returns the parsed result.
// It returns a tea.Cmd, which is a function that Bubbletea runs on a separate
// goroutine. When the function finishes, its return value (a tea.Msg) is sent
// back to Update(). This keeps the UI responsive while git runs.
func loadDiffCmd(repoPath string, file git.FileEntry) tea.Cmd {
	return func() tea.Msg {
		raw, err := git.GetDiff(repoPath, file)
		if err != nil {
			return components.DiffLoadedMsg{File: file, Err: err}
		}
		parsed := diff.Parse(raw)
		return components.DiffLoadedMsg{File: file, FileDiff: parsed}
	}
}
```

**Changes to `Update()`** — handle the new messages. Replace the `FileSelectedMsg` and
add `DiffLoadedMsg`:

```go
	case components.FileSelectedMsg:
		// A file was selected from the list — load its diff.
		m.diffView.SetLoading()
		return m, loadDiffCmd(m.repoPath, msg.File)

	case components.DiffLoadedMsg:
		if msg.Err != nil {
			m.diffView.SetError(msg.Err.Error())
		} else {
			m.diffView.SetDiff(msg.File, msg.FileDiff)
		}
		return m, nil
```

**Changes to `updateFileListSize()`** — also update the diff viewer dimensions:

```go
func (m *Model) updateFileListSize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	m.fileList.SetSize(dim.SidebarWidth-2, dim.FileListHeight-3)
	// Diff viewer: subtract border (2) and title line (1) from pane dimensions
	m.diffView.SetSize(dim.MainWidth-2, dim.DiffHeight-3)
}
```

**Changes to `WindowSizeMsg` handler** — the existing `updateFileListSize()` call already
handles both components since we updated it above. No additional changes needed.

**Changes to `handlePaneKey()`** — forward keys to the diff viewer when Pane 2 is active:

```go
	case core.Pane2:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 2 = Diff Viewer
			var cmd tea.Cmd
			m.diffView, cmd = m.diffView.Update(msg)
			return m, cmd
		}
		// History tab → Changed Files in commit
		return m, nil
```

**Changes to `viewMain()`** — render the diff viewer in Pane 2:

```go
	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	var pane2Content string
	var pane2Title string
	if m.activeTab == core.ChangesTab {
		pane2Title = "Diff"
		pane2Content = m.diffView.View()
	} else {
		pane2Title = core.PaneName(core.Pane2, m.activeTab)
		pane2Content = "(commit list)"
	}
	pane2 := renderPane(
		pane2Title,
		pane2Content,
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
```

**New imports for `app.go`** — add the `diff` package:

```go
import (
	// ... existing imports ...
	"github.com/LeoManrique/leogit/internal/diff"
)
```

**What changed from Phase 6**:

1. **New import**: `diff` package (for `diff.Parse()` in the async command)
2. **New model field**: `diffView components.DiffViewModel` — the diff viewer component
3. **`New()` updated**: initializes `diffView` with `components.NewDiffView()`
4. **`FileSelectedMsg` handler**: no longer a no-op — puts the diff viewer into loading
   state and kicks off `loadDiffCmd()` to fetch and parse the diff asynchronously
5. **New `DiffLoadedMsg` handler**: receives the parsed diff (or error) from the async
   command and calls `diffView.SetDiff()` or `diffView.SetError()`
6. **New `loadDiffCmd()` function**: async command that calls `git.GetDiff()` then
   `diff.Parse()` and returns the result as a `DiffLoadedMsg`
7. **`updateFileListSize()` expanded**: now also calls `diffView.SetSize()` with the main
   pane dimensions (minus borders and title)
8. **`handlePaneKey()` updated**: Pane 2 on the Changes tab now forwards key events to the
   diff viewer component instead of returning no-op
9. **`viewMain()` updated**: Pane 2 renders `m.diffView.View()` instead of a placeholder
   string. The pane title is "Diff" on the Changes tab

### 7.6 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Select a file to show its diff**:

Make some changes to a file in the repo (e.g., add a line to a `.go` file). Press `1` to
focus Pane 1, use `j`/`k` to highlight the modified file, then press `Enter`.

Pane 2 ("Diff") should show the unified diff with:

```
@@ -10,6 +10,8 @@ func main() {               ← cyan hunk header
   10│   10│ existing line                   ← gray context line
   11│   11│ existing line                   ← gray context line
     │   12│+added line                      ← green added line
     │   13│+another added line              ← green added line
   12│   14│ existing line                   ← gray context line
   13│     │-removed line                    ← red deleted line
   14│   15│ existing line                   ← gray context line
```

- Added lines (`+`) should be green with their new line number on the right
- Deleted lines (`-`) should be red with their old line number on the left
- Context lines show both old and new line numbers
- Hunk headers (`@@`) are cyan, spanning the full width

**Test 2 — Syntax highlighting**:

Select a `.go`, `.py`, `.js`, or `.rs` file with changes. The code within each line should
have Chroma syntax highlighting — keywords in different colors, strings highlighted, etc.
The highlighting should be visible even within green (added) and red (deleted) lines.

For a file type Chroma doesn't recognize (e.g., `.txt`), lines should still render correctly
but without syntax coloring — just the diff colors (green/red/gray).

**Test 3 — Scroll with j/k**:

Press `2` to focus Pane 2 (Diff). Then:

- `j` or `↓` — scrolls down one line
- `k` or `↑` — scrolls up one line
- `g` — jumps to the top of the diff
- `G` — jumps to the bottom of the diff
- `d` — scrolls down half a page
- `u` — scrolls up half a page

If the diff is shorter than the pane height, scrolling should be a no-op (the offset stays
at 0).

**Test 4 — Untracked file diff**:

Create a brand-new file:

```bash
cd /path/to/repo
echo "hello world" > brand_new_file.txt
```

Wait 2 seconds for the status poll to pick it up. Select it in the file list and press
`Enter`. The diff should show every line as added (green `+`), since the entire file is new.
The hunk header should read something like `@@ -0,0 +1,1 @@`.

**Test 5 — Staged file diff**:

```bash
cd /path/to/repo
echo "new line" >> somefile.go
git add somefile.go
```

Wait for the status poll. Select the staged file (shown with `●` indicator). The diff
should show the staged changes (what's in the index vs HEAD), not the working tree.

**Test 6 — Side-by-side toggle**:

With Pane 2 focused, press `s`. Nothing visually changes yet (side-by-side rendering is
a stub), but the toggle is wired internally. Press `s` again to toggle back. This confirms
the key binding is working for future implementation.

**Test 7 — Empty diff**:

If a file shows in the status but has no textual diff (e.g., a binary file or a file with
only permissions changed), selecting it should show "No changes in this file" in gray italic
in Pane 2.

**Test 8 — Large diff scrolling**:

Make extensive changes to a file (add 100+ lines). Select it, then press `G` to jump to
the bottom. Line numbers should be correct throughout. Press `g` to jump back to the top.
Press `d` repeatedly for half-page scrolling — each press should move roughly half the
pane height.

**Test 9 — Loading state**:

Select a file — for a brief moment (usually too fast to see on local repos), the diff pane
shows "Loading diff..." in gray italic before the diff appears. You can verify this is
working by temporarily adding a `time.Sleep(1 * time.Second)` inside `loadDiffCmd()` before
the `git.GetDiff()` call.

**Test 10 — Switching between files**:

Navigate the file list with `j`/`k` and press `Enter` on different files. Each time, the
diff pane should reset its scroll to the top and show the diff for the newly selected file.
The previous diff is replaced entirely.

**Phase 7 is complete when**: selecting a file from the Changed Files list shows a
syntax-highlighted unified diff in Pane 2; the diff has a gutter with old/new line numbers;
added lines are green, deleted lines are red, hunk headers are cyan; Chroma syntax
highlighting colors code tokens within diff lines; `j`/`k`/`g`/`G`/`d`/`u` scroll the diff;
`s` toggles the side-by-side flag (rendering is a stub); untracked files show as fully added;
staged files show index-vs-HEAD diffs; and the loading/error/empty states display correctly.

## Phase 8 — Staging

**Goal**: Let the user stage and unstage files at three granularities: entire files, individual
hunks, and individual lines. This is the most complex feature in the Changes tab — it requires
generating partial patches and piping them to `git apply --cached`.

This phase introduces:
1. **Whole-file staging** — `git update-index` for adding/removing entire files
2. **DiffSelection** — a data structure that tracks which lines the user has selected for staging
3. **Partial patch generation** — building a valid unified diff from only the selected lines
4. **`git apply --cached`** — applying the generated patch to the index
5. **UI integration** — `space` toggles staging for a file, `a` stages/unstages all, hunk/line
   selection in the diff viewer

After this phase, pressing `space` on a file in the Changed Files list toggles it between staged
and unstaged. Pressing `a` stages all files or unstages all files (toggles based on whether any
files are currently staged). In the diff viewer (Pane 2), the user can select individual hunks
or lines and stage only those changes. The staging indicator (`●`/`○`) updates in real-time.

### 8.1 Stage / Unstage Entire File

**File**: `internal/git/staging.go` (new file)

This file provides functions to stage and unstage entire files using `git update-index` and
`git reset HEAD`. Staging whole files is a three-step process because renamed files and deleted
files require different `update-index` flags:

| File type | Command | Flags |
|-----------|---------|-------|
| Renamed (old path) | `git update-index` | `--add --remove --force-remove --replace` |
| Normal files | `git update-index` | `--add --remove --replace` |
| Deleted files | `git update-index` | `--add --remove --force-remove --replace` |

For unstaging, `git reset HEAD -- <paths>` is simpler — it works for all file types.

```go
package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// StageFiles stages the given files using git update-index.
// Files are grouped by type (renamed old paths, normal files, deleted files)
// and processed in three passes with appropriate flags.
//
// The -z flag tells git to read NUL-terminated paths from stdin, which
// handles filenames with spaces or special characters correctly.
func StageFiles(repoPath string, files []FileEntry) error {
	var renamed []string  // old paths of renamed files (need --force-remove)
	var normal []string   // normal modified/added files
	var deleted []string  // deleted files (need --force-remove)

	for _, f := range files {
		switch {
		case f.Status == StatusRenamed && f.OrigPath != "":
			renamed = append(renamed, f.OrigPath)
			normal = append(normal, f.Path) // new path goes in normal batch
		case f.Status == StatusDeleted:
			deleted = append(deleted, f.Path)
		default:
			normal = append(normal, f.Path)
		}
	}

	// Pass 1: renamed old paths (--force-remove to remove the old index entry)
	if len(renamed) > 0 {
		if err := updateIndex(repoPath, renamed, true); err != nil {
			return fmt.Errorf("staging renamed files: %w", err)
		}
	}

	// Pass 2: normal files (new paths, modified, untracked)
	if len(normal) > 0 {
		if err := updateIndex(repoPath, normal, false); err != nil {
			return fmt.Errorf("staging files: %w", err)
		}
	}

	// Pass 3: deleted files (--force-remove to remove from index)
	if len(deleted) > 0 {
		if err := updateIndex(repoPath, deleted, true); err != nil {
			return fmt.Errorf("staging deleted files: %w", err)
		}
	}

	return nil
}

// updateIndex runs git update-index with -z --stdin, writing paths as NUL-terminated
// input. If forceRemove is true, adds --force-remove (needed for renames and deletes).
func updateIndex(repoPath string, paths []string, forceRemove bool) error {
	args := []string{"update-index", "--add", "--remove", "--replace", "-z", "--stdin"}
	if forceRemove {
		args = []string{"update-index", "--add", "--remove", "--force-remove", "--replace", "-z", "--stdin"}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath

	// Write NUL-terminated paths to stdin
	input := strings.Join(paths, "\x00") + "\x00"
	cmd.Stdin = strings.NewReader(input)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s: %s", err, string(out))
	}
	return nil
}

// UnstageFiles removes the given files from the staging area using git reset HEAD.
// This works for all file types — it resets the index entry to match HEAD.
func UnstageFiles(repoPath string, files []FileEntry) error {
	if len(files) == 0 {
		return nil
	}

	args := []string{"reset", "HEAD", "--"}
	for _, f := range files {
		args = append(args, f.Path)
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("unstaging files: %s: %s", err, string(out))
	}
	return nil
}

// StageFile stages a single file (convenience wrapper around StageFiles).
func StageFile(repoPath string, file FileEntry) error {
	return StageFiles(repoPath, []FileEntry{file})
}

// UnstageFile unstages a single file (convenience wrapper around UnstageFiles).
func UnstageFile(repoPath string, file FileEntry) error {
	return UnstageFiles(repoPath, []FileEntry{file})
}
```

**How it works**:

- `StageFiles` groups files into three categories based on their status. Renamed files need
  two index operations: `--force-remove` for the old path (to delete the old index entry) and
  a normal `--add` for the new path. Deleted files also need `--force-remove`.
- `updateIndex` pipes NUL-terminated paths to `git update-index -z --stdin`. Using stdin with
  `-z` correctly handles filenames with spaces, quotes, or other special characters.
- `UnstageFiles` uses `git reset HEAD -- <paths>` which works for all cases. It resets each
  file's index entry to match HEAD (effectively unstaging).
- `StageFile`/`UnstageFile` are convenience wrappers for the common single-file case.

### 8.2 DiffSelection — Tracking Selected Lines

**File**: `internal/diff/selection.go` (new file)

The `DiffSelection` struct tracks which lines in a diff the user has selected for staging.
It uses a space-efficient representation: instead of storing a boolean per line, it stores
a `DefaultState` (all selected or none selected) plus a set of lines that diverge from
the default.

For example, if `DefaultState = All` and `DivergingLines = {5, 12}`, then every line is
selected EXCEPT lines 5 and 12. This means toggling a single line only adds one entry to
the set, rather than creating a full-size boolean array.

Only `Add` and `Delete` lines are selectable — context lines, hunk headers, and "no newline"
markers cannot be selected. The `SelectableLines` set tracks which line indices (in the
flattened `AllLines()` list) are selectable.

```go
package diff

// SelectionState represents the default state of all lines.
type SelectionState int

const (
	SelectAll  SelectionState = iota // all lines selected by default
	SelectNone                       // no lines selected by default
)

// DiffSelection tracks which diff lines are selected for staging or discarding.
// Uses a DefaultState + diverging set for space efficiency.
type DiffSelection struct {
	DefaultState    SelectionState // initial state for all lines
	DivergingLines  map[int]bool   // lines that differ from DefaultState (key = flat line index)
	SelectableLines map[int]bool   // only Add/Delete lines are selectable (key = flat line index)
}

// NewDiffSelection creates a DiffSelection for a parsed diff.
// Scans all lines to build the SelectableLines set.
// defaultState determines whether lines start selected (for staging unstaged files)
// or unselected (for unstaging staged files).
func NewDiffSelection(fileDiff *FileDiff, defaultState SelectionState) DiffSelection {
	sel := DiffSelection{
		DefaultState:    defaultState,
		DivergingLines:  make(map[int]bool),
		SelectableLines: make(map[int]bool),
	}

	if fileDiff == nil {
		return sel
	}

	// Build the selectable lines set — only Add and Delete lines can be toggled
	idx := 0
	for _, hunk := range fileDiff.Hunks {
		for _, line := range hunk.Lines {
			if line.Type == LineAdd || line.Type == LineDelete {
				sel.SelectableLines[idx] = true
			}
			idx++
		}
	}

	return sel
}

// IsSelected returns whether the line at the given flat index is selected.
func (s DiffSelection) IsSelected(lineIdx int) bool {
	_, diverges := s.DivergingLines[lineIdx]
	if s.DefaultState == SelectAll {
		return !diverges // selected unless it diverges
	}
	return diverges // not selected unless it diverges
}

// IsSelectable returns whether the line at the given flat index can be toggled.
func (s DiffSelection) IsSelectable(lineIdx int) bool {
	return s.SelectableLines[lineIdx]
}

// WithLineSelection sets a single line's selection state.
// Only works on selectable lines — non-selectable lines are ignored.
func (s DiffSelection) WithLineSelection(lineIdx int, selected bool) DiffSelection {
	if !s.SelectableLines[lineIdx] {
		return s
	}

	// Create a copy of the diverging set
	newDiverging := make(map[int]bool, len(s.DivergingLines))
	for k, v := range s.DivergingLines {
		newDiverging[k] = v
	}

	// wantsDiverge: true when the requested state differs from the default.
	// E.g. DefaultState=SelectAll + selected=false => diverges => add to set.
	// E.g. DefaultState=SelectAll + selected=true  => matches  => remove from set.
	wantsDiverge := (s.DefaultState == SelectAll) != selected
	if wantsDiverge {
		newDiverging[lineIdx] = true
	} else {
		delete(newDiverging, lineIdx)
	}

	return DiffSelection{
		DefaultState:    s.DefaultState,
		DivergingLines:  newDiverging,
		SelectableLines: s.SelectableLines,
	}
}

// WithToggle toggles a single line's selection.
func (s DiffSelection) WithToggle(lineIdx int) DiffSelection {
	return s.WithLineSelection(lineIdx, !s.IsSelected(lineIdx))
}

// WithRangeSelection sets selection for a range of lines (used for hunk-level staging).
// Only affects selectable lines in the range [from, from+count).
func (s DiffSelection) WithRangeSelection(from, count int, selected bool) DiffSelection {
	result := s
	for i := from; i < from+count; i++ {
		result = result.WithLineSelection(i, selected)
	}
	return result
}

// SelectedCount returns the number of currently selected selectable lines.
func (s DiffSelection) SelectedCount() int {
	count := 0
	for idx := range s.SelectableLines {
		if s.IsSelected(idx) {
			count++
		}
	}
	return count
}

// SelectableCount returns the total number of selectable lines.
func (s DiffSelection) SelectableCount() int {
	return len(s.SelectableLines)
}

// AllSelected returns true if every selectable line is selected.
func (s DiffSelection) AllSelected() bool {
	return s.SelectedCount() == s.SelectableCount()
}

// NoneSelected returns true if no selectable lines are selected.
func (s DiffSelection) NoneSelected() bool {
	return s.SelectedCount() == 0
}
```

**How the selection model works**:

- `DefaultState = SelectAll` means "everything is selected unless explicitly deselected".
  This is the natural state when staging an unstaged file — all changes are included by default.
- `DefaultState = SelectNone` is used when the UI starts with nothing selected and the user
  picks individual lines.
- `DivergingLines` is a set (map) of flat line indices that differ from the default. Toggling
  a line either adds it to or removes it from this set.
- `WithLineSelection` returns a new `DiffSelection` (immutable pattern) — it copies the
  diverging set rather than mutating it. This prevents subtle bugs from shared references.
- `WithRangeSelection` applies a selection to a range of lines — used when the user stages
  an entire hunk. It iterates the range and calls `WithLineSelection` for each selectable
  line in the range.

### 8.3 Partial Patch Generation

**File**: `internal/diff/patch.go` (new file)

This is the core staging logic. It takes a parsed `FileDiff` and a `DiffSelection`, and
generates a valid unified diff patch containing only the selected changes. This patch is
then piped to `git apply --cached` to stage the selected lines.

The rules for generating a partial patch from a selection:

| Line type | Selected? | Action | Effect on counts |
|-----------|-----------|--------|-----------------|
| Context | N/A (always included) | Include as-is | oldCount++, newCount++ |
| Add | Yes | Include as-is (`+` prefix) | newCount++ |
| Add | No | **Skip entirely** | (no count change) |
| Delete | Yes | Include as-is (`-` prefix) | oldCount++ |
| Delete | No | **Convert to context** — change `-` to ` ` | oldCount++, newCount++ |

The tricky part is the unselected Delete rule: an unselected deletion must be converted to
a context line (because the line still exists in the old file, and the patch needs to account
for it to maintain valid line counts).

After building the new hunk lines, the hunk header `@@ -old,count +new,count @@` must be
recalculated with the adjusted counts.

```go
package diff

import (
	"fmt"
	"strings"
)

// GeneratePatch creates a unified diff patch from a FileDiff and DiffSelection.
// Only selected Add/Delete lines are included. Unselected deletes become context.
// Unselected adds are skipped entirely.
//
// Returns the patch as a string suitable for piping to `git apply --cached`.
// Returns empty string if no selectable lines are selected.
func GeneratePatch(fileDiff *FileDiff, selection DiffSelection) string {
	if fileDiff == nil || selection.NoneSelected() {
		return ""
	}

	var patch strings.Builder

	// Write the file header (diff --git, index, ---, +++ lines)
	patch.WriteString(fileDiff.FileHeader)
	patch.WriteString("\n")

	// Track the flat line index across all hunks
	flatIdx := 0

	for _, hunk := range fileDiff.Hunks {
		hunkPatch := generateHunkPatch(hunk, selection, &flatIdx)
		if hunkPatch != "" {
			patch.WriteString(hunkPatch)
		}
	}

	result := patch.String()
	if strings.TrimSpace(result) == strings.TrimSpace(fileDiff.FileHeader) {
		// No hunks were generated — nothing to apply
		return ""
	}

	return result
}

// generateHunkPatch generates the patch for a single hunk given the selection.
// Returns empty string if the hunk has no selected changes.
// flatIdx is a pointer to the current position in the flat line array,
// advanced as lines are processed.
func generateHunkPatch(hunk Hunk, selection DiffSelection, flatIdx *int) string {
	var lines []string
	oldCount := 0
	newCount := 0
	hasChanges := false

	for _, line := range hunk.Lines {
		idx := *flatIdx
		*flatIdx++

		switch line.Type {
		case LineHunk:
			// Skip the original hunk header — we'll generate a new one
			continue

		case LineContext:
			lines = append(lines, line.Text)
			oldCount++
			newCount++

		case LineAdd:
			if selection.IsSelected(idx) {
				lines = append(lines, line.Text)
				newCount++
				hasChanges = true
			}
			// Unselected adds: skip entirely (don't include in patch)

		case LineDelete:
			if selection.IsSelected(idx) {
				lines = append(lines, line.Text)
				oldCount++
				hasChanges = true
			} else {
				// Unselected deletes: convert to context line.
				// The line exists in the old file, so the patch must account for it.
				// Note: line.Content omits the prefix char (e.g. "foo" not "-foo"),
				// so " " + Content turns a delete into a context line (" foo").
				contextLine := " " + line.Content
				lines = append(lines, contextLine)
				oldCount++
				newCount++
			}

		case LineNoNewline:
			lines = append(lines, line.Text)
		}
	}

	if !hasChanges {
		return ""
	}

	// Generate the new hunk header with recalculated counts
	header := fmt.Sprintf("@@ -%d,%d +%d,%d @@\n",
		hunk.Header.OldStart, oldCount,
		hunk.Header.NewStart, newCount,
	)

	return header + strings.Join(lines, "\n") + "\n"
}

// GenerateInversePatch creates a reverse patch for discarding selected changes.
// Selected Add lines become Delete lines, selected Delete lines become Add lines.
// This is used by "discard changes" — applied without --cached to modify the working tree.
func GenerateInversePatch(fileDiff *FileDiff, selection DiffSelection) string {
	if fileDiff == nil || selection.NoneSelected() {
		return ""
	}

	var patch strings.Builder
	patch.WriteString(fileDiff.FileHeader)
	patch.WriteString("\n")

	flatIdx := 0

	for _, hunk := range fileDiff.Hunks {
		hunkPatch := generateInverseHunkPatch(hunk, selection, &flatIdx)
		if hunkPatch != "" {
			patch.WriteString(hunkPatch)
		}
	}

	result := patch.String()
	if strings.TrimSpace(result) == strings.TrimSpace(fileDiff.FileHeader) {
		return ""
	}

	return result
}

// generateInverseHunkPatch generates the inverse patch for a single hunk.
// Adds become deletes and deletes become adds (for discarding changes).
func generateInverseHunkPatch(hunk Hunk, selection DiffSelection, flatIdx *int) string {
	var lines []string
	oldCount := 0
	newCount := 0
	hasChanges := false

	for _, line := range hunk.Lines {
		idx := *flatIdx
		*flatIdx++

		switch line.Type {
		case LineHunk:
			continue

		case LineContext:
			lines = append(lines, line.Text)
			oldCount++
			newCount++

		case LineAdd:
			if selection.IsSelected(idx) {
				// Inverse: add becomes delete
				lines = append(lines, "-"+line.Content)
				oldCount++
				hasChanges = true
			} else {
				// Unselected add: stays as context in the inverse patch
				lines = append(lines, " "+line.Content)
				oldCount++
				newCount++
			}

		case LineDelete:
			if selection.IsSelected(idx) {
				// Inverse: delete becomes add
				lines = append(lines, "+"+line.Content)
				newCount++
				hasChanges = true
			} else {
				// Unselected delete: convert to context
				lines = append(lines, " "+line.Content)
				oldCount++
				newCount++
			}

		case LineNoNewline:
			lines = append(lines, line.Text)
		}
	}

	if !hasChanges {
		return ""
	}

	// Inverse patch: swap old/new start positions
	header := fmt.Sprintf("@@ -%d,%d +%d,%d @@\n",
		hunk.Header.NewStart, oldCount,
		hunk.Header.OldStart, newCount,
	)

	return header + strings.Join(lines, "\n") + "\n"
}
```

**How partial patch generation works**:

1. The `FileDiff.FileHeader` (the `diff --git`, `index`, `---`, `+++` lines) is written first —
   `git apply` needs this to know which file the patch applies to.
2. For each hunk, lines are iterated and classified:
   - **Context lines**: always included, increment both old and new counts.
   - **Selected add lines**: included as-is (with `+` prefix), increment only new count.
   - **Unselected add lines**: skipped entirely — they don't exist in either the old or new
     version of the partial patch.
   - **Selected delete lines**: included as-is (with `-` prefix), increment only old count.
   - **Unselected delete lines**: the most subtle case. The line exists in the old file, so it
     must appear in the patch for the line counts to be correct. It's converted to a context
     line by changing the `-` prefix to ` `, and both old and new counts are incremented.
3. The hunk header is regenerated with the recalculated `oldCount` and `newCount`. The start
   positions (`OldStart`, `NewStart`) stay the same — only the counts change.
4. If a hunk has no selected changes, it's omitted entirely from the patch.
5. `GenerateInversePatch` does the reverse: selected adds become deletes and selected deletes
   become adds. This is used by the "discard changes" feature — the inverse patch is applied
   without `--cached` to modify the working tree directly.

> **Beginner note — `line.Text` vs `line.Content`**: `line.Text` is the raw diff line including
> its prefix character (e.g. `+foo`, `-bar`, ` baz`). `line.Content` is the text without the
> prefix (e.g. `foo`, `bar`, `baz`). When converting an unselected delete into a context line,
> we write `" " + line.Content` to replace the `-` prefix with a space.
>
> **Beginner note — `flatIdx *int` (pointer)**: `flatIdx` is a pointer, not a plain int, because
> `generateHunkPatch` is called once per hunk in a loop and the index must carry over between
> calls. Each line needs a globally unique index that matches the `DiffSelection` indices. A
> plain `int` would reset each call, causing wrong selection lookups.

### 8.4 Apply Patch to Index

**File**: `internal/git/staging.go` (add to existing file from 8.1)

Add these functions to the `staging.go` file created in section 8.1:

```go
// ApplyPatchToIndex stages a partial patch using git apply --cached.
// The patch is piped to stdin. Flags:
//   - --cached: apply to the index (staging area), not the working tree
//   - --unidiff-zero: allow hunks with zero context lines (needed for single-line patches)
//   - --whitespace=nowarn: suppress whitespace warnings that would cause failures
func ApplyPatchToIndex(repoPath, patch string) error {
	if patch == "" {
		return nil
	}

	cmd := exec.Command("git",
		"apply",
		"--cached",
		"--unidiff-zero",
		"--whitespace=nowarn",
		"-",
	)
	cmd.Dir = repoPath
	cmd.Stdin = strings.NewReader(patch)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git apply --cached: %s: %s", err, string(out))
	}
	return nil
}

// ApplyPatchToWorkingTree applies a patch to the working tree (for discarding changes).
// Same as ApplyPatchToIndex but without --cached.
func ApplyPatchToWorkingTree(repoPath, patch string) error {
	if patch == "" {
		return nil
	}

	cmd := exec.Command("git",
		"apply",
		"--unidiff-zero",
		"--whitespace=nowarn",
		"-",
	)
	cmd.Dir = repoPath
	cmd.Stdin = strings.NewReader(patch)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git apply: %s: %s", err, string(out))
	}
	return nil
}
```

**How `git apply` works for staging**:

- `git apply --cached` modifies only the index (staging area). The working tree is untouched.
  This is exactly what "stage these lines" means — move selected changes from working tree
  to the index.
- `--unidiff-zero` is required because partial patches may have zero context lines. Normally
  `git apply` requires at least one line of context to locate the patch position in the file.
  `--unidiff-zero` removes this requirement, allowing single-line patches to work.
- `--whitespace=nowarn` suppresses warnings about trailing whitespace, mixed indentation, etc.
  Without this, `git apply` may refuse to apply patches that have whitespace issues.
- The patch is piped via stdin (`-` argument tells `git apply` to read from stdin).
- `ApplyPatchToWorkingTree` is the same but without `--cached` — it modifies the actual files
  on disk. Used for "discard changes" where the user wants to revert selected lines.

### 8.5 Staging UI — File List Integration

**File**: `internal/tui/components/filelist.go` (modify existing from Phase 6)

Update the file list component to handle `space` and `a` keys. These were previously no-ops
reserved for Phase 8. Now they emit staging messages that the app handles.

**New message types** — add above the `FileListModel` struct:

```go
// FileStagingToggleMsg is sent when the user presses space on a file.
// The app handles the actual git staging/unstaging.
type FileStagingToggleMsg struct {
	File git.FileEntry
}

// StageAllMsg is sent when the user presses 'a' to stage/unstage all files.
// StageAll is true when files should be staged, false when they should be unstaged.
type StageAllMsg struct {
	StageAll bool   // true = stage all, false = unstage all
}
```

**Replace the `space` and `a` cases** in the `Update()` method:

```go
		case " ":
			// Toggle staging for the selected file
			if len(m.Files) > 0 && m.cursor < len(m.Files) {
				file := m.Files[m.cursor]
				return m, func() tea.Msg {
					return FileStagingToggleMsg{File: file}
				}
			}
			return m, nil

		case "a":
			// Stage all or unstage all — toggle based on current state.
			// If any files are staged, unstage all. If none are staged, stage all.
			if len(m.Files) > 0 {
				anyStaged := false
				for _, f := range m.Files {
					if f.Staged {
						anyStaged = true
						break
					}
				}
				return m, func() tea.Msg {
					return StageAllMsg{StageAll: !anyStaged}
				}
			}
			return m, nil
```

**How the toggle logic works**:

- `space` emits a `FileStagingToggleMsg` with the currently selected file. The app receives
  this and calls either `git.StageFile()` or `git.UnstageFile()` based on the file's `Staged`
  field, then triggers a status refresh so the `●`/`○` indicators update.
- `a` checks whether any files are currently staged. If at least one file is staged, pressing
  `a` unstages ALL files. If no files are staged, pressing `a` stages ALL files. `a` always flips the "bulk" state.

> **Beginner note — `func() tea.Msg` pattern**: In bubbletea, `Update()` returns
> `(model, tea.Cmd)` where `tea.Cmd` is `func() tea.Msg`. Returning an anonymous function like
> `func() tea.Msg { return FileStagingToggleMsg{...} }` tells bubbletea to run it async and
> feed the resulting message back into the parent app's `Update()`. This is how child components
> communicate with the parent without importing it directly.

### 8.6 Diff Viewer — Line Selection UI

**File**: `internal/tui/components/diffview.go` (modify existing from Phase 7)

The diff viewer needs new fields and key bindings for hunk/line-level staging. When the user
is viewing a diff, they can:
- Press `space` on a line to toggle its selection
- Press `h` on a hunk header to toggle the entire hunk
- Press `S` (capital) to stage the current selection (applies the partial patch)

The diff viewer now tracks a `cursor` position (the currently highlighted line) and a
`DiffSelection` that records which lines are selected.

**New fields** — add to `DiffViewModel`:

```go
type DiffViewModel struct {
	file       git.FileEntry
	fileDiff   *diff.FileDiff
	hasContent bool

	offset     int
	totalLines int
	width      int
	height     int
	sideBySide bool
	loading    bool
	errMsg     string

	// Line selection
	cursor    int               // highlighted line index (in flat AllLines array)
	selection diff.DiffSelection // tracks which lines are selected for staging
	allLines  []diff.Line       // cached flat line array for quick access
}
```

**Update `SetDiff()`** — initialize the selection and line cache:

```go
func (m *DiffViewModel) SetDiff(file git.FileEntry, fileDiff *diff.FileDiff) {
	m.file = file
	m.fileDiff = fileDiff
	m.hasContent = true
	m.loading = false
	m.errMsg = ""
	m.offset = 0
	m.cursor = 0
	if fileDiff != nil {
		m.totalLines = fileDiff.TotalLines()
		m.allLines = fileDiff.AllLines()
		// Default: all lines selected (staging an unstaged file includes everything)
		m.selection = diff.NewDiffSelection(fileDiff, diff.SelectAll)
	} else {
		m.totalLines = 0
		m.allLines = nil
		m.selection = diff.DiffSelection{}
	}
}
```

**New message types** — add to `diffview.go`:

```go
// StagePatchMsg is sent when the user presses S to stage the current line selection.
// The app receives this and applies the generated patch via git apply --cached.
type StagePatchMsg struct {
	File      git.FileEntry
	FileDiff  *diff.FileDiff
	Selection diff.DiffSelection
}

// HunkRange returns the flat line index range for the hunk containing the given line index.
// Returns (startIdx, count) for use with WithRangeSelection.
func hunkRange(fileDiff *diff.FileDiff, flatIdx int) (int, int) {
	if fileDiff == nil {
		return 0, 0
	}

	offset := 0
	for _, hunk := range fileDiff.Hunks {
		hunkLen := len(hunk.Lines)
		if flatIdx >= offset && flatIdx < offset+hunkLen {
			return offset, hunkLen
		}
		offset += hunkLen
	}
	return 0, 0
}
```

**Update `Update()`** — add the new key bindings. Replace the existing `Update()` method:

```go
func (m DiffViewModel) Update(msg tea.Msg) (DiffViewModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < m.totalLines-1 {
				m.cursor++
			}
			// Auto-scroll to keep cursor visible
			if m.cursor >= m.offset+m.height {
				m.offset = m.cursor - m.height + 1
			}
			m.clampOffset()

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
			}
			if m.cursor < m.offset {
				m.offset = m.cursor
			}
			m.clampOffset()

		case "g":
			m.cursor = 0
			m.offset = 0

		case "G":
			if m.totalLines > 0 {
				m.cursor = m.totalLines - 1
			}
			m.offset = m.totalLines - m.height
			m.clampOffset()

		case "s":
			m.sideBySide = !m.sideBySide

		case "d":
			m.offset += m.height / 2
			m.cursor += m.height / 2
			if m.cursor >= m.totalLines {
				m.cursor = m.totalLines - 1
			}
			m.clampOffset()

		case "u":
			m.offset -= m.height / 2
			m.cursor -= m.height / 2
			if m.cursor < 0 {
				m.cursor = 0
			}
			m.clampOffset()

		case " ":
			// Toggle selection on the current line
			if m.totalLines > 0 && m.selection.IsSelectable(m.cursor) {
				m.selection = m.selection.WithToggle(m.cursor)
			}

		case "h":
			// Toggle selection for the entire hunk containing the cursor
			if m.totalLines > 0 && m.fileDiff != nil {
				start, count := hunkRange(m.fileDiff, m.cursor)
				if count > 0 {
					// Check if majority of hunk is selected — if so, deselect all; otherwise select all
					selectedInHunk := 0
					selectableInHunk := 0
					for i := start; i < start+count; i++ {
						if m.selection.IsSelectable(i) {
							selectableInHunk++
							if m.selection.IsSelected(i) {
								selectedInHunk++
							}
						}
					}
					// If half or fewer lines are selected, select all; otherwise deselect all.
					// This gives a natural "toggle" feel for the hunk.
					selectAll := selectedInHunk <= selectableInHunk/2
					m.selection = m.selection.WithRangeSelection(start, count, selectAll)
				}
			}

		case "S":
			// Stage the current selection — emit a message for the app to handle
			if m.totalLines > 0 && m.fileDiff != nil && !m.selection.NoneSelected() {
				return m, func() tea.Msg {
					return StagePatchMsg{
						File:      m.file,
						FileDiff:  m.fileDiff,
						Selection: m.selection,
					}
				}
			}
		}
	}

	return m, nil
}
```

**Update `View()`** — add cursor highlighting and selection indicators:

```go
func (m DiffViewModel) View() string {
	if m.loading {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Loading diff...")
	}

	if m.errMsg != "" {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#F85149")).
			Render("Error: " + m.errMsg)
	}

	if !m.hasContent {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Select a file to view its diff")
	}

	if m.fileDiff == nil || m.totalLines == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("No changes in this file")
	}

	// Render only the visible window with cursor and selection indicators
	return render.RenderDiffWithSelection(
		m.fileDiff, m.allLines, m.selection,
		m.offset, m.height, m.width, m.cursor,
	)
}
```

### 8.7 Diff Renderer — Selection Indicators

**File**: `internal/tui/render/diff.go` (modify existing from Phase 7)

Add a new rendering function that includes selection checkboxes and cursor highlighting.
The existing `RenderDiff` is kept for cases where selection is not needed (e.g., History tab).

Add this function to `diff.go`:

```go
// SelectionColors holds styles for selection indicators in the diff viewer.
var SelectionColors = struct {
	Selected   lipgloss.Style
	Unselected lipgloss.Style
	CursorLine lipgloss.Style
}{
	Selected:   lipgloss.NewStyle().Foreground(lipgloss.Color("#2EA043")), // green checkmark
	Unselected: lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")), // dim circle
	CursorLine: lipgloss.NewStyle().Background(lipgloss.Color("#1F2937")), // subtle highlight
}

// RenderDiffWithSelection renders the visible diff with selection indicators and cursor.
// Each selectable line (Add/Delete) gets a checkbox: [●] selected, [○] unselected.
// The cursor line gets a subtle background highlight.
//
// Parameters:
//   - fileDiff: the parsed diff
//   - allLines: pre-flattened lines (cached for performance)
//   - selection: which lines are selected for staging
//   - offset: scroll position
//   - visibleRows: how many lines fit in the pane
//   - width: pane width
//   - cursor: the currently highlighted line index (flat)
func RenderDiffWithSelection(
	fileDiff *diff.FileDiff,
	allLines []diff.Line,
	selection diff.DiffSelection,
	offset, visibleRows, width, cursor int,
) string {
	if fileDiff == nil || len(allLines) == 0 {
		return ""
	}

	total := len(allLines)

	if offset < 0 {
		offset = 0
	}
	if offset >= total {
		offset = total - 1
	}

	end := offset + visibleRows
	if end > total {
		end = total
	}

	filePath := fileDiff.NewPath
	if filePath == "" || filePath == "/dev/null" {
		filePath = fileDiff.OldPath
	}
	lexer := getLexer(filePath)

	var rendered []string
	for i := offset; i < end; i++ {
		line := allLines[i]

		// Selection indicator for selectable lines
		var indicator string
		if selection.IsSelectable(i) {
			if selection.IsSelected(i) {
				indicator = SelectionColors.Selected.Render("●") + " "
			} else {
				indicator = SelectionColors.Unselected.Render("○") + " "
			}
		} else {
			indicator = "  " // non-selectable lines get blank space to align columns
		}

		// Render the diff line content (gutter + prefix + highlighted code)
		lineContent := RenderDiffLine(line, lexer, width-4) // -4 for indicator + space + margins

		// Compose: indicator + line content
		fullLine := indicator + lineContent

		// Apply cursor highlight
		if i == cursor {
			fullLine = SelectionColors.CursorLine.Render(fullLine)
		}

		rendered = append(rendered, fullLine)
	}

	return strings.Join(rendered, "\n")
}
```

**How the selection UI works**:

- Each selectable line (Add/Delete) gets a selection indicator: `●` (green, selected) or
  `○` (dim gray, unselected). Non-selectable lines (context, hunk headers) get blank space
  to keep columns aligned.
- The cursor line gets a subtle dark background highlight (`#1F2937`) so the user can see
  which line they're on.
- The `cursor` parameter is the flat line index — it's compared against `i` (the rendering
  index) to apply the highlight. The cursor is always within the visible window because the
  diff viewer auto-scrolls in `Update()`.

### 8.8 Wire Staging Into the App

**File**: `internal/tui/app/app.go` (modify existing)

This section connects the staging UI to the actual git operations. The app handles staging
messages by running git commands in async `tea.Cmd` functions, then refreshing the status.

**New message types** — add to `app.go`:

```go
// stagingResultMsg is returned by async staging commands.
type stagingResultMsg struct {
	err error
}
```

**New async commands** — add to `app.go`:

```go
// stageFileCmd stages or unstages a single file asynchronously.
// It checks file.Staged to decide the direction: staged files get unstaged, and vice versa.
func stageFileCmd(repoPath string, file git.FileEntry) tea.Cmd {
	return func() tea.Msg {
		var err error
		if file.Staged {
			err = git.UnstageFile(repoPath, file)
		} else {
			err = git.StageFile(repoPath, file)
		}
		return stagingResultMsg{err: err}
	}
}

// stageAllCmd stages or unstages all files asynchronously.
func stageAllCmd(repoPath string, files []git.FileEntry, stageAll bool) tea.Cmd {
	return func() tea.Msg {
		var err error
		if stageAll {
			err = git.StageFiles(repoPath, files)
		} else {
			err = git.UnstageFiles(repoPath, files)
		}
		return stagingResultMsg{err: err}
	}
}

// stagePatchCmd generates a partial patch from the selection and applies it.
func stagePatchCmd(repoPath string, file git.FileEntry, fileDiff *diff.FileDiff, selection diff.DiffSelection) tea.Cmd {
	return func() tea.Msg {
		patch := diff.GeneratePatch(fileDiff, selection)
		if patch == "" {
			return stagingResultMsg{err: nil}
		}
		err := git.ApplyPatchToIndex(repoPath, patch)
		return stagingResultMsg{err: err}
	}
}
```

**Changes to `Update()`** — add handlers for the new messages. Add these cases to the
`switch msg := msg.(type)` block:

```go
	case components.FileStagingToggleMsg:
		// Toggle staging for a single file
		return m, stageFileCmd(m.repoPath, msg.File)

	case components.StageAllMsg:
		// Stage or unstage all files
		return m, stageAllCmd(m.repoPath, m.fileList.Files, msg.StageAll)

	case components.StagePatchMsg:
		// Stage a partial patch (hunk/line selection)
		return m, stagePatchCmd(m.repoPath, msg.File, msg.FileDiff, msg.Selection)

	case stagingResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Staging Error",
				"Failed to update staging: "+msg.err.Error(),
				true,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Staging succeeded — re-run git status so the file list updates.
		// The Staged field on each FileEntry comes from git status output,
		// so we must refresh to reflect the change in the UI.
		return m, refreshStatusCmd(m.repoPath)
```

**New imports for `app.go`** — ensure the `diff` package is imported (should already be
present from Phase 7):

```go
import (
	// ... existing imports ...
	"github.com/LeoManrique/leogit/internal/diff"
)
```

**What changed from Phase 7**:

1. **New file `internal/git/staging.go`**: `StageFiles`, `UnstageFiles`, `StageFile`,
   `UnstageFile`, `ApplyPatchToIndex`, `ApplyPatchToWorkingTree` — all git staging operations
2. **New file `internal/diff/selection.go`**: `DiffSelection` with `DefaultState`,
   `DivergingLines`, `SelectableLines`, and methods `IsSelected`, `WithLineSelection`,
   `WithToggle`, `WithRangeSelection`, `AllSelected`, `NoneSelected`
3. **New file `internal/diff/patch.go`**: `GeneratePatch`, `GenerateInversePatch` — creates
   valid unified diff patches from partial selections
4. **`filelist.go` updated**: `space` emits `FileStagingToggleMsg`, `a` emits `StageAllMsg`
   (no longer no-ops)
5. **`diffview.go` updated**: new `cursor` field, `selection` field, `allLines` cache.
   `space` toggles line selection, `h` toggles hunk selection, `S` emits `StagePatchMsg`.
   `View()` calls `RenderDiffWithSelection` to show checkboxes and cursor
6. **`render/diff.go` updated**: new `RenderDiffWithSelection` function with selection
   indicators (`●`/`○`) and cursor highlighting
7. **`app.go` updated**: new `FileStagingToggleMsg`, `StageAllMsg`, `StagePatchMsg`,
   `stagingResultMsg` handlers. New async commands `stageFileCmd`, `stageAllCmd`,
   `stagePatchCmd`. Staging errors show in the error modal. Successful staging triggers
   a status refresh

### 8.9 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Stage a single file with space**:

Make changes to a file (e.g., edit `main.go`). In the Changed Files list (Pane 1), highlight
the file and press `space`. The file's indicator should change from `○` (gray, unstaged) to
`●` (green, staged) after the status refreshes (~2 seconds, or faster if the staging triggers
an immediate refresh).

Verify with git:

```bash
git status
# Should show the file in "Changes to be committed"
```

**Test 2 — Unstage a single file with space**:

With the file now staged (`●`), press `space` again. The indicator should change back to
`○` (unstaged).

```bash
git status
# Should show the file in "Changes not staged for commit"
```

**Test 3 — Stage all with 'a'**:

Create several changed files:

```bash
cd /path/to/repo
echo "change1" >> file1.txt
echo "change2" >> file2.txt
echo "new" > file3.txt
```

All files should show `○` (unstaged). Press `a` — all files should change to `●` (staged).

```bash
git status
# All files should be in "Changes to be committed"
```

**Test 4 — Unstage all with 'a'**:

With all files staged, press `a` again. All files should change back to `○` (unstaged).

**Test 5 — Line-level selection in diff viewer**:

1. Make a multi-line change to a file (add several lines)
2. Select the file with `Enter` to show its diff in Pane 2
3. Press `2` to focus the diff viewer
4. Navigate to an added line with `j`/`k` — you should see the cursor highlight
5. All selectable lines (Add/Delete) should show `●` (selected by default)
6. Press `space` on a line — it should toggle to `○` (unselected)
7. Press `space` again — it should toggle back to `●` (selected)

**Test 6 — Hunk-level selection**:

1. View a diff with multiple hunks (make changes in different parts of a file)
2. Navigate to a hunk header (`@@ ... @@` line, cyan)
3. Press `h` — all selectable lines in that hunk should toggle
4. If most lines were selected, they should all become unselected (and vice versa)

**Test 7 — Stage partial selection with S**:

1. View a diff with several added lines
2. Deselect some lines with `space` (so only some lines show `●`)
3. Press `S` (capital S) to stage the selection
4. The status should refresh — the file may show as both staged and unstaged if only some
   changes were staged

Verify with git:

```bash
git diff --cached -- <filename>
# Should show only the lines you selected
git diff -- <filename>
# Should show only the lines you deselected
```

**Test 8 — Stage untracked file**:

Create a brand-new file:

```bash
echo "hello" > brand_new.txt
```

Press `space` on it in the file list. The file should be staged (`●`).

```bash
git status
# "brand_new.txt" should be in "Changes to be committed: new file"
```

**Test 9 — Stage renamed file**:

```bash
git mv oldname.go newname.go
```

The file should show as `[R]` in the list. Press `space` to stage it. Both the old path
removal and new path addition should be staged correctly.

**Test 10 — Stage deleted file**:

```bash
rm somefile.go
```

The file shows as `[-]`. Press `space` to stage the deletion.

```bash
git status
# Should show "deleted: somefile.go" in "Changes to be committed"
```

**Test 11 — Error handling**:

If a staging operation fails (e.g., file was modified externally during staging), the error
modal should appear with the error message. Press `Esc` to dismiss.

**Test 12 — Staging indicator visual consistency**:

After staging/unstaging several files, switch to the History tab with `Tab` and back. The
staging indicators should persist correctly. Resize the terminal — indicators should still
render correctly.

**Phase 8 is complete when**: `space` toggles staging for individual files; `a` stages all
or unstages all files (toggle based on current state); the diff viewer shows a cursor and
selection indicators (`●`/`○`) on Add/Delete lines; `space` in the diff viewer toggles line
selection; `h` toggles hunk selection; `S` stages the partial selection via a generated patch
piped to `git apply --cached`; staging operations trigger a status refresh; and errors are
displayed in the error modal.

## Phase 9 — Commit Message

**Goal**: Build the commit message pane (Pane 3 in the Changes tab) with a summary field,
a description field, an AI-generation button, and the provider infrastructure to generate
commit messages from the staged diff using Claude CLI or Ollama.

This phase introduces:
1. **Commit message component** — a `CommitMsgModel` with a single-line summary input and
   a multi-line description textarea, rendered in the sidebar's bottom pane
2. **Provider interface** — a shared `CommitMessageProvider` interface in `internal/ai/`
3. **Claude CLI provider** — spawns `claude --print --output-format json` as a subprocess
4. **Ollama HTTP provider** — sends `POST /api/generate` to a local Ollama server
5. **Provider selection & loading state** — the user cycles providers with `ctrl+p`, triggers
   generation with `ctrl+g`, and sees a spinner while the AI is thinking

After this phase, pressing `3` focuses the commit message pane. The summary field accepts
a single line (≤72 chars). `Tab` moves focus between summary and description. `ctrl+g`
generates a commit message from the staged diff using the active AI provider. `ctrl+p`
cycles between Claude and Ollama. The generated title and description are inserted into
the fields. A spinner shows during generation. The actual commit execution is Phase 10.

### 9.1 Commit Message Text Input (Summary + Description)

**File**: `internal/tui/components/commitmsg.go` (new file)

This component uses Bubbles v2 `textinput` for the single-line summary and `textarea` for
the multi-line description. The component manages focus between the two fields and emits
messages for AI generation and committing.

The commit message pane is 8 rows tall (fixed by `layout.go`). Inner dimensions after borders
and title: ~6 rows of usable space. The summary takes 1 row, leaving ~4 rows for the
description area, and 1 row for the button bar.

Import paths for Bubbles v2:
- `charm.land/bubbles/v2/textinput`
- `charm.land/bubbles/v2/textarea`

```go
package components

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/bubbles/v2/textinput"
	"charm.land/bubbles/v2/textarea"
	"charm.land/lipgloss/v2"
)

// ── Messages ────────────────────────────────────────────

// CommitRequestMsg is sent when the user presses ctrl+enter to commit.
// The app handles the actual git commit.
type CommitRequestMsg struct {
	Summary     string
	Description string
}

// AIGenerateMsg is sent when the user presses ctrl+g to generate a commit message.
type AIGenerateMsg struct{}

// AICycleProviderMsg is sent when the user presses ctrl+p to cycle AI providers.
type AICycleProviderMsg struct{}

// AIResultMsg is sent when AI generation completes (success or error).
type AIResultMsg struct {
	Title       string
	Description string
	Err         error
}

// ── Focus field ─────────────────────────────────────────

type commitField int

const (
	fieldSummary     commitField = iota
	fieldDescription
)

// ── Model ───────────────────────────────────────────────

// CommitMsgModel is the commit message pane component (Pane 3 in Changes tab).
// It contains a summary text input and a description textarea.
type CommitMsgModel struct {
	summary     textinput.Model
	description textarea.Model
	activeField commitField

	// AI generation state
	aiLoading    bool   // true while AI is generating
	aiProvider   string // display name of active provider (e.g., "Claude", "Ollama")
	aiError      string // last AI error message (cleared on next attempt)

	width  int
	height int
	focused bool
}

// NewCommitMsg creates a new commit message component with default settings.
func NewCommitMsg() CommitMsgModel {
	// Summary: single-line input, max 72 chars
	ti := textinput.New()
	ti.Placeholder = "Summary (required)"
	ti.CharLimit = 72
	ti.SetWidth(30) // will be resized by SetSize

	// Description: multi-line textarea
	ta := textarea.New()
	ta.Placeholder = "Description"
	ta.SetWidth(30)
	ta.SetHeight(3) // compact — fits in the sidebar pane
	ta.ShowLineNumbers = false
	ta.CharLimit = 0 // no limit on description length

	return CommitMsgModel{
		summary:     ti,
		description: ta,
		activeField: fieldSummary,
		aiProvider:  "Claude",
	}
}

// SetSize updates the component dimensions.
// Called when the window resizes or the layout recalculates.
func (m *CommitMsgModel) SetSize(width, height int) {
	m.width = width
	m.height = height

	innerW := width - 2 // leave space for padding
	if innerW < 10 {
		innerW = 10
	}
	m.summary.SetWidth(innerW)

	// Description gets remaining height: total - summary(1) - button bar(1) - gaps(1)
	descHeight := height - 3
	if descHeight < 1 {
		descHeight = 1
	}
	m.description.SetWidth(innerW)
	m.description.SetHeight(descHeight)
}

// Focus activates the commit message pane for text input.
func (m *CommitMsgModel) Focus() {
	m.focused = true
	m.activeField = fieldSummary
	m.summary.Focus()
	m.description.Blur()
}

// Blur deactivates the commit message pane.
func (m *CommitMsgModel) Blur() {
	m.focused = false
	m.summary.Blur()
	m.description.Blur()
}

// Summary returns the current summary text.
func (m CommitMsgModel) Summary() string {
	return strings.TrimSpace(m.summary.Value())
}

// Description returns the current description text.
func (m CommitMsgModel) Description() string {
	return strings.TrimSpace(m.description.Value())
}

// IsEmpty returns true if both summary and description are empty.
func (m CommitMsgModel) IsEmpty() bool {
	return m.Summary() == "" && m.Description() == ""
}

// Clear resets both fields to empty (after a successful commit).
func (m *CommitMsgModel) Clear() {
	m.summary.SetValue("")
	m.description.SetValue("")
	m.activeField = fieldSummary
	m.aiError = ""
	if m.focused {
		m.summary.Focus()
		m.description.Blur()
	}
}

// SetAIResult fills the fields with the AI-generated commit message.
func (m *CommitMsgModel) SetAIResult(title, description string) {
	m.summary.SetValue(title)
	m.description.SetValue(description)
	m.aiLoading = false
	m.aiError = ""
}

// SetAIError records an AI generation error.
func (m *CommitMsgModel) SetAIError(errMsg string) {
	m.aiLoading = false
	m.aiError = errMsg
}

// SetAIProvider updates the displayed provider name.
func (m *CommitMsgModel) SetAIProvider(name string) {
	m.aiProvider = name
}

// ── Update ──────────────────────────────────────────────

// Update handles key messages for the commit message pane.
func (m CommitMsgModel) Update(msg tea.Msg) (CommitMsgModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		key := msg.String()

		switch key {
		case "tab":
			// Toggle between summary and description
			if m.activeField == fieldSummary {
				m.activeField = fieldDescription
				m.summary.Blur()
				m.description.Focus()
			} else {
				m.activeField = fieldSummary
				m.description.Blur()
				m.summary.Focus()
			}
			return m, nil

		case "ctrl+g":
			// Trigger AI commit message generation
			if !m.aiLoading {
				m.aiLoading = true
				m.aiError = ""
				return m, func() tea.Msg { return AIGenerateMsg{} }
			}
			return m, nil

		case "ctrl+p":
			// Cycle AI provider
			return m, func() tea.Msg { return AICycleProviderMsg{} }

		case "ctrl+enter":
			// Request commit (handled by app)
			summary := m.Summary()
			if summary != "" {
				return m, func() tea.Msg {
					return CommitRequestMsg{
						Summary:     summary,
						Description: m.Description(),
					}
				}
			}
			return m, nil
		}
	}

	// Forward to the active field — any key not caught by the switch above
	// (regular characters, backspace, arrows, etc.) falls through to here
	// and is forwarded to the active Bubbles text component for typing.
	var cmd tea.Cmd
	if m.activeField == fieldSummary {
		m.summary, cmd = m.summary.Update(msg)
	} else {
		m.description, cmd = m.description.Update(msg)
	}
	return m, cmd
}

// ── View ────────────────────────────────────────────────

// View renders the commit message pane.
func (m CommitMsgModel) View() string {
	if m.width < 5 || m.height < 2 {
		return ""
	}

	var sections []string

	// Summary field
	sections = append(sections, m.summary.View())

	// Description field
	sections = append(sections, m.description.View())

	// Button bar: [AI: Provider] [ctrl+g Generate] [ctrl+enter Commit]
	buttonBar := m.renderButtonBar()
	sections = append(sections, buttonBar)

	return strings.Join(sections, "\n")
}

// renderButtonBar renders the bottom bar with AI provider and action hints.
func (m CommitMsgModel) renderButtonBar() string {
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	activeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF"))
	errorStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149"))

	var parts []string

	// AI provider indicator
	providerLabel := dimStyle.Render("AI:") + " " + activeStyle.Render(m.aiProvider)
	parts = append(parts, providerLabel)

	// Loading or error state
	if m.aiLoading {
		parts = append(parts, activeStyle.Render("⟳ Generating..."))
	} else if m.aiError != "" {
		// Truncate error to fit
		errText := m.aiError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else {
		parts = append(parts, dimStyle.Render("^g:generate ^p:provider"))
	}

	return strings.Join(parts, " ")
}
```

**How the commit message component works**:

- **Two fields**: `textinput.Model` for the summary (single-line, 72 char limit) and
  `textarea.Model` for the description (multi-line, no char limit). `Tab` switches focus
  between them.
- **Focus management**: when the pane is focused (user pressed `3`), the summary field gets
  focus first. `Tab` toggles to the description. `Blur()` deactivates both fields when the
  user leaves the pane.
- **AI shortcuts**: `ctrl+g` emits `AIGenerateMsg` (the app catches this and runs the active
  provider). `ctrl+p` emits `AICycleProviderMsg` (the app cycles the provider list).
  `ctrl+enter` emits `CommitRequestMsg` (handled in Phase 10).
- **Loading state**: `aiLoading` is set to `true` when generation starts, and the button bar
  shows a spinner. When `AIResultMsg` arrives, `SetAIResult()` fills both fields and clears
  the loading state.
- **Dimensions**: `SetSize()` is called by the app when the layout recalculates. The summary
  is always 1 row. The description gets the remaining height minus the button bar.

### 9.2 AI Provider Interface & Shared Prompt

**File**: `internal/ai/provider.go` (new file)

This file defines the shared interface that both Claude and Ollama providers implement,
the `CommitMessage` struct, the shared prompt template, and error codes.

```go
package ai

import (
	"encoding/json"
	"fmt"
	"strings"
)

// CommitMessageProvider is the interface that AI commit message generators implement.
type CommitMessageProvider interface {
	// ID returns the provider identifier ("claude" or "ollama").
	ID() string

	// DisplayName returns the human-readable name (e.g., "Claude", "Ollama").
	DisplayName() string

	// IsAvailable checks whether the provider can be used (binary exists, server reachable).
	IsAvailable() (bool, error)

	// GenerateCommitMessage takes a staged diff and returns a commit message.
	// The diff is the full output of `git diff --no-ext-diff --patch-with-raw --no-color --staged`.
	GenerateCommitMessage(diff string) (*CommitMessage, error)
}

// CommitMessage holds a generated commit message with title and description.
type CommitMessage struct {
	Title       string `json:"title"`       // ≤50 chars, imperative mood
	Description string `json:"description"` // what changed and why
}

// Error codes for AI generation failures.
const (
	ErrEmptyDiff      = "EMPTY_DIFF"       // nothing staged
	ErrDiffTooLarge   = "DIFF_TOO_LARGE"   // exceeds provider's size limit
	ErrTimeout        = "TIMEOUT"          // provider didn't respond in time
	ErrCLIError       = "CLI_ERROR"        // claude CLI non-zero exit
	ErrConnectionError = "CONNECTION_ERROR" // can't reach Ollama server
	ErrModelNotFound  = "MODEL_NOT_FOUND"  // Ollama model not pulled
	ErrAPIError       = "API_ERROR"        // Ollama HTTP 500+
	ErrInvalidResponse = "INVALID_RESPONSE" // unparseable JSON
)

// AIError is a structured error with a code and message.
type AIError struct {
	Code    string
	Message string
}

func (e *AIError) Error() string {
	return fmt.Sprintf("[%s] %s", e.Code, e.Message)
}

// PromptTemplate is the shared prompt used by both providers.
// The placeholder {DIFF} is replaced with the actual staged diff.
const PromptTemplate = `You are a Git commit message generator. Analyze the provided git diff
and generate a commit message.

Return ONLY valid JSON in this exact format:
{"title": "≤50 char summary in imperative mood", "description": "what changed and why"}

Rules:
- Title MUST be ≤50 characters, imperative mood ("Add", "Fix", "Update")
- Description explains what and why, not how
- Return ONLY the JSON object

Git diff:
` + "```diff\n{DIFF}\n```"

// BuildPrompt creates the full prompt by inserting the diff into the template.
func BuildPrompt(diff string) string {
	return strings.Replace(PromptTemplate, "{DIFF}", diff, 1)
}

// ParseCommitMessage extracts a CommitMessage from a JSON string.
// Handles field normalization: accepts title/summary/subject/message for title,
// and description/body/details for description.
// Strips markdown code fences if present.
func ParseCommitMessage(raw string) (*CommitMessage, error) {
	// Strip markdown code fences (```json ... ```)
	cleaned := raw
	cleaned = strings.TrimSpace(cleaned)
	if strings.HasPrefix(cleaned, "```") {
		lines := strings.Split(cleaned, "\n")
		// Remove first line (```json) and last line (```)
		// A fenced block needs at least 3 lines: opening fence, one or more
		// content lines, and closing fence. Fewer than 3 means the fences
		// are incomplete, so we leave the string as-is.
		if len(lines) >= 3 {
			cleaned = strings.Join(lines[1:len(lines)-1], "\n")
		}
	}
	cleaned = strings.TrimSpace(cleaned)

	// Parse into a generic map for field normalization
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(cleaned), &parsed); err != nil {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: fmt.Sprintf("failed to parse JSON: %s", err),
		}
	}

	// Normalize title field: accept title, summary, subject, message
	title := ""
	for _, key := range []string{"title", "summary", "subject", "message"} {
		if v, ok := parsed[key]; ok {
			if s, ok := v.(string); ok && s != "" {
				title = s
				break
			}
		}
	}

	// Normalize description field: accept description, body, details
	description := ""
	for _, key := range []string{"description", "body", "details"} {
		if v, ok := parsed[key]; ok {
			if s, ok := v.(string); ok && s != "" {
				description = s
				break
			}
		}
	}

	if title == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "no title/summary field found in response",
		}
	}

	// Truncate title to 50 chars if the AI didn't follow instructions
	if len(title) > 50 {
		title = title[:50]
	}

	return &CommitMessage{
		Title:       title,
		Description: description,
	}, nil
}
```

**How the provider infrastructure works**:

- `CommitMessageProvider` is the interface both providers implement. `IsAvailable()` lets
  the app check at startup whether a provider can be used (is `claude` installed? Is the
  Ollama server running?).
- `BuildPrompt()` inserts the staged diff into the shared prompt template. Both providers
  use the same prompt — only the transport differs.
- `ParseCommitMessage()` handles the messy reality of LLM output: strips markdown fences,
  normalizes field names (the AI might return `summary` instead of `title`), truncates
  overlong titles. This function is shared by both providers.
- `AIError` carries a structured error code so the UI can show appropriate messages (e.g.,
  "Nothing staged" for `EMPTY_DIFF`, "Model not found — run `ollama pull`" for
  `MODEL_NOT_FOUND`).

### 9.3 AI Commit Message — Claude CLI Provider

**File**: `internal/ai/claude.go` (new file)

The Claude CLI provider spawns `claude --print --output-format json --model <model>` as a
subprocess, pipes the prompt to stdin, and parses the JSON response. The Claude CLI response
format when using `--output-format json` is:

```json
{"type":"result","result":"```json\n{\"title\": \"...\", \"description\": \"...\"}\n```"}
```

The actual commit message JSON is nested inside the `.result` field, often wrapped in
markdown code fences.

Binary lookup order (Unix): `~/.local/share/pnpm/claude`, `~/.local/bin/claude`,
`/usr/local/bin/claude`, `/usr/bin/claude`, then `$SHELL -l -c 'which claude'` as a
fallback. The first path that exists is used.

```go
package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// ClaudeProvider generates commit messages using the Claude CLI tool.
type ClaudeProvider struct {
	Model       string        // claude model name (default: "haiku")
	Timeout     time.Duration // subprocess timeout (default: 120s)
	MaxDiffSize int           // max diff size in bytes (default: 20MB)
	binaryPath  string        // cached path to the claude binary
}

// NewClaudeProvider creates a Claude provider with settings from the config.
func NewClaudeProvider(model string, timeoutSecs int, maxDiffSize int) *ClaudeProvider {
	if model == "" {
		model = "haiku"
	}
	if timeoutSecs <= 0 {
		timeoutSecs = 120
	}
	if maxDiffSize <= 0 {
		maxDiffSize = 20_971_520 // 20MB
	}

	return &ClaudeProvider{
		Model:       model,
		Timeout:     time.Duration(timeoutSecs) * time.Second,
		MaxDiffSize: maxDiffSize,
	}
}

func (p *ClaudeProvider) ID() string          { return "claude" }
func (p *ClaudeProvider) DisplayName() string { return "Claude" }

// IsAvailable checks whether the Claude CLI binary exists on the system.
func (p *ClaudeProvider) IsAvailable() (bool, error) {
	path, err := p.findBinary()
	if err != nil {
		return false, nil
	}
	p.binaryPath = path
	return true, nil
}

// GenerateCommitMessage runs the Claude CLI with the staged diff and returns
// a parsed commit message.
func (p *ClaudeProvider) GenerateCommitMessage(diff string) (*CommitMessage, error) {
	if strings.TrimSpace(diff) == "" {
		return nil, &AIError{Code: ErrEmptyDiff, Message: "nothing staged"}
	}

	if len(diff) > p.MaxDiffSize {
		return nil, &AIError{
			Code:    ErrDiffTooLarge,
			Message: fmt.Sprintf("diff is %d bytes (max %d)", len(diff), p.MaxDiffSize),
		}
	}

	// Ensure we have the binary path
	if p.binaryPath == "" {
		path, err := p.findBinary()
		if err != nil {
			return nil, &AIError{Code: ErrCLIError, Message: "claude CLI not found"}
		}
		p.binaryPath = path
	}

	prompt := BuildPrompt(diff)

	// Run claude CLI with timeout
	ctx, cancel := context.WithTimeout(context.Background(), p.Timeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, p.binaryPath,
		"--print",
		"--output-format", "json",
		"--model", p.Model,
	)
	cmd.Stdin = strings.NewReader(prompt)

	out, err := cmd.Output()
	if ctx.Err() == context.DeadlineExceeded {
		return nil, &AIError{Code: ErrTimeout, Message: "Claude CLI timed out"}
	}
	if err != nil {
		return nil, &AIError{
			Code:    ErrCLIError,
			Message: fmt.Sprintf("claude CLI error: %s", err),
		}
	}

	// Parse the Claude CLI JSON response
	// Format: {"type":"result","result":"<content>"}
	var cliResponse struct {
		Type   string `json:"type"`
		Result string `json:"result"`
	}
	if err := json.Unmarshal(out, &cliResponse); err != nil {
		// Try parsing the raw output directly as a commit message
		return ParseCommitMessage(string(out))
	}

	if cliResponse.Result == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "empty result from Claude CLI",
		}
	}

	return ParseCommitMessage(cliResponse.Result)
}

// findBinary searches for the claude binary in known locations.
// Returns the first path that exists as an executable.
func (p *ClaudeProvider) findBinary() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		home = ""
	}

	// Check well-known install locations first
	candidates := []string{}
	if home != "" {
		candidates = append(candidates,
			filepath.Join(home, ".local", "share", "pnpm", "claude"),
			filepath.Join(home, ".local", "bin", "claude"),
		)
	}
	candidates = append(candidates,
		"/usr/local/bin/claude",
		"/usr/bin/claude",
	)

	for _, path := range candidates {
		if info, err := os.Stat(path); err == nil && !info.IsDir() {
			return path, nil
		}
	}

	// Fallback: ask the user's login shell.
	// Using $SHELL with -l (login) flag loads the user's full PATH from
	// their shell profile (~/.zshrc, ~/.bashrc, etc.), which catches
	// binaries installed in non-standard locations like nvm/homebrew paths
	// that aren't in the default system PATH.
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}
	out, err := exec.Command(shell, "-l", "-c", "which claude").Output()
	if err == nil {
		path := strings.TrimSpace(string(out))
		if path != "" {
			return path, nil
		}
	}

	return "", fmt.Errorf("claude binary not found")
}
```

**How the Claude CLI provider works**:

1. `findBinary()` searches well-known install locations for the `claude` CLI. It checks
   `~/.local/share/pnpm/claude` first (pnpm global installs), then `~/.local/bin/`,
   `/usr/local/bin/`, `/usr/bin/`, and finally falls back to `$SHELL -l -c 'which claude'`
   to handle non-standard installs.
2. `IsAvailable()` calls `findBinary()` and caches the result. If the binary isn't found,
   it returns `false` (not an error — the provider is simply unavailable).
3. `GenerateCommitMessage()` validates the diff (non-empty, within size limit), builds the
   prompt using the shared template, and spawns the `claude` subprocess with a timeout context.
4. The subprocess is called with `--print` (non-interactive), `--output-format json` (structured
   output), and `--model <model>` (from config). The prompt is piped to stdin.
5. The response is a JSON object with `{"type":"result","result":"<content>"}`. The inner
   `result` field contains the AI's response — typically a JSON object wrapped in markdown
   fences. `ParseCommitMessage()` handles the fence stripping and field normalization.
6. If the Claude CLI isn't installed, `IsAvailable()` returns false and the app can fall back
   to Ollama or skip AI generation entirely.

### 9.4 AI Commit Message — Ollama Provider

**File**: `internal/ai/ollama.go` (new file)

The Ollama provider sends an HTTP POST to `{server_url}/api/generate` with the model name
and prompt. Ollama runs locally — no API key needed.

Availability is checked by sending `GET {server_url}/api/tags` with a 5-second timeout.
If the server responds, it's available. If the model hasn't been pulled, a specific error
code (`MODEL_NOT_FOUND`) is returned so the UI can tell the user to run `ollama pull`.

```go
package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// OllamaProvider generates commit messages using a local Ollama server.
type OllamaProvider struct {
	Model       string        // model name (default: "tavernari/git-commit-message:latest")
	ServerURL   string        // Ollama server URL (default: "http://localhost:11434")
	Timeout     time.Duration // request timeout (default: 120s)
	MaxDiffSize int           // max diff size in bytes (default: 50MB)
}

// NewOllamaProvider creates an Ollama provider with settings from the config.
func NewOllamaProvider(model, serverURL string, timeoutSecs int, maxDiffSize int) *OllamaProvider {
	if model == "" {
		model = "tavernari/git-commit-message:latest"
	}
	if serverURL == "" {
		serverURL = "http://localhost:11434"
	}
	if timeoutSecs <= 0 {
		timeoutSecs = 120
	}
	if maxDiffSize <= 0 {
		maxDiffSize = 52_428_800 // 50MB
	}

	return &OllamaProvider{
		Model:       model,
		ServerURL:   strings.TrimRight(serverURL, "/"),
		Timeout:     time.Duration(timeoutSecs) * time.Second,
		MaxDiffSize: maxDiffSize,
	}
}

func (p *OllamaProvider) ID() string          { return "ollama" }
func (p *OllamaProvider) DisplayName() string { return "Ollama" }

// IsAvailable checks whether the Ollama server is reachable by hitting GET /api/tags.
func (p *OllamaProvider) IsAvailable() (bool, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "GET", p.ServerURL+"/api/tags", nil)
	if err != nil {
		return false, nil
	}

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return false, nil // server not reachable — not an error, just unavailable
	}
	defer resp.Body.Close()

	return resp.StatusCode == http.StatusOK, nil
}

// GenerateCommitMessage sends the staged diff to Ollama and returns a parsed commit message.
func (p *OllamaProvider) GenerateCommitMessage(diff string) (*CommitMessage, error) {
	if strings.TrimSpace(diff) == "" {
		return nil, &AIError{Code: ErrEmptyDiff, Message: "nothing staged"}
	}

	if len(diff) > p.MaxDiffSize {
		return nil, &AIError{
			Code:    ErrDiffTooLarge,
			Message: fmt.Sprintf("diff is %d bytes (max %d)", len(diff), p.MaxDiffSize),
		}
	}

	prompt := BuildPrompt(diff)

	// Build the request body
	reqBody := ollamaRequest{
		Model:  p.Model,
		Prompt: prompt,
		Stream: false,
		Format: "json",
	}

	bodyBytes, err := json.Marshal(reqBody)
	if err != nil {
		return nil, fmt.Errorf("marshaling request: %w", err)
	}

	// Send POST /api/generate with timeout
	ctx, cancel := context.WithTimeout(context.Background(), p.Timeout)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "POST",
		p.ServerURL+"/api/generate",
		bytes.NewReader(bodyBytes),
	)
	if err != nil {
		return nil, &AIError{Code: ErrConnectionError, Message: err.Error()}
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return nil, &AIError{Code: ErrTimeout, Message: "Ollama request timed out"}
		}
		return nil, &AIError{Code: ErrConnectionError, Message: err.Error()}
	}
	defer resp.Body.Close()

	// Read response body
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, &AIError{Code: ErrAPIError, Message: "reading response: " + err.Error()}
	}

	// Check for HTTP errors
	if resp.StatusCode != http.StatusOK {
		// Check for model not found (404)
		if resp.StatusCode == http.StatusNotFound {
			return nil, &AIError{
				Code:    ErrModelNotFound,
				Message: fmt.Sprintf("model %q not found — run: ollama pull %s", p.Model, p.Model),
			}
		}
		return nil, &AIError{
			Code:    ErrAPIError,
			Message: fmt.Sprintf("HTTP %d: %s", resp.StatusCode, string(respBody)),
		}
	}

	// Parse the Ollama response: {"response": "<content>", ...}
	var ollamaResp ollamaResponse
	if err := json.Unmarshal(respBody, &ollamaResp); err != nil {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: fmt.Sprintf("parsing Ollama response: %s", err),
		}
	}

	if ollamaResp.Response == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "empty response from Ollama",
		}
	}

	return ParseCommitMessage(ollamaResp.Response)
}

// ── Request/Response types ──────────────────────────────

type ollamaRequest struct {
	Model  string `json:"model"`
	Prompt string `json:"prompt"`
	Stream bool   `json:"stream"`
	Format string `json:"format"`
}

type ollamaResponse struct {
	Response string `json:"response"`
}
```

**How the Ollama HTTP provider works**:

1. `IsAvailable()` sends `GET /api/tags` to the Ollama server with a 5-second timeout.
   If the server responds with 200 OK, it's available. Connection failures return `false`
   without an error — the provider is simply not running.
2. `GenerateCommitMessage()` validates the diff, builds the prompt using the shared template,
   and constructs the JSON request body with `model`, `prompt`, `stream: false` (we want
   the full response at once, not a token stream), and `format: "json"` (tells Ollama to
   constrain output to valid JSON).
3. The request is sent as `POST /api/generate` with `Content-Type: application/json`.
4. The response has the format `{"response": "<content>"}`. The `response` field contains
   the AI's output — a JSON string with the commit message fields.
5. `ParseCommitMessage()` handles parsing and normalization (same as Claude).
6. HTTP 404 is specifically caught and mapped to `MODEL_NOT_FOUND` with a helpful message
   telling the user to run `ollama pull <model>`.

### 9.5 Get Staged Diff (for AI Input)

**File**: `internal/git/diff.go` (add to existing file from Phase 7)

Add a function to get the full staged diff — this is what gets sent to the AI providers.
It runs `git diff --no-ext-diff --patch-with-raw --no-color --staged` (no path filter —
all staged files).

```go
// GetStagedDiff returns the full staged diff for the repository.
// This is the combined diff of all files in the index vs HEAD.
// Used as input for AI commit message generation.
func GetStagedDiff(repoPath string) (string, error) {
	cmd := exec.Command("git",
		"diff",
		"--no-ext-diff",
		"--patch-with-raw",
		"--no-color",
		"--staged",
	)
	cmd.Dir = repoPath
	// cmd.Environ() copies all environment variables from the current process,
	// so the child inherits PATH, HOME, etc. We then append TERM=dumb on top
	// to prevent git from emitting ANSI escape codes in its output.
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git diff --staged: %w", err)
	}
	return string(out), nil
}
```

**Why a separate function**: `GetDiff()` from Phase 7 diffs a single file. `GetStagedDiff()`
diffs ALL staged files at once — the AI needs the complete picture of what's being committed,
not one file at a time.

### 9.6 Provider Selection & Loading State — App Integration

**File**: `internal/tui/app/app.go` (modify existing)

This section wires the commit message component and AI providers into the app. The app
manages a list of providers, tracks the active one, and handles the async generation flow.

**New fields** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Commit message
	commitMsg    components.CommitMsgModel
	aiProviders  []ai.CommitMessageProvider // available providers
	aiActiveIdx  int                        // index into aiProviders
}
```

**Update `New()`** — initialize the commit message component and AI providers:

```go
func New(cfg *config.Config, repoPath string) Model {
	// Build the AI provider list from config
	claudeProvider := ai.NewClaudeProvider(
		cfg.AI.Claude.Model,
		cfg.AI.Claude.Timeout,
		cfg.AI.Claude.MaxDiffSize,
	)
	ollamaProvider := ai.NewOllamaProvider(
		cfg.AI.Ollama.Model,
		cfg.AI.Ollama.ServerURL,
		cfg.AI.Ollama.Timeout,
		cfg.AI.Ollama.MaxDiffSize,
	)

	providers := []ai.CommitMessageProvider{claudeProvider, ollamaProvider}

	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
		fileList:     components.NewFileList(),
		commitMsg:    components.NewCommitMsg(),
		aiProviders:  providers,
		aiActiveIdx:  0,
	}
}
```

**New async commands** — add to `app.go`:

```go
// generateCommitMsgCmd runs the active AI provider asynchronously.
func generateCommitMsgCmd(repoPath string, provider ai.CommitMessageProvider) tea.Cmd {
	return func() tea.Msg {
		// Get the full staged diff
		diff, err := git.GetStagedDiff(repoPath)
		if err != nil {
			return components.AIResultMsg{Err: err}
		}

		if strings.TrimSpace(diff) == "" {
			return components.AIResultMsg{
				Err: &ai.AIError{Code: ai.ErrEmptyDiff, Message: "nothing staged — stage files first"},
			}
		}

		// Generate the commit message
		msg, err := provider.GenerateCommitMessage(diff)
		if err != nil {
			return components.AIResultMsg{Err: err}
		}

		return components.AIResultMsg{
			Title:       msg.Title,
			Description: msg.Description,
		}
	}
}
```

**Note on `cliPath` vs `repoPath`**: In `New()`, the parameter `repoPath` is stored in the
`cliPath` field — this is the original path passed on the command line (may be empty). The
`repoPath` field on the `Model` struct is set later, once a repo is resolved (either from
`cliPath` or the repo picker in Phase 5). `generateCommitMsgCmd` below uses `m.repoPath`
(the resolved path), which is the correct field — it must point to a validated git repository.

**Changes to `Update()`** — add handlers for the new messages. Add these cases to the
`switch msg := msg.(type)` block:

```go
	case components.AIGenerateMsg:
		// User pressed ctrl+g — run the active AI provider
		if len(m.aiProviders) > 0 {
			provider := m.aiProviders[m.aiActiveIdx]
			return m, generateCommitMsgCmd(m.repoPath, provider)
		}
		return m, nil

	case components.AICycleProviderMsg:
		// User pressed ctrl+p — cycle to the next AI provider
		if len(m.aiProviders) > 0 {
			m.aiActiveIdx = (m.aiActiveIdx + 1) % len(m.aiProviders)
			provider := m.aiProviders[m.aiActiveIdx]
			m.commitMsg.SetAIProvider(provider.DisplayName())
		}
		return m, nil

	case components.AIResultMsg:
		// AI generation completed
		if msg.Err != nil {
			m.commitMsg.SetAIError(msg.Err.Error())
			return m, nil
		}
		m.commitMsg.SetAIResult(msg.Title, msg.Description)
		return m, nil

	case components.CommitRequestMsg:
		// User pressed ctrl+enter — commit
		// For now, just acknowledge the request
		return m, nil
```

**Update `handlePaneKey()`** — wire Pane 3 to the commit message component:

```go
	case core.Pane3:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 3 = Commit Message
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}
		// History tab → Diff Viewer
		return m, nil
```

**Update the focused mode handler** in `handleMainKey()` — when the user enters Pane 3
in focused mode, forward keys to the commit message component:

Replace the focused mode section:

```go
	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			m.commitMsg.Blur()
			return m, nil
		}
		// Forward to active pane component
		if m.activePane == core.Pane3 && m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}
		// TODO: forward to other panes (terminal)
		return m, nil
	}
```

**Update the `3` key handler** in navigable mode — entering Pane 3 should focus the commit
message fields and enter focused mode (since text input requires capturing all keys):

```go
	case "3":
		m.activePane = core.Pane3
		if m.activeTab == core.ChangesTab {
			m.focusMode = core.Focused
			m.commitMsg.Focus()
		}
		return m, nil
```

**Update `updateFileListSize()`** — also resize the commit message component:

```go
func (m *Model) updateFileListSize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	m.fileList.SetSize(dim.SidebarWidth-2, dim.FileListHeight-3)
	m.commitMsg.SetSize(dim.SidebarWidth-2, dim.CommitMsgHeight-3) // -3 for border + title
}
```

**Update `viewMain()`** — replace the placeholder content for Pane 3:

```go
	pane3 := renderPaneWithContent(
		core.PaneName(core.Pane3, m.activeTab),
		m.commitMsg.View(),
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
```

Add a new helper `renderPaneWithContent` that renders real content instead of a placeholder.
If this function doesn't already exist from Phase 6/7 (which used it for file list and diff),
add it:

```go
// renderPaneWithContent draws a bordered box with a title and actual content.
// This is the same as renderPane but with real content instead of a placeholder.
func renderPaneWithContent(title, content string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}
```

**New imports for `app.go`** — add the `ai` and `strings` packages:

```go
import (
	// ... existing imports ...
	"strings"

	"github.com/LeoManrique/leogit/internal/ai"
)
```

**What changed from Phase 8**:

1. **New file `internal/tui/components/commitmsg.go`**: `CommitMsgModel` with `textinput`
   summary, `textarea` description, focus management, AI loading state, button bar
2. **New file `internal/ai/provider.go`**: `CommitMessageProvider` interface, `CommitMessage`
   struct, `BuildPrompt()`, `ParseCommitMessage()`, error codes (`AIError` with structured
   codes)
3. **New file `internal/ai/claude.go`**: `ClaudeProvider` — spawns `claude --print
   --output-format json --model <model>`, parses nested JSON response, binary lookup
4. **New file `internal/ai/ollama.go`**: `OllamaProvider` — HTTP POST to `/api/generate`,
   availability check via `/api/tags`, model-not-found detection
5. **`internal/git/diff.go` updated**: new `GetStagedDiff()` function — full staged diff
   without path filter (all files), used as AI input
6. **`app.go` updated**: new `commitMsg` field, `aiProviders` list, `aiActiveIdx`. `New()`
   creates both providers from config. New handlers: `AIGenerateMsg` → `generateCommitMsgCmd`,
   `AICycleProviderMsg` → cycle provider, `AIResultMsg` → fill fields or show error,
   `CommitRequestMsg` → no-op (Phase 10). Pane 3 forwarding in `handlePaneKey()`. Focused
   mode forwards to commit message. `3` key enters focused mode and calls `Focus()`. Commit
   message component resized in `updateFileListSize()`

### 9.7 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Pane 3 focus and text input**:

Press `3` to focus the commit message pane. The border should turn blue and the summary
field should show a cursor. Type a commit message — characters should appear in the summary
field. The field should accept up to 72 characters and reject further input.

**Test 2 — Tab between fields**:

With Pane 3 focused, press `Tab`. The cursor should move from the summary to the description.
Type some text — it should appear in the description area. Press `Tab` again to return to
the summary.

**Test 3 — Escape from focused mode**:

Press `Esc`. The pane border should return to gray (unfocused). Keys like `j`, `k`, `1`, `2`
should now work for navigation again instead of being captured by the text input.

**Test 4 — AI provider display**:

When Pane 3 is focused, the button bar should show `AI: Claude` (the default provider).
Press `ctrl+p` — it should cycle to `AI: Ollama`. Press `ctrl+p` again — back to
`AI: Claude`.

**Test 5 — AI generation with Claude**:

Prerequisites: Claude CLI installed (`claude --version` works), some files staged.

1. Stage some changes: `git add -A`
2. Press `3` to focus the commit message pane
3. Press `ctrl+g` to trigger AI generation
4. The button bar should show `⟳ Generating...`
5. After a few seconds, the summary and description fields should be filled with the
   AI-generated commit message
6. The summary should be ≤50 characters in imperative mood

**Test 6 — AI generation with Ollama**:

Prerequisites: Ollama running (`ollama serve`), model pulled (`ollama pull
tavernari/git-commit-message:latest`), some files staged.

1. Press `ctrl+p` to switch to Ollama
2. Press `ctrl+g` to generate
3. Same flow as Test 5 — fields should fill after generation completes

**Test 7 — AI error handling — no staged files**:

1. Make sure nothing is staged (`git reset HEAD`)
2. Press `ctrl+g`
3. The button bar should show an error: "nothing staged — stage files first"

**Test 8 — AI error handling — provider unavailable**:

1. Switch to Ollama with `ctrl+p`
2. Stop the Ollama server if it's running
3. Press `ctrl+g`
4. The button bar should show a connection error

**Test 9 — AI error handling — model not found**:

1. Change the Ollama model in config to a nonexistent model name
2. Press `ctrl+g`
3. The error should say the model wasn't found and suggest running `ollama pull`

**Test 10 — Commit request (Phase 10 no-op)**:

With text in the summary field, press `ctrl+enter`. Nothing should happen yet (the commit
execution is Phase 10), but no crash should occur. The `CommitRequestMsg` is silently handled.

**Test 11 — Resize handling**:

Resize the terminal while Pane 3 is focused. The summary and description fields should
reflow to the new width. The button bar should stay at the bottom of the pane.

**Test 12 — Clear after AI generation**:

After AI fills the fields, you should be able to manually edit both the summary and
description. The AI-generated text is not locked — it's just a starting point.

**Phase 9 is complete when**: pressing `3` focuses the commit message pane with a summary
text input and description textarea; `Tab` switches between summary and description; `Esc`
exits focused mode; `ctrl+p` cycles between Claude and Ollama providers; `ctrl+g` generates
a commit message from the staged diff using the active provider; the button bar shows the
active provider, loading spinner during generation, and error messages on failure; generated
messages fill both fields; and `ctrl+enter` emits a commit request (handled in Phase 10).

## Phase 10 — Commit

**Goal**: Execute `git commit` when the user presses `ctrl+enter` in the commit message pane,
with proper validation (non-empty summary, staged files present), message formatting with
optional co-author trailers, and a full post-commit refresh that clears the fields, reloads
git status, and resets the diff viewer.

This phase introduces:
1. **Commit execution** — a `Commit()` function in `internal/git/` that runs `git commit -F -`
   with the commit message piped to stdin
2. **Staged files check** — a `HasStagedChanges()` function that runs `git diff --cached --quiet`
   to verify something is actually staged before committing
3. **Message formatting** — builds the full commit message string from summary + description +
   optional co-author trailers
4. **App integration** — handles `CommitRequestMsg` with validation, async commit execution,
   success/error feedback, field clearing, and status refresh

After this phase, pressing `ctrl+enter` with a non-empty summary and staged files creates a
real git commit. If validation fails (no summary or nothing staged), the commit message pane
shows an error in the button bar. On success, the summary and description fields are cleared,
the git status refreshes (showing the committed files are gone from the changes list), and the
diff viewer resets.

### 10.1 Commit Execution (`git commit`)

**File**: `internal/git/commit.go` (new file)

This file runs `git commit -F -` — the `-F -` flag tells git to read the commit message from
stdin instead of opening an editor. The full commit message is piped in as a single string.

The commit message format follows git conventions:
- Line 1: summary (the title line, ≤72 chars)
- Line 2: blank line (separates summary from body)
- Line 3+: description body (if non-empty)
- Blank line before trailers (if any)
- Trailers: `Co-authored-by: Name <email>` lines

Environment: `TERM=dumb` is always set to prevent any pager or color output.

```go
package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// Commit creates a git commit with the given message.
// The message is piped to `git commit -F -` via stdin.
// The message should already be fully formatted (summary + description + trailers).
func Commit(repoPath string, message string) error {
	cmd := exec.Command("git", "commit", "-F", "-")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")
	cmd.Stdin = strings.NewReader(message)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git commit: %s (%w)", strings.TrimSpace(string(out)), err)
	}
	return nil
}

// HasStagedChanges checks whether the index has any staged changes.
// Returns true if there are staged changes, false if the index matches HEAD.
// Uses `git diff --cached --quiet` — exit code 1 means changes exist.
func HasStagedChanges(repoPath string) (bool, error) {
	cmd := exec.Command("git", "diff", "--cached", "--quiet")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	err := cmd.Run()
	if err != nil {
		// Exit code 1 means there ARE staged changes (diff found).
		// Type assertion: err.(*exec.ExitError) checks if the error is specifically
		// an ExitError (process exited with non-zero code). The "ok" bool tells us
		// if the assertion succeeded. This is needed because cmd.Run() returns a
		// generic error interface — we need the concrete ExitError to read ExitCode().
		if exitErr, ok := err.(*exec.ExitError); ok {
			if exitErr.ExitCode() == 1 {
				return true, nil
			}
		}
		return false, fmt.Errorf("git diff --cached --quiet: %w", err)
	}
	// Exit code 0 means no staged changes (index matches HEAD)
	return false, nil
}

// FormatCommitMessage builds a full commit message string from its parts.
// The result follows git commit message conventions:
//   - Line 1: summary
//   - Line 2: blank (if description or trailers follow)
//   - Line 3+: description body
//   - Blank line before trailers
//   - Co-authored-by trailers (one per line)
func FormatCommitMessage(summary, description string, coAuthors []string) string {
	var parts []string

	parts = append(parts, summary)

	if description != "" || len(coAuthors) > 0 {
		parts = append(parts, "") // blank line after summary
	}

	if description != "" {
		parts = append(parts, description)
	}

	if len(coAuthors) > 0 {
		if description != "" {
			// Extra blank line to separate description body from trailers.
			// When there's no description, the blank line after the summary
			// (added above) already provides the required separation.
			parts = append(parts, "") // blank line before trailers
		}
		for _, author := range coAuthors {
			parts = append(parts, fmt.Sprintf("Co-authored-by: %s", author))
		}
	}

	return strings.Join(parts, "\n")
}
```

**How `git commit -F -` works**:

1. The `-F` flag tells git to read the commit message from a file. `-` means stdin.
2. We pipe the fully formatted message string to stdin via `cmd.Stdin = strings.NewReader(message)`.
3. `CombinedOutput()` captures both stdout and stderr — if the commit fails, the error
   message from git (e.g., "nothing to commit") is included in the returned error.
4. No `--cleanup` flag is used, so git applies the default `strip` cleanup (removes trailing
   whitespace, leading/trailing blank lines, and comment lines starting with `#`).

**How `HasStagedChanges()` works**:

1. `git diff --cached --quiet` compares the index against HEAD silently.
2. Exit code 0 = no differences (nothing staged).
3. Exit code 1 = differences found (something is staged).
4. Any other exit code = actual error (repo not found, corrupt index, etc.).

**How `FormatCommitMessage()` works**:

1. The summary always goes on line 1.
2. If there's a description or co-authors, a blank line separates the summary from the body.
3. The description goes after the blank line (if non-empty).
4. Co-author trailers go at the end, preceded by a blank line if there was a description.
   The `Co-authored-by` trailer format is a git convention recognized by GitHub, GitLab,
   and other platforms.

### 10.2 Commit Validation (Non-Empty Message, Staged Files)

**File**: `internal/tui/components/commitmsg.go` (modify existing from Phase 9)

Add a `commitError` field to `CommitMsgModel` to display validation and commit errors in
the button bar, separate from AI errors. Also add a `committing` flag to prevent double-commits.

**New fields** — add to `CommitMsgModel`:

```go
type CommitMsgModel struct {
	summary     textinput.Model
	description textarea.Model
	activeField commitField

	// AI generation state
	aiLoading    bool   // true while AI is generating
	aiProvider   string // display name of active provider (e.g., "Claude", "Ollama")
	aiError      string // last AI error message (cleared on next attempt)

	// Commit state
	commitError  string // last commit error message (cleared on next attempt)
	committing   bool   // true while commit is in progress

	width  int
	height int
	focused bool
}
```

**New message** — add a `CommitResultMsg` to the messages section:

```go
// CommitResultMsg is sent when the git commit completes (success or error).
type CommitResultMsg struct {
	Err error
}
```

**Update `ctrl+enter` handler** in `Update()` — add a committing guard and clear commit error:

```go
		case "ctrl+enter":
			// Request commit (handled by app)
			if m.committing {
				return m, nil // already committing
			}
			summary := m.Summary()
			if summary == "" {
				m.commitError = "summary is required"
				return m, nil
			}
			m.commitError = ""
			m.committing = true
			return m, func() tea.Msg {
				return CommitRequestMsg{
					Summary:     summary,
					Description: m.Description(),
				}
			}
```

**New methods** — add commit state helpers:

```go
// SetCommitError records a commit error (validation or git error).
func (m *CommitMsgModel) SetCommitError(errMsg string) {
	m.committing = false
	m.commitError = errMsg
}

// CommitSuccess clears the fields and resets state after a successful commit.
func (m *CommitMsgModel) CommitSuccess() {
	m.committing = false
	m.commitError = ""
	m.Clear()
}
```

**Update `Clear()`** — also clear commit state:

```go
func (m *CommitMsgModel) Clear() {
	m.summary.SetValue("")
	m.description.SetValue("")
	m.activeField = fieldSummary
	m.aiError = ""
	m.commitError = ""
	m.committing = false
	if m.focused {
		m.summary.Focus()
		m.description.Blur()
	}
}
```

**Update `renderButtonBar()`** — show commit errors and committing state. Commit errors
take priority over AI hints since a commit failure is more immediately relevant:

```go
func (m CommitMsgModel) renderButtonBar() string {
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	activeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF"))
	errorStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149"))
	successStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))

	var parts []string

	// AI provider indicator
	providerLabel := dimStyle.Render("AI:") + " " + activeStyle.Render(m.aiProvider)
	parts = append(parts, providerLabel)

	// Status: commit error > committing > AI loading > AI error > hints
	if m.commitError != "" {
		errText := m.commitError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else if m.committing {
		parts = append(parts, successStyle.Render("Committing..."))
	} else if m.aiLoading {
		parts = append(parts, activeStyle.Render("⟳ Generating..."))
	} else if m.aiError != "" {
		errText := m.aiError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else {
		parts = append(parts, dimStyle.Render("^g:generate ^p:provider"))
	}

	return strings.Join(parts, " ")
}
```

**Full updated `commitmsg.go`** — here is the complete file with all Phase 10 changes
integrated into the Phase 9 code:

```go
package components

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/bubbles/v2/textinput"
	"charm.land/bubbles/v2/textarea"
	"charm.land/lipgloss/v2"
)

// ── Messages ────────────────────────────────────────────

// CommitRequestMsg is sent when the user presses ctrl+enter to commit.
// The app handles the actual git commit.
type CommitRequestMsg struct {
	Summary     string
	Description string
}

// CommitResultMsg is sent when the git commit completes (success or error).
type CommitResultMsg struct {
	Err error
}

// AIGenerateMsg is sent when the user presses ctrl+g to generate a commit message.
type AIGenerateMsg struct{}

// AICycleProviderMsg is sent when the user presses ctrl+p to cycle AI providers.
type AICycleProviderMsg struct{}

// AIResultMsg is sent when AI generation completes (success or error).
type AIResultMsg struct {
	Title       string
	Description string
	Err         error
}

// ── Focus field ─────────────────────────────────────────

type commitField int

const (
	fieldSummary     commitField = iota
	fieldDescription
)

// ── Model ───────────────────────────────────────────────

// CommitMsgModel is the commit message pane component (Pane 3 in Changes tab).
// It contains a summary text input and a description textarea.
type CommitMsgModel struct {
	summary     textinput.Model
	description textarea.Model
	activeField commitField

	// AI generation state
	aiLoading    bool   // true while AI is generating
	aiProvider   string // display name of active provider (e.g., "Claude", "Ollama")
	aiError      string // last AI error message (cleared on next attempt)

	// Commit state
	commitError  string // last commit error message (cleared on next attempt)
	committing   bool   // true while commit is in progress

	width  int
	height int
	focused bool
}

// NewCommitMsg creates a new commit message component with default settings.
func NewCommitMsg() CommitMsgModel {
	// Summary: single-line input, max 72 chars
	ti := textinput.New()
	ti.Placeholder = "Summary (required)"
	ti.CharLimit = 72
	ti.SetWidth(30) // will be resized by SetSize

	// Description: multi-line textarea
	ta := textarea.New()
	ta.Placeholder = "Description"
	ta.SetWidth(30)
	ta.SetHeight(3) // compact — fits in the sidebar pane
	ta.ShowLineNumbers = false
	ta.CharLimit = 0 // no limit on description length

	return CommitMsgModel{
		summary:     ti,
		description: ta,
		activeField: fieldSummary,
		aiProvider:  "Claude",
	}
}

// SetSize updates the component dimensions.
// Called when the window resizes or the layout recalculates.
func (m *CommitMsgModel) SetSize(width, height int) {
	m.width = width
	m.height = height

	innerW := width - 2 // leave space for padding
	if innerW < 10 {
		innerW = 10
	}
	m.summary.SetWidth(innerW)

	// Description gets remaining height: total - summary(1) - button bar(1) - gaps(1)
	descHeight := height - 3
	if descHeight < 1 {
		descHeight = 1
	}
	m.description.SetWidth(innerW)
	m.description.SetHeight(descHeight)
}

// Focus activates the commit message pane for text input.
func (m *CommitMsgModel) Focus() {
	m.focused = true
	m.activeField = fieldSummary
	m.summary.Focus()
	m.description.Blur()
}

// Blur deactivates the commit message pane.
func (m *CommitMsgModel) Blur() {
	m.focused = false
	m.summary.Blur()
	m.description.Blur()
}

// Summary returns the current summary text.
func (m CommitMsgModel) Summary() string {
	return strings.TrimSpace(m.summary.Value())
}

// Description returns the current description text.
func (m CommitMsgModel) Description() string {
	return strings.TrimSpace(m.description.Value())
}

// IsEmpty returns true if both summary and description are empty.
func (m CommitMsgModel) IsEmpty() bool {
	return m.Summary() == "" && m.Description() == ""
}

// Clear resets both fields to empty (after a successful commit).
func (m *CommitMsgModel) Clear() {
	m.summary.SetValue("")
	m.description.SetValue("")
	m.activeField = fieldSummary
	m.aiError = ""
	m.commitError = ""
	m.committing = false
	if m.focused {
		m.summary.Focus()
		m.description.Blur()
	}
}

// SetAIResult fills the fields with the AI-generated commit message.
func (m *CommitMsgModel) SetAIResult(title, description string) {
	m.summary.SetValue(title)
	m.description.SetValue(description)
	m.aiLoading = false
	m.aiError = ""
}

// SetAIError records an AI generation error.
func (m *CommitMsgModel) SetAIError(errMsg string) {
	m.aiLoading = false
	m.aiError = errMsg
}

// SetAIProvider updates the displayed provider name.
func (m *CommitMsgModel) SetAIProvider(name string) {
	m.aiProvider = name
}

// SetCommitError records a commit error (validation or git error).
func (m *CommitMsgModel) SetCommitError(errMsg string) {
	m.committing = false
	m.commitError = errMsg
}

// CommitSuccess clears the fields and resets state after a successful commit.
func (m *CommitMsgModel) CommitSuccess() {
	m.committing = false
	m.commitError = ""
	m.Clear()
}

// ── Update ──────────────────────────────────────────────

// Update handles key messages for the commit message pane.
func (m CommitMsgModel) Update(msg tea.Msg) (CommitMsgModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		key := msg.String()

		switch key {
		case "tab":
			// Toggle between summary and description
			if m.activeField == fieldSummary {
				m.activeField = fieldDescription
				m.summary.Blur()
				m.description.Focus()
			} else {
				m.activeField = fieldSummary
				m.description.Blur()
				m.summary.Focus()
			}
			return m, nil

		case "ctrl+g":
			// Trigger AI commit message generation
			if !m.aiLoading {
				m.aiLoading = true
				m.aiError = ""
				return m, func() tea.Msg { return AIGenerateMsg{} }
			}
			return m, nil

		case "ctrl+p":
			// Cycle AI provider
			return m, func() tea.Msg { return AICycleProviderMsg{} }

		case "ctrl+enter":
			// Request commit (handled by app)
			if m.committing {
				return m, nil // already committing
			}
			summary := m.Summary()
			if summary == "" {
				m.commitError = "summary is required"
				return m, nil
			}
			m.commitError = ""
			m.committing = true
			return m, func() tea.Msg {
				return CommitRequestMsg{
					Summary:     summary,
					Description: m.Description(),
				}
			}
		}
	}

	// Forward to the active field — any key not caught by the switch above
	// (regular characters, backspace, arrows, etc.) falls through to here
	// and is forwarded to the active Bubbles text component for typing.
	var cmd tea.Cmd
	if m.activeField == fieldSummary {
		m.summary, cmd = m.summary.Update(msg)
	} else {
		m.description, cmd = m.description.Update(msg)
	}
	return m, cmd
}

// ── View ────────────────────────────────────────────────

// View renders the commit message pane.
func (m CommitMsgModel) View() string {
	if m.width < 5 || m.height < 2 {
		return ""
	}

	var sections []string

	// Summary field
	sections = append(sections, m.summary.View())

	// Description field
	sections = append(sections, m.description.View())

	// Button bar: [AI: Provider] [status/hints]
	buttonBar := m.renderButtonBar()
	sections = append(sections, buttonBar)

	return strings.Join(sections, "\n")
}

// renderButtonBar renders the bottom bar with AI provider, commit status, and action hints.
func (m CommitMsgModel) renderButtonBar() string {
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	activeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF"))
	errorStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149"))
	successStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))

	var parts []string

	// AI provider indicator
	providerLabel := dimStyle.Render("AI:") + " " + activeStyle.Render(m.aiProvider)
	parts = append(parts, providerLabel)

	// Status: commit error > committing > AI loading > AI error > hints
	if m.commitError != "" {
		errText := m.commitError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else if m.committing {
		parts = append(parts, successStyle.Render("Committing..."))
	} else if m.aiLoading {
		parts = append(parts, activeStyle.Render("⟳ Generating..."))
	} else if m.aiError != "" {
		errText := m.aiError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else {
		parts = append(parts, dimStyle.Render("^g:generate ^p:provider"))
	}

	return strings.Join(parts, " ")
}
```

**What changed from Phase 9**:

1. **New `CommitResultMsg`**: message sent when the git commit completes (success or error)
2. **New `commitError` field**: stores commit validation/execution errors, displayed in the
   button bar with higher priority than AI errors
3. **New `committing` field**: prevents double-commits by ignoring `ctrl+enter` while a
   commit is in progress
4. **`ctrl+enter` updated**: now validates that summary is non-empty before emitting
   `CommitRequestMsg`. If empty, sets `commitError` directly without sending a message
5. **New `SetCommitError()` method**: sets the error and clears the `committing` flag
6. **New `CommitSuccess()` method**: clears all fields and resets state after a successful commit
7. **`Clear()` updated**: also resets `commitError` and `committing`
8. **`renderButtonBar()` updated**: shows commit errors (red) and "Committing..." (green)
   with priority over AI status. Added `successStyle` for the committing indicator

### 10.3 Post-Commit Status Refresh — App Integration

**File**: `internal/tui/app/app.go` (modify existing)

This section wires the commit execution into the app. When the user presses `ctrl+enter`,
the commit message component emits `CommitRequestMsg`. The app validates that staged changes
exist, formats the commit message, runs `git commit`, and on success clears the fields and
refreshes the status.

**New async command** — add `commitCmd` to `app.go`.

> **Bubbletea pattern recap**: `tea.Cmd` is a function that returns a `tea.Msg`. When you
> return a `tea.Cmd` from `Update()`, bubbletea runs it in a goroutine (off the main thread)
> and feeds the resulting message back into `Update()`. This is how bubbletea does async
> work without blocking the UI. Here, `commitCmd` returns a function that does the slow git
> work, then returns a `CommitResultMsg` that the app's `Update()` will handle.

```go
// commitCmd runs git commit asynchronously with validation.
func commitCmd(repoPath string, summary, description string) tea.Cmd {
	return func() tea.Msg {
		// Check for staged changes first
		hasStaged, err := git.HasStagedChanges(repoPath)
		if err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("checking staged changes: %w", err)}
		}
		if !hasStaged {
			return components.CommitResultMsg{Err: fmt.Errorf("no staged changes — stage files first")}
		}

		// Format the commit message (no co-authors for now — the settings UI handles this)
		message := git.FormatCommitMessage(summary, description, nil)

		// Execute the commit
		if err := git.Commit(repoPath, message); err != nil {
			return components.CommitResultMsg{Err: err}
		}

		return components.CommitResultMsg{Err: nil}
	}
}
```

**Update the `CommitRequestMsg` handler** in `Update()` — replace the Phase 9 no-op:

```go
	case components.CommitRequestMsg:
		// User pressed ctrl+enter — validate and commit
		return m, commitCmd(m.repoPath, msg.Summary, msg.Description)
```

**Add a new `CommitResultMsg` handler** in `Update()`:

```go
	case components.CommitResultMsg:
		if msg.Err != nil {
			m.commitMsg.SetCommitError(msg.Err.Error())
			return m, nil
		}
		// Commit succeeded — clear fields and refresh status.
		// Note: refreshStatusCmd() was defined earlier — it runs `git status`
		// asynchronously and sends a statusResultMsg to update the file list.
		m.commitMsg.CommitSuccess()
		return m, refreshStatusCmd(m.repoPath)
```

**Full `Update()` changes** — here is the complete switch block showing where the new
handlers fit among the existing Phase 9 handlers. The new code is the `CommitRequestMsg`
update and the new `CommitResultMsg` case:

```go
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	// ... existing cases (WindowSizeMsg, authResultMsg, etc.) ...

	case components.AIGenerateMsg:
		// User pressed ctrl+g — run the active AI provider
		if len(m.aiProviders) > 0 {
			provider := m.aiProviders[m.aiActiveIdx]
			return m, generateCommitMsgCmd(m.repoPath, provider)
		}
		return m, nil

	case components.AICycleProviderMsg:
		// User pressed ctrl+p — cycle to the next AI provider
		if len(m.aiProviders) > 0 {
			m.aiActiveIdx = (m.aiActiveIdx + 1) % len(m.aiProviders)
			provider := m.aiProviders[m.aiActiveIdx]
			m.commitMsg.SetAIProvider(provider.DisplayName())
		}
		return m, nil

	case components.AIResultMsg:
		// AI generation completed
		if msg.Err != nil {
			m.commitMsg.SetAIError(msg.Err.Error())
			return m, nil
		}
		m.commitMsg.SetAIResult(msg.Title, msg.Description)
		return m, nil

	case components.CommitRequestMsg:
		// User pressed ctrl+enter — validate and commit
		return m, commitCmd(m.repoPath, msg.Summary, msg.Description)

	case components.CommitResultMsg:
		// Commit completed (success or failure)
		if msg.Err != nil {
			m.commitMsg.SetCommitError(msg.Err.Error())
			return m, nil
		}
		// Commit succeeded — clear fields and refresh status
		m.commitMsg.CommitSuccess()
		return m, refreshStatusCmd(m.repoPath)

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}
```

**New import** — add `fmt` if not already present (used in `commitCmd`):

```go
import (
	// ... existing imports ...
	"fmt"
)
```

**What changed from Phase 9**:

1. **New file `internal/git/commit.go`**: `Commit()` — runs `git commit -F -` with message
   on stdin; `HasStagedChanges()` — runs `git diff --cached --quiet` to check if anything
   is staged; `FormatCommitMessage()` — builds the full message string with summary,
   description, and co-author trailers
2. **`commitmsg.go` updated**: new `CommitResultMsg` message, `commitError` and `committing`
   fields, `SetCommitError()` and `CommitSuccess()` methods, validation in `ctrl+enter`
   handler (rejects empty summary), updated `renderButtonBar()` with commit status display
3. **`app.go` updated**: new `commitCmd()` async command (validates staged changes, formats
   message, runs commit), `CommitRequestMsg` handler calls `commitCmd()` instead of no-op,
   new `CommitResultMsg` handler clears fields on success or shows error on failure,
   successful commit triggers `refreshStatusCmd()` to update the file list

### 10.4 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Commit with summary only**:

1. Make a change in the repo and stage it:
   ```bash
   echo "test change" >> somefile.txt
   git add somefile.txt
   ```
2. Press `3` to focus the commit message pane
3. Type a summary: `Add test change to somefile`
4. Press `ctrl+enter`
5. The button bar should briefly show "Committing..." in green
6. On success: both fields clear, the file list refreshes (staged file disappears from
   the changes list), and git log shows the new commit:
   ```bash
   git log -1 --oneline
   ```
   Should show: `<hash> Add test change to somefile`

**Test 2 — Commit with summary and description**:

1. Stage another change
2. Press `3`, type a summary: `Fix typo in README`
3. Press `Tab` to move to the description field
4. Type: `The README had a misspelling in the installation section.`
5. Press `ctrl+enter`
6. Verify with:
   ```bash
   git log -1 --format="%s%n%n%b"
   ```
   Should show the summary on line 1 and description in the body.

**Test 3 — Validation: empty summary**:

1. Press `3` to focus the commit message pane
2. Leave the summary empty
3. Press `ctrl+enter`
4. The button bar should show "summary is required" in red
5. No commit should be created (verify with `git log -1`)

**Test 4 — Validation: no staged changes**:

1. Make sure nothing is staged: `git reset HEAD`
2. Press `3`, type a summary: `This should fail`
3. Press `ctrl+enter`
4. The button bar should show "no staged changes — stage files first" in red
5. No commit should be created

**Test 5 — Double-commit prevention**:

1. Stage some changes
2. Press `3`, type a summary
3. Press `ctrl+enter` rapidly multiple times
4. Only one commit should be created (the `committing` flag prevents re-entry)
5. Check with `git log --oneline -5` — only one new commit

**Test 6 — Error clears on next attempt**:

1. Trigger a "no staged changes" error (Test 4)
2. The button bar shows the error in red
3. Stage a file: `git add somefile.txt`
4. Type a summary and press `ctrl+enter`
5. The error should clear and the commit should succeed

**Test 7 — Post-commit file list refresh**:

1. Have 3 changed files, stage 2 of them
2. Press `3`, type a summary, press `ctrl+enter`
3. After the commit succeeds:
   - The 2 staged files should disappear from the Changed Files list
   - The 1 unstaged file should remain
   - The file count in the pane title should update (e.g., "Changed Files (1)")

**Test 8 — Post-commit diff viewer reset**:

1. Select a staged file in the file list (Pane 1) and view its diff (Pane 2)
2. Press `3`, type a summary, press `ctrl+enter`
3. After the commit: the file list updates, the previously selected file may be gone.
   If the cursor was on a file that was committed, the diff pane should either show the
   diff of the new cursor position or be empty if no files remain.

**Test 9 — AI generate then commit flow**:

1. Stage some changes
2. Press `3` to focus the commit pane
3. Press `ctrl+g` to generate an AI commit message
4. Wait for the AI to fill the fields
5. Optionally edit the generated summary or description
6. Press `ctrl+enter` to commit
7. The commit should succeed with the (possibly edited) AI-generated message
8. Fields clear, file list refreshes

**Test 10 — Commit message formatting**:

1. Stage changes and commit with:
   - Summary: `Update build configuration`
   - Description: `Switch from Webpack to Rspack for faster builds.\nThis reduces cold start time by 60%.`
2. Verify the commit message format:
   ```bash
   git log -1 --format="%B"
   ```
   Should show:
   ```
   Update build configuration

   Switch from Webpack to Rspack for faster builds.
   This reduces cold start time by 60%.
   ```
   Note the blank line between summary and description.

**Phase 10 is complete when**: pressing `ctrl+enter` with a non-empty summary and staged
files creates a r eal git commit; the commit message is properly formatted with summary,
blank line, and description; empty summary shows "summary is required" error; no staged
changes shows "no staged changes" error; the button bar shows "Committing..." during
execution; on success the fields clear and git status refreshes (committed files disappear
from the changes list); double-commits are prevented by the `committing` guard; and the
full AI-generate-then-commit flow works end-to-end.

## Phase 11 — Push

**Goal**: Add push-to-remote functionality triggered by pressing `p` in navigable mode. The
header action label changes to "Pushing..." during the operation, the push runs asynchronously,
and on completion the status refreshes to update the ahead/behind counts. If no upstream
tracking branch exists (first push of a new branch), `--set-upstream` is used automatically
to create one.

This phase introduces:
1. **Push execution** — a `Push()` function in `internal/git/` that runs `git push` with
   configurable options (remote, branch, set-upstream, force-with-lease)
2. **Remote detection** — `GetDefaultRemote()` discovers the remote name from git config,
   `RemoteFromUpstream()` extracts it from the upstream tracking ref
3. **Push state feedback** — the header action label shows "Pushing..." during the operation,
   an error modal appears on failure, and a post-push status refresh updates the ahead count
4. **Upstream handling** — when `hasUpstream` is false, the push automatically uses
   `--set-upstream` so subsequent pushes/pulls just work

After this phase, pressing `p` pushes the current branch to its remote. The header changes
from "↑ Push" to "↑ Pushing..." during the operation. On success, the ahead count drops to 0
and the action label changes back to "↻ Fetch". On failure (e.g., rejected non-fast-forward),
an error modal shows the git error message. New branches without an upstream get
`--set-upstream` automatically.

### 11.1 Push to Remote (`git push`)

**File**: `internal/git/push.go` (new file)

This file runs `git push` with the appropriate flags. The push command format is:

```
git push [--progress] <remote> <branch> [--set-upstream] [--force-with-lease]
```

Flags:
- `--progress` — forces progress output to stderr even when not connected to a terminal.
  We include this so the progress text is captured in the combined output (useful for
  debugging), even though Phase 11 does not display real-time progress.
- `--set-upstream` (`-u`) — creates a tracking relationship between the local branch and
  the remote branch. Used on first push of a new branch so that subsequent `git push`
  and `git pull` work without specifying the remote and branch.
- `--force-with-lease` — a safer alternative to `--force`. Refuses to push if the remote
  ref has been updated by someone else since your last fetch. Phase 11 does NOT use this
  flag — it is included in `PushOptions` for Phase 13 (Settings) to wire up.

Remote detection:
- If the branch has an upstream tracking ref (e.g., `origin/main`), the remote is extracted
  from the ref by splitting on `/` — everything before the first `/` is the remote name.
- If there's no upstream, `GetDefaultRemote()` runs `git remote` and returns the first
  listed remote (usually `origin`). If no remotes are configured, it returns an error.

Environment: `TERM=dumb` is always set to prevent any pager or color output.

```go
package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// PushOptions configures the git push command.
type PushOptions struct {
	Remote         string // remote name (e.g., "origin")
	Branch         string // local branch name to push
	SetUpstream    bool   // --set-upstream: create tracking relationship
	ForceWithLease bool // --force-with-lease: safe force push
}

// Push runs `git push` with the given options.
// The command is: git push [--progress] [--set-upstream] [--force-with-lease] <remote> <branch>
// Progress output goes to stderr; CombinedOutput captures both stdout and stderr.
func Push(repoPath string, opts PushOptions) error {
	args := []string{"push", "--progress"}

	if opts.SetUpstream {
		args = append(args, "--set-upstream")
	}
	if opts.ForceWithLease {
		args = append(args, "--force-with-lease")
	}

	args = append(args, opts.Remote, opts.Branch)

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	// cmd.Environ() copies the current process's environment variables (PATH, HOME,
	// SSH_AUTH_SOCK, etc.) so git inherits them — without this, git wouldn't find
	// SSH keys or the git binary itself. We then append TERM=dumb which prevents
	// git from using a pager (like `less`) or ANSI color codes in its output.
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git push: %s (%w)", strings.TrimSpace(string(out)), err)
	}
	return nil
}

// GetDefaultRemote returns the name of the first configured remote.
// In most repositories this is "origin". If no remotes are configured,
// an error is returned.
func GetDefaultRemote(repoPath string) (string, error) {
	cmd := exec.Command("git", "remote")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git remote: %w", err)
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) == 0 || lines[0] == "" {
		return "", fmt.Errorf("no remotes configured")
	}
	return lines[0], nil
}

// RemoteFromUpstream extracts the remote name from an upstream tracking ref.
// For example, "origin/main" returns "origin", "upstream/feature" returns "upstream".
// If the format is unexpected, it returns "origin" as a fallback.
func RemoteFromUpstream(upstream string) string {
	if idx := strings.Index(upstream, "/"); idx > 0 {
		return upstream[:idx]
	}
	return "origin"
}
```

**How `Push()` works**:

1. Builds the `git push` argument list starting with `--progress`.
2. Conditionally appends `--set-upstream` (for first push) and `--force-with-lease` (for
   safe force push — wired up in Phase 13).
3. Appends the remote name and branch name as positional arguments.
4. `CombinedOutput()` captures both stdout (the push summary) and stderr (progress output
   and error messages). If the push fails, the error includes the full git output.

**How `GetDefaultRemote()` works**:

1. Runs `git remote` which lists all configured remotes, one per line.
2. Returns the first remote (usually `origin`). If no remotes exist, returns an error.
3. This is only used when there's no upstream tracking ref — when an upstream exists,
   `RemoteFromUpstream()` is more accurate since it handles multi-remote setups.

**How `RemoteFromUpstream()` works**:

1. Git stores upstream refs as `<remote>/<branch>` (e.g., `origin/main`, `upstream/develop`).
2. Splitting on the first `/` gives the remote name.
3. Falls back to `"origin"` if the format is unexpected (shouldn't happen in practice).

### 11.2 Push State Feedback (Progress, Success, Error)

**File**: `internal/tui/views/header.go` (modify existing)

Add a `Pushing` field to `HeaderData` so the header can show "Pushing..." during the
operation. Update `actionLabel()` to check this field before the normal ahead/behind logic.

**New field** — add to `HeaderData`:

```go
// HeaderData holds the information needed to render the header bar.
type HeaderData struct {
	RepoName    string
	BranchName  string
	Ahead       int
	Behind      int
	HasUpstream bool
	Pushing     bool // true while a push is in progress
}
```

**Update `actionLabel()`** — check `Pushing` before the normal logic:

```go
// actionLabel returns the quick action text based on ahead/behind state.
func actionLabel(data HeaderData) string {
	if data.Pushing {
		return "↑ Pushing..."
	}

	if !data.HasUpstream {
		return "↑ Publish branch"
	}

	switch {
	case data.Ahead > 0 && data.Behind > 0:
		return "↕ Pull / Push"
	case data.Behind > 0:
		return "↓ Pull"
	case data.Ahead > 0:
		return "↑ Push"
	default:
		return "↻ Fetch"
	}
}
```

**What changed from Phase 5**:

1. **New `Pushing` field**: `HeaderData` gets a `Pushing bool` field to indicate push in progress
2. **`actionLabel()` updated**: checks `Pushing` first — returns "↑ Pushing..." when true.
   Also, when `!data.HasUpstream` the label now shows "↑ Publish branch" instead of
   "↻ Fetch" — this is more accurate because pressing `p` on a branch without an upstream
   will push and set upstream, not fetch.

**File**: `internal/tui/app/app.go` (modify existing)

This section wires push into the app. The `p` key triggers a push in navigable mode. The
push runs asynchronously, shows "Pushing..." in the header, and refreshes status on
completion. Errors are shown in the error modal.

**New message** — add to the messages section at the top of `app.go`:

```go
// pushResultMsg is sent when the async git push completes (success or error).
type pushResultMsg struct {
	err error
}
```

**New fields** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Push state
	pushing  bool   // true while push is in progress (prevents double-push)
	upstream string // upstream tracking ref (e.g., "origin/main"), empty if none
}
```

**Update `statusResultMsg` handler** — also save the upstream string:

```go
	case statusResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Update branch info
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		m.upstream = msg.status.Upstream save upstream for push
		// Parse and update changed files
		files := git.ParseFiles(msg.status.RawOutput)
		m.fileList.SetFiles(files)
		m.updateFileListSize()
		return m, nil
```

**New async command** — add `pushCmd` to `app.go`:

```go
// pushCmd runs git push asynchronously.
// It determines the remote from the upstream ref (if set) or falls back to the default
// remote. If there's no upstream, --set-upstream is added to create one.
func pushCmd(repoPath, branchName, upstream string, hasUpstream bool) tea.Cmd {
	// Bubbletea runs tea.Cmd functions in a background goroutine so the push
	// doesn't block the UI thread (the TUI stays responsive and can render
	// "Pushing..." in the header). The returned closure captures the outer
	// function's parameters (repoPath, branchName, etc.) so they're available
	// when the goroutine executes. When the closure returns a tea.Msg, bubbletea
	// delivers it back to Update() on the main thread.
	return func() tea.Msg {
		// Determine the remote to push to
		var remote string
		if hasUpstream && upstream != "" {
			remote = git.RemoteFromUpstream(upstream)
		} else {
			// No upstream — discover the default remote
			var err error
			remote, err = git.GetDefaultRemote(repoPath)
			if err != nil {
				return pushResultMsg{err: fmt.Errorf("cannot push: %s", err)}
			}
		}

		// Build push options
		opts := git.PushOptions{
			Remote:      remote,
			Branch:      branchName,
			SetUpstream: !hasUpstream, // auto-set upstream on first push
		}

		// Execute the push
		if err := git.Push(repoPath, opts); err != nil {
			return pushResultMsg{err: err}
		}

		return pushResultMsg{err: nil}
	}
}
```

**Add the `pushResultMsg` handler** in `Update()` — place it after the `CommitResultMsg`
handler and before the `tea.KeyPressMsg` handler:

```go
	case pushResultMsg:
		m.pushing = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Push Failed",
				msg.err.Error(),
				false, // not retryable — user should inspect the error
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Push succeeded — refresh status to update ahead/behind counts.
		// refreshStatusCmd triggers a new `git status --porcelain=2` call, which will
		// report `# branch.ab +0 -0` after a successful push, so the statusResultMsg
		// handler sets ahead=0 and the header label switches from "↑ Push" to "↻ Fetch".
		return m, refreshStatusCmd(m.repoPath)
```

**Add the `p` keybinding** in `handleMainKey()` navigable mode — add a new case to the
`switch msg.String()` block, after the `` ` `` (terminal toggle) case:

```go
	case "p":
		// Push to remote
		if m.pushing {
			return m, nil // already pushing
		}
		if m.branchName == "" {
			// Detached HEAD — can't push.
			// When HEAD is detached, `git status --porcelain=2` reports `# branch.oid <sha>`
			// but the `# branch.head` line contains `(detached)` instead of a branch name.
			// The status parser sets `Branch` to "" in that case, so an empty
			// branchName here means we're in detached HEAD state and there's no branch to push.
			return m, nil
		}
		m.pushing = true
		return m, pushCmd(m.repoPath, m.branchName, m.upstream, m.hasUpstream)
```

**Update `viewMain()`** — pass the pushing state to `HeaderData`:

```go
	// ── Header + Tab bar ──
	headerData := views.HeaderData{
		RepoName:    git.RepoName(m.repoPath),
		BranchName:  m.branchName,
		Ahead:       m.ahead,
		Behind:      m.behind,
		HasUpstream: m.hasUpstream,
		Pushing: m.pushing, // show pushing state in header
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)
```

**Full `Update()` changes** — here is the complete switch block showing where the new
handler fits among the existing handlers. The new code is the `pushResultMsg` case and
the updated `statusResultMsg` case:

```go
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	// ... existing cases (WindowSizeMsg, authResultMsg, etc.) ...

	case statusResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Update branch info
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		m.upstream = msg.status.Upstream 
		// Parse and update changed files
		files := git.ParseFiles(msg.status.RawOutput)
		m.fileList.SetFiles(files)
		m.updateFileListSize()
		return m, nil

	// ... existing cases (AIGenerateMsg, AICycleProviderMsg, AIResultMsg) ...

	case components.CommitRequestMsg:
		// User pressed ctrl+enter — validate and commit
		return m, commitCmd(m.repoPath, msg.Summary, msg.Description)

	case components.CommitResultMsg:
		// Commit completed (success or failure)
		if msg.Err != nil {
			m.commitMsg.SetCommitError(msg.Err.Error())
			return m, nil
		}
		// Commit succeeded — clear fields and refresh status
		m.commitMsg.CommitSuccess()
		return m, refreshStatusCmd(m.repoPath)

	case pushResultMsg:
		// Push completed (success or failure)
		m.pushing = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Push Failed",
				msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Push succeeded — refresh status to update ahead/behind counts.
		// refreshStatusCmd triggers a new `git status --porcelain=2` call, which will
		// report `# branch.ab +0 -0` after a successful push, so the statusResultMsg
		// handler sets ahead=0 and the header label switches from "↑ Push" to "↻ Fetch".
		return m, refreshStatusCmd(m.repoPath)

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}
```

**Full navigable mode keybinding addition** — here is the relevant section of
`handleMainKey()` showing where `p` fits:

```go
	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1
		return m, nil

	case "1":
		m.activePane = core.Pane1
		return m, nil

	case "2":
		m.activePane = core.Pane2
		return m, nil

	case "3":
		m.activePane = core.Pane3
		if m.activeTab == core.ChangesTab {
			m.focusMode = core.Focused
			m.commitMsg.Focus()
		}
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
		} else if m.activePane == core.PaneTerminal {
			m.activePane = core.Pane1
		}
		m.updateFileListSize()
		return m, nil

	case "p":
		// Push to remote
		if m.pushing {
			return m, nil // already pushing
		}
		if m.branchName == "" {
			return m, nil // detached HEAD — can't push
		}
		m.pushing = true
		return m, pushCmd(m.repoPath, m.branchName, m.upstream, m.hasUpstream)

	case "escape":
		return m, nil
	}
```

**What changed from Phase 10**:

1. **New file `internal/git/push.go`**: `PushOptions` struct, `Push()` — runs `git push`
   with configurable flags; `GetDefaultRemote()` — returns the first configured remote;
   `RemoteFromUpstream()` — extracts remote name from an upstream tracking ref
2. **`header.go` updated**: `HeaderData` gets a `Pushing bool` field; `actionLabel()`
   checks `Pushing` first (returns "↑ Pushing...") and shows "↑ Publish branch" when
   there's no upstream instead of "↻ Fetch"
3. **`app.go` updated**: new `pushing bool` and `upstream string` fields in Model;
   `statusResultMsg` handler saves `Upstream` string; new `pushResultMsg` message type;
   new `pushCmd()` async command (detects remote, auto-sets upstream on first push);
   `pushResultMsg` handler clears pushing state, shows error modal on failure, refreshes
   status on success; `p` keybinding in navigable mode triggers push with double-push
   prevention; `viewMain()` passes `Pushing` state to `HeaderData`

### 11.3 Upstream Branch Handling (Set Upstream on First Push)

The upstream detection and `--set-upstream` logic is built into `pushCmd()` in section 11.2.
Here is how the full flow works for a new branch that has never been pushed:

**Scenario**: User creates a local branch `feature/new-thing`, makes commits, and presses `p`.

1. **Status polling** detects the branch state: `branchName = "feature/new-thing"`,
   `hasUpstream = false`, `upstream = ""`. The `# branch.upstream` and `# branch.ab` lines
   are absent from `git status --porcelain=2` output because no tracking branch is configured.

2. **Header display**: Since `hasUpstream` is false, `actionLabel()` returns
   `"↑ Publish branch"` — telling the user that pressing `p` will publish the branch to
   the remote for the first time.

3. **User presses `p`**: The `p` keybinding sets `m.pushing = true` and dispatches
   `pushCmd(repoPath, "feature/new-thing", "", false)`.

4. **Inside `pushCmd()`**:
   - `hasUpstream` is false and `upstream` is empty, so it falls into the `else` branch
   - `GetDefaultRemote()` runs `git remote` and returns `"origin"` (the first remote)
   - `PushOptions` is built with `SetUpstream: true` (because `!hasUpstream`)
   - The resulting command is: `git push --progress --set-upstream origin feature/new-thing`

5. **Git creates the remote branch**: The remote now has `origin/feature/new-thing`, and
   git sets the local branch to track it.

6. **Push completes**: `pushResultMsg{err: nil}` is sent back to the app.

7. **Status refresh**: `refreshStatusCmd()` runs `git status --porcelain=2 --branch -z`.
   Now the output includes:
   ```
   # branch.upstream origin/feature/new-thing
   # branch.ab +0 -0
   ```

8. **Model updates**: `hasUpstream = true`, `upstream = "origin/feature/new-thing"`,
   `ahead = 0`, `behind = 0`. The header now shows `"↻ Fetch"` (no divergence).

**Subsequent pushes**: When the user makes more commits and presses `p` again:
- `hasUpstream` is now true, so `RemoteFromUpstream("origin/feature/new-thing")` returns
  `"origin"`
- `SetUpstream` is false (upstream already exists)
- The command is: `git push --progress origin feature/new-thing`

**Error case — no remotes configured**: If the user tries to push but the repo has no
remotes (e.g., a purely local repo), `GetDefaultRemote()` returns an error and the push
fails with "cannot push: no remotes configured" shown in the error modal.

**Error case — rejected push (non-fast-forward)**: If the remote has commits that aren't
in the local branch, `git push` returns exit code 1 with a message like:
```
! [rejected]        main -> main (non-fast-forward)
error: failed to push some refs to 'origin'
hint: Updates were rejected because the tip of your current branch is behind
hint: its remote counterpart.
```
This full message is captured by `CombinedOutput()` and shown in the error modal. The user
can then pull first (Phase 14) or use force push (Phase 13).

### 11.4 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Push with commits ahead**:

1. Make a commit in a repo that has a remote and upstream tracking branch:
   ```bash
   cd /path/to/repo
   echo "test" >> somefile.txt
   git add somefile.txt
   git commit -m "Test commit for push"
   ```
2. The header should show `↑1` (green, one commit ahead) and the action should be `↑ Push`
3. Press `p` in navigable mode
4. The header action should change to `↑ Pushing...`
5. After a moment, the push completes:
   - The header action changes to `↻ Fetch`
   - The `↑1` indicator disappears (ahead count drops to 0)
6. Verify on the remote:
   ```bash
   git log origin/main --oneline -1
   ```
   Should show the commit you just pushed.

**Test 2 — Publish a new branch (first push, no upstream)**:

1. Create a new local branch:
   ```bash
   git checkout -b test-push-branch
   echo "new branch content" > newfile.txt
   git add newfile.txt
   git commit -m "First commit on new branch"
   ```
2. Open the app. The header should show `↑ Publish branch` (no upstream exists yet)
3. Press `p`
4. The header should show `↑ Pushing...`
5. After completion:
   - The header action changes to `↻ Fetch`
   - The branch now has an upstream tracking ref
6. Verify:
   ```bash
   git branch -vv
   ```
   Should show `test-push-branch` tracking `origin/test-push-branch`.

**Test 3 — Push with nothing to push**:

1. Make sure you're up-to-date (no commits ahead):
   ```bash
   git status
   ```
   Should show "Your branch is up to date with 'origin/main'"
2. The header should show `↻ Fetch` (no divergence)
3. Press `p`
4. The push runs, git says "Everything up-to-date", and the push succeeds silently
5. No error modal should appear — this is not an error condition

**Test 4 — Push rejected (non-fast-forward)**:

1. Simulate a rejected push:
   ```bash
   # Create a commit locally
   echo "local" >> somefile.txt
   git add somefile.txt
   git commit -m "Local commit"
   # Simulate the remote being ahead (push from another clone)
   # Or use: git push origin HEAD~1:main (to move remote back, then push)
   ```
   Alternatively, have a collaborator push a commit to the same branch.
2. Press `p`
3. An error modal should appear with the git rejection message:
   ```
   Push Failed
   git push: ! [rejected] main -> main (non-fast-forward)
   error: failed to push some refs to 'origin'
   ```
4. Press `Esc` or `Enter` to dismiss the modal
5. The `pushing` state should be reset (header no longer shows "Pushing...")

**Test 5 — Double-push prevention**:

1. Make a commit so you have something to push
2. Press `p` rapidly multiple times
3. Only one push operation should execute — the `pushing` flag prevents re-entry
4. The header shows "Pushing..." for the duration of the single push

**Test 6 — Push on detached HEAD**:

1. Detach HEAD:
   ```bash
   git checkout --detach HEAD
   ```
2. The header should show `(detached)` as the branch name
3. Press `p` — nothing should happen (the `branchName == ""` guard prevents push)
4. No error, no crash — the keypress is silently ignored

**Test 7 — Push with no remotes configured**:

1. Create a repo with no remotes:
   ```bash
   cd /tmp && mkdir no-remote-test && cd no-remote-test && git init
   echo "test" > file.txt
   git add file.txt
   git commit -m "Initial commit"
   ```
2. Open in the app: `./leogit /tmp/no-remote-test`
3. Press `p`
4. An error modal should appear: "cannot push: no remotes configured"

**Test 8 — Header action label transitions**:

Verify the header action label changes correctly through these states:

| State | Header Action |
|-------|---------------|
| No upstream, not pushing | `↑ Publish branch` |
| Pushing in progress | `↑ Pushing...` |
| Push complete, ahead = 0, behind = 0 | `↻ Fetch` |
| 1 commit ahead | `↑ Push` |
| 2 commits behind | `↓ Pull` |
| 1 ahead, 1 behind | `↕ Pull / Push` |

**Test 9 — Push then commit cycle**:

1. Stage and commit some changes (Phase 10 flow)
2. The header should show `↑1` and `↑ Push`
3. Press `p` to push
4. After push: header shows `↻ Fetch`, no ahead count
5. Make another commit
6. Header shows `↑1` and `↑ Push` again
7. Press `p` again — second push succeeds
8. Verify both commits are on the remote

**Test 10 — Upstream detection after push**:

1. Create a new branch and push it (Test 2)
2. Make another commit on the same branch
3. Press `p` again
4. This time `--set-upstream` should NOT be used (the upstream already exists from the
   first push). The command should be `git push --progress origin test-push-branch`
   (without `--set-upstream`).
5. Verify by checking that git doesn't print the "branch set up to track" message.

**Phase 11 is complete when**: pressing `p` in navigable mode pushes the current branch
to its remote; the header shows "↑ Pushing..." during the operation; on success the ahead
count drops and the action label updates; on failure an error modal shows the git error;
new branches without an upstream get `--set-upstream` automatically and the header shows
"↑ Publish branch" before their first push; double-pushes are prevented by the `pushing`
flag; detached HEAD silently ignores the push key; and the full commit-then-push cycle
works end-to-end.

---

## Phase 12 — Embedded Terminal

**Goal**: Embed a fully interactive terminal pane in the bottom-right of the main layout.
The terminal runs the user's default shell (`$SHELL`, falling back to `/bin/sh`) via a PTY
(pseudo-terminal) spawned with `creack/pty`. The terminal emulator parses ANSI escape
sequences and maintains screen state using `taigrr/bubbleterm`. Pressing `` ` `` toggles the
terminal pane's visibility. When the terminal is focused, all keystrokes (including `Ctrl+C`,
`Ctrl+D`) are forwarded to the PTY subprocess. `Esc` unfocuses the terminal and returns to
navigable mode. `Ctrl+Shift+Up` / `Ctrl+Shift+Down` resizes the terminal pane by one row.

This phase introduces:
1. **PTY spawning** — a `Terminal` component in `internal/tui/components/` that creates a PTY
   subprocess using `creack/pty`, starts the user's shell at the repo root, and manages the
   lifecycle (start, resize, close)
2. **Terminal rendering** — uses `taigrr/bubbleterm` to parse ANSI escape sequences, maintain
   a virtual screen buffer, and render the terminal output as a string compatible with lipgloss
3. **Focus management** — terminal enters focused mode on `` ` `` (when already visible) or
   direct navigation. While focused, all keys go to the PTY. `Esc` unfocuses.
4. **Resize** — `Ctrl+Shift+Up` grows the terminal by 1 row, `Ctrl+Shift+Down` shrinks it.
   The PTY is notified of size changes via `pty.Setsize()`.

After this phase, pressing `` ` `` opens a live interactive terminal at the bottom of the
main column. You can run any command (`git log`, `htop`, `vim`, etc.) inside it. `Esc` returns
to the navigable TUI. The terminal remembers its state across focus/unfocus cycles.

### 12.0 Install New Dependency

The `bubbleterm` library provides a headless terminal emulator that integrates with Bubbletea.
It handles ANSI parsing, screen state, cursor tracking, colors, and scrollback — so you don't
need to write a VT100 parser yourself.

```bash
go get github.com/taigrr/bubbleterm@latest
```

After running this, `go.mod` should list `github.com/taigrr/bubbleterm` alongside
`github.com/creack/pty`.

### 12.1 PTY Spawning (creack/pty)

**File**: `internal/tui/components/terminal.go` (new file)

This file creates and manages the terminal component. It owns the PTY subprocess, the
bubbleterm emulator, and the focus state. The component follows the Bubbletea Model pattern
(`Init`, `Update`, `View`) so it can be embedded in the app like any other component.

**How PTY spawning works**:

1. `os/exec.Command` creates a shell command (`$SHELL` or `/bin/sh`)
2. `pty.StartWithSize()` from `creack/pty` does three things atomically:
   - Opens a new pseudo-terminal pair (master + slave)
   - Connects the slave side to the command's stdin/stdout/stderr
   - Starts the command with the specified initial size (cols × rows)
   - Returns the master side as an `*os.File`
3. The master file is both readable (shell output) and writable (user input)
4. `bubbleterm.NewWithPipes()` receives the master file as both reader (output) and
   writer (input), connecting the PTY to the terminal emulator

**How bubbleterm works**:

- It runs a background goroutine that reads from the PTY output
- Each read parses ANSI escape sequences (CSI, OSC, ESC) and updates a virtual screen buffer
- The `View()` method renders the current screen state as a string with ANSI color codes
- The `Update()` method handles `tea.KeyPressMsg` by converting keypresses to bytes and
  writing them to the PTY input
- `Focus()` / `Blur()` control whether keypresses are forwarded to the PTY

**Environment**: The shell is started with:
- `cmd.Dir` set to the repo path (so the shell opens in the repo root)
- `TERM=xterm-256color` for full color support
- All parent environment variables inherited

```go
package components

import (
	"os"
	"os/exec"

	tea "charm.land/bubbletea/v2"
	"github.com/creack/pty"
	"github.com/taigrr/bubbleterm"
)

// TerminalModel manages an embedded terminal pane with a PTY subprocess.
type TerminalModel struct {
	term     *bubbleterm.Model // terminal emulator (ANSI parser + screen buffer)
	ptmx     *os.File         // PTY master file (read/write to the subprocess)
	cmd      *exec.Cmd        // shell subprocess
	repoPath string           // working directory for the shell
	width    int              // current terminal width in columns
	height   int              // current terminal height in rows
	started  bool             // whether the PTY has been started
}

// NewTerminal creates a new terminal component. The PTY is not started yet —
// call Start() to spawn the shell subprocess.
func NewTerminal(repoPath string) TerminalModel {
	return TerminalModel{
		repoPath: repoPath,
	}
}

// Start spawns a PTY subprocess running the user's default shell.
// The shell opens in the repo root directory with the given initial size.
// Returns a tea.Cmd that can be used to initialize the bubbleterm component.
func (m *TerminalModel) Start(width, height int) tea.Cmd {
	if m.started {
		return nil
	}

	m.width = width
	m.height = height

	// Determine the shell to run
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}

	// Create the shell command
	m.cmd = exec.Command(shell)
	m.cmd.Dir = m.repoPath
	m.cmd.Env = append(os.Environ(), "TERM=xterm-256color")

	// Start the PTY with the specified size
	var err error
	m.ptmx, err = pty.StartWithSize(m.cmd, &pty.Winsize{
		Rows: uint16(height),
		Cols: uint16(width),
	})
	if err != nil {
		// If PTY fails, the terminal pane will show nothing.
		// This can happen on unsupported platforms.
		return nil
	}

	// Create the bubbleterm emulator connected to the PTY.
	// The PTY master file is both the reader (shell output) and writer (user input).
	m.term, err = bubbleterm.NewWithPipes(width, height, m.ptmx, m.ptmx)
	if err != nil {
		m.ptmx.Close()
		return nil
	}

	m.started = true

	// Return the bubbleterm Init command to start reading PTY output
	return m.term.Init()
}

// Started returns whether the PTY subprocess has been started.
func (m *TerminalModel) Started() bool {
	return m.started
}

// Close shuts down the PTY subprocess and terminal emulator.
// This should be called when the app exits.
func (m *TerminalModel) Close() {
	if m.term != nil {
		m.term.Close()
	}
	if m.ptmx != nil {
		m.ptmx.Close()
	}
	if m.cmd != nil && m.cmd.Process != nil {
		m.cmd.Process.Kill()
		m.cmd.Wait()
	}
}
```

**How `Start()` works**:

1. Determines the shell from `$SHELL` (e.g., `/bin/zsh`, `/bin/bash`), falling back to
   `/bin/sh` if the variable isn't set.
2. Creates the command with `exec.Command(shell)` — no arguments, so the shell starts in
   interactive mode.
3. Sets `cmd.Dir` to the repo path so the shell's working directory is the repo root.
4. Sets `TERM=xterm-256color` for full color and 256-color escape sequence support.
5. `pty.StartWithSize()` creates the PTY pair and starts the process in one call. The
   `Winsize` struct specifies the initial dimensions. The returned `ptmx` is the master
   side of the PTY.
6. `bubbleterm.NewWithPipes()` takes the PTY master as both reader and writer. This works
   because PTY master files are bidirectional — reading gets shell output, writing sends
   user input.
7. `m.term.Init()` returns a `tea.Cmd` that starts the bubbleterm background goroutine
   for reading PTY output.

### 12.2 Terminal Rendering in TUI

**File**: `internal/tui/components/terminal.go` (continue)

Add the `Update()`, `View()`, `Focus()`, `Blur()`, and `Resize()` methods to complete the
Bubbletea component interface.

**How rendering works**:

- `bubbleterm` maintains a virtual screen buffer (rows × cols grid of cells)
- Each cell has a character, foreground color, background color, and attributes (bold, etc.)
- `View()` renders this buffer as a string with ANSI escape codes for colors
- The output is compatible with lipgloss — it can be placed inside a lipgloss-styled box
- The terminal re-renders when the bubbleterm emulator processes new PTY output

**How input forwarding works**:

- When the terminal is focused, `Update()` receives `tea.KeyPressMsg` events
- `bubbleterm` converts these to the appropriate byte sequences and writes them to the PTY
- Special keys (arrows, Ctrl+C, etc.) are converted to their ANSI escape sequences
- The PTY subprocess receives the input as if it were typed in a real terminal

```go
// Update handles messages for the terminal component.
// When focused, key events are forwarded to the PTY via bubbleterm.
func (m TerminalModel) Update(msg tea.Msg) (TerminalModel, tea.Cmd) {
	if !m.started || m.term == nil {
		return m, nil
	}

	termModel, cmd := m.term.Update(msg)
	m.term = termModel.(*bubbleterm.Model)
	return m, cmd
}

// View renders the terminal screen buffer as a string.
// Returns empty string if the terminal hasn't been started.
func (m TerminalModel) View() string {
	if !m.started || m.term == nil {
		return ""
	}
	return m.term.View()
}

// Focus tells bubbleterm to start capturing key events.
// While focused, all keypresses are forwarded to the PTY subprocess.
func (m *TerminalModel) Focus() {
	if m.term != nil {
		m.term.Focus()
	}
}

// Blur tells bubbleterm to stop capturing key events.
// Keypresses will no longer be forwarded to the PTY.
func (m *TerminalModel) Blur() {
	if m.term != nil {
		m.term.Blur()
	}
}

// Focused returns whether the terminal is currently capturing key events.
func (m *TerminalModel) Focused() bool {
	if m.term == nil {
		return false
	}
	return m.term.Focused()
}

// Resize updates the terminal dimensions and notifies the PTY subprocess.
// This is called when the terminal pane height changes (user resize or
// Ctrl+Shift+Up/Down) or when the window is resized.
func (m *TerminalModel) Resize(width, height int) tea.Cmd {
	if width < 1 || height < 1 {
		return nil
	}
	m.width = width
	m.height = height

	// Notify the PTY of the new size so the shell can reflow its output
	if m.ptmx != nil {
		_ = pty.Setsize(m.ptmx, &pty.Winsize{
			Rows: uint16(height),
			Cols: uint16(width),
		})
	}

	// Notify bubbleterm of the new size so it can resize its screen buffer
	if m.term != nil {
		return m.term.Resize(width, height)
	}
	return nil
}
```

**How `Resize()` works**:

1. Updates the stored width/height on the component.
2. Calls `pty.Setsize()` to send a `SIGWINCH` signal to the PTY subprocess. This tells
   the shell and any running program (e.g., `vim`, `htop`) about the new dimensions so
   they can redraw.
3. Calls `m.term.Resize()` to resize bubbleterm's internal screen buffer. This returns
   a `tea.Cmd` that triggers a re-render.

**How `Update()` works**:

> **Beginner note**: `Update()` uses a **value receiver** `(m TerminalModel)` — not a pointer — because Bubbletea expects `Update` to return a new copy of the model. The call `m.term.Update(msg)` returns a `tea.Model` interface, which we type-assert back to `*bubbleterm.Model` with `m.term, _ = updated.(*bubbleterm.Model)`. The `.*bubbleterm.Model` syntax is a Go type assertion — it tells the compiler "I know this interface value is actually a `*bubbleterm.Model`, give it back as that concrete type."

1. Delegates entirely to `bubbleterm.Update()`. This handles:
   - `tea.KeyPressMsg` — converts to bytes and writes to the PTY (only when focused)
   - Internal tick messages — reads new output from the PTY and updates the screen buffer
   - Resize messages — adjusts the screen buffer dimensions
2. The returned model is type-asserted back to `*bubbleterm.Model`.
3. The returned `tea.Cmd` may contain follow-up commands (e.g., schedule next read).

### 12.3 Terminal Toggle, Focus & Unfocus

**File**: `internal/tui/app/app.go` (modify existing)

This section wires the terminal component into the app. The existing scaffolding from
Phase 4 already handles:
- `` ` `` toggles `terminalOpen` and sets `activePane = PaneTerminal`
- `terminalHeight` is initialized to `DefaultTerminalRows()` on first open
- The layout calculates `TerminalHeight` and `DiffHeight` based on `terminalOpen`
- The `viewMain()` renders a placeholder pane when `terminalOpen` is true

Phase 12 replaces the placeholder with the real terminal and adds:
- Lazy PTY start (spawn the shell on first toggle, not on app start)
- Focused mode when navigating to the terminal pane
- `Esc` to unfocus the terminal (already partially handled by focused mode in Phase 9)
- Key forwarding to bubbleterm in focused mode
- Terminal resize on layout changes

**New field** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Embedded terminal
	terminal components.TerminalModel // PTY + bubbleterm component
}
```

**Update `New()`** — initialize the terminal component:

```go
func New(cfg *config.Config, cliPath string) Model {
	return Model{
		// ... existing fields ...

		// Terminal
		terminal: components.NewTerminal(""), // repoPath set later when repo is resolved
	}
}
```

The terminal is created with an empty repo path because the repo isn't known yet at app
creation time. The repo path is set when the app enters `stateMain`.

**Update the repo resolution handler** — when the repo path is resolved and the app enters
`stateMain`, set the terminal's repo path. Find the place in `Update()` where `m.repoPath`
is set and `m.state = stateMain`:

```go
	// After m.repoPath is set and m.state = stateMain:
	m.terminal = components.NewTerminal(m.repoPath)
```

This ensures the terminal's working directory matches the resolved repo. The PTY is not
started yet — it starts lazily on first `` ` `` press.

**Update the `` ` `` keybinding** in `handleMainKey()` — start the PTY on first open and
enter focused mode:

```go
	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
			m.focusMode = core.Focused

			// Calculate the terminal pane's inner dimensions
			dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
			innerW := dim.MainWidth - 2  // subtract border: 1 left + 1 right = 2
			innerH := dim.TerminalHeight - 2 // same: 1 top + 1 bottom = 2

			// Lazy start: spawn the PTY on first open
			if !m.terminal.Started() {
				cmd := m.terminal.Start(innerW, innerH)
				m.terminal.Focus()
				m.updateFileListSize()
				return m, cmd
			}

			// Already started — just focus and resize to current dimensions
			m.terminal.Focus()
			cmd := m.terminal.Resize(innerW, innerH)
			m.updateFileListSize()
			return m, cmd
		} else {
			// Closing the terminal pane
			m.terminal.Blur()
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.activePane = core.Pane1
			}
		}
		m.updateFileListSize()
		return m, nil
```

**How the toggle works**:

1. **First open** (`!m.terminal.Started()`):
   - Initializes `terminalHeight` to 10 rows (the default)
   - Calculates the inner dimensions (total pane size minus 2 for the border)
   - Calls `m.terminal.Start()` which spawns the PTY and creates bubbleterm
   - Calls `m.terminal.Focus()` to start forwarding keys
   - Returns the `tea.Cmd` from `Start()` to begin reading PTY output
   - Enters focused mode so all subsequent keypresses go to the shell

2. **Subsequent opens** (PTY already running):
   - The shell is still alive in the background, maintaining its state
   - Calls `Resize()` to match the current pane dimensions (they may have changed)
   - Calls `Focus()` to resume key forwarding
   - Enters focused mode

3. **Closing** (`!m.terminalOpen`):
   - Calls `Blur()` to stop key forwarding
   - Returns to navigable mode
   - Falls back to Pane 1 if the terminal was the active pane

**Update the focused mode handler** in `handleMainKey()` — forward keys to the terminal
when the terminal pane is focused. Find the existing focused mode section (from Phase 9):

```go
	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.terminal.Blur()
			} else {
				m.commitMsg.Blur()
			}
			return m, nil
		}

		// Forward to active pane component
		if m.activePane == core.PaneTerminal {
			// Terminal: forward ALL keys to bubbleterm → PTY
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			return m, cmd
		}
		if m.activePane == core.Pane3 && m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}
		return m, nil
	}
```

**What changed from Phase 9's focused mode handler**:

1. **`Esc` now checks which pane is focused**: if the terminal pane is active, it calls
   `m.terminal.Blur()` instead of `m.commitMsg.Blur()`
2. **New terminal forwarding**: when `activePane == PaneTerminal`, all key events are
   forwarded to `m.terminal.Update()` which passes them to bubbleterm → PTY
3. **Key forwarding is unconditional**: unlike Pane 3 (which only forwards in Changes tab),
   the terminal forwards all keys in all tabs because the terminal is the same in both tabs

**Update `handlePaneKey()`** — wire the terminal pane:

```go
	case core.PaneTerminal:
		// Terminal pane: forward keys to bubbleterm
		var cmd tea.Cmd
		m.terminal, cmd = m.terminal.Update(msg)
		return m, cmd
```

**Update the `Update()` method** — forward non-key messages to the terminal component.
Bubbleterm sends internal messages (tick events, output events) that need to reach the
component even when the terminal isn't focused. Add a new case before the `tea.KeyPressMsg`
handler:

```go
	// Forward bubbleterm internal messages (PTY output, ticks)
	// These must reach the terminal even when it's not focused.
	default:
		if m.terminalOpen && m.terminal.Started() {
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			if cmd != nil {
				return m, cmd
			}
		}
```

Place this `default` case at the **very end** of the `switch msg := msg.(type)` block in
`Update()` — after all typed cases like `statusResultMsg`, `pushResultMsg`,
`tea.KeyPressMsg`, etc. In Go, `default` catches any type not matched by a preceding `case`.
This ensures that unhandled message types (which include bubbleterm's internal messages like
PTY output and tick events) are forwarded to the terminal component.

**Update `viewMain()`** — replace the placeholder terminal pane with real content:

```go
	if m.terminalOpen {
		var termContent string
		if m.terminal.Started() {
			termContent = m.terminal.View()
		} else {
			termContent = "Press ` to start terminal"
		}
		termPane := renderPaneWithContent(
			"Terminal",
			termContent,
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}
```

**Update the `WindowSizeMsg` handler** — resize the terminal when the window changes:

```go
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.updateFileListSize()

		// Resize terminal if it's open and started
		if m.terminalOpen && m.terminal.Started() {
			dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
			innerW := dim.MainWidth - 2
			innerH := dim.TerminalHeight - 2
			cmd := m.terminal.Resize(innerW, innerH)
			return m, cmd
		}
		return m, nil
```

**Update the cleanup** — close the PTY when the app exits. Find the `q` keybinding handler:

```go
	case "q":
		m.terminal.Close() // clean up PTY subprocess
		m.quitting = true
		return m, tea.Quit
```

Also update the `Ctrl+C` handler if one exists, to ensure the PTY is cleaned up on
forced quit as well.

**Full `handleMainKey()` changes** — here is the complete focused mode + navigable mode
section showing where all terminal code fits:

```go
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority ──
	if m.errorModal.Visible {
		m.errorModal = m.errorModal.HandleKey(msg)
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		// Esc always unfocuses, regardless of which pane
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.terminal.Blur()
			} else {
				m.commitMsg.Blur()
			}
			return m, nil
		}

		// Terminal pane: ALL keys (including Ctrl+C, Ctrl+D) go to PTY
		if m.activePane == core.PaneTerminal {
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			return m, cmd
		}

		// Commit message pane: keys go to text input/textarea
		if m.activePane == core.Pane3 && m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}

		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.terminal.Close()
		m.quitting = true
		return m, tea.Quit

	// ... existing cases (?, tab, 1, 2, 3) ...

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
			m.focusMode = core.Focused

			dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
			innerW := dim.MainWidth - 2
			innerH := dim.TerminalHeight - 2

			if !m.terminal.Started() {
				cmd := m.terminal.Start(innerW, innerH)
				m.terminal.Focus()
				m.updateFileListSize()
				return m, cmd
			}

			m.terminal.Focus()
			cmd := m.terminal.Resize(innerW, innerH)
			m.updateFileListSize()
			return m, cmd
		} else {
			m.terminal.Blur()
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.activePane = core.Pane1
			}
		}
		m.updateFileListSize()
		return m, nil

	case "p":
		// Push to remote
		if m.pushing {
			return m, nil
		}
		if m.branchName == "" {
			return m, nil
		}
		m.pushing = true
		return m, pushCmd(m.repoPath, m.branchName, m.upstream, m.hasUpstream)

	case "escape":
		return m, nil
	}

	// ── Pane-specific keys (j/k/enter/space etc.) ──
	return m.handlePaneKey(msg)
}
```

**What changed from Phase 11**:

1. **New `terminal` field in Model**: `components.TerminalModel` — owns the PTY + bubbleterm
2. **`New()` updated**: creates a `TerminalModel` with empty repo path
3. **Repo resolution updated**: recreates the terminal with the resolved repo path
4. **`` ` `` keybinding rewritten**: now enters focused mode, lazily starts the PTY on first
   open, resizes on subsequent opens, blurs on close
5. **Focused mode updated**: `Esc` checks active pane to call the right `Blur()`. Terminal
   pane forwards all keys to `m.terminal.Update()`. The `TODO` comment from Phase 9 is
   resolved.
6. **`handlePaneKey()` updated**: `PaneTerminal` case now forwards to `m.terminal.Update()`
   instead of returning nil
7. **`Update()` updated**: `default` case forwards unhandled messages to the terminal
   component (bubbleterm internal messages). `WindowSizeMsg` resizes the terminal.
8. **`viewMain()` updated**: terminal pane renders `m.terminal.View()` instead of placeholder
9. **`q` keybinding updated**: calls `m.terminal.Close()` before quitting
10. **`updateFileListSize()` unchanged**: already called from the `` ` `` handler and window
    resize — the layout recalculation already accounts for `terminalOpen` and `terminalHeight`

### 12.4 Terminal Resize (Ctrl+Shift+Up/Down)

**File**: `internal/tui/app/app.go` (modify existing)

Add `Ctrl+Shift+Up` and `Ctrl+Shift+Down` keybindings to resize the terminal pane while it
is focused. These adjust `terminalHeight` by one row and notify the PTY of the new size.

Constraints from the layout module (defined in Phase 4):
- Minimum terminal height: 3 rows (`minTermRows`)
- Maximum terminal height: 80% of content height
- The diff pane shrinks/grows inversely — it gets whatever space the terminal doesn't use

**Add resize cases** to the focused mode handler, before the `Esc` check:

```go
	// ── Focused mode — only Esc escapes, plus terminal resize ──
	if m.focusMode == core.Focused {
		// Terminal resize: Ctrl+Shift+Up/Down
		if m.activePane == core.PaneTerminal {
			switch msg.String() {
			case "ctrl+shift+up":
				// Grow terminal by 1 row. We ask layout.Calculate to try +1,
				// then check if it actually increased — because Calculate clamps
				// to the 80% max defined earlier. If clamped, the requested
				// height equals the current height and no resize happens.
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight+1)
				if dim.TerminalHeight > m.terminalHeight {
					m.terminalHeight = dim.TerminalHeight
					m.updateFileListSize()
					innerW := dim.MainWidth - 2
					innerH := dim.TerminalHeight - 2
					cmd := m.terminal.Resize(innerW, innerH)
					return m, cmd
				}
				return m, nil

			case "ctrl+shift+down":
				// Shrink terminal by 1 row
				if m.terminalHeight <= 3 { // minTermRows
					return m, nil
				}
				m.terminalHeight--
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
				m.updateFileListSize()
				innerW := dim.MainWidth - 2
				innerH := dim.TerminalHeight - 2
				cmd := m.terminal.Resize(innerW, innerH)
				return m, cmd
			}
		}

		// Esc always unfocuses
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.terminal.Blur()
			} else {
				m.commitMsg.Blur()
			}
			return m, nil
		}

		// Terminal pane: ALL keys go to PTY
		if m.activePane == core.PaneTerminal {
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			return m, cmd
		}

		// Commit message pane
		if m.activePane == core.Pane3 && m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}

		return m, nil
	}
```

**How the resize works**:

1. **Ctrl+Shift+Up (grow)**: Tries `terminalHeight + 1`. The `layout.Calculate()` function
   clamps the height to `maxTerm` (80% of content height). If the clamped result is larger
   than the current height, the resize is applied. Otherwise, the terminal is already at
   maximum size and the keypress is ignored.

2. **Ctrl+Shift+Down (shrink)**: Checks if `terminalHeight <= 3` (minimum). If at minimum,
   ignores the keypress. Otherwise, decrements by 1 and recalculates.

3. **After any resize**: `updateFileListSize()` is called to recalculate the file list and
   commit message pane sizes (since the diff pane height changed). `m.terminal.Resize()` is
   called to notify the PTY and bubbleterm of the new dimensions.

4. **Why resize is checked before Esc and key forwarding**: `Ctrl+Shift+Up/Down` must be
   intercepted before being forwarded to the PTY. If forwarded, the shell would receive
   unknown escape sequences instead of the terminal pane resizing.

**Update the help overlay** — add the terminal resize keybindings to the help text. Find
the help rows in `views/help.go` and add:

```go
	// Terminal section (add to the help rows)
	row("` ", "Toggle terminal pane"),
	row("Esc", "Unfocus terminal (when focused)"),
	row("Ctrl+Shift+Up", "Grow terminal by 1 row"),
	row("Ctrl+Shift+Down", "Shrink terminal by 1 row"),
```

### 12.5 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

**Test 1 — Terminal toggle and shell start**:

1. Press `` ` `` in navigable mode
2. The terminal pane should appear at the bottom of the main column, below the diff viewer
3. A shell prompt should appear (e.g., `user@host:~/repo$` or `%` for zsh)
4. The terminal pane border should be blue (focused)
5. The diff viewer height should shrink to make room for the terminal

**Test 2 — Interactive shell commands**:

1. With the terminal focused, type `ls` and press Enter
2. The file listing should appear in the terminal pane
3. Type `git status` and press Enter — git status output should appear
4. Type `echo "hello world"` — the output should appear on the next line
5. Try a command with color output: `git log --oneline --color` — colors should render

**Test 3 — Special keys in terminal**:

1. Type a partial command, then press `Ctrl+C` — the command should be cancelled
   (shell prints `^C` and shows a new prompt)
2. Type `cat` and press Enter (starts reading stdin), then press `Ctrl+D` — cat should
   exit (EOF)
3. Press `Ctrl+L` — the terminal should clear
4. Use arrow keys to navigate command history (Up/Down)
5. Use `Ctrl+A` / `Ctrl+E` to move to start/end of line

**Test 4 — Escape to unfocus**:

1. With the terminal focused, press `Esc`
2. The terminal pane border should change from blue to gray (unfocused)
3. Navigation keys should work again: press `1` to focus Pane 1, press `j`/`k` to navigate
4. The terminal content should still be visible (just not focused)
5. The shell process is still running — it doesn't exit on unfocus

**Test 5 — Refocus terminal**:

1. After unfocusing (Test 4), press `` ` `` again
2. The terminal should regain focus (blue border)
3. The previous shell session should still be there (history, working directory preserved)
4. Type another command — it should work normally
5. The shell remembers its state across focus/unfocus cycles

**Test 6 — Close and reopen terminal**:

1. Press `` ` `` to close the terminal pane (hides it)
2. The diff viewer should expand to fill the space
3. Press `` ` `` again to reopen
4. The terminal should reappear with the same shell session intact
5. Previous command output should still be visible

**Test 7 — Terminal resize with Ctrl+Shift+Up/Down**:

1. Open and focus the terminal (press `` ` ``)
2. Press `Ctrl+Shift+Up` — the terminal pane should grow by 1 row
3. Press `Ctrl+Shift+Up` several more times — the terminal keeps growing
4. The diff viewer above should shrink correspondingly
5. Eventually the terminal stops growing (at 80% of content height)
6. Press `Ctrl+Shift+Down` — the terminal shrinks by 1 row
7. Press `Ctrl+Shift+Down` repeatedly — stops shrinking at 3 rows (minimum)
8. Commands in the shell should reflow correctly after resize (try `ls` in a narrow terminal)

**Test 8 — Window resize with terminal open**:

1. Open the terminal pane
2. Resize the entire terminal window (drag the window border)
3. The terminal pane should resize proportionally
4. The shell should receive the new dimensions (try running `tput cols; tput lines` before
   and after resize — the values should change)
5. Programs like `htop` or `vim` should redraw correctly after resize

**Test 9 — Full-screen program in terminal**:

1. Open the terminal and type `vim test.txt` (or any full-screen program)
2. Vim should render correctly in the terminal pane (status line, line numbers)
3. Type `i` to enter insert mode, type some text
4. Press `Esc` — **this unfocuses the terminal pane, not vim**. This is the expected
   behavior: `Esc` always returns to navigable mode.
5. Press `` ` `` to refocus, then `:q!` and Enter to quit vim
6. The shell prompt should reappear

**Test 10 — Terminal with tab switching**:

1. Open the terminal pane on the Changes tab
2. Press `Esc` to unfocus, then `Tab` to switch to the History tab
3. The terminal pane should still be visible at the bottom (it's shared across tabs)
4. Press `` ` `` to focus the terminal — it should work on the History tab too
5. Press `Esc`, then `Tab` back to Changes — terminal still there

**Test 11 — Quit with terminal running**:

1. Open the terminal and start a long-running command: `sleep 999`
2. Press `Esc` to unfocus, then `q` to quit
3. The app should exit cleanly — no zombie processes left behind
4. Verify: `ps aux | grep sleep` should not show the sleep process

**Test 12 — Terminal working directory**:

1. Open the terminal
2. Run `pwd` — it should show the repo path you passed to the app
3. Run `git status` — it should show the status of the correct repo
4. This confirms the shell started with `cmd.Dir` set to the repo path

**Phase 12 is complete when**: pressing `` ` `` opens a terminal pane with a live interactive
shell at the repo root; all keystrokes (including Ctrl+C, Ctrl+D, arrow keys) are forwarded
to the PTY while the terminal is focused; `Esc` unfocuses the terminal and returns to
navigable mode; `Ctrl+Shift+Up` / `Ctrl+Shift+Down` resize the terminal pane by one row
(clamped to 3–80% of height); the terminal pane persists across focus/unfocus and
open/close cycles without losing shell state; full-screen programs (vim, htop) render
correctly; window resize updates the PTY dimensions; the shell process is cleaned up on
quit; and the terminal is shared across Changes and History tabs.

## Phase 13 — Settings & Preferences

**Goal**: Add an in-app settings overlay where the user can view and modify configuration
values without manually editing the TOML file. Pressing `S` (uppercase) in navigable mode
opens a fullscreen settings view — a scrollable list of all configurable options, grouped by
section (Appearance, Diff, AI, Git, Confirmations, Repos). The user navigates with `j`/`k`,
toggles booleans with `space`, cycles through enum options with `space`, and edits
string/number values by pressing `Enter` to start typing. Changes are applied immediately
to the in-memory config and written to disk. `Esc` closes the settings view and returns to
the main layout.

This phase introduces:
1. **Config saving** — a `Save()` function in the config package that writes the current
   config struct back to the TOML file using `BurntSushi/toml`'s encoder
2. **Settings view** — a fullscreen overlay component with grouped, scrollable settings items
3. **Theme switching** — changing the `appearance.theme` setting applies a light/dark color
   palette to the render module, taking effect on the next `View()` cycle
4. **AI provider configuration** — editing Claude model name, Ollama server URL, timeouts,
   and diff size limits from within the app

After this phase, pressing `S` opens a settings panel where you can change any config value.
Changes persist to `config.toml` in the OS config directory and take effect immediately.

### 13.0 Config Save Function

**File**: `internal/config/config.go` (modify existing)

Phase 1 created the `Load()` function that reads and decodes the TOML config file. Now add
a `Save()` function that encodes the current config struct back to TOML and writes it to disk.
Also add a `Path()` function that returns the config file path (used by both `Load` and `Save`).

**How TOML encoding works with BurntSushi/toml**:

1. `toml.NewEncoder(writer)` creates an encoder that writes to any `io.Writer`
2. `encoder.Encode(cfg)` serializes the Go struct into TOML format, using the `toml` struct
   tags to determine key names
3. Go maps are sorted alphabetically, struct fields follow declaration order
4. Zero values are included in the output (e.g., `false`, `0`, `""`) — this is fine because
   we always write the full config, never a partial one

```go
// Path returns the config file path using the OS-appropriate directory
// (via configDir() defined in config.go — same package).
func Path() (string, error) {
	dir, err := configDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "config.toml"), nil
}

// Save writes the current config to the TOML file.
// Creates the parent directory if it doesn't exist.
func Save(cfg *Config) error {
	path, err := Path()
	if err != nil {
		return err
	}

	// Ensure the directory exists
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}

	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()

	encoder := toml.NewEncoder(f)
	return encoder.Encode(cfg)
}
```

**Update `Load()`** to use the shared `Path()` function. Note: `writeDefaultConfig` was
already defined in Phase 1's `config.go` — it stays as-is. The only change here is
replacing the inline `configDir()` call with `Path()`:

```go
func Load() (*Config, error) {
	cfg := newDefaultConfig()

	path, err := Path()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			// Create directory and default config
			dir := filepath.Dir(path)
			if mkErr := os.MkdirAll(dir, 0o755); mkErr != nil {
				return cfg, nil
			}
			if writeErr := writeDefaultConfig(path, cfg); writeErr != nil {
				return cfg, nil
			}
			return cfg, nil
		}
		return nil, err
	}

	if err := toml.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	return cfg, nil
}
```

**How `Save()` works**:

1. Gets the config file path using the shared `Path()` function.
2. Creates the config directory if it doesn't exist (`MkdirAll` is
   idempotent — it's a no-op if the directory already exists).
3. Creates (or truncates) the config file with `os.Create`.
4. Uses `toml.NewEncoder(f)` to create an encoder that writes directly to the file.
5. `encoder.Encode(cfg)` serializes the entire `Config` struct to TOML. The `toml` struct
   tags on each field control the key names in the output (e.g., `toml:"theme"` → `theme = "dark"`).

The output looks like:

```toml
[appearance]
  theme = "dark"

[diff]
  side_by_side = false
  hide_whitespace = false
  tab_size = 4
  context_lines = 3

[ai]
  [ai.claude]
    model = "haiku"
    timeout = 120
    max_diff_size = 20971520

  [ai.ollama]
    model = "tavernari/git-commit-message:latest"
    server_url = "http://localhost:11434"
    timeout = 120
    max_diff_size = 52428800

[git]
  fetch_interval = 300

[confirmations]
  discard_changes = true
  force_push = true
  branch_delete = true

[repos]
  mode = "folders"
  scan_paths = []
  scan_depth = 1
  manual_paths = []
```

### 13.1 In-App Settings View

**File**: `internal/tui/views/settings.go` (new file)

This file creates the settings overlay — a fullscreen view that lists all configurable
options grouped by section. Each item displays its key name, current value, and a brief
description. The view follows the same centering and boxing pattern as the help overlay.

**Settings item types**:

| Type | Input | Examples |
|------|-------|---------|
| `Toggle` | `space` flips between `true`/`false` | `side_by_side`, `hide_whitespace`, `discard_changes` |
| `Cycle` | `space` cycles through a list of options | `theme` (dark/light/system), `mode` (folders/manual) |
| `Number` | `Enter` starts editing, type digits, `Enter` confirms | `tab_size`, `timeout`, `fetch_interval` |
| `Text` | `Enter` starts editing, type text, `Enter` confirms | `model`, `server_url` |

**How the settings model works**:

1. On creation, it reads the current `*config.Config` and builds a flat list of `settingItem`
   structs — one per configurable field.
2. Items are grouped by section headers (Appearance, Diff, AI: Claude, AI: Ollama, etc.).
3. The user navigates with `j`/`k`. Section headers are rendered but skipped during navigation.
4. `space` toggles/cycles the current item's value immediately. The change is applied to the
   config struct and saved to disk.
5. `Enter` on a `Number` or `Text` item enters edit mode — a small inline text input appears.
   `Enter` confirms the edit, `Esc` cancels.
6. `Esc` (when not editing) closes the settings view entirely.

```go
package views

import (
	"fmt"
	"strconv"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
)

// SettingsClosedMsg is sent when the user presses Esc to close the settings view.
type SettingsClosedMsg struct{}

// SettingsChangedMsg is sent whenever a setting value is modified.
// The app uses this to react to changes that require immediate effect
// (e.g., theme switch, fetch interval change).
type SettingsChangedMsg struct {
	Key string // the setting key that changed (e.g., "appearance.theme")
}

// settingType defines how a setting item is edited.
type settingType int

// iota auto-assigns incrementing integers starting from 0.
// So settingToggle = 0, settingCycle = 1, settingNumber = 2, etc.
const (
	settingToggle settingType = iota // bool: space flips
	settingCycle                     // enum: space cycles through options
	settingNumber                    // int: Enter to edit, type digits
	settingText                      // string: Enter to edit, type text
	settingHeader                    // section header (not editable, not selectable)
)

// settingItem represents a single row in the settings list.
type settingItem struct {
	Type        settingType
	Key         string   // display name (e.g., "Theme")
	ConfigKey   string   // dotted config path (e.g., "appearance.theme")
	Description string   // short help text
	Options     []string // for Cycle type: list of valid values
	// Get and Set are function fields — each settingItem stores its own
	// getter/setter as a closure that knows which config field to access.
	// This avoids a giant switch statement: each item carries its own logic.
	Get         func(*config.Config) string
	Set         func(*config.Config, string)
}

// SettingsModel is the state for the settings overlay.
type SettingsModel struct {
	items      []settingItem
	cursor     int    // index of highlighted item (skips headers)
	offset     int    // scroll offset
	editing    bool   // true when typing into a Number/Text field
	editBuffer string // current text being typed
	width      int
	height     int
	cfg        *config.Config
}

// NewSettings creates a settings view from the current config.
func NewSettings(cfg *config.Config, width, height int) SettingsModel {
	m := SettingsModel{
		cfg:    cfg,
		width:  width,
		height: height,
	}
	m.items = buildSettingsItems()
	// Start cursor on first non-header item
	for i, item := range m.items {
		if item.Type != settingHeader {
			m.cursor = i
			break
		}
	}
	return m
}

// buildSettingsItems creates the flat list of all settings, grouped by section.
func buildSettingsItems() []settingItem {
	return []settingItem{
		// ── Appearance ──
		{Type: settingHeader, Key: "Appearance"},
		{
			Type: settingCycle, Key: "Theme", ConfigKey: "appearance.theme",
			Description: "Color scheme: dark, light, or follow system",
			Options:     []string{"dark", "light", "system"},
			Get:         func(c *config.Config) string { return c.Appearance.Theme },
			Set:         func(c *config.Config, v string) { c.Appearance.Theme = v },
		},

		// ── Diff Display ──
		{Type: settingHeader, Key: "Diff Display"},
		{
			Type: settingToggle, Key: "Side-by-Side", ConfigKey: "diff.side_by_side",
			Description: "Show diffs in split view instead of unified",
			Get:         func(c *config.Config) string { return fmt.Sprintf("%v", c.Diff.SideBySide) },
			Set: func(c *config.Config, v string) {
				c.Diff.SideBySide = v == "true"
			},
		},
		{
			Type: settingToggle, Key: "Hide Whitespace", ConfigKey: "diff.hide_whitespace",
			Description: "Ignore whitespace-only changes in diffs",
			Get:         func(c *config.Config) string { return fmt.Sprintf("%v", c.Diff.HideWhitespace) },
			Set: func(c *config.Config, v string) {
				c.Diff.HideWhitespace = v == "true"
			},
		},
		{
			Type: settingNumber, Key: "Tab Size", ConfigKey: "diff.tab_size",
			Description: "Tab width in the diff viewer (1-16)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.Diff.TabSize) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n >= 1 && n <= 16 {
					c.Diff.TabSize = n
				}
			},
		},
		{
			Type: settingNumber, Key: "Context Lines", ConfigKey: "diff.context_lines",
			Description: "Lines of context around diff changes (0-10)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.Diff.ContextLines) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n >= 0 && n <= 10 {
					c.Diff.ContextLines = n
				}
			},
		},

		// ── AI: Claude ──
		{Type: settingHeader, Key: "AI: Claude CLI"},
		{
			Type: settingText, Key: "Model", ConfigKey: "ai.claude.model",
			Description: "Claude model name (e.g., haiku, sonnet, opus)",
			Get:         func(c *config.Config) string { return c.AI.Claude.Model },
			Set:         func(c *config.Config, v string) { c.AI.Claude.Model = v },
		},
		{
			Type: settingNumber, Key: "Timeout", ConfigKey: "ai.claude.timeout",
			Description: "Request timeout in seconds (10-600)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.AI.Claude.Timeout) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n >= 10 && n <= 600 {
					c.AI.Claude.Timeout = n
				}
			},
		},
		{
			Type: settingNumber, Key: "Max Diff Size", ConfigKey: "ai.claude.max_diff_size",
			Description: "Maximum diff size in bytes for AI generation",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.AI.Claude.MaxDiffSize) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n > 0 {
					c.AI.Claude.MaxDiffSize = n
				}
			},
		},

		// ── AI: Ollama ──
		{Type: settingHeader, Key: "AI: Ollama"},
		{
			Type: settingText, Key: "Model", ConfigKey: "ai.ollama.model",
			Description: "Ollama model name (e.g., tavernari/git-commit-message:latest)",
			Get:         func(c *config.Config) string { return c.AI.Ollama.Model },
			Set:         func(c *config.Config, v string) { c.AI.Ollama.Model = v },
		},
		{
			Type: settingText, Key: "Server URL", ConfigKey: "ai.ollama.server_url",
			Description: "Ollama server address (e.g., http://localhost:11434)",
			Get:         func(c *config.Config) string { return c.AI.Ollama.ServerURL },
			Set:         func(c *config.Config, v string) { c.AI.Ollama.ServerURL = v },
		},
		{
			Type: settingNumber, Key: "Timeout", ConfigKey: "ai.ollama.timeout",
			Description: "Request timeout in seconds (10-600)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.AI.Ollama.Timeout) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n >= 10 && n <= 600 {
					c.AI.Ollama.Timeout = n
				}
			},
		},
		{
			Type: settingNumber, Key: "Max Diff Size", ConfigKey: "ai.ollama.max_diff_size",
			Description: "Maximum diff size in bytes for AI generation",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.AI.Ollama.MaxDiffSize) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n > 0 {
					c.AI.Ollama.MaxDiffSize = n
				}
			},
		},

		// ── Git Behavior ──
		{Type: settingHeader, Key: "Git"},
		{
			Type: settingNumber, Key: "Fetch Interval", ConfigKey: "git.fetch_interval",
			Description: "Auto-fetch every N seconds (0 = disabled, min 30)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.Git.FetchInterval) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil {
					if n == 0 || n >= 30 {
						c.Git.FetchInterval = n
					}
				}
			},
		},

		// ── Confirmations ──
		{Type: settingHeader, Key: "Confirmations"},
		{
			Type: settingToggle, Key: "Discard Changes", ConfigKey: "confirmations.discard_changes",
			Description: "Ask before discarding unstaged changes",
			Get:         func(c *config.Config) string { return fmt.Sprintf("%v", c.Confirmations.DiscardChanges) },
			Set:         func(c *config.Config, v string) { c.Confirmations.DiscardChanges = v == "true" },
		},
		{
			Type: settingToggle, Key: "Force Push", ConfigKey: "confirmations.force_push",
			Description: "Ask before force-pushing to remote",
			Get:         func(c *config.Config) string { return fmt.Sprintf("%v", c.Confirmations.ForcePush) },
			Set:         func(c *config.Config, v string) { c.Confirmations.ForcePush = v == "true" },
		},
		{
			Type: settingToggle, Key: "Branch Delete", ConfigKey: "confirmations.branch_delete",
			Description: "Ask before deleting a branch",
			Get:         func(c *config.Config) string { return fmt.Sprintf("%v", c.Confirmations.BranchDelete) },
			Set:         func(c *config.Config, v string) { c.Confirmations.BranchDelete = v == "true" },
		},

		// ── Repository Discovery ──
		{Type: settingHeader, Key: "Repository Discovery"},
		{
			Type: settingCycle, Key: "Discovery Mode", ConfigKey: "repos.mode",
			Description: "How repos are found: scan folders or use a manual list",
			Options:     []string{"folders", "manual"},
			Get:         func(c *config.Config) string { return c.Repos.Mode },
			Set:         func(c *config.Config, v string) { c.Repos.Mode = v },
		},
		{
			Type: settingNumber, Key: "Scan Depth", ConfigKey: "repos.scan_depth",
			Description: "How many directory levels deep to search for .git (1-5)",
			Get:         func(c *config.Config) string { return strconv.Itoa(c.Repos.ScanDepth) },
			Set: func(c *config.Config, v string) {
				if n, err := strconv.Atoi(v); err == nil && n >= 1 && n <= 5 {
					c.Repos.ScanDepth = n
				}
			},
		},
	}
}
```

**How `settingItem` works**:

- **`Get` function**: Reads the current value from the config struct and returns it as a
  string for display. Called every render cycle.
- **`Set` function**: Writes a new string value into the config struct. Includes validation
  (e.g., `tab_size` clamped to 1-16, `timeout` clamped to 10-600). Called when the user
  changes a value.
- **`ConfigKey`**: The dotted path (e.g., `"diff.tab_size"`) used in `SettingsChangedMsg` so
  the app can identify which setting changed and react accordingly.
- **`Options`**: Only used by `settingCycle` type — the list of valid values to rotate through.

**Note**: `scan_paths` and `manual_paths` (string arrays) are not included in the settings
view because editing arrays inline is awkward in a TUI. Users edit these in the TOML file
directly. The settings view covers scalar values only.

Now add the `Update()` and `View()` methods:

```go
// Update handles key events in the settings overlay.
func (m SettingsModel) Update(msg tea.Msg) (SettingsModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		if m.editing {
			return m.handleEditKey(msg)
		}
		return m.handleNavigateKey(msg)
	}
	return m, nil
}

// handleNavigateKey processes keys when browsing the settings list.
func (m SettingsModel) handleNavigateKey(msg tea.KeyPressMsg) (SettingsModel, tea.Cmd) {
	switch msg.String() {
	case "escape", "S":
		// Close settings view
		return m, func() tea.Msg { return SettingsClosedMsg{} }

	case "j", "down":
		m.moveDown()
		return m, nil

	case "k", "up":
		m.moveUp()
		return m, nil

	case " ":
		// Toggle or cycle the current item
		item := &m.items[m.cursor]
		switch item.Type {
		case settingToggle:
			current := item.Get(m.cfg)
			if current == "true" {
				item.Set(m.cfg, "false")
			} else {
				item.Set(m.cfg, "true")
			}
			// _ = discards the error. A failed save is not fatal — the
			// in-memory config still holds the new value.
			_ = config.Save(m.cfg)
			return m, func() tea.Msg {
				return SettingsChangedMsg{Key: item.ConfigKey}
			}

		case settingCycle:
			current := item.Get(m.cfg)
			next := cycleOption(item.Options, current)
			item.Set(m.cfg, next)
			_ = config.Save(m.cfg)
			return m, func() tea.Msg {
				return SettingsChangedMsg{Key: item.ConfigKey}
			}
		}
		return m, nil

	case "enter":
		// Start editing Number or Text items
		item := m.items[m.cursor]
		if item.Type == settingNumber || item.Type == settingText {
			m.editing = true
			m.editBuffer = item.Get(m.cfg) // pre-fill with current value
		}
		return m, nil
	}

	return m, nil
}

// handleEditKey processes keys while editing a Number or Text field.
func (m SettingsModel) handleEditKey(msg tea.KeyPressMsg) (SettingsModel, tea.Cmd) {
	switch msg.String() {
	case "escape":
		// Cancel editing — discard changes
		m.editing = false
		m.editBuffer = ""
		return m, nil

	case "enter":
		// Confirm editing — apply the value.
		// If the buffer is empty (user deleted all text), skip Set/Save
		// to avoid writing an empty string — the config value stays unchanged.
		item := &m.items[m.cursor]
		if m.editBuffer != "" {
			item.Set(m.cfg, m.editBuffer)
			_ = config.Save(m.cfg)
		}
		m.editing = false
		m.editBuffer = ""
		return m, func() tea.Msg {
			return SettingsChangedMsg{Key: item.ConfigKey}
		}

	case "backspace":
		if len(m.editBuffer) > 0 {
			m.editBuffer = m.editBuffer[:len(m.editBuffer)-1]
		}
		return m, nil

	default:
		// Append typed character (single runes only)
		key := msg.String()
		if len(key) == 1 {
			m.editBuffer += key
		}
		return m, nil
	}
}

// moveDown advances the cursor to the next non-header item.
func (m *SettingsModel) moveDown() {
	for i := m.cursor + 1; i < len(m.items); i++ {
		if m.items[i].Type != settingHeader {
			m.cursor = i
			m.clampOffset()
			return
		}
	}
}

// moveUp moves the cursor to the previous non-header item.
func (m *SettingsModel) moveUp() {
	for i := m.cursor - 1; i >= 0; i-- {
		if m.items[i].Type != settingHeader {
			m.cursor = i
			m.clampOffset()
			return
		}
	}
}

// clampOffset keeps the cursor visible within the scrollable viewport.
func (m *SettingsModel) clampOffset() {
	visible := m.height - 8 // subtract box padding, title, hints
	if visible < 5 {
		visible = 5
	}
	if m.cursor >= m.offset+visible {
		m.offset = m.cursor - visible + 1
	}
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

// cycleOption returns the next option in the list, wrapping around.
func cycleOption(options []string, current string) string {
	for i, opt := range options {
		if opt == current {
			// % is modulo — wraps to 0 when i+1 equals len(options).
			// E.g. options=["dark","light","system"], current="system" (i=2):
			//   (2+1) % 3 = 0 → returns "dark"
			return options[(i+1)%len(options)]
		}
	}
	if len(options) > 0 {
		return options[0]
	}
	return current
}
```

**How navigation works**:

1. `moveDown()` / `moveUp()` skip items with `Type == settingHeader` — headers are displayed
   but can't be selected. This makes the cursor jump between editable items only.
2. `clampOffset()` adjusts the scroll offset to keep the cursor visible, similar to how the
   file list scrolling works in Phase 6.
3. `space` on a `settingToggle` flips between "true" and "false". On a `settingCycle`, it
   rotates to the next option using `cycleOption()`.
4. `Enter` on a `settingNumber` or `settingText` starts inline editing. The edit buffer is
   pre-filled with the current value so the user can modify it rather than retype.
5. While editing, `Enter` confirms (applies the value and saves), `Esc` cancels (discards
   the buffer). `backspace` deletes the last character. Any single-character key appends.
6. After every change, `config.Save(m.cfg)` writes to disk and a `SettingsChangedMsg` is
   emitted so the app can react (e.g., restart the fetch timer if `fetch_interval` changed).
7. **`cycleOption` uses the `%` (modulo) operator** to wrap back to index 0 when you reach
   the last option. For example, with `["dark", "light", "system"]` and `current = "system"`
   (index 2): `(2+1) % 3 = 0`, returning `"dark"`. The fallback `if len(options) > 0`
   handles the case where the current config value is not in the options list (e.g., the
   user manually typed an invalid value in the TOML file).

Now add the `View()` method:

```go
// View renders the settings overlay as a centered box with scrollable content.
func (m SettingsModel) View() string {
	// ── Styles ──
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	sectionStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#D2A8FF")).
		MarginTop(1)

	keyStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Width(18)

	valueStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Width(36)

	descStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Italic(true)

	cursorStyle := lipgloss.NewStyle().
		Background(lipgloss.Color("#264F78"))

	editStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FF7B72")).
		Bold(true)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	// ── Build rows ──
	visible := m.height - 8
	if visible < 5 {
		visible = 5
	}
	end := m.offset + visible
	if end > len(m.items) {
		end = len(m.items)
	}

	var rows []string
	for i := m.offset; i < end; i++ {
		item := m.items[i]

		if item.Type == settingHeader {
			rows = append(rows, sectionStyle.Render("── "+item.Key+" ──"))
			continue
		}

		// Current value display
		val := item.Get(m.cfg)
		valDisplay := valueStyle.Render(val)

		// If editing this item, show the edit buffer with a cursor
		if m.editing && i == m.cursor {
			valDisplay = editStyle.Render(m.editBuffer + "█")
		}

		// Type hint suffix
		hint := ""
		switch item.Type {
		case settingToggle:
			hint = " [space: toggle]"
		case settingCycle:
			hint = " [space: cycle]"
		case settingNumber, settingText:
			hint = " [enter: edit]"
		}

		line := keyStyle.Render(item.Key) + valDisplay +
			descStyle.Render("  "+item.Description+hint)

		if i == m.cursor && !m.editing {
			// Highlight the row
			line = cursorStyle.Render(line)
		}

		rows = append(rows, line)
	}

	content := strings.Join(rows, "\n")

	// ── Box ──
	header := titleStyle.Render("Settings")
	hints := hintStyle.Render("j/k: navigate • space: toggle/cycle • enter: edit • Esc: close")

	boxContent := header + "\n\n" + content + "\n\n" + hints

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Width(min(90, m.width-4)).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
```

**How rendering works**:

1. The view calculates how many rows fit in the viewport (`height - 8` for box padding,
   title, hints).
2. Iterates from `m.offset` to `m.offset + visible`, rendering each item as a row.
3. Section headers render with a purple color and `──` separators.
4. Editable items show: key name (bold white, 18-char wide) + value (green, 36-char wide) +
   description (gray italic with type hint).
5. The cursor row gets a blue highlight background (same as the file list cursor).
6. When editing, the value column shows the edit buffer in red with a block cursor (`█`).
7. The entire list is wrapped in a bordered box centered on screen, capped at 90 columns wide.

### 13.2 Theme / Appearance Options

**File**: `internal/tui/render/theme.go` (new file)

This file defines the color palette for light and dark themes. The app stores the active
theme name in the config (`appearance.theme`). When the theme changes via the settings view,
the palette is recalculated and the next `View()` cycle picks up the new colors.

The theme system is simple — a `Theme` struct holds all the colors used across the app,
and a `CurrentTheme()` function returns the active palette based on the config value.

```go
package render

import (
	"os"

	"charm.land/lipgloss/v2"
)

// Theme holds all colors used across the application.
// Components read from this struct instead of hardcoding colors.
type Theme struct {
	// Backgrounds
	HeaderBg  lipgloss.Color
	TabBarBg  lipgloss.Color
	PaneBg    lipgloss.Color

	// Borders
	BorderActive   lipgloss.Color
	BorderInactive lipgloss.Color

	// Text
	TextPrimary   lipgloss.Color
	TextSecondary lipgloss.Color
	TextMuted     lipgloss.Color

	// Status colors (shared across themes)
	StatusGreen  lipgloss.Color
	StatusRed    lipgloss.Color
	StatusYellow lipgloss.Color
	StatusBlue   lipgloss.Color

	// Cursor / selection
	CursorBg lipgloss.Color
	CursorFg lipgloss.Color

	// Diff colors
	DiffAddBg    lipgloss.Color
	DiffRemoveBg lipgloss.Color
	DiffAddFg    lipgloss.Color
	DiffRemoveFg lipgloss.Color
}

// DarkTheme returns the default dark color palette (GitHub Dark-inspired).
func DarkTheme() Theme {
	return Theme{
		HeaderBg:       lipgloss.Color("#1E1E1E"),
		TabBarBg:       lipgloss.Color("#161B22"),
		PaneBg:         lipgloss.Color("#0D1117"),
		BorderActive:   lipgloss.Color("#58A6FF"),
		BorderInactive: lipgloss.Color("#484F58"),
		TextPrimary:    lipgloss.Color("#FFFFFF"),
		TextSecondary:  lipgloss.Color("#8B949E"),
		TextMuted:      lipgloss.Color("#484F58"),
		StatusGreen:    lipgloss.Color("#3FB950"),
		StatusRed:      lipgloss.Color("#F85149"),
		StatusYellow:   lipgloss.Color("#D29922"),
		StatusBlue:     lipgloss.Color("#58A6FF"),
		CursorBg:       lipgloss.Color("#264F78"),
		CursorFg:       lipgloss.Color("#FFFFFF"),
		DiffAddBg:      lipgloss.Color("#1B3A2A"),
		DiffRemoveBg:   lipgloss.Color("#3A1B1B"),
		DiffAddFg:      lipgloss.Color("#3FB950"),
		DiffRemoveFg:   lipgloss.Color("#F85149"),
	}
}

// LightTheme returns a light color palette (GitHub Light-inspired).
func LightTheme() Theme {
	return Theme{
		HeaderBg:       lipgloss.Color("#F6F8FA"),
		TabBarBg:       lipgloss.Color("#FFFFFF"),
		PaneBg:         lipgloss.Color("#FFFFFF"),
		BorderActive:   lipgloss.Color("#0969DA"),
		BorderInactive: lipgloss.Color("#D0D7DE"),
		TextPrimary:    lipgloss.Color("#24292F"),
		TextSecondary:  lipgloss.Color("#57606A"),
		TextMuted:      lipgloss.Color("#8C959F"),
		StatusGreen:    lipgloss.Color("#1A7F37"),
		StatusRed:      lipgloss.Color("#CF222E"),
		StatusYellow:   lipgloss.Color("#9A6700"),
		StatusBlue:     lipgloss.Color("#0969DA"),
		CursorBg:       lipgloss.Color("#DDF4FF"),
		CursorFg:       lipgloss.Color("#24292F"),
		DiffAddBg:      lipgloss.Color("#DAFBE1"),
		DiffRemoveBg:   lipgloss.Color("#FFEBE9"),
		DiffAddFg:      lipgloss.Color("#1A7F37"),
		DiffRemoveFg:   lipgloss.Color("#CF222E"),
	}
}

// CurrentTheme returns the appropriate theme based on the config value.
// "system" uses the COLORFGBG environment variable as a heuristic:
// if the background component is < 8, the terminal is dark.
func CurrentTheme(themeName string) Theme {
	switch themeName {
	case "light":
		return LightTheme()
	case "system":
		if isSystemDark() {
			return DarkTheme()
		}
		return LightTheme()
	default:
		return DarkTheme()
	}
}

// isSystemDark checks the COLORFGBG environment variable to guess the terminal
// background darkness. Format is "fg;bg" where bg < 8 typically means dark.
// Falls back to dark (true) if the variable isn't set or can't be parsed.
func isSystemDark() bool {
	val := os.Getenv("COLORFGBG")
	if val == "" {
		return true // default to dark when unknown
	}
	parts := splitLast(val, ";")
	if len(parts) != 2 {
		return true
	}
	// ANSI color 0-7 are dark, 8-15 are bright
	bg := parts[1]
	return bg == "" || bg == "0" || bg == "1" || bg == "2" || bg == "3" ||
		bg == "4" || bg == "5" || bg == "6" || bg == "7"
}

// splitLast splits a string on the last occurrence of sep.
func splitLast(s, sep string) []string {
	i := len(s) - 1
	for i >= 0 {
		if string(s[i]) == sep {
			return []string{s[:i], s[i+1:]}
		}
		i--
	}
	return []string{s}
}
```

**How theme switching works**:

1. `CurrentTheme()` maps the config string (`"dark"`, `"light"`, `"system"`) to a `Theme`
   struct. The `"system"` option uses the `COLORFGBG` environment variable — a common
   convention where terminals advertise their foreground and background colors.

2. The app stores the active theme in the `Model` struct (added in section 13.3 below).
   On startup, it calls `render.CurrentTheme(cfg.Appearance.Theme)` to initialize.

3. When the user changes the theme in settings, the `SettingsChangedMsg` with
   `Key == "appearance.theme"` triggers a recalculation: the app calls
   `render.CurrentTheme()` again with the new value and stores the result.

4. All rendering functions (`renderPane`, `RenderHeader`, `RenderTabBar`, file list, diff
   viewer, etc.) should read colors from the `Theme` struct rather than hardcoding hex values.
   **However**, migrating all existing rendering code to use the theme struct is a large
   refactor that would obscure the Phase 13 changes. Instead, this phase:
   - Creates the `Theme` struct and both palettes as the **foundation**
   - Wires theme selection into the settings view
   - Passes the theme to the `renderPane` function as a demonstration
   - Future phases gradually migrate other components to use `theme.X` instead of hardcoded colors

**Update `renderPane()`** in `internal/tui/app/app.go` to accept a theme:

```go
// renderPaneThemed draws a bordered box using theme colors.
// This replaces the hardcoded colors in renderPane().
func renderPaneThemed(title, content string, width, height int, focused bool, theme render.Theme) string {
	borderColor := theme.BorderInactive
	titleColor := theme.TextSecondary
	if focused {
		borderColor = theme.BorderActive
		titleColor = theme.BorderActive
	}

	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}
```

The old `renderPane()` function can remain as a backward-compatible wrapper that uses the
dark theme defaults. Components that have been migrated call `renderPaneThemed()` instead.

### 13.3 AI Provider Configuration

The AI provider settings (Claude model, Ollama server URL, timeouts, max diff sizes) are
already included in the settings item list from section 13.1. This section describes how
changes to AI settings take effect.

**File**: `internal/tui/app/app.go` (modify existing)

**New fields** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Settings & theme
	showSettings bool                    // true when the settings overlay is visible
	settings     views.SettingsModel     // settings overlay state
	theme        render.Theme            // active color palette
}
```

**Update `New()`** — initialize the theme:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		// ... existing fields ...

		// Theme
		theme: render.CurrentTheme(cfg.Appearance.Theme),
	}
}
```

**Add the `S` keybinding** to `handleMainKey()` in navigable mode:

```go
	case "S":
		// Open settings overlay
		m.showSettings = true
		m.settings = views.NewSettings(m.config, m.width, m.height)
		return m, nil
```

**Handle settings messages** in `Update()` — add these cases to the main `switch msg.(type)`:

```go
	case views.SettingsClosedMsg:
		m.showSettings = false
		return m, nil

	case views.SettingsChangedMsg:
		// React to specific setting changes
		switch msg.Key {
		case "appearance.theme":
			m.theme = render.CurrentTheme(m.config.Appearance.Theme)
		case "git.fetch_interval":
			// The next tick will pick up the new interval automatically
			// because startTickCmd reads from m.config.Git.FetchInterval.
			// No action needed here — the timer restarts on its own.
		}
		return m, nil
```

**Forward keys to settings** in `handleKey()` — when the settings overlay is visible, all
keys go to the settings model instead of the main key handler:

```go
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	if msg.String() == "ctrl+c" {
		m.terminal.Close()
		m.quitting = true
		return m, tea.Quit
	}

	// Settings overlay intercepts all keys when visible
	if m.showSettings {
		var cmd tea.Cmd
		m.settings, cmd = m.settings.Update(msg)
		return m, cmd
	}

	switch m.state {
	// ... existing state handling ...
	}

	return m, nil
}
```

**Update `View()`** — render the settings overlay when visible:

```go
	case stateMain:
		if m.showSettings {
			content = m.settings.View()
		} else if m.errorModal.Visible {
			content = m.errorModal.View()
		} else if m.showHelp {
			content = views.RenderHelpOverlay(m.width, m.height)
		} else {
			content = m.viewMain()
		}
```

**Forward `WindowSizeMsg` to settings** when visible:

```go
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		if m.showSettings {
			m.settings, _ = m.settings.Update(msg)
		}
		// ... existing window size handling ...
```

**Update the help overlay** — add the settings keybinding to the help text. Find the help
rows in `views/helpoverlay.go` and add:

```go
	// Settings section (add to the help rows)
	row("S", "Open settings"),
```

**How AI settings take effect**:

1. The AI providers (`ClaudeProvider`, `OllamaProvider`) are created from `m.config` when the
   user triggers AI commit message generation (Phase 9). They read `m.config.AI.Claude.Model`,
   `m.config.AI.Ollama.ServerURL`, etc. at creation time.
2. When the user changes an AI setting in the settings view, the value is written to
   `m.config` immediately (the config is a `*config.Config` pointer, so both the app and settings view reference the same struct in memory — a change in one is instantly visible to the other).
3. The next time the user triggers AI generation, the provider is created with the updated
   values. No restart needed.
4. The `config.Save()` call persists the change to disk, so the new values survive app restarts.

**What changed from Phase 12**:

1. **New `Save()` and `Path()` in config package**: enables writing config changes to disk
2. **New `internal/tui/views/settings.go`**: fullscreen settings overlay with scrollable
   grouped settings, toggle/cycle/edit interactions, and inline editing
3. **New `internal/tui/render/theme.go`**: `Theme` struct with dark and light palettes,
   `CurrentTheme()` function with system detection via `COLORFGBG`
4. **New `renderPaneThemed()` in app.go**: theme-aware version of `renderPane()`
5. **New model fields**: `showSettings`, `settings`, `theme`
6. **`New()` updated**: initializes `theme` from config
7. **`handleKey()` updated**: settings overlay intercepts keys when visible
8. **`handleMainKey()` updated**: `S` opens the settings overlay
9. **`Update()` updated**: handles `SettingsClosedMsg` and `SettingsChangedMsg`
10. **`View()` updated**: renders settings overlay when `showSettings` is true
11. **Help overlay updated**: new `S` keybinding listed

### 13.4 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

> **Cross-platform note**: The `cat`/`rm` commands below use Linux paths (`~/.config/leogit/`).
> On **macOS**, replace with `~/Library/Application\ Support/leogit/`.
> On **Windows**, use `%APPDATA%\leogit\`.

**Test 1 — Open and close settings**:

1. Press `S` (uppercase) in navigable mode
2. A settings overlay should appear centered on screen with a blue border
3. You should see grouped sections: Appearance, Diff Display, AI: Claude CLI, AI: Ollama,
   Git, Confirmations, Repository Discovery
4. The first editable item (Theme) should be highlighted with a blue background
5. Press `Esc` — the settings overlay closes and the main layout returns

**Test 2 — Navigate settings**:

1. Open settings with `S`
2. Press `j` — the cursor moves to the next editable item (Side-by-Side)
3. Press `j` several more times — the cursor skips section headers and moves through
   Hide Whitespace, Tab Size, Context Lines, etc.
4. Press `k` — the cursor moves up, again skipping headers
5. If the list is long enough, it should scroll when the cursor reaches the bottom/top of
   the visible area

**Test 3 — Toggle a boolean setting**:

1. Navigate to "Side-by-Side" (under Diff Display)
2. The current value should show "false" in green
3. Press `space` — the value should flip to "true"
4. Press `space` again — back to "false"
5. Verify the change was saved:
   ```bash
   cat ~/.config/leogit/config.toml | grep side_by_side
   ```
   The value should match what you set in the TUI

**Test 4 — Cycle an enum setting**:

1. Navigate to "Theme" (first editable item)
2. The current value should show "dark"
3. Press `space` — it should cycle to "light"
4. Press `space` — it should cycle to "system"
5. Press `space` — it wraps back to "dark"
6. Verify by checking the config file after each change

**Test 5 — Edit a number setting**:

1. Navigate to "Tab Size" (under Diff Display)
2. The current value should show "4"
3. Press `Enter` — the value should change to an editable field showing "4█" (with a
   block cursor in red)
4. Press `backspace` to delete the "4"
5. Type "8"
6. Press `Enter` to confirm — the value should update to "8"
7. Verify: `cat ~/.config/leogit/config.toml | grep tab_size`

**Test 6 — Edit a text setting**:

1. Navigate to "Model" under "AI: Claude CLI"
2. The current value should show "haiku"
3. Press `Enter` to start editing
4. Clear the value with multiple `backspace` presses
5. Type "sonnet"
6. Press `Enter` to confirm
7. Close settings with `Esc`, then press the AI button (Phase 9 flow) — the Claude
   provider should now use "sonnet" instead of "haiku"
8. Verify: `cat ~/.config/leogit/config.toml | grep model`

**Test 7 — Cancel editing with Esc**:

1. Navigate to "Fetch Interval" (under Git)
2. Press `Enter` to start editing
3. Clear and type "999"
4. Press `Esc` instead of `Enter` — the edit should be cancelled
5. The value should revert to its previous value (e.g., "300")
6. The config file should not have changed

**Test 8 — Theme switching takes effect**:

1. Open settings with `S`
2. Navigate to Theme, press `space` to switch to "light"
3. Close settings with `Esc`
4. The `renderPaneThemed()` borders should use light theme colors (if panes use the
   themed renderer). The theme struct is stored in `m.theme` and available for
   all rendering functions.
5. Switch back to "dark" — colors should revert

**Test 9 — Number validation**:

1. Navigate to "Tab Size"
2. Press `Enter`, clear, type "99"
3. Press `Enter` — the value should NOT change (the `Set` function clamps to 1-16)
4. The value remains at the previous valid number

**Test 10 — Config file re-creation from scratch**:

1. Delete the existing config file (use the path for your OS):
   ```bash
   rm ~/.config/leogit/config.toml        # Linux
   rm ~/Library/Application\ Support/leogit/config.toml  # macOS
   ```
2. Restart the app — `Load()` should auto-create a fresh default config file
3. Press `S` — settings should show default values
4. Change any setting (e.g., toggle Side-by-Side to true)
5. Verify the config file was updated with the new value

**Test 11 — Settings persist across restarts**:

1. Change several settings (theme to light, tab size to 8, Claude model to sonnet)
2. Quit the app with `q`
3. Restart the app
4. Press `S` — all changed values should be preserved
5. The theme should also be applied from startup (light theme if that's what was set)

**Test 12 — Settings overlay blocks main keys**:

1. Open settings with `S`
2. Press `q` — the app should NOT quit (settings overlay intercepts the key)
3. Press `Tab` — no tab switching should occur
4. Press `1`, `2`, `3` — no pane switching
5. Only `Esc` closes the settings overlay
6. Press `Esc` — now `q` should quit as normal

**Phase 13 is complete when**: pressing `S` opens a scrollable settings overlay showing all
configurable options grouped by section; `j`/`k` navigates between items (skipping section
headers); `space` toggles booleans and cycles through enum options; `Enter` starts inline
editing for numbers and text; `Esc` cancels editing or closes the overlay; all changes are
written to `config.toml` (in the OS config directory) immediately; theme switching updates the
`Theme` struct for rendering; AI provider settings take effect on next generation; number
inputs are validated with min/max constraints; the settings overlay blocks all global keys
while visible; and changes persist across app restarts.

## Phase 14 — Fetch & Pull

**Goal**: Add background auto-fetch on a configurable timer, manual fetch and pull triggered by
keyboard shortcuts, and merge conflict detection after pull operations. The header's action
button already displays the correct label (Fetch / Pull / Push) based on ahead/behind state
(Phase 5). This phase makes `F` (fetch) and `P` (pull) actually do something.

> **Terminology recap for beginners**:
> - **Fetch**: downloads new commits from the remote but does NOT change your local files or branch. It only updates your knowledge of what the remote has.
> - **Pull**: does a fetch AND then merges the remote changes into your current branch (this changes your local files).
> - **Ahead/Behind**: "ahead by 2" means you have 2 local commits not on the remote; "behind by 3" means the remote has 3 commits you do not have locally.

This phase introduces:
1. **Git remote commands** — `Fetch()`, `Pull()`, and `GetAheadBehind()` wrappers in the git package
2. **Background auto-fetch** — a goroutine-based timer that fetches silently on the configured
   `fetch_interval` (default: 300 seconds) and notifies the user when ahead/behind counts change
3. **Manual fetch/pull** — `F` key triggers a fetch, `P` key triggers a pull, both run
   asynchronously with a spinner in the header
4. **Merge conflict detection** — after a pull, the status refresh detects conflicted files
   (`UU` status code) and shows a modal listing them

After this phase, the app fetches from the remote automatically, the user can manually fetch
with `F` or pull with `P`, and merge conflicts are detected and surfaced in a modal.

### 14.0 Git Remote Commands

**File**: `internal/git/remote.go` (new file)

This file wraps `git fetch`, `git pull`, and `git rev-list --left-right --count` into
Go functions. All commands run with `TERM=dumb` to suppress color output and pager behavior.

**How `git fetch` works**:

```bash
git fetch --prune --recurse-submodules=on-demand <remote>
```

- `--prune` removes remote-tracking references that no longer exist on the remote
- `--recurse-submodules=on-demand` fetches submodule changes only when the superproject
  references a new submodule commit
- Progress output goes to stderr (not stdout). We capture stderr for error reporting but
  don't parse progress — the TUI shows a spinner instead of a progress bar.
- We omit `--progress` because it's only useful when piping to a terminal. Since we capture
  output as bytes, progress lines would just add noise.

**How `git pull` works**:

```bash
git pull --ff --recurse-submodules <remote>
```

- `--ff` allows fast-forward merges (the default, but explicit for clarity)
- `--recurse-submodules` updates submodules after the pull
- If the pull results in a merge conflict, `git pull` exits with a non-zero code AND the
  working tree contains conflicted files. We detect this in Phase 14.3.

**How `git rev-list --left-right --count` works**:

```bash
git rev-list --left-right --count HEAD...<upstream> --
```

This counts commits that are reachable from one side but not the other:
- Left count (`HEAD` side) = commits ahead of upstream
- Right count (`upstream` side) = commits behind upstream
- The `...` (three dots) is the symmetric difference operator
- The `--` at the end prevents ambiguity with path names

This is used after a fetch to detect ahead/behind changes independently of `git status`,
because `git status` reads the local refs which are already updated by the fetch.

> **Note for beginners**: A "remote" is a named reference to a remote repository (e.g.,
> `origin` points to `https://github.com/user/repo.git`). Most repos cloned from GitHub
> have exactly one remote called `origin`.

**How `GetRemote()` works**:

```bash
git remote
```

Returns the list of configured remotes, one per line. We take the first one (usually
`origin`). If no remote is configured, fetch/pull operations are silently skipped.

```go
package git

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

// Fetch runs `git fetch --prune` for the given remote.
// Returns nil on success, error on failure.
// Progress is not captured — the TUI shows a spinner while this runs.
func Fetch(repoPath, remote string) error {
	cmd := exec.Command("git", "fetch",
		"--prune",
		"--recurse-submodules=on-demand",
		remote,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git fetch failed: %w\n%s", err, string(output))
	}
	return nil
}

// Pull runs `git pull --ff` for the given remote.
// Returns nil on success. On merge conflict, returns an error whose message
// contains "CONFLICT" — the caller checks for this to trigger conflict detection.
func Pull(repoPath, remote string) error {
	cmd := exec.Command("git", "pull",
		"--ff",
		"--recurse-submodules",
		remote,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		msg := string(output)
		return fmt.Errorf("git pull failed: %w\n%s", err, msg)
	}
	return nil
}

// GetAheadBehind runs `git rev-list --left-right --count HEAD...<upstream>`
// and returns the ahead and behind counts.
// Returns (0, 0, nil) if upstream is empty or the command fails.
func GetAheadBehind(repoPath, upstream string) (ahead, behind int, err error) {
	if upstream == "" {
		return 0, 0, nil
	}

	cmd := exec.Command("git", "rev-list",
		"--left-right",
		"--count",
		fmt.Sprintf("HEAD...%s", upstream),
		"--",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return 0, 0, fmt.Errorf("git rev-list failed: %w", err)
	}

	// Output format: "<ahead>\t<behind>\n"
	parts := strings.Fields(strings.TrimSpace(string(out)))
	if len(parts) != 2 {
		return 0, 0, fmt.Errorf("unexpected rev-list output: %q", string(out))
	}

	ahead, _ = strconv.Atoi(parts[0])
	behind, _ = strconv.Atoi(parts[1])
	return ahead, behind, nil
}

// GetRemote returns the name of the first configured remote (usually "origin").
// Returns an empty string if no remote is configured.
func GetRemote(repoPath string) string {
	cmd := exec.Command("git", "remote")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return ""
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) == 0 || lines[0] == "" {
		return ""
	}
	return lines[0]
}
```

**How `Fetch()` and `Pull()` handle errors**:

1. Both use `cmd.CombinedOutput()` to capture stdout + stderr together. This is simpler than
   separating them and gives us the full error context in one string.
2. If the command exits non-zero, the Go `exec` package wraps the exit code in an `*exec.ExitError`.
   We wrap that with `fmt.Errorf` and include the output so the error modal can show the
   git error message (e.g., "fatal: couldn't find remote ref" or "CONFLICT (content)").
3. `Pull()` does NOT specially detect conflicts — it just returns the error. The merge conflict
   detection (Phase 14.3) happens after the pull by re-running `git status` and checking for
   unmerged entries.

### 14.1 Background Fetch

**File**: `internal/tui/app/app.go` (modify existing)

Background fetch runs on a separate timer from the 2-second status poll. The fetch interval
is configurable via `config.Git.FetchInterval` (default: 300 seconds, 0 = disabled).

**How the background fetch timer works**:

1. When the app enters `stateMain`, it starts both the status poll timer (2s) and the fetch
   timer (N seconds from config). Both run concurrently.
2. When `fetchTickMsg` fires, it triggers `backgroundFetchCmd` — an async command that runs
   `git fetch` and then `git rev-list --left-right --count` to check if ahead/behind changed.
3. `FetchCompleteMsg` carries the result back to `Update()`. If ahead/behind counts changed,
   a modal notification is shown. The status is also refreshed immediately.
4. While a fetch is in progress, `fetching` is set to `true` to prevent concurrent fetches
   and to show a spinner in the header.
5. The timer restarts after each fetch completes, not when it fires — this prevents fetch
   operations from stacking up if the network is slow.

**New messages**:

```go
// fetchTickMsg is sent by the background fetch timer.
type fetchTickMsg struct{}

// FetchCompleteMsg carries the result of a background or manual fetch.
type FetchCompleteMsg struct {
	Err           error
	OldAhead      int  // ahead count before the fetch
	OldBehind     int  // behind count before the fetch
	NewAhead      int  // ahead count after the fetch
	NewBehind     int  // behind count after the fetch
	AheadChanged  bool // true if ahead/behind counts differ
	Manual        bool // true if this was triggered by the user (F key)
}

// PullCompleteMsg carries the result of a pull operation.
type PullCompleteMsg struct {
	Err error
}
```

**New commands**:

```go
// backgroundFetchCmd runs git fetch and checks if ahead/behind changed.
// It captures the old ahead/behind values before fetching so it can compare.
func backgroundFetchCmd(repoPath string, oldAhead, oldBehind int, upstream string, manual bool) tea.Cmd {
	return func() tea.Msg {
		remote := git.GetRemote(repoPath)
		if remote == "" {
			return FetchCompleteMsg{
				Err:      fmt.Errorf("no remote configured"),
				OldAhead: oldAhead, OldBehind: oldBehind,
				NewAhead: oldAhead, NewBehind: oldBehind,
				Manual:   manual,
			}
		}

		err := git.Fetch(repoPath, remote)
		if err != nil {
			return FetchCompleteMsg{
				Err:      err,
				OldAhead: oldAhead, OldBehind: oldBehind,
				NewAhead: oldAhead, NewBehind: oldBehind,
				Manual:   manual,
			}
		}

		// Re-check ahead/behind after fetch
		newAhead, newBehind, _ := git.GetAheadBehind(repoPath, upstream)

		return FetchCompleteMsg{
			OldAhead:     oldAhead,
			OldBehind:    oldBehind,
			NewAhead:     newAhead,
			NewBehind:    newBehind,
			AheadChanged: newAhead != oldAhead || newBehind != oldBehind,
			Manual:       manual,
		}
	}
}

// pullCmd runs git pull asynchronously.
func pullCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		remote := git.GetRemote(repoPath)
		if remote == "" {
			return PullCompleteMsg{Err: fmt.Errorf("no remote configured")}
		}
		err := git.Pull(repoPath, remote)
		return PullCompleteMsg{Err: err}
	}
}

// startFetchTickCmd starts the background fetch timer using the configured interval.
// Returns nil if fetch_interval is 0 (disabled) or < 30 seconds.
func startFetchTickCmd(intervalSecs int) tea.Cmd {
	if intervalSecs <= 0 {
		return nil
	}
	// Enforce minimum 30-second interval (matches settings validation)
	if intervalSecs < 30 {
		intervalSecs = 30
	}
	return tea.Tick(time.Duration(intervalSecs)*time.Second, func(t time.Time) tea.Msg {
		return fetchTickMsg{}
	})
}
```

**New model fields** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Fetch & Pull
	fetching      bool      // true while a fetch is in progress (prevents concurrent)
	pulling       bool      // true while a pull is in progress
	lastFetchTime time.Time // when the last successful fetch completed
	remote        string    // cached remote name (usually "origin")
}
```

**Update `New()`** — no changes needed. `fetching`, `pulling` default to `false`,
`lastFetchTime` defaults to zero value, `remote` defaults to empty string.

**Update the `repoResolvedMsg` handler** — cache the remote name and start the fetch timer
alongside the status poll. Find the existing handler:

```go
	case repoResolvedMsg:
		if msg.path != "" {
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			m.remote = git.GetRemote(m.repoPath) // cache the remote name
			// Start polling: fetch initial status + start the 2s tick timer + start fetch timer
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
				startFetchTickCmd(m.config.Git.FetchInterval),
			)
		}
```

Do the same for the `RepoSelectedMsg` handler (when a repo is chosen from the picker):

```go
	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		m.remote = git.GetRemote(m.repoPath) // cache the remote name
		return m, tea.Batch(
			refreshStatusCmd(m.repoPath),
			startTickCmd(),
			startFetchTickCmd(m.config.Git.FetchInterval),
		)
```

**Handle `fetchTickMsg`** — add this case to `Update()`:

```go
	case fetchTickMsg:
		// Background fetch timer fired. Only fetch if:
		// 1. We're in the main state
		// 2. No fetch is already in progress
		// 3. A remote is configured
		if m.state == stateMain && !m.fetching && m.remote != "" {
			m.fetching = true
			// "@{upstream}" is git refspec syntax: "main@{upstream}" resolves to
			// the remote-tracking branch, e.g. "origin/main". This tells
			// GetAheadBehind() which remote ref to compare against.
			return m, backgroundFetchCmd(
				m.repoPath, m.ahead, m.behind,
				m.branchName+"@{upstream}",
				false, // not manual
			)
		}
		// If we can't fetch now, restart the timer to try again later
		return m, startFetchTickCmd(m.config.Git.FetchInterval)
```

**Handle `FetchCompleteMsg`** — add this case to `Update()`:

```go
	case FetchCompleteMsg:
		m.fetching = false
		m.lastFetchTime = time.Now()

		var cmds []tea.Cmd

		// Always refresh status after a fetch to update ahead/behind in the header
		cmds = append(cmds, refreshStatusCmd(m.repoPath))

		// Restart the fetch timer for the next cycle
		cmds = append(cmds, startFetchTickCmd(m.config.Git.FetchInterval))

		if msg.Err != nil {
			// Only show error modal for manual fetches — background fetch errors are silent.
			// This prevents network blips from spamming the user with error modals.
			if msg.Manual {
				m.errorModal = views.ShowError(
					"Fetch Error",
					msg.Err.Error(),
					true, // retryable
					backgroundFetchCmd(m.repoPath, m.ahead, m.behind,
						m.branchName+"@{upstream}", true),
					m.width, m.height,
				)
			}
			return m, tea.Batch(cmds...)
		}

		// Show notification if ahead/behind changed (background or manual)
		if msg.AheadChanged {
			notification := formatAheadBehindChange(msg)
			m.errorModal = views.ShowError(
				"Remote Updated",
				notification,
				false, // not retryable — informational
				nil,
				m.width, m.height,
			)
		}

		return m, tea.Batch(cmds...)
```

**Handle `PullCompleteMsg`** — add this case to `Update()`:

```go
	case PullCompleteMsg:
		m.pulling = false

		// Always refresh status after a pull
		cmd := refreshStatusCmd(m.repoPath)

		if msg.Err != nil {
			errMsg := msg.Err.Error()

			// Check if the error contains conflict markers
			if strings.Contains(errMsg, "CONFLICT") || strings.Contains(errMsg, "conflict") {
				// Merge conflict detected — show a specific modal
				m.errorModal = views.ShowError(
					"Merge Conflicts",
					"Pull completed with merge conflicts.\n\n"+
						"Conflicted files are marked with [!] in the file list.\n"+
						"Use the terminal (`) to resolve conflicts:\n\n"+
						"  git mergetool\n"+
						"  # or edit files manually, then:\n"+
						"  git add <resolved-file>\n"+
						"  git commit",
					false, // not retryable
					nil,
					m.width, m.height,
				)
			} else {
				// Generic pull error
				m.errorModal = views.ShowError(
					"Pull Error",
					errMsg,
					true, // retryable
					pullCmd(m.repoPath),
					m.width, m.height,
				)
			}
			return m, cmd
		}

		// Pull succeeded — refresh status (ahead/behind will update)
		return m, cmd
```

**Add the `formatAheadBehindChange` helper**:

```go
// formatAheadBehindChange creates a human-readable notification about ahead/behind changes.
func formatAheadBehindChange(msg FetchCompleteMsg) string {
	var parts []string

	if msg.NewBehind > msg.OldBehind {
		diff := msg.NewBehind - msg.OldBehind
		parts = append(parts, fmt.Sprintf(
			"%d new commit(s) available to pull (now %d behind)",
			diff, msg.NewBehind,
		))
	} else if msg.NewBehind < msg.OldBehind {
		parts = append(parts, fmt.Sprintf(
			"Now %d commit(s) behind (was %d)",
			msg.NewBehind, msg.OldBehind,
		))
	}

	if msg.NewAhead != msg.OldAhead {
		parts = append(parts, fmt.Sprintf(
			"Ahead count changed: %d → %d",
			msg.OldAhead, msg.NewAhead,
		))
	}

	if len(parts) == 0 {
		return "Remote tracking refs updated."
	}

	return strings.Join(parts, "\n")
}
```

**Update the header rendering** in `viewMain()` to show fetch status:

The header's action button from Phase 5 already shows `↻ Fetch`, `↓ Pull`, `↑ Push`, or
`↕ Pull / Push` based on ahead/behind state. Now we add two things:
1. A spinner/indicator when fetching or pulling is in progress
2. The time since last fetch (e.g., "28m ago")

Update the `HeaderData` struct and `viewMain()` call:

**File**: `internal/tui/views/header.go` (modify existing)

Add new fields (`Fetching`, `Pulling`, `LastFetchTime`) to `HeaderData`. Your existing struct already has a `Pushing bool` field from Phase 11 -- keep that field, just add the new ones alongside it:

```go
// HeaderData holds the information needed to render the header bar.
type HeaderData struct {
	RepoName      string
	BranchName    string
	Ahead         int
	Behind        int
	HasUpstream   bool
	Pushing bool // true while a push is in progress
	Fetching      bool      // true when a fetch is in progress
	Pulling       bool      // true when a pull is in progress
	LastFetchTime time.Time // when the last fetch completed (zero = never)
}
```

Add the `"time"` import to the file's import block.

Update `RenderHeader` to show the fetch/pull status in the action area:

```go
// RenderHeader renders the top header bar with repo name, branch, ahead/behind
// indicators, and a context-aware action button with fetch timing.
func RenderHeader(data HeaderData, width int) string {
	branchDisplay := data.BranchName
	if branchDisplay == "" {
		branchDisplay = "(detached)"
	}

	bg := lipgloss.Color("#1E1E1E")

	repoStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(bg).
		Padding(0, 1)

	branchStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Background(bg).
		Padding(0, 1)

	actionStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(bg).
		Padding(0, 1)

	activeStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#D29922")).
		Background(bg).
		Padding(0, 1)

	sep := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Background(bg).
		Render(" │ ")

	// ── Branch name + ahead/behind indicators ──
	branchText := "ᚠ " + branchDisplay

	if data.HasUpstream && (data.Ahead > 0 || data.Behind > 0) {
		branchText += " "
		if data.Ahead > 0 && data.Behind > 0 {
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#D29922")).
				Background(bg).
				Render(fmt.Sprintf("↑%d ↓%d", data.Ahead, data.Behind))
		} else if data.Ahead > 0 {
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#3FB950")).
				Background(bg).
				Render(fmt.Sprintf("↑%d", data.Ahead))
		} else {
			branchText += lipgloss.NewStyle().
				Foreground(lipgloss.Color("#F85149")).
				Background(bg).
				Render(fmt.Sprintf("↓%d", data.Behind))
		}
	}

	// ── Action button (context-aware) with status ──
	// Priority: Pulling > Fetching > actionLabel() (which handles Pushing + normal states).
	// A later update adds Pushing to this if-chain for consistent activeStyle.
	var actionText string
	if data.Pulling {
		actionText = activeStyle.Render("⟳ Pulling...")
	} else if data.Fetching {
		actionText = activeStyle.Render("⟳ Fetching...")
	} else {
		action := actionLabel(data)

		// Append time since last fetch
		if !data.LastFetchTime.IsZero() {
			elapsed := time.Since(data.LastFetchTime)
			action += " (" + formatDuration(elapsed) + ")"
		}

		actionText = actionStyle.Render(action)
	}

	left := repoStyle.Render("⎇ "+data.RepoName) +
		sep +
		branchStyle.Render(branchText) +
		sep +
		actionText

	return lipgloss.NewStyle().Width(width).Background(bg).Render(left)
}

// formatDuration returns a human-readable short duration string.
// Examples: "5s", "2m", "28m", "1h", "3h"
func formatDuration(d time.Duration) string {
	if d < time.Minute {
		return fmt.Sprintf("%ds", int(d.Seconds()))
	}
	if d < time.Hour {
		return fmt.Sprintf("%dm", int(d.Minutes()))
	}
	return fmt.Sprintf("%dh", int(d.Hours()))
}
```

**Update `viewMain()` in `app.go`** — pass the new header fields:

```go
	header := views.RenderHeader(views.HeaderData{
		RepoName:      git.RepoName(m.repoPath),
		BranchName:    m.branchName,
		Ahead:         m.ahead,
		Behind:        m.behind,
		HasUpstream:   m.hasUpstream,
		Fetching:      m.fetching,
		Pulling:       m.pulling,
		LastFetchTime: m.lastFetchTime,
	}, dim.Width)
```

**How the `viewMain()` header call changed from Phase 5**:

In Phase 5, the header was rendered with positional arguments:
```go
header := views.RenderHeader(git.RepoName(m.repoPath), "", dim.Width)
```

Phase 5 later changed this to use `HeaderData`:
```go
header := views.RenderHeader(views.HeaderData{...}, dim.Width)
```

Now we add three new fields: `Fetching`, `Pulling`, `LastFetchTime`. The header shows a
spinning indicator (`⟳ Fetching...` or `⟳ Pulling...`) when operations are in progress,
and appends the time since last fetch (e.g., `↻ Fetch (28m)`) when idle.

### 14.2 Pull from Remote

**File**: `internal/tui/app/app.go` (modify existing)

Add `F` and `P` keybindings to `handleMainKey()` in the navigable mode section:

```go
	// ── Navigable mode keybindings ──
	switch msg.String() {
	// ... existing cases (q, ?, tab, 1, 2, 3, `, escape, S) ...

	case "F":
		// Manual fetch — works any time a remote is configured
		if !m.fetching && !m.pulling && m.remote != "" {
			m.fetching = true
			return m, backgroundFetchCmd(
				m.repoPath, m.ahead, m.behind,
				m.branchName+"@{upstream}",
				true, // manual
			)
		}
		return m, nil

	case "P":
		// Pull from remote — only when behind (or when the user insists)
		if !m.pulling && !m.fetching && m.remote != "" {
			m.pulling = true
			return m, pullCmd(m.repoPath)
		}
		return m, nil
	}
```

**How `F` (fetch) works**:

1. The user presses `F` in navigable mode.
2. If no fetch or pull is already running and a remote is configured, `m.fetching` is set
   to `true` and `backgroundFetchCmd` is dispatched.
3. The header immediately shows `⟳ Fetching...` because `m.fetching` is true.
4. When the fetch completes, `FetchCompleteMsg` is handled (see 14.1 above):
   - `m.fetching` is set back to `false`
   - `m.lastFetchTime` is updated
   - If ahead/behind changed, a notification modal is shown
   - Status is refreshed to update the header
5. If the fetch fails (e.g., network error), the error modal shows with a Retry button
   (because `msg.Manual` is `true`).

**How `P` (pull) works**:

1. The user presses `P` in navigable mode.
2. If no pull or fetch is running and a remote is configured, `m.pulling` is set to `true`
   and `pullCmd` is dispatched.
3. The header immediately shows `⟳ Pulling...` because `m.pulling` is true.
4. When the pull completes, `PullCompleteMsg` is handled (see 14.1 above):
   - `m.pulling` is set back to `false`
   - Status is refreshed (this updates ahead/behind counts and the file list)
   - If there were merge conflicts, a specific modal is shown (see 14.3)
   - If there was a different error, a generic error modal is shown with Retry

**Why `P` doesn't check `m.behind > 0`**:

The user might want to pull even when the local state shows `behind == 0` — maybe they
know there are new commits but haven't fetched yet, or the status is stale. Letting `P`
always work (when no operation is running) is simpler and more predictable. If there's
nothing to pull, `git pull` completes instantly with "Already up to date."

**Update the help overlay** — add the fetch/pull keybindings to the help text. Find the
help rows in `views/helpoverlay.go` and add:

```go
	// Fetch & Pull section (add to the help rows)
	row("F", "Fetch from remote"),
	row("P", "Pull from remote"),
```

### 14.3 Merge Conflict Detection

Merge conflicts are detected through two mechanisms:

1. **Pull error output** — when `git pull` encounters a merge conflict, it exits non-zero
   and the output contains "CONFLICT". The `PullCompleteMsg` handler checks for this and
   shows a specific modal (see 14.1 above).

2. **Status polling** — the 2-second `git status` poll detects conflicted files via the
   `UU` status code (and related unmerged codes). This catches conflicts even if the user
   ran `git merge` in the embedded terminal.

**File**: `internal/git/status.go` (modify existing)

The file entry parsing is already handled in Phase 6 (`ParseStatusEntries`). Conflicted
files are detected by the `u` prefix in porcelain v2 output and mapped to `StatusConflicted`.

Add a helper function to check if the current status has any conflicted files:

```go
// HasConflicts returns true if any of the given file entries have a conflicted status.
// This is used to detect merge conflicts after a pull or merge operation.
func HasConflicts(entries []FileEntry) bool {
	for _, e := range entries {
		if e.Status == StatusConflicted {
			return true
		}
	}
	return false
}

// ConflictedFiles returns the paths of all conflicted files in the given entries.
func ConflictedFiles(entries []FileEntry) []string {
	var paths []string
	for _, e := range entries {
		if e.Status == StatusConflicted {
			paths = append(paths, e.Path)
		}
	}
	return paths
}
```

**File**: `internal/tui/app/app.go` (modify existing)

Update the `statusResultMsg` handler to detect conflicts after a pull. The pull itself may
have already shown a conflict modal (via `PullCompleteMsg`), but this catches conflicts
that appear from other sources (e.g., the user ran `git merge` in the terminal).

Add a `hadPull` concept: after a pull completes, the next status refresh checks for
conflicts. We track this with a simple boolean field.

> **Why a flag instead of checking every time?** We only need to show the conflict modal
> right after a pull. Checking for conflicts on every 2-second poll would show the modal
> repeatedly until the user resolves the conflicts, which would be annoying.

Add to the `Model` struct:

```go
	postPullCheck bool // true after a pull completes — next status checks for conflicts
```

Update the `PullCompleteMsg` handler to set the flag (add before the existing error check):

```go
	case PullCompleteMsg:
		m.pulling = false
		m.postPullCheck = true // next status refresh will check for conflicts
		// ... rest of handler unchanged ...
```

Update the `statusResultMsg` handler to check for conflicts after a pull:

```go
	case statusResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}

		// Update model with fresh status data
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream

		// Update the file list
		// files := git.ParseStatusEntries(msg.status.RawOutput)
		// m.fileList.SetFiles(files)

		// Post-pull conflict detection
		if m.postPullCheck {
			m.postPullCheck = false

			// Parse file entries to check for conflicts
			files := git.ParseStatusEntries(msg.status.RawOutput)
			conflicted := git.ConflictedFiles(files)

			if len(conflicted) > 0 && !m.errorModal.Visible {
				// Build a list of conflicted file paths for the modal
				fileList := strings.Join(conflicted, "\n  ")
				m.errorModal = views.ShowError(
					"Merge Conflicts Detected",
					fmt.Sprintf(
						"The following files have conflicts:\n\n  %s\n\n"+
							"Resolve conflicts in the terminal (`):\n"+
							"  git mergetool\n"+
							"  # or edit files, then:\n"+
							"  git add <file>\n"+
							"  git commit",
						fileList,
					),
					false, // not retryable
					nil,
					m.width, m.height,
				)
			}
		}

		return m, nil
```

**How conflict detection works end-to-end**:

1. User presses `P` to pull → `m.pulling = true`, `pullCmd` dispatched
2. Header shows `⟳ Pulling...`
3. `git pull` runs asynchronously:
   - **Success (no conflicts)**: `PullCompleteMsg{Err: nil}` → sets `m.postPullCheck = true`,
     refreshes status → ahead/behind update, file list updates, no conflicts found
   - **Conflict during merge**: `git pull` exits non-zero with "CONFLICT" in output →
     `PullCompleteMsg{Err: ...}` → handler detects "CONFLICT" in error message → shows
     conflict modal with instructions
   - **Other error** (network, auth, etc.): `PullCompleteMsg{Err: ...}` → handler shows
     generic error modal with Retry
4. After a successful pull, the next `statusResultMsg` checks for conflicts via
   `git.ConflictedFiles()`. This catches edge cases where `git pull` succeeded but left
   conflicts (shouldn't happen with `--ff`, but defensive).
5. Conflicted files appear in the file list with `[!]` icons (Phase 6 already handles
   `StatusConflicted` rendering).

**Why two conflict detection paths?**

- **`PullCompleteMsg` path**: catches the immediate `git pull` failure. The error output
  from git includes "CONFLICT" which we detect. This gives instant feedback.
- **`statusResultMsg` path**: catches conflicts that appear after terminal operations (e.g.,
  the user ran `git merge feature-branch` in the embedded terminal). The 2-second status
  poll picks up the unmerged entries and shows the modal.

Both paths are needed because the embedded terminal is a first-class feature — users may
run git commands directly, and the TUI should detect and surface conflicts regardless of
how they were created.

**Update the `SettingsChangedMsg` handler** — when `git.fetch_interval` changes in settings,
the fetch timer should adjust. The existing handler from Phase 13 already has a comment
about this. Now make it explicit:

```go
	case views.SettingsChangedMsg:
		switch msg.Key {
		case "appearance.theme":
			m.theme = render.CurrentTheme(m.config.Appearance.Theme)
		case "git.fetch_interval":
			// The new interval takes effect on the next fetch timer restart.
			// We don't need to cancel the current timer — when the current fetchTickMsg
			// fires, startFetchTickCmd will use the new value from m.config.
			// If the user set it to 0 (disabled), the next startFetchTickCmd returns nil.
			// In other words: the old timer still fires once, but then the new interval
			// is used for all subsequent timers. This is simpler than canceling timers.
		}
		return m, nil
```

**What changed from Phase 13**:

1. **New `internal/git/remote.go`**: `Fetch()`, `Pull()`, `GetAheadBehind()`, `GetRemote()`
   — four git command wrappers for remote operations
2. **New `HasConflicts()` and `ConflictedFiles()` in `git/status.go`**: helpers that scan
   parsed file entries for `StatusConflicted`
3. **New messages**: `fetchTickMsg`, `FetchCompleteMsg`, `PullCompleteMsg`
4. **New commands**: `backgroundFetchCmd`, `pullCmd`, `startFetchTickCmd`
5. **New model fields**: `fetching`, `pulling`, `lastFetchTime`, `remote`, `postPullCheck`
6. **`repoResolvedMsg` handler updated**: caches remote name, starts fetch timer
7. **`RepoSelectedMsg` handler updated**: same as above (repo picker path)
8. **`Update()` additions**: handles `fetchTickMsg`, `FetchCompleteMsg`, `PullCompleteMsg`
9. **`handleMainKey()` additions**: `F` for manual fetch, `P` for pull
10. **`statusResultMsg` handler updated**: post-pull conflict detection
11. **`HeaderData` updated**: new `Fetching`, `Pulling`, `LastFetchTime` fields
12. **`RenderHeader()` updated**: shows spinner during fetch/pull, time since last fetch
13. **`formatDuration()` added**: human-readable duration (5s, 2m, 28m, 1h)
14. **`formatAheadBehindChange()` added**: notification text for ahead/behind changes
15. **Help overlay updated**: `F` and `P` keybindings listed

### 14.4 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

Choose a repo that has a remote configured (e.g., a GitHub clone). You need network access
for fetch/pull to work.

**Test 1 — Background fetch runs automatically**:

1. Start the app with a repo that has a remote
2. Check your `config.toml` — `fetch_interval` should be 300 (default)
3. For testing, open settings (`S`) and change Fetch Interval to 30 (minimum)
4. Wait 30 seconds — the header should briefly show `⟳ Fetching...` then return to
   `↻ Fetch (Ns)` where N is the seconds since the fetch completed
5. The `(Ns)` counter should update on each 2-second status poll render cycle

**Test 2 — Manual fetch with `F`**:

1. Press `F` — the header should immediately show `⟳ Fetching...`
2. After a few seconds (depends on network), the header should show `↻ Fetch (0s)`
3. The `(Ns)` timer should start counting up from 0
4. If there are new commits on the remote, a notification modal should appear saying
   something like "2 new commit(s) available to pull (now 2 behind)"
5. Press `Esc` to dismiss the modal

**Test 3 — Fetch with no remote**:

1. Create a local-only repo: `git init /tmp/test-no-remote`
2. Start the app with that repo: `./leogit /tmp/test-no-remote`
3. Press `F` — nothing should happen (no error, no crash). The `F` key silently
   does nothing because `m.remote` is empty
4. The header should still show `↻ Fetch` (no timing info since no fetch ever ran)

**Test 4 — Fetch error handling**:

1. Temporarily disconnect from the network (e.g., turn off WiFi)
2. Press `F` — the header shows `⟳ Fetching...`
3. After the timeout, an error modal should appear: "Fetch Error" with the git error message
4. The modal should have a `[Retry]` button
5. Reconnect to the network and press `Tab` to select Retry, then `Enter`
6. The fetch should succeed this time

**Test 5 — Background fetch error is silent**:

1. Disconnect from the network
2. Wait for the background fetch timer to fire (check your fetch_interval setting)
3. No error modal should appear — background fetch errors are intentionally silent
4. The app should continue working normally
5. Reconnect — the next background fetch should succeed silently

**Test 6 — Pull with `P`**:

1. Set up a test scenario: push a commit from another machine (or use the GitHub web UI
   to create a commit on the remote)
2. Press `F` to fetch — the notification modal should show "N new commit(s) available to pull"
3. The header should now show `↓ Pull` instead of `↻ Fetch`
4. Press `Esc` to dismiss the notification, then press `P`
5. The header should show `⟳ Pulling...`
6. After the pull completes, the header should go back to `↻ Fetch (0s)` and the
   ahead/behind counts should be 0
7. The file list should update if the pulled commits changed any files

**Test 7 — Pull when already up to date**:

1. Make sure the repo is fully up to date (fetch shows no changes)
2. Press `P` — the header shows `⟳ Pulling...` briefly
3. The pull completes instantly with no errors and no modals
4. This confirms `P` works even when there's nothing to pull

**Test 8 — Pull with merge conflict**:

1. Set up a conflict scenario:
   ```bash
   # In your repo, create a file and push it
   echo "original" > conflict-test.txt
   git add conflict-test.txt && git commit -m "add test file" && git push

   # On the remote (e.g., GitHub web UI), edit conflict-test.txt to say "remote change"

   # Locally, edit the same file differently
   echo "local change" > conflict-test.txt
   git add conflict-test.txt && git commit -m "local change"
   ```
2. Press `F` to fetch — the notification should show you're behind
3. Press `P` to pull
4. A "Merge Conflicts" modal should appear listing `conflict-test.txt`
5. The modal should show instructions for resolving conflicts
6. Press `Esc` to dismiss
7. The file list should show `conflict-test.txt` with a `[!]` icon (red, conflicted)
8. Open the terminal with `` ` ``, resolve the conflict manually, then `git add` and `git commit`
9. After the next status poll (2 seconds), the `[!]` icon should disappear

**Test 9 — Concurrent fetch/pull prevention**:

1. Press `F` to start a fetch
2. While `⟳ Fetching...` is shown, press `F` again — nothing should happen (no second fetch)
3. While `⟳ Fetching...` is shown, press `P` — nothing should happen (can't pull during fetch)
4. After the fetch completes, press `P` — it should work
5. While `⟳ Pulling...` is shown, press `F` — nothing should happen (can't fetch during pull)

**Test 10 — Fetch interval setting takes effect**:

1. Open settings with `S`
2. Navigate to "Fetch Interval" under Git
3. Change it to 60 (1 minute)
4. Close settings with `Esc`
5. Wait and observe — fetches should happen every ~60 seconds
6. Change it to 0 (disabled) — background fetches should stop entirely
7. Manual `F` should still work even with interval set to 0

**Test 11 — Header timing display**:

1. Press `F` to fetch manually
2. The header should show `↻ Fetch (0s)` immediately after
3. Wait a few seconds — it should update to `↻ Fetch (5s)`, then `↻ Fetch (10s)`, etc.
4. After 60 seconds it shows `↻ Fetch (1m)`, after 5 minutes `↻ Fetch (5m)`, etc.
5. After another fetch (manual or background), the timer resets to `(0s)`

**Test 12 — Pull updates ahead/behind in header**:

1. Start with a repo that is behind (e.g., after `F` shows you're behind)
2. The header should show `↓ Pull` with a red `↓N` indicator
3. Press `P` to pull
4. After the pull completes, the red `↓N` indicator should disappear
5. The header action should change back to `↻ Fetch`

**Phase 14 is complete when**: background fetch runs automatically on the configured interval;
`F` triggers a manual fetch with a spinner in the header; `P` triggers a pull with a spinner;
fetch errors are shown as modals for manual fetches but silent for background fetches; the
header shows `⟳ Fetching...` or `⟳ Pulling...` during operations and `↻ Fetch (Nm)` with
elapsed time when idle; the ahead/behind notification modal appears when new commits are
available after fetch; merge conflicts from `git pull` are detected and shown in a modal
with resolution instructions; conflicted files appear with `[!]` in the file list; concurrent
fetch/pull operations are prevented; the fetch interval setting takes effect immediately;
and `P` works even when there's nothing to pull.

## Phase 15 — Branch Operations

**Goal**: Add a branch dropdown overlay accessible from the header, where the user can list,
filter, create, switch, delete, and rename branches. Pressing `B` (uppercase) in navigable
mode opens the branch dropdown — a fullscreen overlay with a filterable list of local and
remote branches. The dropdown enters focused mode so all keystrokes go to it (search/filter,
navigation, actions). `Esc` closes it and returns to navigable mode.

This phase introduces:
1. **Git branch commands** — `ListBranches()`, `CreateBranch()`, `SwitchBranch()`,
   `DeleteBranch()`, `DeleteRemoteBranch()`, and `RenameBranch()` wrappers in the git package
2. **Branch dropdown overlay** — a filterable list component showing local and remote branches,
   with the current branch highlighted. Follows the same pattern as the repo picker (Phase 3)
   but rendered as an overlay on top of the main layout
3. **Branch actions** — create (`c`), delete (`d`), and rename (`r`) triggered by single-key
   shortcuts within the dropdown
4. **Branch switching** — `Enter` on a branch switches to it, with async execution and a
   spinner in the header

After this phase, pressing `B` opens a branch picker overlay where you can search branches,
switch with `Enter`, create with `c`, delete with `d`, and rename with `r`.

### 15.0 Git Branch Commands

**File**: `internal/git/branch.go` (new file)

This file wraps all branch-related git commands into Go functions. All commands run with
`TERM=dumb` to suppress color output and pager behavior.

**How `git branch --format` works**:

```bash
git branch --format='%(refname:short)' --sort=-committerdate
```

- `--format='%(refname:short)'` outputs just the branch name, one per line, without the
  leading `*` or `remotes/` prefix
- `--sort=-committerdate` sorts by most recently committed (descending) — the branch you
  were working on most recently appears first
- By default, this lists only local branches
- Adding `-a` (or `--all`) includes remote-tracking branches too
- Remote branches appear as `origin/main`, `origin/feature-x`, etc.

**How `git branch <name>` works**:

```bash
git branch <name> [<start-point>] [--no-track]
```

- Creates a new branch pointing at `<start-point>` (defaults to HEAD)
- `--no-track` prevents automatic upstream tracking setup
- Does NOT switch to the new branch — use `git checkout` for that

**How `git checkout <branch>` works**:

```bash
git checkout <branch> --
```

- Switches the working tree to `<branch>`
- The `--` at the end prevents ambiguity with file paths
- For remote branches that don't exist locally yet, git auto-creates a local tracking branch
  (e.g., `git checkout feature-x` when `origin/feature-x` exists creates a local `feature-x`
  tracking `origin/feature-x`)

**How `git branch -D <name>` works**:

```bash
git branch -D <name>
```

- `-D` is shorthand for `--delete --force` — deletes the branch even if it hasn't been merged
- We use `-D` instead of `-d` because `-d` refuses to delete unmerged branches. The TUI
  shows a confirmation dialog (Phase 15.4) before calling this, so the force is intentional.
- Cannot delete the currently checked-out branch — git returns an error

**How `git push <remote> :<branch>` works**:

```bash
git push <remote> :<branch>
```

- The colon prefix with no local ref means "push nothing to the remote branch" — effectively
  deleting it
- This is the standard way to delete a remote branch

**How `git branch -m` works**:

```bash
git branch -m <old-name> <new-name>
```

- `-m` renames a branch. If you're on the branch being renamed, you can omit `<old-name>`
- Refuses to rename if `<new-name>` already exists (use `-M` to force overwrite, but we
  don't — we show an error instead)

```go
package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// BranchInfo holds metadata about a single branch.
type BranchInfo struct {
	Name      string // short name (e.g., "main", "feature-x")
	IsRemote  bool   // true for remote-tracking branches (e.g., "origin/main")
	IsCurrent bool   // true if this is the currently checked-out branch
}

// ListBranches returns all local and remote branches, sorted by most recent commit.
// The current branch is marked with IsCurrent = true.
// Remote branches have IsRemote = true and names like "origin/main".
func ListBranches(repoPath string) ([]BranchInfo, error) {
	// Get the current branch name first
	currentCmd := exec.Command("git", "branch", "--show-current")
	currentCmd.Dir = repoPath
	currentCmd.Env = append(currentCmd.Environ(), "TERM=dumb")
	currentOut, _ := currentCmd.Output()
	currentBranch := strings.TrimSpace(string(currentOut))

	// List all branches (local + remote) sorted by most recent commit
	cmd := exec.Command("git", "branch",
		"--all",
		"--format=%(refname:short)",
		"--sort=-committerdate",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git branch --all failed: %w", err)
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	var branches []BranchInfo

	seen := make(map[string]bool) // deduplicate remote branches that have local equivalents

	for _, line := range lines {
		name := strings.TrimSpace(line)
		if name == "" {
			continue
		}

		// Skip HEAD pointer entries like "origin/HEAD -> origin/main"
		if strings.Contains(name, "->") {
			continue
		}

		isRemote := strings.Contains(name, "/")

		// For remote branches, check if we already have a local branch with the same name.
		// e.g., if we have local "main" and remote "origin/main", skip the remote one.
		if isRemote {
			// Extract the branch name after the remote prefix (e.g., "origin/main" → "main")
			parts := strings.SplitN(name, "/", 2)
			if len(parts) == 2 && seen[parts[1]] {
				continue // already have a local branch with this name
			}
		} else {
			seen[name] = true
		}

		branches = append(branches, BranchInfo{
			Name:      name,
			IsRemote:  isRemote,
			IsCurrent: !isRemote && name == currentBranch,
		})
	}

	return branches, nil
}

// CreateBranch creates a new branch at the given start point (or HEAD if empty).
// Does NOT switch to the new branch.
func CreateBranch(repoPath, name, startPoint string) error {
	args := []string{"branch", name}
	if startPoint != "" {
		args = append(args, startPoint)
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch failed: %w\n%s", err, string(output))
	}
	return nil
}

// SwitchBranch checks out the given branch.
// For remote branches (e.g., "origin/feature-x"), git auto-creates a local tracking branch.
func SwitchBranch(repoPath, branch string) error {
	cmd := exec.Command("git", "checkout", branch, "--")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git checkout failed: %w\n%s", err, string(output))
	}
	return nil
}

// DeleteBranch force-deletes a local branch.
// Returns an error if you try to delete the currently checked-out branch.
func DeleteBranch(repoPath, name string) error {
	cmd := exec.Command("git", "branch", "-D", name)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch -D failed: %w\n%s", err, string(output))
	}
	return nil
}

// DeleteRemoteBranch deletes a branch on the remote.
// The branch parameter should be the short name (e.g., "feature-x"), not "origin/feature-x".
func DeleteRemoteBranch(repoPath, remote, branch string) error {
	cmd := exec.Command("git", "push", remote, ":"+branch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git push delete failed: %w\n%s", err, string(output))
	}
	return nil
}

// RenameBranch renames a branch from oldName to newName.
// If oldName is empty, renames the currently checked-out branch.
func RenameBranch(repoPath, oldName, newName string) error {
	args := []string{"branch", "-m"}
	if oldName != "" {
		args = append(args, oldName)
	}
	args = append(args, newName)

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch -m failed: %w\n%s", err, string(output))
	}
	return nil
}
```

**How `ListBranches()` deduplicates**:

When you have a local branch `main` that tracks `origin/main`, `git branch --all` shows both.
We don't want the user to see duplicate entries in the dropdown. The function tracks local
branch names in a `seen` map. When processing remote branches, if a local branch with the
same name already exists (e.g., local `main` exists, so skip `origin/main`), the remote
entry is skipped. This means:

- If you have both local `main` and `origin/main` → only `main` (local) appears
- If you only have `origin/feature-x` (not checked out locally) → `origin/feature-x` appears
  as a remote branch the user can check out

**How `SwitchBranch()` handles remote branches**:

When the user selects a remote branch like `origin/feature-x`, `git checkout` automatically:
1. Creates a local branch `feature-x`
2. Sets it to track `origin/feature-x`
3. Switches to it

This means the user never needs to manually run `git checkout -b feature-x origin/feature-x`.
The `git checkout` command handles the remote-to-local tracking branch creation automatically.

### 15.1 Branch List

**File**: `internal/tui/views/branchdropdown.go` (new file)

The branch dropdown is a fullscreen overlay that shows a filterable list of branches. It
follows the same pattern as the repo picker (Phase 3) — a `filter` string, an `allBranches`
slice, a `filtered` slice, and a `cursor` index. The key difference is that the branch
dropdown also supports action shortcuts: `c` (create), `d` (delete), `r` (rename).

The dropdown has three sub-modes:
1. **Browse mode** (default) — navigate and filter the branch list. Type to filter, `j`/`k`
   to navigate, `Enter` to switch, `c`/`d`/`r` for actions, `Esc` to close.
2. **Create mode** — a text input at the top for the new branch name. `Enter` confirms,
   `Esc` cancels back to browse mode.
3. **Rename mode** — a text input at the top pre-filled with the current name. `Enter`
   confirms, `Esc` cancels back to browse mode.

Delete doesn't need a sub-mode — it shows a confirmation modal via the app's error modal
system (the branch dropdown sends a message, the app shows the modal, and the user confirms).

**Messages sent by the branch dropdown**:

```go
// BranchSwitchMsg is sent when the user selects a branch to switch to.
type BranchSwitchMsg struct {
    Name string // branch name to switch to
}

// BranchCreateMsg is sent when the user confirms a new branch name.
type BranchCreateMsg struct {
    Name string // new branch name
}

// BranchDeleteMsg is sent when the user presses 'd' on a branch.
// The app shows a confirmation modal before actually deleting.
type BranchDeleteMsg struct {
    Name     string // branch to delete
    IsRemote bool   // true if this is a remote branch
}

// BranchRenameMsg is sent when the user confirms a rename.
type BranchRenameMsg struct {
    OldName string
    NewName string
}

// BranchDropdownClosedMsg is sent when the user presses Esc to close the dropdown.
type BranchDropdownClosedMsg struct{}
```

**How the branch dropdown manages state**:

The dropdown holds its own list of `BranchInfo` items, loaded when the dropdown opens. The
app calls `SetBranches()` after loading branches from `git.ListBranches()`. The filter works
the same as the repo picker — case-insensitive substring match against the branch name. The
current branch gets a special indicator (`✓`) and cannot be deleted or switched to (it's
already the active branch).

```go
package views

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// ── Messages ────────────────────────────────────────────

// BranchSwitchMsg is sent when the user selects a branch to switch to.
type BranchSwitchMsg struct {
	Name string
}

// BranchCreateMsg is sent when the user confirms a new branch name.
type BranchCreateMsg struct {
	Name string
}

// BranchDeleteMsg is sent when the user presses 'd' on a branch.
// The app shows a confirmation modal before actually deleting.
type BranchDeleteMsg struct {
	Name     string
	IsRemote bool
}

// BranchRenameMsg is sent when the user confirms a rename.
type BranchRenameMsg struct {
	OldName string
	NewName string
}

// BranchDropdownClosedMsg is sent when the user presses Esc to close the dropdown.
type BranchDropdownClosedMsg struct{}

// ── Dropdown Mode ───────────────────────────────────────

type branchMode int

const (
	branchModeBrowse branchMode = iota
	branchModeCreate
	branchModeRename
)

// ── Model ───────────────────────────────────────────────

// BranchDropdownModel is a fullscreen overlay for browsing and managing branches.
type BranchDropdownModel struct {
	allBranches []git.BranchInfo // all branches from git
	filtered    []git.BranchInfo // branches matching the current filter
	filter      string           // current search/filter text
	cursor      int              // index in filtered list
	width       int
	height      int
	Visible     bool

	mode       branchMode // browse, create, or rename
	inputText  string     // text being typed in create/rename mode
	renameFrom string     // original branch name when renaming
}

// NewBranchDropdown creates a new hidden branch dropdown.
func NewBranchDropdown() BranchDropdownModel {
	return BranchDropdownModel{}
}

// SetBranches replaces the branch list and resets the filter.
// Called after loading branches from git.ListBranches().
func (m *BranchDropdownModel) SetBranches(branches []git.BranchInfo) {
	m.allBranches = branches
	m.filter = ""
	m.inputText = ""
	m.mode = branchModeBrowse
	m.applyFilter()
}

// Open makes the dropdown visible and resets state.
func (m *BranchDropdownModel) Open(width, height int) {
	m.Visible = true
	m.width = width
	m.height = height
	m.filter = ""
	m.inputText = ""
	m.mode = branchModeBrowse
	m.cursor = 0
	m.applyFilter()
}

// ── Update ──────────────────────────────────────────────

// Update handles input for the branch dropdown.
func (m BranchDropdownModel) Update(msg tea.Msg) (BranchDropdownModel, tea.Cmd) {
	if !m.Visible {
		return m, nil
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch m.mode {
		case branchModeBrowse:
			return m.updateBrowse(msg)
		case branchModeCreate:
			return m.updateCreate(msg)
		case branchModeRename:
			return m.updateRename(msg)
		}
	}

	return m, nil
}

// updateBrowse handles keys in browse mode (the default branch list view).
func (m BranchDropdownModel) updateBrowse(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "escape":
		// If there's a filter, clear it first
		if m.filter != "" {
			m.filter = ""
			m.applyFilter()
			return m, nil
		}
		// Otherwise close the dropdown
		m.Visible = false
		return m, func() tea.Msg { return BranchDropdownClosedMsg{} }

	case "enter":
		// Switch to the selected branch
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
			// Don't switch to the current branch — it's already active
			if branch.IsCurrent {
				return m, nil
			}
			m.Visible = false
			return m, func() tea.Msg {
				return BranchSwitchMsg{Name: branch.Name}
			}
		}
		return m, nil

	case "up", "k":
		if m.cursor > 0 {
			m.cursor--
		}
		return m, nil

	case "down", "j":
		if m.cursor < len(m.filtered)-1 {
			m.cursor++
		}
		return m, nil

	case "c":
		// Enter create mode
		// NOTE: Because "c" is handled here before the default case, typing
		// "c" in browse mode triggers create mode instead of adding "c" to
		// the filter. Same applies to "d" and "r" below. This is intentional:
		// these are action shortcuts, not filter characters.
		m.mode = branchModeCreate
		m.inputText = ""
		return m, nil

	case "d":
		// Delete the selected branch (sends message for confirmation)
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
			// Can't delete the current branch
			if branch.IsCurrent {
				return m, nil
			}
			m.Visible = false
			return m, func() tea.Msg {
				return BranchDeleteMsg{
					Name:     branch.Name,
					IsRemote: branch.IsRemote,
				}
			}
		}
		return m, nil

	case "r":
		// Rename the selected branch
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
			// Can't rename remote branches
			if branch.IsRemote {
				return m, nil
			}
			m.mode = branchModeRename
			m.renameFrom = branch.Name
			m.inputText = branch.Name // pre-fill with current name
			return m, nil
		}
		return m, nil

	case "backspace":
		if len(m.filter) > 0 {
			m.filter = m.filter[:len(m.filter)-1]
			m.applyFilter()
		}
		return m, nil

	default:
		// Single printable character → add to filter
		if len(msg.String()) == 1 {
			char := msg.String()
			if char >= " " && char <= "~" {
				m.filter += char
				m.applyFilter()
			}
		}
		return m, nil
	}
}

// updateCreate handles keys in create mode (typing a new branch name).
func (m BranchDropdownModel) updateCreate(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "escape":
		m.mode = branchModeBrowse
		m.inputText = ""
		return m, nil

	case "enter":
		name := strings.TrimSpace(m.inputText)
		if name == "" {
			return m, nil
		}
		m.Visible = false
		m.mode = branchModeBrowse
		return m, func() tea.Msg {
			return BranchCreateMsg{Name: name}
		}

	case "backspace":
		if len(m.inputText) > 0 {
			m.inputText = m.inputText[:len(m.inputText)-1]
		}
		return m, nil

	default:
		if len(msg.String()) == 1 {
			char := msg.String()
			if char >= " " && char <= "~" {
				m.inputText += char
			}
		}
		return m, nil
	}
}

// updateRename handles keys in rename mode (typing the new branch name).
func (m BranchDropdownModel) updateRename(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "escape":
		m.mode = branchModeBrowse
		m.inputText = ""
		m.renameFrom = ""
		return m, nil

	case "enter":
		newName := strings.TrimSpace(m.inputText)
		if newName == "" || newName == m.renameFrom {
			// Empty or unchanged — cancel
			m.mode = branchModeBrowse
			return m, nil
		}
		oldName := m.renameFrom
		m.Visible = false
		m.mode = branchModeBrowse
		m.renameFrom = ""
		return m, func() tea.Msg {
			return BranchRenameMsg{OldName: oldName, NewName: newName}
		}

	case "backspace":
		if len(m.inputText) > 0 {
			m.inputText = m.inputText[:len(m.inputText)-1]
		}
		return m, nil

	default:
		if len(msg.String()) == 1 {
			char := msg.String()
			if char >= " " && char <= "~" {
				m.inputText += char
			}
		}
		return m, nil
	}
}

// ── Filter ──────────────────────────────────────────────

// applyFilter updates the filtered list based on the current filter text.
func (m *BranchDropdownModel) applyFilter() {
	if m.filter == "" {
		m.filtered = m.allBranches
		m.cursor = 0
		return
	}

	query := strings.ToLower(m.filter)
	var matches []git.BranchInfo
	for _, b := range m.allBranches {
		if strings.Contains(strings.ToLower(b.Name), query) {
			matches = append(matches, b)
		}
	}
	m.filtered = matches

	if m.cursor >= len(m.filtered) {
		m.cursor = max(0, len(m.filtered)-1)
	}
}

// ── View ────────────────────────────────────────────────

// View renders the branch dropdown overlay.
func (m BranchDropdownModel) View() string {
	if !m.Visible {
		return ""
	}

	// ── Styles ──────────────────────────────────────────
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	filterStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	inputLabelStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#D29922")).
		Bold(true)

	inputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Bold(true)

	selectedStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#000000")).
		Background(lipgloss.Color("#3FB950")).
		Bold(true)

	normalStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	currentStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	remoteStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Italic(true)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	// ── Title ───────────────────────────────────────────
	title := titleStyle.Render("Switch Branch")

	// ── Input area (mode-dependent) ─────────────────────
	var inputLine string
	switch m.mode {
	case branchModeBrowse:
		inputLine = "Filter: "
		if m.filter != "" {
			inputLine += filterStyle.Render(m.filter)
		}
		inputLine += filterStyle.Render("█")

	case branchModeCreate:
		inputLine = inputLabelStyle.Render("New branch: ") +
			inputStyle.Render(m.inputText) +
			inputStyle.Render("█")

	case branchModeRename:
		inputLine = inputLabelStyle.Render("Rename to: ") +
			inputStyle.Render(m.inputText) +
			inputStyle.Render("█")
	}

	// ── Branch list ─────────────────────────────────────
	var listLines []string

	if len(m.filtered) == 0 {
		if len(m.allBranches) == 0 {
			listLines = append(listLines, normalStyle.Render("No branches found."))
		} else {
			listLines = append(listLines, normalStyle.Render("No matches for \""+m.filter+"\""))
		}
	} else {
		// Reserve lines for: title, blank, input, blank, list..., blank, hint
		maxVisible := m.height - 6
		if maxVisible < 3 {
			maxVisible = 3
		}

		start := 0
		if m.cursor >= maxVisible {
			start = m.cursor - maxVisible + 1
		}
		end := start + maxVisible
		if end > len(m.filtered) {
			end = len(m.filtered)
		}

		for i := start; i < end; i++ {
			branch := m.filtered[i]

			// Build the display line
			prefix := "  "
			if branch.IsCurrent {
				prefix = "✓ "
			}

			name := branch.Name
			var line string

			if i == m.cursor {
				// Highlighted row
				line = selectedStyle.Render(" " + prefix + name + " ")
			} else if branch.IsCurrent {
				line = currentStyle.Render(prefix + name)
			} else if branch.IsRemote {
				line = remoteStyle.Render(prefix + name)
			} else {
				line = normalStyle.Render(prefix + name)
			}

			listLines = append(listLines, line)
		}

		// Scroll indicator
		if len(m.filtered) > maxVisible {
			indicator := hintStyle.Render(
				"  (" + strings.Itoa(m.cursor+1) + "/" + strings.Itoa(len(m.filtered)) + ")",
			)
			listLines = append(listLines, indicator)
		}
	}

	// ── Hint ────────────────────────────────────────────
	var hint string
	switch m.mode {
	case branchModeBrowse:
		hint = hintStyle.Render("Type to filter • Enter switch • c create • d delete • r rename • Esc close")
	case branchModeCreate:
		hint = hintStyle.Render("Type branch name • Enter create • Esc cancel")
	case branchModeRename:
		hint = hintStyle.Render("Type new name • Enter rename • Esc cancel")
	}

	// ── Assemble ────────────────────────────────────────
	sections := []string{title, "", inputLine, ""}
	sections = append(sections, listLines...)
	sections = append(sections, "", hint)

	content := strings.Join(sections, "\n")

	// ── Box ─────────────────────────────────────────────
	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#3FB950")).
		Padding(1, 3).
		MaxWidth(m.width - 4)

	box := boxStyle.Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
```

**How the branch dropdown integrates with the app**:

The branch dropdown follows the same overlay pattern as the settings view (Phase 13) and
help overlay (Phase 4). When `Visible` is true, `View()` renders the dropdown instead of
the main layout. The dropdown captures all keystrokes — global shortcuts are blocked. `Esc`
closes it and returns to the main layout.

**Why the dropdown sends messages instead of executing git commands directly**:

The dropdown is a view component — it doesn't know about the repo path, config, or error
handling. By sending messages (`BranchSwitchMsg`, `BranchCreateMsg`, etc.), the app's
`Update()` handler can:
1. Run the git command asynchronously
2. Show a spinner in the header during the operation
3. Handle errors with the error modal system
4. Refresh status after the operation completes
5. Show confirmation dialogs before destructive actions (delete)

This keeps the dropdown stateless with respect to git operations — it just manages the UI
for browsing and selecting.

### 15.2 Create Branch

**File**: `internal/tui/app/app.go` (modify existing)

**New messages for branch operations**:

```go
// branchListResultMsg carries the result of loading branches.
type branchListResultMsg struct {
	branches []git.BranchInfo
	err      error
}

// branchSwitchCompleteMsg is sent after a branch switch completes.
type branchSwitchCompleteMsg struct {
	branchName string
	err        error
}

// branchCreateCompleteMsg is sent after a new branch is created.
type branchCreateCompleteMsg struct {
	name string
	err  error
}

// branchDeleteCompleteMsg is sent after a branch is deleted.
type branchDeleteCompleteMsg struct {
	name string
	err  error
}

// branchRenameCompleteMsg is sent after a branch is renamed.
type branchRenameCompleteMsg struct {
	oldName string
	newName string
	err     error
}

// branchDeleteConfirmedMsg is sent when the user confirms branch deletion
// via the error modal's "Retry" button (repurposed as "Delete").
type branchDeleteConfirmedMsg struct {
	name     string
	isRemote bool
}
```

**New commands**:

```go
// loadBranchesCmd fetches the branch list asynchronously.
func loadBranchesCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		branches, err := git.ListBranches(repoPath)
		return branchListResultMsg{branches: branches, err: err}
	}
}

// switchBranchCmd runs git checkout asynchronously.
func switchBranchCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		err := git.SwitchBranch(repoPath, branch)
		return branchSwitchCompleteMsg{branchName: branch, err: err}
	}
}

// createBranchCmd creates a new branch and switches to it.
func createBranchCmd(repoPath, name string) tea.Cmd {
	return func() tea.Msg {
		// Create the branch at HEAD
		if err := git.CreateBranch(repoPath, name, ""); err != nil {
			return branchCreateCompleteMsg{name: name, err: err}
		}
		// Switch to the newly created branch
		if err := git.SwitchBranch(repoPath, name); err != nil {
			return branchCreateCompleteMsg{name: name, err: err}
		}
		return branchCreateCompleteMsg{name: name, err: nil}
	}
}

// deleteBranchCmd deletes a local or remote branch.
// The `remote` parameter is the fallback remote name (e.g., "origin") used only if
// the branch name doesn't contain a "/" prefix (rare edge case). Normally, the remote
// name is extracted from the branch name itself (e.g., "origin/feature-x" → remote="origin").
func deleteBranchCmd(repoPath, remote, name string, isRemote bool) tea.Cmd {
	return func() tea.Msg {
		if isRemote {
			// Extract the branch name after the remote prefix: "origin/feature-x" → "feature-x"
			parts := strings.SplitN(name, "/", 2)
			remoteName := remote   // fallback if name has no "/" (shouldn't happen for remote branches)
			branchName := name
			if len(parts) == 2 {
				remoteName = parts[0]
				branchName = parts[1]
			}
			err := git.DeleteRemoteBranch(repoPath, remoteName, branchName)
			return branchDeleteCompleteMsg{name: name, err: err}
		}
		err := git.DeleteBranch(repoPath, name)
		return branchDeleteCompleteMsg{name: name, err: err}
	}
}

// renameBranchCmd renames a branch.
func renameBranchCmd(repoPath, oldName, newName string) tea.Cmd {
	return func() tea.Msg {
		err := git.RenameBranch(repoPath, oldName, newName)
		return branchRenameCompleteMsg{oldName: oldName, newName: newName, err: err}
	}
}
```

**New model fields** — add to the `Model` struct:

```go
type Model struct {
	// ... existing fields ...

	// Branch operations
	branchDropdown views.BranchDropdownModel
	switching      bool   // true while a branch switch is in progress
	pendingDelete  *views.BranchDeleteMsg // held while waiting for confirmation
}
```

**Update `New()`** — initialize the branch dropdown:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		// ... existing fields ...
		branchDropdown: views.NewBranchDropdown(),
	}
}
```

**How branch creation works end-to-end**:

1. User presses `B` to open the branch dropdown → dropdown becomes visible
2. User presses `c` → dropdown enters create mode, showing "New branch: █"
3. User types a branch name (e.g., `feature-new-thing`) and presses `Enter`
4. Dropdown sends `BranchCreateMsg{Name: "feature-new-thing"}` and hides itself
5. App's `Update()` handles `BranchCreateMsg`:
   - Sets `m.switching = true` (shows spinner in header)
   - Dispatches `createBranchCmd(m.repoPath, msg.Name)`
6. `createBranchCmd` runs `git branch feature-new-thing` then `git checkout feature-new-thing`
7. `branchCreateCompleteMsg` arrives:
   - Sets `m.switching = false`
   - If error → shows error modal
   - If success → refreshes status (which updates branch name in header) and caches new remote

**Handle `BranchCreateMsg`** — add this case to `Update()`:

```go
	case views.BranchCreateMsg:
		m.switching = true
		return m, createBranchCmd(m.repoPath, msg.Name)

	case branchCreateCompleteMsg:
		m.switching = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Branch Error",
				"Failed to create branch \""+msg.name+"\": "+msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Success — refresh status to update the branch name in the header
		return m, refreshStatusCmd(m.repoPath)
```

### 15.3 Switch Branch

**File**: `internal/tui/app/app.go` (modify existing)

Add `B` keybinding to `handleMainKey()` in the navigable mode section. When pressed, it
loads the branch list asynchronously. When the list arrives, the dropdown opens.

```go
	// ── Navigable mode keybindings ──
	switch msg.String() {
	// ... existing cases (q, ?, tab, 1, 2, 3, `, escape, S, F, P) ...

	case "B":
		// Open branch dropdown — load branches first
		if !m.switching {
			return m, loadBranchesCmd(m.repoPath)
		}
		return m, nil
	}
```

**Handle `branchListResultMsg`** — add this case to `Update()`:

```go
	case branchListResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Branch Error",
				"Failed to list branches: "+msg.err.Error(),
				true,
				loadBranchesCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Open the dropdown with the loaded branches
		m.branchDropdown.SetBranches(msg.branches)
		m.branchDropdown.Open(m.width, m.height)
		return m, nil
```

**Handle `BranchSwitchMsg`** — add this case to `Update()`:

```go
	case views.BranchSwitchMsg:
		m.switching = true
		return m, switchBranchCmd(m.repoPath, msg.Name)

	case branchSwitchCompleteMsg:
		m.switching = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Branch Error",
				"Failed to switch to \""+msg.branchName+"\": "+msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Success — refresh status and re-cache the remote
		// (the new branch might track a different remote)
		m.remote = git.GetRemote(m.repoPath)
		return m, refreshStatusCmd(m.repoPath)
```

**How branch switching works end-to-end**:

1. User presses `B` → `loadBranchesCmd` dispatched
2. `branchListResultMsg` arrives → dropdown opens with the branch list
3. User types to filter, navigates with `j`/`k`, presses `Enter` on a branch
4. Dropdown sends `BranchSwitchMsg{Name: "feature-x"}` and hides itself
5. App handles `BranchSwitchMsg`:
   - Sets `m.switching = true` (shows "Switching..." in header)
   - Dispatches `switchBranchCmd(m.repoPath, "feature-x")`
6. `switchBranchCmd` runs `git checkout feature-x --`
7. `branchSwitchCompleteMsg` arrives:
   - Sets `m.switching = false`
   - If error → error modal (e.g., "error: Your local changes would be overwritten")
   - If success → refreshes status (updates branch name, ahead/behind), re-caches remote

**Why we re-cache `m.remote` after switching**:

Different branches might track different remotes. For example, `main` might track `origin`
while `fork-branch` might track `upstream`. After switching, we call `git.GetRemote()` to
ensure the cached remote name matches the new branch's remote. This affects fetch and pull
operations (Phase 14).

**Update the `HeaderData` struct and `viewMain()`** — add the `Switching` field:

**File**: `internal/tui/views/header.go` (modify existing)

Add a new field to `HeaderData`:

```go
type HeaderData struct {
	RepoName      string
	BranchName    string
	Ahead         int
	Behind        int
	HasUpstream   bool
	Fetching      bool
	Pulling       bool
	Switching     bool      // true when a branch switch is in progress
	LastFetchTime time.Time
}
```

Update `RenderHeader` to show the switching spinner:

```go
	// ── Action button (context-aware) with status ──
	var actionText string
	if data.Switching {
		actionText = activeStyle.Render("⟳ Switching...")
	} else if data.Pulling {
		actionText = activeStyle.Render("⟳ Pulling...")
	} else if data.Fetching {
		actionText = activeStyle.Render("⟳ Fetching...")
	} else {
		action := actionLabel(data)
		if !data.LastFetchTime.IsZero() {
			elapsed := time.Since(data.LastFetchTime)
			action += " (" + formatDuration(elapsed) + ")"
		}
		actionText = actionStyle.Render(action)
	}
```

Update the `viewMain()` call in `app.go` to pass the new field:

```go
	header := views.RenderHeader(views.HeaderData{
		RepoName:      git.RepoName(m.repoPath),
		BranchName:    m.branchName,
		Ahead:         m.ahead,
		Behind:        m.behind,
		HasUpstream:   m.hasUpstream,
		Fetching:      m.fetching,
		Pulling:       m.pulling,
		Switching:     m.switching,
		LastFetchTime: m.lastFetchTime,
	}, dim.Width)
```

**Integrate the branch dropdown into `View()`**:

The dropdown renders as a fullscreen overlay, like the help and settings overlays. Update the
`stateMain` case in `View()`:

```go
		case stateMain:
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.branchDropdown.Visible {
				content = m.branchDropdown.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else {
				content = m.viewMain()
			}
```

**Forward keys to the branch dropdown when visible**:

Update `handleMainKey()` to route keys to the dropdown before the navigable mode switch:

```go
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Branch dropdown takes priority over navigable mode ──
	if m.branchDropdown.Visible {
		var cmd tea.Cmd
		m.branchDropdown, cmd = m.branchDropdown.Update(msg)
		return m, cmd
	}

	// ── Settings overlay ──
	// (existing settings handling)

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "escape" {
			m.showHelp = false
		}
		return m, nil
	}

	// ... rest of handleMainKey unchanged ...
}
```

**Handle `BranchDropdownClosedMsg`** — add this case to `Update()`:

```go
	case views.BranchDropdownClosedMsg:
		// Dropdown was closed without selecting — no action needed
		return m, nil
```

### 15.4 Delete Branch

**File**: `internal/tui/app/app.go` (modify existing)

Branch deletion uses a two-step confirmation flow. When the user presses `d` in the branch
dropdown, the dropdown sends a `BranchDeleteMsg`. The app stores this in `pendingDelete` and
shows a confirmation modal using the error modal system. If the user confirms (presses the
"Delete" button), the app dispatches the actual delete command.

**Handle `BranchDeleteMsg`** — add this case to `Update()`:

```go
	case views.BranchDeleteMsg:
		// Store the pending delete and show confirmation
		m.pendingDelete = &msg

		var typeStr string
		if msg.IsRemote {
			typeStr = "remote"
		} else {
			typeStr = "local"
		}

		// We reuse the error modal as a confirmation dialog. The key trick:
		// the 4th argument (func() tea.Msg) is the modal's "RetryCmd". When
		// the user clicks "Retry", it produces branchDeleteConfirmedMsg —
		// which triggers the actual delete. If the user presses "Dismiss" or
		// Esc instead, ErrorDismissedMsg is sent and pendingDelete is cleared.
		m.errorModal = views.ShowError(
			"Delete Branch?",
			"Are you sure you want to delete the "+typeStr+" branch \""+msg.Name+"\"?\n\n"+
				"This action cannot be undone.",
			true, // retryable — we repurpose "Retry" as "Delete"
			func() tea.Msg {
				return branchDeleteConfirmedMsg{
					name:     msg.Name,
					isRemote: msg.IsRemote,
				}
			},
			m.width, m.height,
		)
		return m, nil
```

**Handle `branchDeleteConfirmedMsg`** — add this case to `Update()`:

```go
	case branchDeleteConfirmedMsg:
		// User confirmed the deletion
		m.pendingDelete = nil
		// m.remote is the cached remote name (e.g., "origin").
		// deleteBranchCmd only uses it when msg.isRemote is true.
		return m, deleteBranchCmd(m.repoPath, m.remote, msg.name, msg.isRemote)

	case branchDeleteCompleteMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Delete Error",
				"Failed to delete branch \""+msg.name+"\": "+msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Success — refresh status in case we deleted a tracking branch
		return m, refreshStatusCmd(m.repoPath)
```

**Handle `ErrorDismissedMsg` update** — when the user dismisses the delete confirmation
without clicking "Delete" (e.g., presses `Esc`), clear the pending delete:

```go
	case views.ErrorDismissedMsg:
		m.pendingDelete = nil
		return m, nil
```

**How the delete confirmation flow works**:

1. User presses `d` on a branch in the dropdown
2. Dropdown sends `BranchDeleteMsg{Name: "old-feature", IsRemote: false}` and hides itself
3. App stores `pendingDelete` and shows: "Delete Branch? Are you sure..."
4. The modal has "Retry" (which we use as "Delete") and "Dismiss" buttons
5. **User clicks "Delete" (Retry)**: `branchDeleteConfirmedMsg` is sent via the `RetryCmd` →
   `deleteBranchCmd` dispatched → `branchDeleteCompleteMsg` arrives
6. **User clicks "Dismiss" or presses Esc**: `ErrorDismissedMsg` arrives → `pendingDelete`
   cleared, nothing happens

**Why we repurpose the error modal for confirmation**:

The error modal already has the Retry/Dismiss two-button pattern. Repurposing it for
confirmations avoids creating a separate confirmation modal component. The "Retry" button
acts as the "confirm" action — we pass the actual delete command as `RetryCmd`. This is the
same pattern the app already uses for retryable errors, just with different text.

**Config-gated confirmation**:

The `confirmations.branch_delete` config option (Phase 13) controls whether the confirmation
is shown. If set to `false`, the delete happens immediately without a modal:

```go
	case views.BranchDeleteMsg:
		if m.config.Confirmations.BranchDelete {
			// Show confirmation modal (code above)
			m.pendingDelete = &msg
			// ... show modal ...
		} else {
			// Skip confirmation — delete immediately
			return m, deleteBranchCmd(m.repoPath, m.remote, msg.Name, msg.IsRemote)
		}
```

### 15.5 Rename Branch

**File**: `internal/tui/app/app.go` (modify existing)

Branch renaming is simpler than deletion — no confirmation needed. When the user presses `r`
in the branch dropdown, the dropdown enters rename mode with the current name pre-filled.
When the user presses `Enter`, the dropdown sends `BranchRenameMsg`.

**Handle `BranchRenameMsg`** — add this case to `Update()`:

```go
	case views.BranchRenameMsg:
		return m, renameBranchCmd(m.repoPath, msg.OldName, msg.NewName)

	case branchRenameCompleteMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Rename Error",
				"Failed to rename \""+msg.oldName+"\" to \""+msg.newName+"\": "+msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Success — refresh status (branch name may have changed if we renamed current branch)
		return m, refreshStatusCmd(m.repoPath)
```

**How renaming works end-to-end**:

1. User presses `B` to open the branch dropdown
2. User navigates to a local branch and presses `r`
3. Dropdown enters rename mode: "Rename to: current-name█"
4. User edits the name (backspace to delete, type new characters)
5. User presses `Enter` → dropdown sends `BranchRenameMsg{OldName: "old", NewName: "new"}`
6. App dispatches `renameBranchCmd`
7. `branchRenameCompleteMsg` arrives:
   - If error (e.g., "a branch named 'new' already exists") → error modal
   - If success → status refresh (the header updates if the current branch was renamed)

**Why remote branches can't be renamed**:

Git doesn't support renaming remote branches directly. To "rename" a remote branch, you'd
need to: (1) create a new branch with the new name, (2) push it, (3) delete the old remote
branch, (4) update tracking. This is complex and error-prone, so the TUI blocks `r` on
remote branches. The user can do this in the embedded terminal if needed.

**Update the help overlay** — add branch operation keybindings:

**File**: `internal/tui/views/helpoverlay.go` (modify existing)

Add a new section to the help rows:

```go
	// Branch Operations section (add to the help rows)
	"",
	sectionStyle.Render("Branch Operations"),
	row("B", "Open branch dropdown"),
	row("Enter", "Switch to selected branch (in dropdown)"),
	row("c", "Create new branch (in dropdown)"),
	row("d", "Delete selected branch (in dropdown)"),
	row("r", "Rename selected branch (in dropdown)"),
```

**Forward `WindowSizeMsg` to the branch dropdown** — update the `WindowSizeMsg` handler in
`Update()`:

```go
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		if m.branchDropdown.Visible {
			m.branchDropdown, _ = m.branchDropdown.Update(msg)
		}
		return m, nil
```

**What changed from Phase 14**:

1. **New `internal/git/branch.go`**: `ListBranches()`, `CreateBranch()`, `SwitchBranch()`,
   `DeleteBranch()`, `DeleteRemoteBranch()`, `RenameBranch()` — six git command wrappers
2. **New `internal/tui/views/branchdropdown.go`**: `BranchDropdownModel` with browse/create/
   rename modes, filterable branch list, action shortcuts (`c`/`d`/`r`), and five message types
3. **New messages**: `branchListResultMsg`, `branchSwitchCompleteMsg`,
   `branchCreateCompleteMsg`, `branchDeleteCompleteMsg`, `branchRenameCompleteMsg`,
   `branchDeleteConfirmedMsg`
4. **New commands**: `loadBranchesCmd`, `switchBranchCmd`, `createBranchCmd`,
   `deleteBranchCmd`, `renameBranchCmd`
5. **New model fields**: `branchDropdown`, `switching`, `pendingDelete`
6. **`handleMainKey()` updated**: `B` opens the branch dropdown; branch dropdown gets key
   priority when visible (above help overlay, below error modal)
7. **`Update()` additions**: handles all branch messages — list result, switch/create/delete/
   rename complete, delete confirmation, dropdown closed
8. **`View()` updated**: renders branch dropdown overlay when visible
9. **`HeaderData` updated**: new `Switching` field; `RenderHeader` shows `⟳ Switching...`
   during branch operations
10. **`WindowSizeMsg` handler updated**: forwards size changes to the branch dropdown
11. **Help overlay updated**: `B`, `c`, `d`, `r` keybindings listed under "Branch Operations"

### 15.6 Test It

Build and run:

```bash
go build -o leogit ./cmd/leogit && ./leogit /path/to/any/git/repo
# or: go run ./cmd/leogit /path/to/any/git/repo
```

Choose a repo with multiple branches and a remote for best coverage.

**Test 1 — Open and close branch dropdown**:

1. Press `B` — the branch dropdown should appear as a centered overlay
2. The current branch should have a `✓` prefix and be styled in green
3. Remote-only branches should appear in gray italic (e.g., `origin/feature-x`)
4. The filter cursor (`█`) should be visible at the top
5. Press `Esc` — the dropdown should close and return to the main layout
6. All global keys (`q`, `?`, `Tab`, `1/2/3`) should be blocked while the dropdown is open

**Test 2 — Filter branches**:

1. Press `B` to open the dropdown
2. Start typing a branch name (e.g., "main") — the list should filter in real-time
3. The filter is case-insensitive: typing "MAIN" should match "main"
4. Press `Backspace` to remove characters from the filter
5. Press `Esc` once to clear the filter (all branches reappear)
6. Press `Esc` again to close the dropdown

**Test 3 — Switch branch with Enter**:

1. Press `B` to open the dropdown
2. Navigate with `j`/`k` to a different branch
3. Press `Enter` — the dropdown should close
4. The header should show `⟳ Switching...` briefly
5. After switching, the header should show the new branch name
6. The file list should update if the branches have different changes
7. The ahead/behind indicators should update for the new branch's tracking state

**Test 4 — Switch to current branch (no-op)**:

1. Press `B` to open the dropdown
2. Navigate to the branch marked with `✓` (the current branch)
3. Press `Enter` — nothing should happen (no switch, no error)
4. The dropdown stays open

**Test 5 — Switch to remote branch**:

1. Press `B` and find a remote-only branch (gray italic, e.g., `origin/feature-x`)
2. Press `Enter` — the header should show `⟳ Switching...`
3. After switching, the branch name should show `feature-x` (local, not `origin/feature-x`)
4. Git automatically created a local tracking branch

**Test 6 — Create branch**:

1. Press `B` to open the dropdown
2. Press `c` — the input area should change to "New branch: █"
3. Type a new branch name (e.g., `test-branch-123`)
4. Press `Enter` — the dropdown should close
5. The header should show `⟳ Switching...` briefly (create + switch)
6. After completion, the header should show `test-branch-123` as the current branch
7. The new branch should be based on the previous HEAD

**Test 7 — Create branch with invalid name**:

1. Press `B`, then `c`
2. Type a name with spaces or invalid characters (e.g., `my branch`)
3. Press `Enter` — an error modal should appear: "Branch Error: Failed to create..."
4. Git rejects invalid branch names (spaces, `..`, leading dots, etc.)

**Test 8 — Create branch, cancel with Esc**:

1. Press `B`, then `c`
2. Type something, then press `Esc`
3. The input should disappear and you're back in browse mode (not closed)
4. Press `Esc` again to close the dropdown

**Test 9 — Delete local branch**:

1. First create a test branch: `B` → `c` → type `delete-me` → `Enter`
2. Switch back to `main`: `B` → navigate to `main` → `Enter`
3. Open the dropdown again: `B`
4. Navigate to `delete-me` and press `d`
5. A confirmation modal should appear: "Delete Branch? Are you sure..."
6. Press `Tab` to switch between "Retry" (Delete) and "Dismiss"
7. Press `Enter` on "Retry" — the branch should be deleted
8. Open the dropdown again — `delete-me` should be gone

**Test 10 — Delete current branch (blocked)**:

1. Press `B` and navigate to the current branch (marked with `✓`)
2. Press `d` — nothing should happen (can't delete the current branch)

**Test 11 — Delete confirmation dismissed**:

1. Press `B`, navigate to a deletable branch, press `d`
2. The confirmation modal appears
3. Press `Esc` — the modal closes and the branch is NOT deleted
4. Verify the branch still exists by pressing `B` again

**Test 12 — Rename local branch**:

1. Create a test branch: `B` → `c` → type `rename-me` → `Enter`
2. You should now be on `rename-me`
3. Press `B`, navigate to `rename-me`, press `r`
4. The input should show "Rename to: rename-me█" (pre-filled)
5. Use `Backspace` to clear, then type `renamed-branch`
6. Press `Enter` — the branch should be renamed
7. The header should update to show `renamed-branch`

**Test 13 — Rename remote branch (blocked)**:

1. Press `B` and navigate to a remote-only branch (gray italic)
2. Press `r` — nothing should happen (remote branches can't be renamed)

**Test 14 — Rename to existing name (error)**:

1. Press `B`, navigate to a local branch, press `r`
2. Type the name of a branch that already exists (e.g., `main`)
3. Press `Enter` — an error modal should appear saying the branch already exists

**Test 15 — Branch switch with uncommitted changes**:

1. Make some changes in the working tree (modify a file, don't commit)
2. Press `B` and try to switch to another branch
3. If the changes conflict with the target branch, an error modal should appear:
   "error: Your local changes to the following files would be overwritten by checkout"
4. If the changes don't conflict, git switches cleanly and carries the changes

**Test 16 — Concurrent operation prevention**:

1. Press `B` → select a branch → press `Enter` to start switching
2. While `⟳ Switching...` is showing, press `B` — nothing should happen
3. While `⟳ Switching...` is showing, press `F` — nothing should happen
4. After the switch completes, `B` and `F` should work again

**Test 17 — Branch dropdown scroll**:

1. Navigate to a repo with many branches (10+)
2. Press `B` — the dropdown should show a scrollable list
3. Press `j` repeatedly to scroll down past the visible area
4. A scroll indicator (e.g., "(15/23)") should appear at the bottom
5. Press `k` to scroll back up

**Test 18 — Branch delete with confirmation disabled**:

1. Open settings (`S`) and navigate to Confirmations → Branch Delete
2. Toggle it to `false`
3. Close settings, press `B`, navigate to a branch, press `d`
4. The branch should be deleted immediately without a confirmation modal

**Phase 15 is complete when**: pressing `B` opens a fullscreen branch dropdown overlay with
a filterable list of local and remote branches; the current branch is marked with `✓`;
remote branches appear in gray italic; typing filters the list in real-time; `j`/`k`
navigate; `Enter` switches to the selected branch with a spinner in the header; `c` enters
create mode where typing a name and pressing `Enter` creates and switches to a new branch;
`d` triggers a delete confirmation modal (or deletes immediately if confirmation is disabled)
for local and remote branches; `r` enters rename mode for local branches; the current branch
cannot be deleted or switched to; remote branches cannot be renamed; all operations show
error modals on failure; the branch dropdown blocks global keys while visible; `Esc` closes
the dropdown; status refreshes after every branch operation to update the header; and the
`Switching` field controls the header's spinner display.

## Phase 16 — History & Log

Phase 16 implements the History tab — a scrollable commit log with commit details, changed
files per commit, and a diff viewer for individual files within a commit. When the user
presses `Tab` to switch from the Changes tab, the commit log loads asynchronously in batches
of 50. Selecting a commit shows its metadata, lists the files it changed, and allows viewing
syntax-highlighted diffs for each file.

> **Note**: This phase calls `DiffViewModel.Clear()` in several places, but `Clear()` was not defined in Phase 7. You need to add it to `internal/tui/components/diffview.go` alongside the existing `SetDiff`/`SetLoading`/`SetError` methods. It should reset all display state fields to their zero values: `file`, `fileDiff`, `loading`, `hasContent`, `errMsg`, `offset`, `totalLines`, and (if you've already implemented Phase 8) `cursor`, `selection`, and `allLines`. Leave `width`, `height`, and `sideBySide` untouched — those are layout/preference fields set externally.

**Layout on the History tab:**

```
┌──────────────────┬──────────────────────────────────────┐
│ [Changes] History│ Commit Details                       │
├──────────────────┤ summary, author, date, hash          │
│                  ├──────────────────┬───────────────────┤
│  Commit List     │ Changed Files    │   Diff Viewer     │
│  (scrollable,    │  file1.go  [M]   │  (selected file,  │
│   lazy-loaded)   │  file2.go  [+]   │   syntax          │
│                  │  file3.go  [-]   │   highlighted)    │
│                  │                  │                   │
└──────────────────┴──────────────────┴───────────────────┘
```

The sidebar (left column) contains the Commit List using the full sidebar height — the
Pane 3 slot used by the Changes tab's Commit Message is merged into Pane 1 on the History
tab. The main column (right) has a fixed-height Commit Details area at the top, with the
remaining height split horizontally between Changed Files and Diff Viewer.

Pane focus shortcuts on the History tab:

| Key | Pane |
|-----|------|
| `1` | Commit List |
| `2` | Changed Files |
| `3` | Diff Viewer |

**New files:**
- `internal/git/log.go` — git log parsing, commit file listing, relative date formatting
- `internal/tui/components/commitlist.go` — scrollable commit list component
- `internal/tui/views/commitdetail.go` — commit detail renderer

**Modified files:**
- `internal/tui/app/app.go` — messages, commands, Model fields, layout, handlers
- `internal/tui/views/helpoverlay.go` — History tab keybindings section

### 16.0 Git Log Parser

**File**: `internal/git/log.go` (new file)

This file wraps `git log` and `git diff-tree` for the History tab. The log command uses a
custom format string with `\x01` (SOH) as the field separator and `\x00` (NULL) as the
record separator. This avoids conflicts with multi-line commit bodies and trailers — these
control characters never appear in legitimate commit messages.

The format string fields (separated by `\x01`):

| # | Placeholder | Description |
|---|-------------|-------------|
| 1 | `%H` | full 40-character SHA |
| 2 | `%h` | abbreviated SHA (7 chars) |
| 3 | `%s` | subject/summary (first line of commit message) |
| 4 | `%b` | body (remaining lines, can be multi-line) |
| 5 | `%an` | author name |
| 6 | `%ae` | author email |
| 7 | `%ad` | author date (raw format: unix timestamp + timezone) |
| 8 | `%cn` | committer name |
| 9 | `%ce` | committer email |
| 10 | `%cd` | committer date (raw format) |
| 11 | `%P` | parent SHAs (space-separated, empty for root commit) |
| 12 | `%(trailers:unfold,only)` | git trailers (can be multi-line) |
| 13 | `%D` | ref decorations (e.g., "HEAD -> main, origin/main") |

The `--date=raw` flag outputs dates as `"1647360000 -0500"` (unix timestamp + offset).
The `--max-count` and `--skip` flags enable lazy loading in batches.

`GetCommitFiles` uses `git diff-tree --no-commit-id -r --name-status` to list files changed
in a specific commit. The output format is:

```
M	src/main.go
A	src/new_file.go
D	src/old_file.go
R100	src/old_name.go	src/new_name.go
```

This is parsed into the same `FileEntry` structs from Phase 6, so the existing
`FileListModel` component can display them directly.

```go
package git

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"time"
)

// CommitInfo holds metadata about a single commit from git log.
type CommitInfo struct {
	SHA            string    // full 40-character hash
	ShortSHA       string    // abbreviated hash (7 characters)
	Summary        string    // first line of commit message
	Body           string    // remaining commit message lines (may be empty)
	AuthorName     string
	AuthorEmail    string
	AuthorDate     time.Time
	CommitterName  string
	CommitterEmail string
	CommitterDate  time.Time
	Parents        []string // parent SHA(s); empty for root commits, 2+ for merges
	Trailers       string   // git trailers (e.g., "Signed-off-by: ...")
	Refs           string   // ref decorations (e.g., "HEAD -> main, origin/main")
}

// LogOptions controls which commits to fetch and how many.
type LogOptions struct {
	MaxCount int // maximum number of commits to return (default 50)
	Skip     int // number of commits to skip (for pagination)
}

// logFormat is the git log format string. Fields are separated by \x01 (SOH),
// records by \x00 (NULL). This handles multi-line bodies and trailers safely.
const logFormat = "%H%x01%h%x01%s%x01%b%x01%an%x01%ae%x01%ad%x01%cn%x01%ce%x01%cd%x01%P%x01%(trailers:unfold,only)%x01%D%x00"

// GetLog returns a list of commits from the repository's current branch.
// Commits are sorted newest-first (default git log order). Use opts.Skip
// for pagination — the app loads 50 commits at a time and fetches more
// as the user scrolls down.
func GetLog(repoPath string, opts LogOptions) ([]CommitInfo, error) {
	if opts.MaxCount == 0 {
		opts.MaxCount = 50
	}

	cmd := exec.Command("git",
		"log",
		"--date=raw",
		fmt.Sprintf("--max-count=%d", opts.MaxCount),
		fmt.Sprintf("--skip=%d", opts.Skip),
		"--format="+logFormat,
		"--no-show-signature",
		"--no-color",
		"--",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git log: %w", err)
	}

	return parseLogOutput(string(out))
}

// parseLogOutput splits the raw git log output into CommitInfo structs.
// Each commit record is separated by \x00 (NULL) and each field within
// a record is separated by \x01 (SOH).
func parseLogOutput(raw string) ([]CommitInfo, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, nil
	}

	// Split by \x00 (record separator). The trailing \x00 produces an empty
	// last element which we skip.
	records := strings.Split(raw, "\x00")
	var commits []CommitInfo

	for _, record := range records {
		record = strings.TrimSpace(record)
		if record == "" {
			continue
		}

		// Split by \x01 (field separator). We expect 13 fields, but some
		// may be missing if the commit has no trailers/refs.
		fields := strings.SplitN(record, "\x01", 13)
		if len(fields) < 10 {
			continue // malformed record, skip
		}

		commit := CommitInfo{
			SHA:            fields[0],
			ShortSHA:       fields[1],
			Summary:        fields[2],
			Body:           strings.TrimSpace(fields[3]),
			AuthorName:     fields[4],
			AuthorEmail:    fields[5],
			AuthorDate:     parseRawDate(fields[6]),
			CommitterName:  fields[7],
			CommitterEmail: fields[8],
			CommitterDate:  parseRawDate(fields[9]),
		}

		if len(fields) > 10 {
			commit.Parents = splitParents(fields[10])
		}
		if len(fields) > 11 {
			commit.Trailers = strings.TrimSpace(fields[11])
		}
		if len(fields) > 12 {
			commit.Refs = strings.TrimSpace(fields[12])
		}

		commits = append(commits, commit)
	}

	return commits, nil
}

// parseRawDate parses a git raw date string ("1647360000 -0500") into time.Time.
// The timezone offset is parsed but we use time.Unix which returns UTC — the
// RelativeDate function only cares about duration since the timestamp.
func parseRawDate(raw string) time.Time {
	raw = strings.TrimSpace(raw)
	parts := strings.SplitN(raw, " ", 2)
	if len(parts) == 0 {
		return time.Time{}
	}

	unix, err := strconv.ParseInt(parts[0], 10, 64)
	if err != nil {
		return time.Time{}
	}

	return time.Unix(unix, 0)
}

// splitParents splits the space-separated parent SHA string into a slice.
// Returns nil for root commits (empty string).
func splitParents(raw string) []string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}
	return strings.Fields(raw)
}

// GetCommitFiles returns the list of files changed in a specific commit.
// Uses git diff-tree to produce a name-status listing. The returned FileEntry
// structs reuse the same types (StatusModified, StatusNew, etc.)
// with Staged always false (commits don't have a staging concept).
func GetCommitFiles(repoPath, sha string) ([]FileEntry, error) {
	cmd := exec.Command("git",
		"diff-tree",
		"--no-commit-id",
		"-r",
		"--name-status",
		sha,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git diff-tree: %w", err)
	}

	return parseDiffTree(string(out)), nil
}

// parseDiffTree parses the output of git diff-tree --name-status into FileEntry structs.
// Format per line: "STATUS\tPATH" or "STATUS\tOLD_PATH\tNEW_PATH" for renames/copies.
func parseDiffTree(raw string) []FileEntry {
	var files []FileEntry

	for _, line := range strings.Split(strings.TrimSpace(raw), "\n") {
		if line == "" {
			continue
		}

		parts := strings.SplitN(line, "\t", 3)
		if len(parts) < 2 {
			continue
		}

		statusCode := parts[0]
		entry := FileEntry{Path: parts[1]}

		switch {
		case statusCode == "M":
			entry.Status = StatusModified
		case statusCode == "A":
			entry.Status = StatusNew
		case statusCode == "D":
			entry.Status = StatusDeleted
		case strings.HasPrefix(statusCode, "R"):
			entry.Status = StatusRenamed
			if len(parts) > 2 {
				entry.OrigPath = parts[1]
				entry.Path = parts[2]
			}
		case strings.HasPrefix(statusCode, "C"):
			entry.Status = StatusCopied
			if len(parts) > 2 {
				entry.Path = parts[2]
			}
		case statusCode == "T":
			entry.Status = StatusTypeChanged
		default:
			entry.Status = StatusUnknown
		}

		files = append(files, entry)
	}

	return files
}

// RelativeDate formats a time.Time as a human-readable relative string.
// Used by the commit list to show "2 hours ago", "3 days ago", etc.
func RelativeDate(t time.Time) string {
	if t.IsZero() {
		return ""
	}

	d := time.Since(t)

	switch {
	case d < time.Minute:
		return "just now"
	case d < time.Hour:
		m := int(d.Minutes())
		if m == 1 {
			return "1 minute ago"
		}
		return fmt.Sprintf("%d minutes ago", m)
	case d < 24*time.Hour:
		h := int(d.Hours())
		if h == 1 {
			return "1 hour ago"
		}
		return fmt.Sprintf("%d hours ago", h)
	case d < 30*24*time.Hour:
		days := int(d.Hours() / 24)
		if days == 1 {
			return "1 day ago"
		}
		return fmt.Sprintf("%d days ago", days)
	case d < 365*24*time.Hour:
		months := int(d.Hours() / 24 / 30)
		if months <= 1 {
			return "1 month ago"
		}
		return fmt.Sprintf("%d months ago", months)
	default:
		years := int(d.Hours() / 24 / 365)
		if years <= 1 {
			return "1 year ago"
		}
		return fmt.Sprintf("%d years ago", years)
	}
}
```

**How it works:**

- `GetLog` runs `git log` with the custom format string. Fields are separated by `\x01`
  (SOH) and commits by `\x00` (NULL). This is critical because the body field (`%b`) and
  trailers field (`%(trailers:unfold,only)`) can contain newlines — using `\n` as a separator
  would break parsing. `\x01` and `\x00` are control characters that never appear in commit
  messages. This `\x01`/`\x00` pattern is used by other graphical Git clients for the same reason.
- `parseRawDate` handles git's raw date format (unix timestamp + timezone offset). The
  timezone is available but we use `time.Unix()` which returns UTC. `RelativeDate()` only
  needs the duration since the timestamp, so timezone doesn't matter.
- `GetCommitFiles` uses `git diff-tree` instead of `git diff` because it operates on a
  single commit without needing to specify parent ranges. The `--no-commit-id` flag
  suppresses the commit SHA line, and `-r` recurses into subdirectories. Status codes
  (`M`, `A`, `D`, `R100`, `C100`, `T`) map to the same `FileStatus` constants from Phase 6.
  For renames (`R`) and copies (`C`), the percentage suffix (e.g., `R100` = 100% rename) is
  ignored — we only care about the status letter. **Note**: `StatusCopied`, `StatusTypeChanged`,
  and `StatusUnknown` are used in `parseDiffTree` but were not defined in Phase 6's `FileStatus`
  iota block. You need to add them to the `const` block in `internal/git/status.go` (after
  `StatusRenamed`) before this code will compile.
- `RelativeDate` provides human-readable time distances. It uses simple thresholds (30-day
  months, 365-day years) — accurate enough for UI display. The function is exported because
  the commit detail view (section 16.2) also uses it.

### 16.1 Commit List UI

**File**: `internal/tui/components/commitlist.go` (new file)

This is the scrollable commit list for the History tab's Pane 1. It follows the same
component pattern as `FileListModel` (Phase 6.3): a struct with `cursor`, `offset`,
`width`, `height`, and `Update`/`View` methods.

Each row shows the short SHA (dim), commit summary (truncated to fit), optional ref
decorations in green (e.g., "HEAD -> main"), and relative date (dim, right-aligned). The
cursor row gets a blue highlight background. The list scrolls to keep the cursor visible.

When the user presses `Enter`, a `CommitSelectedMsg` is sent to the app. When the cursor
gets within 5 commits of the bottom, a `LoadMoreCommitsMsg` triggers lazy loading of the
next batch (section 16.3).

```go
package components

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// CommitSelectedMsg is sent when the user presses Enter on a commit in the list.
// The app uses this to load the commit's changed files and show its details.
type CommitSelectedMsg struct {
	Index  int
	Commit git.CommitInfo
}

// LoadMoreCommitsMsg is sent when the cursor approaches the bottom of the loaded
// commits, signaling that the app should load the next batch via pagination.
type LoadMoreCommitsMsg struct{}

// loadMoreThreshold is how close to the bottom the cursor must be to trigger loading.
const loadMoreThreshold = 5

// CommitListModel displays a scrollable list of commits with SHA, summary, and date.
type CommitListModel struct {
	Commits []git.CommitInfo // loaded commits (newest first)
	cursor  int              // index of the highlighted commit
	offset  int              // scroll offset (first visible index)
	width   int              // available width for rendering (inner, excluding borders)
	height  int              // available height in rows (inner, excluding borders and title)
}

// NewCommitList creates an empty commit list. Commits are set via SetCommits().
func NewCommitList() CommitListModel {
	return CommitListModel{}
}

// SetCommits replaces the entire commit list and resets the cursor if out of bounds.
// Called on initial load or when the branch changes (history invalidation).
func (m *CommitListModel) SetCommits(commits []git.CommitInfo) {
	m.Commits = commits
	if m.cursor >= len(m.Commits) {
		m.cursor = max(0, len(m.Commits)-1)
	}
	m.clampOffset()
}

// AppendCommits adds commits to the end of the list (for pagination).
// Does not change the cursor position — the user stays where they were.
func (m *CommitListModel) AppendCommits(commits []git.CommitInfo) {
	m.Commits = append(m.Commits, commits...)
}

// SetSize updates the available rendering dimensions.
// Called when the terminal resizes or when switching to the History tab.
func (m *CommitListModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.clampOffset()
}

// SelectedCommit returns the currently highlighted commit, or nil if empty.
func (m CommitListModel) SelectedCommit() *git.CommitInfo {
	if len(m.Commits) == 0 || m.cursor >= len(m.Commits) {
		return nil
	}
	return &m.Commits[m.cursor]
}

// Update handles navigation keys when the commit list pane is focused.
// Only called when Pane 1 on the History tab is the active pane.
func (m CommitListModel) Update(msg tea.Msg) (CommitListModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < len(m.Commits)-1 {
				m.cursor++
				m.clampOffset()
			}
			// Trigger lazy loading when near the bottom
			if len(m.Commits) > 0 && m.cursor >= len(m.Commits)-loadMoreThreshold {
				return m, func() tea.Msg {
					return LoadMoreCommitsMsg{}
				}
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.clampOffset()
			}
			return m, nil

		case "g":
			m.cursor = 0
			m.clampOffset()
			return m, nil

		case "G":
			if len(m.Commits) > 0 {
				m.cursor = len(m.Commits) - 1
				m.clampOffset()
			}
			// Also trigger load-more when jumping to bottom
			return m, func() tea.Msg {
				return LoadMoreCommitsMsg{}
			}

		case "enter":
			if len(m.Commits) > 0 && m.cursor < len(m.Commits) {
				commit := m.Commits[m.cursor]
				return m, func() tea.Msg {
					return CommitSelectedMsg{Index: m.cursor, Commit: commit}
				}
			}
			return m, nil
		}
	}

	return m, nil
}

// clampOffset keeps the scroll offset within valid bounds, ensuring the cursor
// is always visible within the viewport. Same algorithm as FileListModel.
func (m *CommitListModel) clampOffset() {
	if m.height <= 0 {
		return
	}

	// Cursor below viewport → scroll down
	if m.cursor >= m.offset+m.height {
		m.offset = m.cursor - m.height + 1
	}
	// Cursor above viewport → scroll up
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	// Don't scroll past the end
	maxOffset := len(m.Commits) - m.height
	if maxOffset < 0 {
		maxOffset = 0
	}
	if m.offset > maxOffset {
		m.offset = maxOffset
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

// View renders the commit list as a string that fits within the configured dimensions.
func (m CommitListModel) View() string {
	if len(m.Commits) == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Italic(true).
			Render("No commits")
	}

	// Styles
	shaStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	summaryStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9"))
	dateStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58"))
	cursorStyle := lipgloss.NewStyle().Background(lipgloss.Color("#1F3A5F"))
	refsStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	var rows []string
	end := m.offset + m.height
	if end > len(m.Commits) {
		end = len(m.Commits)
	}

	for i := m.offset; i < end; i++ {
		commit := m.Commits[i]
		isCursor := i == m.cursor

		// Build the row: "sha summary (refs) date"
		date := git.RelativeDate(commit.AuthorDate)
		dateWidth := len(date)

		// Refs decoration (e.g., "HEAD → main")
		var refsStr string
		var refsWidth int
		if commit.Refs != "" {
			refsStr = " (" + commit.Refs + ")"
			refsWidth = len(refsStr)
		}

		// Summary gets remaining space after SHA + refs + date
		usedWidth := 7 + 1 + refsWidth + 1 + dateWidth // sha + space + refs + space + date
		summaryWidth := m.width - usedWidth
		if summaryWidth < 5 {
			summaryWidth = 5
		}

		summary := truncateStr(commit.Summary, summaryWidth)
		// Pad summary to fill the column so the date aligns right
		if len(summary) < summaryWidth {
			summary += strings.Repeat(" ", summaryWidth-len(summary))
		}

		if isCursor {
			// Cursor row: render with blue background across full width
			plainRow := commit.ShortSHA + " " + summary
			if commit.Refs != "" {
				plainRow += " (" + commit.Refs + ")"
			}
			plainRow += " " + date
			row := cursorStyle.Width(m.width).Render(plainRow)
			rows = append(rows, row)
		} else {
			// Normal row: styled segments
			row := shaStyle.Render(commit.ShortSHA) + " " +
				summaryStyle.Render(summary)
			if commit.Refs != "" {
				row += refsStyle.Render(refsStr)
			}
			row += " " + dateStyle.Render(date)
			rows = append(rows, row)
		}
	}

	return strings.Join(rows, "\n")
}

// truncateStr truncates a string to maxLen, adding "…" if truncated.
func truncateStr(s string, maxLen int) string {
	if maxLen <= 0 {
		return ""
	}
	runes := []rune(s)
	if len(runes) <= maxLen {
		return s
	}
	if maxLen <= 1 {
		return "…"
	}
	return string(runes[:maxLen-1]) + "…"
}
```

**How the component works:**

- **Data flow**: `SetCommits()` replaces the entire list (initial load or branch switch).
  `AppendCommits()` adds to the end (pagination). Both keep the cursor valid.
- **Scrolling**: Uses the same `clampOffset()` algorithm as `FileListModel` — the cursor
  is always within the visible viewport. Scrolling adjusts automatically on `j`/`k`.
- **Lazy loading trigger**: When `j` or `G` moves the cursor within `loadMoreThreshold`
  (5) commits of the bottom, a `LoadMoreCommitsMsg` is emitted. The app handles this by
  loading the next 50 commits (section 16.3). The component doesn't know about pagination —
  it just signals "near bottom".
- **Row rendering**: Each row shows `SHA  summary  (refs)  date`. The summary is truncated
  with "…" to fit the available width. Refs like "HEAD -> main" appear in green bold when
  present. The date is right-aligned using padding on the summary.
- **Cursor highlight**: The cursor row renders as plain text inside a
  `cursorStyle.Width(m.width)` block. This ensures the blue background extends the full row
  width without ANSI color artifacts from styled segments bleeding into the background.

### 16.2 Commit Detail Display

**File**: `internal/tui/views/commitdetail.go` (new file)

This is a pure render function — no state, no `Update`. It takes a `CommitInfo` pointer
and returns a styled string showing the commit's metadata. When no commit is selected, it
shows a placeholder.

The detail view occupies a fixed-height area (7 rows including borders) at the top of the
main column on the History tab, showing: summary (bold white), body (dim, truncated to 2
lines), author with relative date, and full SHA hash.

```go
package views

import (
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// RenderCommitDetail renders the commit detail section for the History tab.
// Returns a styled string showing summary, body, author, date, and hash.
// If commit is nil, returns a placeholder message.
func RenderCommitDetail(commit *git.CommitInfo, width int) string {
	if commit == nil {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Italic(true).
			Render("Select a commit to view details")
	}

	summaryStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF"))

	bodyStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E"))

	labelStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58"))

	valueStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	shaStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E"))

	var lines []string

	// Summary (always shown)
	lines = append(lines, summaryStyle.Render(commit.Summary))

	// Body (if present, truncate to 2 lines to keep the detail area compact)
	if commit.Body != "" {
		bodyLines := strings.SplitN(commit.Body, "\n", 3)
		if len(bodyLines) > 2 {
			bodyLines = bodyLines[:2]
			bodyLines[1] += "…"
		}
		body := strings.Join(bodyLines, "\n")
		lines = append(lines, bodyStyle.Render(body))
	}

	// Author + relative date
	author := labelStyle.Render("Author: ") +
		valueStyle.Render(commit.AuthorName+" <"+commit.AuthorEmail+">") +
		labelStyle.Render("  ") +
		valueStyle.Render(git.RelativeDate(commit.AuthorDate))
	lines = append(lines, author)

	// Full SHA
	sha := labelStyle.Render("Commit: ") + shaStyle.Render(commit.SHA)
	lines = append(lines, sha)

	return strings.Join(lines, "\n")
}
```

**How it works:**

- The function is stateless — called every render cycle with the currently selected commit.
  This is efficient because the detail area is small (3-5 lines of text).
- The body is truncated to 2 lines to keep the detail area compact. The full body can be
  viewed in the terminal via `git show <sha>`.
- The committer info is intentionally omitted (usually the same as the author) to save
  vertical space. If different, the user can view it via `git log --format=fuller`.
- Width is passed for future use (line wrapping) but not currently used — long summaries
  or author lines are clipped by the pane border.

### 16.3 Wire It Into the App

**File**: `internal/tui/app/app.go` (modify existing)

This section connects the History tab components to the main app. The changes are:

1. New messages for commit log results, commit file lists, and commit diffs
2. New async commands that run git operations in the background
3. New Model fields for History tab state
4. Modified `handlePaneKey()` to route keys to History tab components
5. Modified `handleMainKey()` to trigger log loading on tab switch
6. New message handlers in `Update()` for all History tab messages
7. Modified `FileSelectedMsg` handler to check active tab
8. History invalidation when branch changes

**New imports** — add `diff` to the import block:

```go
import (
	// ... existing imports ...
	"github.com/LeoManrique/leogit/internal/diff"
)
```

**New messages** — add after the existing message types:

```go
// logResultMsg carries the result of a git log command.
type logResultMsg struct {
	commits []git.CommitInfo
	append  bool  // true if this is a pagination load (append to existing)
	err     error
}

// commitFilesResultMsg carries the list of files changed in a specific commit.
type commitFilesResultMsg struct {
	sha   string
	files []git.FileEntry
	err   error
}

// commitDiffLoadedMsg carries the parsed diff for a file within a specific commit.
// This is separate from DiffLoadedMsg because the diff source is
// git log --patch (historical commit) rather than git diff (working tree).
type commitDiffLoadedMsg struct {
	sha      string
	filePath string
	fileDiff *diff.FileDiff
	err      error
}
```

**New async commands** — add after the existing command functions:

```go
// loadLogCmd runs git log asynchronously and returns the commits.
// When skip > 0, the result is marked as an append (pagination load).
func loadLogCmd(repoPath string, skip int) tea.Cmd {
	return func() tea.Msg {
		commits, err := git.GetLog(repoPath, git.LogOptions{
			MaxCount: 50,
			Skip:     skip,
		})
		return logResultMsg{
			commits: commits,
			append:  skip > 0,
			err:     err,
		}
	}
}

// loadCommitFilesCmd runs git diff-tree to list files changed in a commit.
func loadCommitFilesCmd(repoPath, sha string) tea.Cmd {
	return func() tea.Msg {
		files, err := git.GetCommitFiles(repoPath, sha)
		return commitFilesResultMsg{sha: sha, files: files, err: err}
	}
}

// loadCommitDiffCmd runs git log --patch to get the diff for a single file
// within a specific commit. Uses GetCommitDiff.
func loadCommitDiffCmd(repoPath, sha, filePath string) tea.Cmd {
	return func() tea.Msg {
		raw, err := git.GetCommitDiff(repoPath, sha, filePath)
		if err != nil {
			return commitDiffLoadedMsg{sha: sha, filePath: filePath, err: err}
		}
		parsed := diff.Parse(raw)
		return commitDiffLoadedMsg{sha: sha, filePath: filePath, fileDiff: parsed}
	}
}
```

**Changes to the Model struct** — add History tab fields:

```go
	// History tab
	commitList     components.CommitListModel  // scrollable commit log (Pane 1)
	commitFiles    components.FileListModel    // files changed in selected commit (Pane 2)
	commitDiffView components.DiffViewModel    // diff for selected file in commit (Pane 3)
	selectedCommit *git.CommitInfo              // currently selected commit for detail display
	logLoading     bool                         // true while a log load is in progress
	logExhausted   bool                         // true when no more commits to load
```

`commitFiles` reuses `FileListModel` from Phase 6 and `commitDiffView` reuses
`DiffViewModel` from Phase 7. The same components work for both tabs because the
underlying data structures (`FileEntry` and `FileDiff`) are tab-agnostic. The only
difference is the data source — working tree diffs vs historical commit diffs.

**Changes to `New()`** — initialize the History tab components:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:         cfg,
		cliPath:        repoPath,
		state:          stateAuthChecking,
		authChecking:   true,
		activeTab:      core.ChangesTab,
		activePane:     core.Pane1,
		focusMode:      core.Navigable,
		fileList:       components.NewFileList(),
		diffView:       components.NewDiffView(),
		commitList:     components.NewCommitList(),
		commitFiles:    components.NewFileList(),  // reuse FileListModel
		commitDiffView: components.NewDiffView(),  // reuse DiffViewModel
	}
}
```

**Changes to `handleMainKey()`** — trigger log loading when switching to History tab.
Replace the existing `case "tab":` block:

```go
	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
			m.activePane = core.Pane1
			m.updateHistoryComponentSizes()
			// Load commit log on first visit (or after branch switch cleared it)
			if len(m.commitList.Commits) == 0 && !m.logLoading {
				m.logLoading = true
				return m, loadLogCmd(m.repoPath, 0)
			}
			return m, nil
		}
		m.activeTab = core.ChangesTab
		m.activePane = core.Pane1
		m.updateFileListSize()
		return m, nil
```

Note: `updateHistoryComponentSizes()` and `updateFileListSize()` are called on tab switch
to ensure component dimensions match the current tab's layout. The History tab has different
pane sizes than the Changes tab (e.g., Pane 1 uses the full sidebar height).

**Changes to `handlePaneKey()`** — route keys to History tab components. Replace the
existing stubs with real component forwarding:

```go
func (m Model) handlePaneKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.activePane {

	case core.Pane1:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 1 = Changed Files list
			var cmd tea.Cmd
			m.fileList, cmd = m.fileList.Update(msg)
			return m, cmd
		}
		// History tab → Pane 1 = Commit List
		var cmd tea.Cmd
		m.commitList, cmd = m.commitList.Update(msg)
		return m, cmd

	case core.Pane2:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 2 = Diff Viewer
			var cmd tea.Cmd
			m.diffView, cmd = m.diffView.Update(msg)
			return m, cmd
		}
		// History tab → Pane 2 = Changed Files in commit
		var cmd tea.Cmd
		m.commitFiles, cmd = m.commitFiles.Update(msg)
		return m, cmd

	case core.Pane3:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 3 = Commit Message
			return m, nil
		}
		// History tab → Pane 3 = Diff Viewer for commit
		var cmd tea.Cmd
		m.commitDiffView, cmd = m.commitDiffView.Update(msg)
		return m, cmd

	case core.PaneTerminal:
		// Terminal pane — same for both tabs
		return m, nil
	}

	return m, nil
}
```

**Changes to `Update()`** — add handlers for the new messages. Add these cases to the
existing `switch msg := msg.(type)` block:

```go
	case logResultMsg:
		m.logLoading = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Log Error",
				msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		if msg.append {
			// Pagination — append to existing commits
			if len(msg.commits) == 0 {
				m.logExhausted = true // no more commits to load
				return m, nil
			}
			m.commitList.AppendCommits(msg.commits)
		} else {
			// Initial load — replace all commits
			m.commitList.SetCommits(msg.commits)
			m.logExhausted = false
			// Auto-select the first commit and load its files
			if len(msg.commits) > 0 {
				m.selectedCommit = &msg.commits[0]
				return m, loadCommitFilesCmd(m.repoPath, msg.commits[0].SHA)
			}
		}
		return m, nil

	case components.CommitSelectedMsg:
		m.selectedCommit = &msg.Commit
		// Clear previous commit's files and diff
		m.commitFiles.SetFiles(nil)
		m.commitDiffView.Clear()
		// Load the selected commit's changed files
		return m, loadCommitFilesCmd(m.repoPath, msg.Commit.SHA)

	case commitFilesResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Commit Files Error",
				msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		m.commitFiles.SetFiles(msg.files)
		// Auto-select the first file and load its diff
		if len(msg.files) > 0 && m.selectedCommit != nil {
			m.commitDiffView.SetLoading()
			return m, loadCommitDiffCmd(
				m.repoPath, m.selectedCommit.SHA, msg.files[0].Path,
			)
		}
		return m, nil

	case commitDiffLoadedMsg:
		if msg.err != nil {
			m.commitDiffView.SetError(msg.err.Error())
		} else {
			file := git.FileEntry{Path: msg.filePath}
			m.commitDiffView.SetDiff(file, msg.fileDiff)
		}
		return m, nil

	case components.LoadMoreCommitsMsg:
		// Lazy loading — load the next batch of 50 commits
		if !m.logLoading && !m.logExhausted {
			m.logLoading = true
			return m, loadLogCmd(m.repoPath, len(m.commitList.Commits))
		}
		return m, nil
```

**Changes to `FileSelectedMsg` handler** — add History tab context. The existing handler
(from Phase 7) loads a working tree diff. When on the History tab, it should load a commit
diff instead. Modify the handler to check the active tab:

```go
	case components.FileSelectedMsg:
		if m.activeTab == core.HistoryTab && m.selectedCommit != nil {
			// History tab — load diff for the selected file in the selected commit
			m.commitDiffView.SetLoading()
			return m, loadCommitDiffCmd(
				m.repoPath, m.selectedCommit.SHA, msg.File.Path,
			)
		}
		// Changes tab — load working tree diff (existing behavior)
		m.diffView.SetLoading()
		return m, loadDiffCmd(m.repoPath, msg.File)
```

This works because Bubbletea is single-threaded: only the active tab's pane receives key
events, so only the relevant `FileListModel` instance (`m.fileList` or `m.commitFiles`)
can emit `FileSelectedMsg` at any given time.

**Changes to `statusResultMsg` handler** — invalidate history when branch changes. When
the user switches branches externally (e.g., via the terminal or another tool), the status
poll picks up the new branch name. The commit history needs to be cleared so it reloads
on the next History tab visit. Add this check before updating `m.branchName`:

```go
	case statusResultMsg:
		if msg.err != nil {
			return m, nil
		}
		// Clear history log if branch changed (handles external branch switches)
		if msg.status.Branch != m.branchName && m.branchName != "" {
			m.commitList.SetCommits(nil)
			m.selectedCommit = nil
			m.commitFiles.SetFiles(nil)
			m.commitDiffView.Clear()
			m.logExhausted = false
		}
		// ... existing status update code (m.branchName = msg.status.Branch, etc.) ...
```

This also handles the case where the user switches branches via the branch dropdown
(Phase 15) — `branchSwitchCompleteMsg` triggers a status refresh, which detects the
branch name change and clears the history.

**Changes to `WindowSizeMsg` handler** — update History tab component sizes:

```go
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// ... existing resize handling ...
		// Update component sizes for the active tab
		if m.activeTab == core.HistoryTab {
			m.updateHistoryComponentSizes()
		} else {
			m.updateFileListSize()
		}
		return m, nil
```

**New helper method** — `updateHistoryComponentSizes()` calculates and applies the correct
dimensions for all History tab components:

```go
// updateHistoryComponentSizes sets the width/height for all History tab components
// based on the current layout dimensions.
func (m *Model) updateHistoryComponentSizes() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// Commit list gets the full sidebar height (Pane1 + Pane3 combined).
	// Subtract 3 for borders (2) + title row (1) of the single pane.
	commitListH := dim.FileListHeight + dim.CommitMsgHeight - 3
	commitListW := dim.SidebarWidth - 2 // subtract left + right border
	m.commitList.SetSize(commitListW, commitListH)

	// Changed files and diff viewer split the main column below the detail area.
	detailHeight := 7
	bottomH := dim.DiffHeight - detailHeight - 3 // subtract borders + title
	if bottomH < 3 {
		bottomH = 3
	}
	filesW := dim.MainWidth/2 - 2                 // left half, subtract borders
	diffW := dim.MainWidth - dim.MainWidth/2 - 2  // right half, subtract borders
	m.commitFiles.SetSize(filesW, bottomH)
	m.commitDiffView.SetSize(diffW, bottomH)
}
```

### 16.4 History Tab Layout

**File**: `internal/tui/app/app.go` (modify existing — continued)

The `viewMain()` function needs to render a different layout when on the History tab.
The Changes tab layout is unchanged: sidebar (Pane 1 + Pane 3 stacked) | main (Pane 2).
The History tab uses: sidebar (Pane 1, full height) | main column (Commit Details at top +
[Changed Files | Diff Viewer] side by side at bottom).

**Restructure `viewMain()`** — replace the sidebar and main column construction with a
tab-conditional layout. The header, tab bar, terminal, and final compose remain the same.

Replace the section between the tab bar and the compose line:

```go
func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar (same for both tabs) ──
	headerData := views.HeaderData{
		RepoName:    git.RepoName(m.repoPath),
		BranchName:  m.branchName,
		Ahead:       m.ahead,
		Behind:      m.behind,
		HasUpstream: m.hasUpstream,
		// Fetching/Pulling/Switching fields from Phases 14-15
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Tab-specific layout ──
	var sidebar, mainCol string

	if m.activeTab == core.ChangesTab {
		// ── Changes tab layout (unchanged from Phases 5-7) ──
		fileCount := len(m.fileList.Files)
		pane1Title := "Changed Files"
		if fileCount > 0 {
			pane1Title = fmt.Sprintf("Changed Files (%d)", fileCount)
		}
		pane1 := renderPane(pane1Title, m.fileList.View(),
			dim.SidebarWidth, dim.FileListHeight, m.activePane == core.Pane1)
		pane3 := renderPane(
			core.PaneName(core.Pane3, m.activeTab),
			"(commit message)",
			dim.SidebarWidth, dim.CommitMsgHeight, m.activePane == core.Pane3)
		sidebar = lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

		mainCol = renderPane(
			core.PaneName(core.Pane2, m.activeTab),
			m.diffView.View(),
			dim.MainWidth, dim.DiffHeight, m.activePane == core.Pane2)

	} else {
		// ── History tab layout ──
		// Sidebar: Commit List uses full sidebar height (no Pane 3 split)
		commitListHeight := dim.FileListHeight + dim.CommitMsgHeight
		commitCount := len(m.commitList.Commits)
		pane1Title := "Commit List"
		if commitCount > 0 {
			pane1Title = fmt.Sprintf("Commit List (%d)", commitCount)
		}
		if m.logLoading {
			pane1Title += " ⟳"
		}
		sidebar = renderPane(pane1Title, m.commitList.View(),
			dim.SidebarWidth, commitListHeight, m.activePane == core.Pane1)

		// Main column: Commit Details (top, fixed 7 rows) +
		//              [Changed Files | Diff Viewer] (bottom, side by side)
		detailHeight := 7
		detail := renderPane("Commit Details",
			views.RenderCommitDetail(m.selectedCommit, dim.MainWidth-4),
			dim.MainWidth, detailHeight, false) // not focusable

		bottomHeight := dim.DiffHeight - detailHeight
		if bottomHeight < 4 {
			bottomHeight = 4
		}
		filesWidth := dim.MainWidth / 2
		diffWidth := dim.MainWidth - filesWidth

		fileCount := len(m.commitFiles.Files)
		filesTitle := "Changed Files"
		if fileCount > 0 {
			filesTitle = fmt.Sprintf("Changed Files (%d)", fileCount)
		}

		pane2 := renderPane(filesTitle, m.commitFiles.View(),
			filesWidth, bottomHeight, m.activePane == core.Pane2)
		pane3 := renderPane("Diff Viewer", m.commitDiffView.View(),
			diffWidth, bottomHeight, m.activePane == core.Pane3)

		bottomRow := lipgloss.JoinHorizontal(lipgloss.Top, pane2, pane3)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, detail, bottomRow)
	}

	// ── Terminal (appended to main column regardless of tab) ──
	if m.terminalOpen {
		termPane := renderPane("Terminal",
			"(terminal)",
			dim.MainWidth, dim.TerminalHeight, m.activePane == core.PaneTerminal)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, mainCol, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}
```

**Key layout decisions:**

- **Commit List uses full sidebar height**: On the Changes tab, the sidebar is split
  between Pane 1 (file list) and Pane 3 (commit message). On the History tab, there's no
  commit message pane, so Pane 1 (commit list) gets the combined height
  (`dim.FileListHeight + dim.CommitMsgHeight`). This maximizes the number of visible commits.
- **Commit Details is non-focusable**: The `false` argument to `renderPane` means the
  detail pane always has a gray border — it's a display-only area, not an interactive pane.
  The user can't focus it with `1`/`2`/`3`.
- **Changed Files and Diff Viewer are side by side**: Unlike the Changes tab where the
  diff viewer occupies the entire main column, the History tab splits the main column's
  bottom area horizontally. Each gets half the width (`dim.MainWidth / 2`). This matches
  the design spec's 3-column layout (Commit List | Changed Files | Diff Viewer).
- **Terminal pane is tab-agnostic**: The terminal is appended to `mainCol` after the
  tab-specific layout, so it appears at the bottom of the right column on both tabs.
- **Spinner in title**: The `⟳` character is appended to the Commit List title while
  a log load is in progress, matching the header spinner pattern from Phase 14.

### 16.5 Help Overlay Update

**File**: `internal/tui/views/helpoverlay.go` (modify existing)

Add a "History Tab" section to the help overlay. Insert these lines after the existing
"Pane Navigation" section in the `lines` slice:

```go
		"",
		sectionStyle.Render("History Tab (Commit List)"),
		row("j / k", "Navigate commits"),
		row("Enter", "Select commit (show files + details)"),
		row("g / G", "Jump to first / last commit"),
		"",
		sectionStyle.Render("History Tab (Changed Files)"),
		row("j / k", "Navigate files in commit"),
		row("Enter", "View diff for selected file"),
		"",
		sectionStyle.Render("History Tab (Diff Viewer)"),
		row("j / k", "Scroll diff"),
		row("g / G", "Jump to top / bottom"),
		row("d / u", "Page down / up"),
```

### 16.6 Test It

Build and run:
```bash
cd ~/leogit
go build ./...
go run ./cmd/leogit /path/to/any/git/repo
```

**Test scenarios:**

1. **Tab switch loads log**: Press `Tab` to switch to History tab. The Commit List title
   should show "⟳" briefly, then populate with commits showing SHA, summary, and date.
   The first commit should be auto-selected with details shown in the Commit Details pane.

2. **Commit list navigation**: Press `j`/`k` to move through the commit list. The blue
   cursor should move. Press `g` to jump to the top, `G` to jump to the bottom.

3. **Commit selection**: Press `Enter` on a commit. The Commit Details pane should update
   to show the commit's summary, author, date, and full SHA. The Changed Files pane should
   populate with the files modified in that commit.

4. **Changed files display**: The Changed Files pane should show files with status icons
   (`[M]`, `[+]`, `[-]`, etc.) matching the Changes tab format. Press `2` to focus the
   Changed Files pane, then `j`/`k` to navigate files.

5. **Commit diff viewer**: Press `Enter` on a file in the Changed Files list. The Diff
   Viewer (Pane 3) should show a syntax-highlighted diff for that file within the selected
   commit. Press `3` to focus the Diff Viewer, then `j`/`k` to scroll, `d`/`u` for
   half-page scrolling.

6. **Pane focus**: Press `1`, `2`, `3` to focus Commit List, Changed Files, Diff Viewer
   respectively. Active pane should have a blue border, others gray.

7. **Lazy loading**: Scroll to the bottom of the commit list (press `G` or hold `j`).
   When within 5 commits of the end, the title should show "⟳" and more commits should
   load. Verify the cursor stays in place and new commits appear below.

8. **Log exhaustion**: In a repo with fewer than 50 commits, scrolling to the bottom
   should not show the loading spinner (no more commits to fetch).

9. **Refs display**: If the HEAD commit has refs (e.g., "HEAD -> main, origin/main"),
   they should appear in green next to the summary in the commit list.

10. **Empty repo**: Open a repo with no commits. The Commit List should show "No commits"
    in dim italic. The Commit Details should show "Select a commit to view details".

11. **Tab switch back**: Press `Tab` to switch back to Changes tab. The layout should
    return to the normal Changes tab layout (file list + commit message + diff viewer).
    Press `Tab` again — the History tab should show the previously loaded commits without
    re-fetching (the data is cached).

12. **Branch switch clears history**: Switch branches (via `B` key from Phase 15 or
    externally). Switch to the History tab — the commit log should reload for the new
    branch, not show stale commits from the previous branch.

13. **Error handling**: In a broken repo, the error modal should appear with "Log Error"
    title. Verify no crashes when git log fails.

14. **Window resize**: Resize the terminal while on the History tab. All three panes
    (Commit List, Changed Files, Diff Viewer) should resize proportionally. The commit
    list should remain scrollable, and the diff viewer should re-render at the new width.

15. **Large commit**: Select a commit that modifies many files (20+). The Changed Files
    pane should be scrollable. Select a file with a large diff — the Diff Viewer should
    handle scrolling through thousands of lines.

16. **Merge commits**: Select a merge commit (2 parents). It should display correctly
    in the commit list and show the files changed using `--first-parent` diff.

17. **Auto-select flow**: When switching to History tab for the first time, verify the
    full auto-select chain: log loads → first commit selected → commit files load →
    first file selected → diff loads. All panes should populate automatically.

18. **Terminal on History tab**: Open the terminal (`` ` ``) while on the History tab.
    It should appear at the bottom of the main column, below the Changed Files + Diff
    Viewer row. The panes above should shrink to make room.

**Summary**: Phase 16 adds the complete History tab with a lazy-loaded commit log in the
sidebar, commit detail display at the top of the main column, changed files list and
syntax-highlighted diff viewer side by side at the bottom. The commit list supports
`j`/`k`/`g`/`G`/`Enter` navigation, lazy loads in batches of 50, and auto-selects the
first commit on initial load. Selecting a commit populates the changed files list (reusing
`FileListModel` from Phase 6) and selecting a file loads its diff (reusing `DiffViewModel`
from Phase 7 and `GetCommitDiff` also from Phase 7). The `\x01`/`\x00` format string
pattern ensures robust parsing of multi-line commit bodies and trailers. History is
invalidated on branch switch so stale commits are never displayed. The layout adapts the
existing 2-column structure — the sidebar uses full height for the commit list, and the
main column splits into a detail area and a horizontally-divided files/diff area.

## Phase 17 — Merge

Phase 17 adds the ability to merge any branch into the current branch. The user triggers
a merge from the existing branch dropdown (Phase 15) by pressing `m` on a branch, which
opens a merge confirmation overlay with a squash option. After the merge completes, the
app detects whether it was a clean merge or produced conflicts. Clean merges trigger a
status refresh. Conflicts surface a modal listing the conflicted files and directing the
user to the embedded terminal for resolution — conflict resolution is intentionally
terminal-deferred, not built into the TUI.

This phase also adds `git merge --abort` via the `A` keybinding when the repo is in a
merge state (conflicted files detected), and a `MERGING` indicator in the header when
`.git/MERGE_HEAD` exists.

**Git commands used:**

| Command | Purpose |
|---------|---------|
| `git merge --no-edit <branch>` | Merge branch into current (auto-commit with default message) |
| `git merge --squash <branch>` | Stage merged changes without creating a merge commit |
| `git merge --abort` | Cancel an in-progress merge and restore pre-merge state |
| `git merge-base <a> <b>` | Find the common ancestor commit of two branches |
| `git commit --no-edit --cleanup=strip` | Finalize a squash merge (commit staged changes) |

**How `git merge --no-edit` works:**

When you run `git merge <branch>`, git attempts to merge the branch's changes into the
current branch:

- **Fast-forward**: If the current branch is a direct ancestor of the target branch, git
  just moves the branch pointer forward. No merge commit is created. The `--no-edit` flag
  has no effect in this case.
- **Auto-merge (no conflicts)**: If both branches have diverged but the changes don't
  overlap, git creates a merge commit automatically. The `--no-edit` flag tells git to use
  the default merge message ("Merge branch 'feature-x' into main") without opening an
  editor. This is critical for a TUI — we can't open `$EDITOR` inside a Bubbletea program.
- **Conflicts**: Git merges what it can and marks conflicted files with conflict markers
  (`<<<<<<<`, `=======`, `>>>>>>>`). The merge is paused — `.git/MERGE_HEAD` exists, and
  conflicted files show as `UU` in `git status`. The user must resolve conflicts manually,
  stage the files, and run `git commit`.

**How `git merge --squash` works:**

Squash merge stages all the changes from the target branch into the working tree and index,
but does NOT create a merge commit or record the merge in the commit graph. The user must
then run `git commit` to create a single commit with all the squashed changes. This is
useful for keeping a clean linear history. After `git merge --squash`, the repo is in a
"squash in progress" state — the changes are staged but not committed.

**How `git merge --abort` works:**

If a merge is in progress (conflicted files, `.git/MERGE_HEAD` exists), `git merge --abort`
reverts the working tree and index to the pre-merge state. All conflict markers and partially
merged changes are discarded. This is safe — it restores exactly the state before
`git merge` was run.

**How `git merge-base` works:**

`git merge-base <a> <b>` finds the best common ancestor between two commits. This is the
point where the branches diverged. It's useful for showing "X commits will be merged" in
the confirmation overlay — the count is the number of commits in `<branch>` that are not
in the current branch (i.e., `git rev-list --count <merge-base>...<branch>`).

**New files:**
- `internal/git/merge.go` — merge, squash merge, abort, merge-base, and merge state detection

**Modified files:**
- `internal/tui/views/branchdropdown.go` — add `m` key for merge action
- `internal/tui/views/header.go` — add `MERGING` indicator
- `internal/tui/app/app.go` — merge messages, commands, handlers, `A` keybinding
- `internal/tui/views/helpoverlay.go` — merge keybinding documentation

### 17.0 Git Merge Commands

**File**: `internal/git/merge.go` (new file)

This file wraps all merge-related git commands. Like all git wrappers in this project,
commands run with `TERM=dumb` to suppress color output and pager behavior. Error output
is captured via `CombinedOutput()` so we can detect conflict messages.

```go
package git

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// MergeResult describes the outcome of a merge operation.
type MergeResult struct {
	Success      bool     // true if the merge completed without conflicts
	FastForward  bool     // true if the merge was a fast-forward (no merge commit created)
	Conflicts    []string // paths of conflicted files (empty on success)
	ErrorMessage string   // raw error output from git (empty on success)
}

// MergeBranch merges the given branch into the current branch using --no-edit
// to auto-accept the default merge message without opening an editor.
//
// Returns a MergeResult describing the outcome:
//   - Success + FastForward: branch pointer moved forward, no merge commit
//   - Success + !FastForward: auto-merge created a merge commit
//   - !Success: conflicts detected, files listed in Conflicts field
func MergeBranch(repoPath, branch string) MergeResult {
	cmd := exec.Command("git", "merge", "--no-edit", branch)
	cmd.Dir = repoPath
	// cmd.Environ() copies the current process environment, then we append
	// TERM=dumb to suppress git's color codes and pager output (git detects
	// the "dumb" terminal and disables both automatically).
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	outStr := string(output)

	if err == nil {
		// Merge succeeded — check if it was a fast-forward
		ff := strings.Contains(outStr, "Fast-forward") ||
			strings.Contains(outStr, "fast-forward")
		return MergeResult{Success: true, FastForward: ff}
	}

	// Merge failed — check for conflicts
	if strings.Contains(outStr, "CONFLICT") || strings.Contains(outStr, "Automatic merge failed") {
		// Parse conflicted file names from the output.
		// Git outputs lines like: "CONFLICT (content): Merge conflict in <path>"
		var conflicts []string
		for _, line := range strings.Split(outStr, "\n") {
			if strings.Contains(line, "Merge conflict in ") {
				// Extract path after "Merge conflict in "
				idx := strings.Index(line, "Merge conflict in ")
				if idx >= 0 {
					path := strings.TrimSpace(line[idx+len("Merge conflict in "):])
					if path != "" {
						conflicts = append(conflicts, path)
					}
				}
			}
		}
		return MergeResult{
			Success:      false,
			Conflicts:    conflicts,
			ErrorMessage: outStr,
		}
	}

	// Other error (not conflict-related)
	return MergeResult{
		Success:      false,
		ErrorMessage: outStr,
	}
}

// MergeSquash performs a squash merge of the given branch into the current branch.
// This stages all changes from the target branch but does NOT create a commit.
// The caller must run a separate commit (e.g., via CommitSquashMerge) to finalize.
//
// A squash merge can also produce conflicts, which are reported the same way
// as a regular merge.
func MergeSquash(repoPath, branch string) MergeResult {
	cmd := exec.Command("git", "merge", "--squash", branch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	outStr := string(output)

	if err == nil {
		return MergeResult{Success: true}
	}

	// Check for conflicts (same parsing as MergeBranch)
	if strings.Contains(outStr, "CONFLICT") || strings.Contains(outStr, "Automatic merge failed") {
		var conflicts []string
		for _, line := range strings.Split(outStr, "\n") {
			if strings.Contains(line, "Merge conflict in ") {
				idx := strings.Index(line, "Merge conflict in ")
				if idx >= 0 {
					path := strings.TrimSpace(line[idx+len("Merge conflict in "):])
					if path != "" {
						conflicts = append(conflicts, path)
					}
				}
			}
		}
		return MergeResult{
			Success:      false,
			Conflicts:    conflicts,
			ErrorMessage: outStr,
		}
	}

	return MergeResult{
		Success:      false,
		ErrorMessage: outStr,
	}
}

// CommitSquashMerge finalizes a squash merge by committing the staged changes
// with the default merge message. Uses --no-edit to skip the editor and
// --cleanup=strip to remove comment lines from the default message.
func CommitSquashMerge(repoPath string) error {
	cmd := exec.Command("git", "commit", "--no-edit", "--cleanup=strip")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git commit failed: %w\n%s", err, string(output))
	}
	return nil
}

// MergeAbort cancels an in-progress merge and restores the pre-merge state.
// This only works when a merge is in progress (.git/MERGE_HEAD exists).
// Returns an error if no merge is in progress or if the abort fails.
func MergeAbort(repoPath string) error {
	cmd := exec.Command("git", "merge", "--abort")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git merge --abort failed: %w\n%s", err, string(output))
	}
	return nil
}

// IsMerging returns true if a merge is currently in progress.
// Detected by checking for the presence of .git/MERGE_HEAD.
func IsMerging(repoPath string) bool {
	// Handle both normal repos (.git/MERGE_HEAD) and worktrees
	// where .git might be a file pointing to the actual git dir.
	gitDir := filepath.Join(repoPath, ".git")
	info, err := os.Stat(gitDir)
	if err != nil {
		return false
	}

	var mergeHeadPath string
	if info.IsDir() {
		// Normal repo — .git is a directory
		mergeHeadPath = filepath.Join(gitDir, "MERGE_HEAD")
	} else {
		// Worktree — .git is a file containing "gitdir: /path/to/actual/git/dir"
		data, err := os.ReadFile(gitDir)
		if err != nil {
			return false
		}
		actualDir := strings.TrimSpace(strings.TrimPrefix(string(data), "gitdir: "))
		mergeHeadPath = filepath.Join(actualDir, "MERGE_HEAD")
	}

	_, err = os.Stat(mergeHeadPath)
	return err == nil
}

// GetMergeBase returns the SHA of the best common ancestor between two commits.
// Used to calculate how many commits will be merged (the count of commits
// between the merge base and the target branch).
func GetMergeBase(repoPath, commitA, commitB string) (string, error) {
	cmd := exec.Command("git", "merge-base", commitA, commitB)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git merge-base: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CountCommitsToMerge returns the number of commits in targetBranch that are
// not in the current branch (HEAD). This is the number of commits that would
// be merged. Uses git rev-list --count <merge-base>...<target>.
func CountCommitsToMerge(repoPath, targetBranch string) (int, error) {
	base, err := GetMergeBase(repoPath, "HEAD", targetBranch)
	if err != nil {
		return 0, err
	}

	cmd := exec.Command("git", "rev-list", "--count", base+".."+targetBranch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return 0, fmt.Errorf("git rev-list --count: %w", err)
	}

	count, err := strconv.Atoi(strings.TrimSpace(string(out)))
	if err != nil {
		return 0, fmt.Errorf("parse commit count: %w", err)
	}
	return count, nil
}
```

**How it works:**

- `MergeBranch` returns a `MergeResult` struct instead of a simple `error`. This lets the
  app distinguish between fast-forward merges (no commit created), auto-merges (commit
  created), and conflicts (merge paused). The conflict file list is parsed from git's output
  by looking for lines containing `"Merge conflict in "`.
- `MergeSquash` follows the same pattern but uses `--squash`. After a successful squash
  merge, the changes are staged but NOT committed. The app calls `CommitSquashMerge()` to
  create the final commit with `--no-edit --cleanup=strip`.
- `CommitSquashMerge` uses `--no-edit` to skip the editor (same reason as merge — can't
  open `$EDITOR` inside Bubbletea) and `--cleanup=strip` to remove the auto-generated
  comment lines that `git merge --squash` adds to the commit message template.
- `IsMerging` checks for `.git/MERGE_HEAD` which git creates when a merge is paused due to
  conflicts. This file contains the SHA of the branch being merged. When the merge is
  completed (via `git commit`) or aborted (via `git merge --abort`), git removes this file.
  The function also handles git worktrees where `.git` is a file, not a directory.
- `CountCommitsToMerge` first finds the merge base (common ancestor), then counts commits
  between the merge base and the target branch. This is displayed in the merge confirmation
  overlay as "Merge 5 commits from feature-x into main".

### 17.1 Merge Branch Into Current

The merge flow is triggered from the existing branch dropdown (Phase 15). When the user
presses `m` on a branch in the dropdown, the dropdown emits a `BranchMergeMsg` and closes.
The app shows a merge confirmation overlay, and on confirmation, runs the merge
asynchronously.

**Flow overview:**

```
1. User presses B → branch dropdown opens
2. User navigates to a branch, presses m
3. Dropdown emits BranchMergeMsg{Name: "feature-x"} and closes
4. App loads commit count asynchronously (CountCommitsToMerge)
5. App shows merge confirmation overlay:
   "Merge 5 commits from feature-x into main?"
   [Merge] [Squash & Merge] [Cancel]
6. User selects an option:
   - Merge → git merge --no-edit feature-x
   - Squash & Merge → git merge --squash feature-x + git commit --no-edit
   - Cancel → close overlay
7. Result:
   - Success → refresh status, clear history cache
   - Conflicts → conflict modal with file list + terminal instructions
```

**File**: `internal/tui/views/branchdropdown.go` (modify existing)

Add a new message type for merge requests:

```go
// BranchMergeMsg is sent when the user presses 'm' on a branch to merge it
// into the current branch.
type BranchMergeMsg struct {
	Name string // branch to merge into current
}
```

Add the `m` key handler in the `branchModeBrowse` section of the dropdown's `Update()`,
alongside the existing `c`, `d`, `r` handlers:

```go
		case "m":
			// Merge selected branch into current — cannot merge current into itself
			if len(m.filtered) > 0 {
				branch := m.filtered[m.cursor]
				if branch.IsCurrent {
					return m, nil // can't merge current branch into itself
				}
				m.Visible = false
				return m, func() tea.Msg {
					return BranchMergeMsg{Name: branch.Name}
				}
			}
			return m, nil
```

This follows the same pattern as the delete (`d`) handler — it checks that the selected
branch isn't the current branch (you can't merge a branch into itself), hides the dropdown,
and sends a message to the app.

> **Go closure note:** Notice that `branch := m.filtered[m.cursor]` is captured into a local
> variable before being used inside the closure (`func() tea.Msg { ... }`). This is a Go
> best practice — variables used inside closures should be copied into local variables first,
> because the closure runs later (asynchronously) and the original value (e.g., `m.cursor`)
> may have changed by the time it executes. Without this, the closure could read a stale or
> wrong index.

**File**: `internal/tui/views/mergeoverlay.go` (new file)

The merge confirmation overlay is a simple modal with three options. It's similar to the
error modal but purpose-built for merge confirmation. It shows the branch name, commit
count, and lets the user choose between normal merge, squash merge, or cancel.

```go
package views

import (
	"fmt"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// MergeConfirmMsg is sent when the user confirms a merge from the overlay.
type MergeConfirmMsg struct {
	Branch string
	Squash bool // true for squash merge, false for normal merge
}

// MergeCancelMsg is sent when the user cancels the merge overlay.
type MergeCancelMsg struct{}

// MergeOverlayModel is a confirmation dialog for merge operations.
type MergeOverlayModel struct {
	Visible      bool
	branch       string // branch being merged
	intoBranch   string // current branch (merge target)
	commitCount  int    // number of commits to be merged
	selectedBtn  int    // 0 = Merge, 1 = Squash & Merge, 2 = Cancel
	width        int
	height       int
}

// NewMergeOverlay creates a hidden merge overlay.
func NewMergeOverlay() MergeOverlayModel {
	return MergeOverlayModel{}
}

// Show opens the merge confirmation overlay with the given parameters.
func (m *MergeOverlayModel) Show(branch, intoBranch string, commitCount, width, height int) {
	m.Visible = true
	m.branch = branch
	m.intoBranch = intoBranch
	m.commitCount = commitCount
	m.selectedBtn = 0
	m.width = width
	m.height = height
}

// Update handles key events for the merge overlay.
func (m MergeOverlayModel) Update(msg tea.Msg) (MergeOverlayModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "left", "h":
			if m.selectedBtn > 0 {
				m.selectedBtn--
			}
			return m, nil

		case "right", "l":
			if m.selectedBtn < 2 {
				m.selectedBtn++
			}
			return m, nil

		case "tab":
			m.selectedBtn = (m.selectedBtn + 1) % 3
			return m, nil

		case "enter":
			m.Visible = false
			switch m.selectedBtn {
			case 0: // Merge
				branch := m.branch
				return m, func() tea.Msg {
					return MergeConfirmMsg{Branch: branch, Squash: false}
				}
			case 1: // Squash & Merge
				branch := m.branch
				return m, func() tea.Msg {
					return MergeConfirmMsg{Branch: branch, Squash: true}
				}
			default: // Cancel
				return m, func() tea.Msg {
					return MergeCancelMsg{}
				}
			}

		case "escape":
			m.Visible = false
			return m, func() tea.Msg {
				return MergeCancelMsg{}
			}
		}
	}

	return m, nil
}

// View renders the merge confirmation overlay as a centered modal.
func (m MergeOverlayModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	textStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	branchStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#3FB950"))

	dimStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E"))

	// Button styles
	activeBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#238636")).
		Padding(0, 2)

	inactiveBtnStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9")).
		Background(lipgloss.Color("#30363D")).
		Padding(0, 2)

	cancelActiveBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#DA3633")).
		Padding(0, 2)

	// Title
	title := titleStyle.Render("Merge Branch")

	// Description
	commitWord := "commits"
	if m.commitCount == 1 {
		commitWord = "commit"
	}
	desc := textStyle.Render("Merge ") +
		branchStyle.Render(m.branch) +
		textStyle.Render(" into ") +
		branchStyle.Render(m.intoBranch)

	var countLine string
	if m.commitCount > 0 {
		countLine = dimStyle.Render(
			fmt.Sprintf("%d %s will be merged", m.commitCount, commitWord),
		)
	}

	// Buttons
	btnLabels := []string{"Merge", "Squash & Merge", "Cancel"}
	var buttons []string
	for i, label := range btnLabels {
		if i == m.selectedBtn {
			if i == 2 { // Cancel button is red when active
				buttons = append(buttons, cancelActiveBtnStyle.Render(label))
			} else {
				buttons = append(buttons, activeBtnStyle.Render(label))
			}
		} else {
			buttons = append(buttons, inactiveBtnStyle.Render(label))
		}
	}
	buttonRow := lipgloss.JoinHorizontal(lipgloss.Center, buttons[0], " ", buttons[1], " ", buttons[2])

	// Hint
	hint := dimStyle.Render("←/→ select • Enter confirm • Esc cancel")

	// Compose box content
	var lines []string
	lines = append(lines, title, "", desc)
	if countLine != "" {
		lines = append(lines, countLine)
	}
	lines = append(lines, "", buttonRow, "", hint)

	boxContent := lipgloss.JoinVertical(lipgloss.Center, lines...)

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Align(lipgloss.Center).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
```

**How the overlay works:**

- The overlay has 3 buttons: **Merge**, **Squash & Merge**, and **Cancel**. Navigation
  uses `←`/`→` (or `h`/`l`) and `Tab`. `Enter` confirms the selected option, `Esc` cancels.
- **Merge** sends `MergeConfirmMsg{Squash: false}` — triggers `git merge --no-edit <branch>`.
- **Squash & Merge** sends `MergeConfirmMsg{Squash: true}` — triggers
  `git merge --squash <branch>` followed by `git commit --no-edit --cleanup=strip`.
- **Cancel** sends `MergeCancelMsg` — no action taken, overlay closes.
- The commit count is loaded asynchronously before the overlay opens (via
  `CountCommitsToMerge`), so the user sees "5 commits will be merged" immediately.
- Button styling follows these conventions: green for action buttons, red for cancel
  (when focused), gray for unfocused buttons.

### 17.2 Wire It Into the App

**File**: `internal/tui/app/app.go` (modify existing)

This section connects the merge overlay and git merge operations to the main app. The
changes are:

1. New messages for merge operations
2. New async commands for merge, squash merge, abort, and commit count
3. New Model fields for merge state
4. New `mergeOverlay` integration in `handleMainKey()` and `Update()`
5. `A` keybinding for merge abort
6. Merge state indicator in header
7. Post-merge conflict detection

**New imports** — no new packages needed beyond what's already imported.

**New messages** — add after the existing message types:

```go
// mergeCountResultMsg carries the commit count for a pending merge.
type mergeCountResultMsg struct {
	branch string
	count  int
	err    error
}

// mergeCompleteMsg carries the result of a merge or squash merge operation.
type mergeCompleteMsg struct {
	branch string
	squash bool
	result git.MergeResult
}

// mergeAbortCompleteMsg is sent after git merge --abort completes.
type mergeAbortCompleteMsg struct {
	err error
}

// squashCommitCompleteMsg is sent after the squash merge commit completes.
type squashCommitCompleteMsg struct {
	branch string
	err    error
}
```

**New async commands** — add after the existing command functions:

```go
// countMergeCommitsCmd counts how many commits will be merged from the
// target branch into the current branch. Runs asynchronously because
// git merge-base + rev-list can be slow on large repos.
func countMergeCommitsCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		count, err := git.CountCommitsToMerge(repoPath, branch)
		return mergeCountResultMsg{branch: branch, count: count, err: err}
	}
}

// mergeCmd runs git merge --no-edit asynchronously.
func mergeCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		result := git.MergeBranch(repoPath, branch)
		return mergeCompleteMsg{branch: branch, squash: false, result: result}
	}
}

// squashMergeCmd runs git merge --squash followed by git commit --no-edit.
// Both steps run in the same command function — if the squash produces
// conflicts, the commit step is skipped (the merge result reports conflicts).
func squashMergeCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		result := git.MergeSquash(repoPath, branch)
		if !result.Success {
			return mergeCompleteMsg{branch: branch, squash: true, result: result}
		}
		// Squash succeeded — now commit the staged changes
		err := git.CommitSquashMerge(repoPath)
		if err != nil {
			return squashCommitCompleteMsg{branch: branch, err: err}
		}
		return mergeCompleteMsg{
			branch: branch,
			squash: true,
			result: git.MergeResult{Success: true},
		}
	}
}

// mergeAbortCmd runs git merge --abort asynchronously.
func mergeAbortCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		err := git.MergeAbort(repoPath)
		return mergeAbortCompleteMsg{err: err}
	}
}
```

**Changes to the Model struct** — add merge state fields:

```go
	// Merge operations
	mergeOverlay  views.MergeOverlayModel // merge confirmation dialog
	merging       bool                    // true while a merge operation is in progress
	isMergeState  bool                    // true when .git/MERGE_HEAD exists (conflicts)
	pendingMerge  string                  // branch name waiting for commit count before overlay
```

- `mergeOverlay` is the confirmation dialog shown before running the merge.
- `merging` is `true` while the async merge command is running (shows spinner in header).
- `isMergeState` tracks whether the repo is in a merge-in-progress state (conflicts exist).
  This is updated from the `statusResultMsg` handler by checking `git.IsMerging()`.
- `pendingMerge` temporarily holds the branch name between receiving `BranchMergeMsg` and
  the `mergeCountResultMsg` arriving. Once the count is known, the overlay opens.

**Changes to `New()`** — initialize the merge overlay:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		// ... existing fields ...
		mergeOverlay: views.NewMergeOverlay(),
	}
}
```

**Changes to `handleMainKey()`** — add merge overlay priority and `A` keybinding.

The merge overlay should be checked AFTER the error modal and help overlay, but BEFORE
navigable mode keybindings. Add this block between the help overlay check and the
focused mode check:

```go
	// ── Merge overlay ──
	if m.mergeOverlay.Visible {
		var cmd tea.Cmd
		m.mergeOverlay, cmd = m.mergeOverlay.Update(msg)
		return m, cmd
	}
```

Add the `A` keybinding in the navigable mode section (alongside existing `B`, `S`, `F`, `P`):

```go
	case "A":
		// Abort merge — only available when in a merge state
		if m.isMergeState && !m.merging {
			m.merging = true
			return m, mergeAbortCmd(m.repoPath)
		}
		return m, nil
```

The `A` key is uppercase to avoid conflicts with the `a` key used for "stage all" in the
Changes tab file list (Phase 8). It only works when `m.isMergeState` is `true` (detected
by `IsMerging()` in the status handler), preventing accidental invocation.

**Changes to `Update()`** — add handlers for all merge-related messages. Add these cases
to the existing `switch msg := msg.(type)` block:

```go
	case views.BranchMergeMsg:
		// User pressed 'm' on a branch in the dropdown — load commit count
		// before showing the merge overlay (so we can display "N commits")
		m.pendingMerge = msg.Name
		return m, countMergeCommitsCmd(m.repoPath, msg.Name)

	case mergeCountResultMsg:
		if m.pendingMerge == "" {
			return m, nil // stale message, ignore
		}
		count := msg.count
		if msg.err != nil {
			// Failed to count — still show overlay but without count
			count = 0
		}
		m.mergeOverlay.Show(
			m.pendingMerge,
			m.branchName,
			count,
			m.width, m.height,
		)
		m.pendingMerge = ""
		return m, nil

	case views.MergeConfirmMsg:
		m.merging = true
		if msg.Squash {
			return m, squashMergeCmd(m.repoPath, msg.Branch)
		}
		return m, mergeCmd(m.repoPath, msg.Branch)

	case views.MergeCancelMsg:
		// User cancelled the merge overlay — no action needed
		return m, nil

	case mergeCompleteMsg:
		m.merging = false
		if msg.result.Success {
			// Merge succeeded — refresh status to update branch info and file list.
			// Also clear the history cache since new commits were added.
			m.commitList.SetCommits(nil)
			m.selectedCommit = nil
			m.commitFiles.SetFiles(nil)
			m.commitDiffView.Clear()
			m.logExhausted = false
			return m, refreshStatusCmd(m.repoPath)
		}
		// Merge failed — check if it was a conflict or other error
		if len(msg.result.Conflicts) > 0 {
			// Conflicts detected — show modal with file list and instructions.
			// The status poll will also pick up isMergeState on the next tick.
			fileList := strings.Join(msg.result.Conflicts, "\n  ")
			mergeType := "Merge"
			if msg.squash {
				mergeType = "Squash merge"
			}
			m.errorModal = views.ShowError(
				"Merge Conflicts",
				fmt.Sprintf(
					"%s of \"%s\" produced conflicts:\n\n  %s\n\n"+
						"Resolve conflicts in the terminal (`):\n"+
						"  # Edit conflicted files, then:\n"+
						"  git add <file>\n"+
						"  git commit\n\n"+
						"Or abort the merge:\n"+
						"  Press A (or run git merge --abort)",
					mergeType, msg.branch, fileList,
				),
				false, // not retryable
				nil,
				m.width, m.height,
			)
			// Trigger a status refresh to pick up the merge state
			m.postMergeCheck = true
			return m, refreshStatusCmd(m.repoPath)
		}
		// Other error (not conflicts)
		m.errorModal = views.ShowError(
			"Merge Error",
			fmt.Sprintf(
				"Failed to merge \"%s\":\n\n%s",
				msg.branch, msg.result.ErrorMessage,
			),
			false,
			nil,
			m.width, m.height,
		)
		return m, nil

	case squashCommitCompleteMsg:
		m.merging = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Squash Commit Error",
				fmt.Sprintf(
					"Squash merge of \"%s\" succeeded but commit failed:\n\n%s\n\n"+
						"The changes are staged. Commit manually in the terminal (`).",
					msg.branch, msg.err.Error(),
				),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Should not normally reach here — squashMergeCmd sends mergeCompleteMsg
		// on success. This handles the edge case where the commit step fails
		// separately from the squash step.
		return m, refreshStatusCmd(m.repoPath)

	case mergeAbortCompleteMsg:
		m.merging = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Merge Abort Error",
				"Failed to abort merge: "+msg.err.Error(),
				false,
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Abort succeeded — refresh status (clears merge state, updates files)
		return m, refreshStatusCmd(m.repoPath)
```

**Changes to `statusResultMsg` handler** — detect merge state. Add this check after the
existing status update and branch change detection:

```go
	case statusResultMsg:
		if msg.err != nil {
			// ... existing error handling ...
			return m, nil
		}

		// ... existing branch change detection and status updates ...

		// Detect merge state
		m.isMergeState = git.IsMerging(m.repoPath)

		return m, nil
```

This runs on every status poll (every 2 seconds), so the merge state is always up to date.
When the user resolves conflicts and commits in the terminal, the next status poll detects
that `.git/MERGE_HEAD` is gone and sets `m.isMergeState = false`.

**Changes to `View()`** — add merge overlay rendering. In the `stateMain` section, add the
merge overlay check after the error modal and help overlay, but before `viewMain()`:

```go
		case stateMain:
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else if m.mergeOverlay.Visible {
				content = m.mergeOverlay.View()
			} else {
				content = m.viewMain()
			}
```

The merge overlay takes over the full screen (like the error modal and help overlay) because
it's a modal dialog that requires user interaction before proceeding.

**Changes to `HeaderData` and header rendering** — add merge state indicators.

**File**: `internal/tui/views/header.go` (modify existing)

Add two new fields to `HeaderData`:

```go
type HeaderData struct {
	// ... existing fields ...
	Merging      bool // true while a merge operation is running
	IsMergeState bool // true when repo is in merge state (conflicts)
}
```

In `RenderHeader()`, add indicators next to the branch name. After the existing
`Switching`/`Fetching`/`Pulling` indicators, add:

```go
	// Merge indicators
	if data.Merging {
		indicators = append(indicators, spinnerStyle.Render("⟳ Merging..."))
	} else if data.IsMergeState {
		mergeStateStyle := lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#DA3633")) // red
		indicators = append(indicators, mergeStateStyle.Render("MERGING — press A to abort"))
	}
```

This follows the same pattern as the existing `Fetching`/`Pulling`/`Switching` indicators.
The spinner shows during the async merge operation. The `MERGING` state indicator persists
until the user either resolves conflicts + commits, or aborts with `A`.

**Changes to `viewMain()`** — pass merge state to header:

```go
	headerData := views.HeaderData{
		// ... existing fields ...
		Merging:      m.merging,
		IsMergeState: m.isMergeState,
	}
```

**End-to-end merge flow (normal merge, no conflicts):**

1. User presses `B` → branch dropdown opens with the branch list
2. User navigates to `feature-x`, presses `m`
3. Dropdown emits `BranchMergeMsg{Name: "feature-x"}` and closes
4. App dispatches `countMergeCommitsCmd(m.repoPath, "feature-x")`
5. `mergeCountResultMsg{branch: "feature-x", count: 5}` arrives
6. App opens merge overlay: "Merge feature-x into main? 5 commits will be merged"
7. User presses `Enter` on "Merge" (default button)
8. Overlay emits `MergeConfirmMsg{Branch: "feature-x", Squash: false}` and hides
9. App sets `m.merging = true`, dispatches `mergeCmd`
10. Header shows `⟳ Merging...`
11. `git merge --no-edit feature-x` runs asynchronously
12. `mergeCompleteMsg{result: {Success: true, FastForward: true}}` arrives
13. App clears history cache, refreshes status
14. Status update picks up new branch state — merge complete, header returns to normal

**End-to-end merge flow (conflicts):**

1. Steps 1-10 same as above
2. `git merge --no-edit feature-x` exits non-zero with CONFLICT output
3. `mergeCompleteMsg{result: {Success: false, Conflicts: ["file1.go", "file2.go"]}}` arrives
4. App shows error modal:
   ```
   Merge Conflicts
   Merge of "feature-x" produced conflicts:
     file1.go
     file2.go
   Resolve conflicts in the terminal (`):
     # Edit conflicted files, then:
     git add <file>
     git commit
   Or abort the merge:
     Press A (or run git merge --abort)
   ```
5. User dismisses modal, status poll detects `isMergeState = true`
6. Header shows red `MERGING — press A to abort`
7. User opens terminal (`` ` ``), resolves conflicts, runs `git add` + `git commit`
8. Next status poll: `.git/MERGE_HEAD` gone → `isMergeState = false`, header normal
   OR: User presses `A` → `git merge --abort` → status refresh → back to normal

**End-to-end squash merge flow:**

1. Steps 1-6 same as normal merge
2. User presses `→` to select "Squash & Merge", then `Enter`
3. Overlay emits `MergeConfirmMsg{Branch: "feature-x", Squash: true}`
4. App dispatches `squashMergeCmd` which runs:
   a. `git merge --squash feature-x` (stages changes, no commit)
   b. `git commit --no-edit --cleanup=strip` (creates single commit)
5. `mergeCompleteMsg{squash: true, result: {Success: true}}` arrives
6. App clears history cache, refreshes status — done

### 17.2 Merge Conflict Resolution UI

Conflict resolution is **terminal-deferred** — the TUI does not provide a built-in conflict
resolution editor. Instead, it detects conflicts, surfaces them clearly, and directs the
user to the embedded terminal.

This design decision is intentional:

- **Conflict resolution is complex** — it requires viewing both sides of a conflict,
  understanding the semantic meaning of changes, and making judgment calls. A specialized
  tool like `git mergetool`, `vim` with conflict markers, or a GUI diff tool handles this
  far better than a custom TUI could.
- **The embedded terminal is already available** — pressing `` ` `` opens a full shell at the
  repo root. The user can run `git mergetool`, edit files with their preferred editor, or
  use any conflict resolution workflow they're comfortable with.
- **The TUI still adds value** — it detects conflicts via status polling, shows them in the
  file list with `[!]` icons, displays a clear modal with instructions, and provides the
  `A` keybinding for quick abort.

**What the TUI does during a merge conflict:**

1. **Error modal** (immediate): Shows the list of conflicted files and resolution
   instructions. Dismissed with `Enter` or `Esc`.

2. **Header indicator** (persistent): Shows red `MERGING — press A to abort` as long as
   `.git/MERGE_HEAD` exists. Disappears when the user commits or aborts.

3. **File list `[!]` icons** (persistent): Conflicted files appear with the `[!]` status
   icon (red) in the Changes tab file list. The existing `StatusConflicted` rendering from
   Phase 6 handles this automatically — no new code needed.

4. **`A` keybinding** (persistent): Runs `git merge --abort` to cancel the merge. Only
   active when `m.isMergeState` is `true`. Shows the abort spinner while running.

5. **Status poll** (continuous): Every 2 seconds, the status poll checks `IsMerging()` and
   updates `m.isMergeState`. When the user resolves conflicts and commits in the terminal,
   the next poll detects that `.git/MERGE_HEAD` is gone and clears the merge state.

**What happens to existing status polling during conflicts:**

The existing `statusResultMsg` handler (Phase 14.3) already detects conflicted files via
`ConflictedFiles()` and shows a modal. Phase 17 adds a `postMergeCheck` flag (similar to
the existing `postPullCheck`) to show the conflict modal on the first status refresh after
a merge:

Add to the Model struct:

```go
	postMergeCheck bool // true after a merge completes — next status checks for conflicts
```

This flag is set in the `mergeCompleteMsg` handler when conflicts are detected (before
dispatching `refreshStatusCmd`). The `statusResultMsg` handler checks it the same way it
checks `postPullCheck`:

```go
		// Post-merge conflict detection
		// This catches conflicts that appear after the immediate merge result,
		// such as when the user resolves some conflicts but creates new ones.
		if m.postMergeCheck {
			m.postMergeCheck = false

			files := git.ParseStatusEntries(msg.status.RawOutput)
			conflicted := git.ConflictedFiles(files)

			if len(conflicted) > 0 && !m.errorModal.Visible {
				fileList := strings.Join(conflicted, "\n  ")
				m.errorModal = views.ShowError(
					"Merge Conflicts Detected",
					fmt.Sprintf(
						"The following files have conflicts:\n\n  %s\n\n"+
							"Resolve conflicts in the terminal (`):\n"+
							"  git mergetool\n"+
							"  # or edit files, then:\n"+
							"  git add <file>\n"+
							"  git commit\n\n"+
							"Or abort the merge:\n"+
							"  Press A (or run git merge --abort)",
						fileList,
					),
					false,
					nil,
					m.width, m.height,
				)
			}
		}
```

This provides defense-in-depth: the `mergeCompleteMsg` handler shows conflicts from the
git merge output, and the `statusResultMsg` handler catches them from `git status`. Both
paths show the same modal with the same instructions.

### 17.3 Help Overlay Update

**File**: `internal/tui/views/helpoverlay.go` (modify existing)

Add merge keybindings to the help overlay. Insert after the existing "Branch Management"
section:

```go
		"",
		sectionStyle.Render("Merge"),
		row("m", "Merge branch (in branch dropdown)"),
		row("A", "Abort merge (when in merge state)"),
```

These are added as a separate section because merge is a distinct workflow from branch
management. The `m` key only works inside the branch dropdown (Phase 15), and `A` only
works when the repo is in a merge state.

### 17.4 Test It

Build and run:
```bash
cd ~/leogit
go build ./...
go run ./cmd/leogit /path/to/any/git/repo
```

> **Reminder:** The directory name `leogit` and binary path `cmd/leogit` were set up in
> Phase 0. If you used a different name there, adjust accordingly.

**Setup**: Create a test repo with two diverged branches for testing:

```bash
mkdir /tmp/merge-test && cd /tmp/merge-test && git init
echo "line 1" > file.txt && git add . && git commit -m "initial"
git checkout -b feature-branch
echo "line 2" >> file.txt && git commit -am "feature: add line 2"
echo "line 3" >> file.txt && git commit -am "feature: add line 3"
git checkout main
echo "other change" > other.txt && git add . && git commit -am "main: add other.txt"
```

**Test scenarios:**

1. **Merge via branch dropdown**: Press `B` to open the branch dropdown. Navigate to
   `feature-branch` and press `m`. The merge overlay should appear showing:
   "Merge feature-branch into main? 2 commits will be merged"
   with three buttons: [Merge] [Squash & Merge] [Cancel].

2. **Normal merge (no conflicts)**: In the merge overlay, press `Enter` on "Merge".
   The header should show `⟳ Merging...` briefly. After completion, the header should
   return to normal. Press `Tab` to switch to History — the commit list should show the
   new merge commit at the top.

3. **Fast-forward merge**: Create a scenario where the current branch is a direct ancestor
   of the target branch (no divergence). The merge should succeed with a fast-forward — no
   merge commit is created, the branch pointer simply moves forward.

4. **Squash merge**: Reset the test repo and repeat the merge, but select "Squash & Merge"
   in the overlay. After completion, the History tab should show a single commit (not a
   merge commit) with all the squashed changes.

5. **Cancel merge**: Open the merge overlay and press `Esc` or select "Cancel". No merge
   should occur. Verify the repo state is unchanged.

6. **Merge with conflicts**: Set up conflicting changes:
   ```bash
   git checkout -b conflict-branch
   echo "conflict A" > conflict.txt && git add . && git commit -m "branch A"
   git checkout main
   echo "conflict B" > conflict.txt && git add . && git commit -m "branch B"
   ```
   Merge `conflict-branch` via the overlay. The error modal should appear listing
   `conflict.txt` with resolution instructions. After dismissing the modal, the header
   should show red `MERGING — press A to abort`.

7. **Conflict file icons**: After a conflict merge, press `Tab` to go to Changes tab.
   The file list should show `conflict.txt` with a `[!]` icon (red) for conflicted status.

8. **Resolve conflicts in terminal**: With an active merge conflict, press `` ` `` to open
   the terminal. Edit the conflicted file, run `git add conflict.txt` and `git commit`.
   After a few seconds (status poll), the header should return to normal — the `MERGING`
   indicator should disappear.

9. **Abort merge**: Start a merge that produces conflicts. Instead of resolving, press `A`.
   The header should show `⟳ Merging...` briefly while the abort runs. After completion,
   the repo should be back to the pre-merge state. Verify with `git status` in the terminal.

10. **Abort when not merging**: Press `A` when not in a merge state. Nothing should happen
    (the key is ignored when `m.isMergeState` is `false`).

11. **Cannot merge current branch**: In the branch dropdown, navigate to the current branch
    (marked with `✓`) and press `m`. Nothing should happen — you can't merge a branch into
    itself.

12. **Merge spinner in header**: During a merge operation, verify the header shows
    `⟳ Merging...` spinner. It should disappear when the merge completes or fails.

13. **History cache invalidation**: After a successful merge, switch to the History tab.
    The commit list should reload (not show cached stale data). The new merge commit (or
    squash commit) should appear at the top.

14. **Squash merge with conflicts**: Set up conflicting changes and try a squash merge.
    The conflict modal should appear with "Squash merge of..." in the message. After
    resolving conflicts in the terminal, the user needs to `git add` + `git commit`
    manually (squash merges don't auto-commit).

15. **Commit count display**: In the merge overlay, verify the commit count is accurate.
    If the target branch has 5 commits ahead of the merge base, the overlay should show
    "5 commits will be merged". For a branch with 1 commit, it should say "1 commit will
    be merged" (singular).

16. **Window resize during overlay**: Resize the terminal while the merge overlay is
    visible. The overlay should remain centered and readable.

17. **Multiple merge attempts**: After aborting a merge, immediately start another merge
    with a different branch. The overlay should show the correct branch name and commit
    count for the new merge target.

**Summary**: Phase 17 adds merge support through the existing branch dropdown. Pressing `m`
on a branch opens a confirmation overlay with "Merge", "Squash & Merge", and "Cancel"
options. The commit count is loaded asynchronously and displayed in the overlay. Normal
merges use `git merge --no-edit` to auto-accept the default merge message. Squash merges
use `git merge --squash` followed by `git commit --no-edit --cleanup=strip` to create a
single commit. Conflicts are detected from git's output and surfaced in an error modal
listing the conflicted files with resolution instructions pointing to the embedded terminal.
The header shows a persistent red `MERGING — press A to abort` indicator when
`.git/MERGE_HEAD` exists (detected by the 2-second status poll). The `A` keybinding runs
`git merge --abort` to cancel the merge and restore the pre-merge state. Conflict resolution
is terminal-deferred — the TUI detects and surfaces conflicts but does not provide a
built-in conflict editor. Conflicted files appear with `[!]` icons in the Changes tab file
list (reusing the existing `StatusConflicted` rendering from Phase 6). After a successful
merge, the history cache is cleared so the commit list reloads with the new commits.

## Phase 18 — Pull Requests (GH CLI)

Phase 18 adds pull request workflows to the TUI via the `gh` CLI. Pressing `R` (uppercase)
in navigable mode opens a fullscreen PR overlay that lists pull requests for the current
repository. The overlay is split into two panes: a scrollable PR list on the left and a
detail view on the right showing the selected PR's body, CI checks, and review status. From
the overlay, the user can checkout a PR's branch (`Enter`), create a new PR (`c`), and
filter by state (`o` cycles through Open/Closed/Merged/All). PR creation opens a separate
form overlay with title, body, base branch, and draft toggle fields.

The header also shows a PR status indicator for the current branch — if the current branch
has an open PR, the header displays the PR number and review decision icon.

PR reviews (`gh pr review`, `gh pr comment`) and CI rerun operations (`gh run rerun`) are
intentionally **terminal-deferred** — the TUI surfaces status information but does not
provide inline review or CI management. Users drop into the embedded terminal for these
advanced operations. Issue management (`gh issue list`, `gh issue create`) is also
terminal-deferred.

**GH CLI commands used:**

| Command | Purpose |
|---------|---------|
| `gh pr list --state <state> --json <fields> --limit 30` | List PRs with structured JSON output |
| `gh pr checks <number> --json name,state,bucket` | Get CI check statuses for a PR |
| `gh pr create --title <t> --body <b> --base <branch> [--draft]` | Create a new PR from current branch |
| `gh pr create --fill [--base <branch>] [--draft]` | Create PR with auto-filled title/body from commits |
| `gh pr checkout <number>` | Checkout a PR's branch locally |
| `gh pr list --head <branch> --state open --limit 1 --json <fields>` | Get the open PR for the current branch |

**How `gh pr list --json` works:**

`gh pr list` queries GitHub's API for pull requests in the current repository. The `--json`
flag requests specific fields in machine-readable JSON format instead of the default
human-readable table. The `--state` flag filters by PR state (`open`, `closed`, `merged`,
or `all`). The `--limit` flag controls how many results are returned (default 30). The
output is a JSON array of objects. The `author` field is a nested object with a `login`
field containing the GitHub username:

```json
[
  {
    "number": 42,
    "title": "Fix login bug",
    "author": {"login": "username"},
    "state": "OPEN",
    "isDraft": false,
    "baseRefName": "main",
    "headRefName": "fix-login",
    "reviewDecision": "APPROVED",
    "additions": 23,
    "deletions": 5,
    "changedFiles": 3,
    "createdAt": "2025-01-15T10:30:00Z",
    "url": "https://github.com/owner/repo/pull/42"
  }
]
```

All JSON fields available across `gh pr list/view`:
`number`, `title`, `body`, `author`, `state`, `isDraft`, `baseRefName`,
`headRefName`, `mergeable`, `reviewDecision`, `statusCheckRollup`,
`additions`, `deletions`, `changedFiles`, `createdAt`, `updatedAt`, `url`

**How `gh pr checks --json` works:**

`gh pr checks` queries the CI/CD check statuses for a specific PR. With `--json`, it
returns an array of check objects. The `bucket` field normalizes check status to one of
five values: `pass`, `fail`, `pending`, `skipping`, or `cancel`. This is simpler than
parsing the raw `statusCheckRollup` field from `gh pr view`, which has different schemas
for GitHub Actions (`CheckRun`) and external status checks (`StatusContext`).

Important: `gh pr checks` returns exit code 0 when all checks pass and exit code 1 when
any check fails, but the JSON output is valid in both cases. The wrapper must attempt JSON
parsing before treating non-zero exit as an error.

**How `gh pr create` works:**

Creates a pull request from the current branch to the specified base branch (defaults to
the repository's default branch). The `--title` and `--body` flags set the PR content.
The `--draft` flag creates it as a draft PR. The `--fill` flag auto-populates the title
from the first commit subject and the body from all commit messages — no title/body flags
needed. On success, the command outputs the PR URL to stdout. The current branch must be
pushed to the remote before creating a PR.

**How `gh pr checkout` works:**

Checks out the branch associated with a pull request. If the branch doesn't exist locally,
it creates a local tracking branch. If the PR is from a fork, it adds the fork as a remote
and checks out the branch. This is equivalent to `git fetch` + `git checkout` but handles
fork PRs automatically.

**How to get the current branch's PR:**

There is no single command for "what PR is open for my current branch." Instead, use
`gh pr list --head <current-branch> --state open --limit 1 --json <fields>` to find the
open PR where the current branch is the head (source) branch. If the result array is
empty, there is no open PR for this branch. This query runs once on branch change (not on
every status poll) to avoid excessive API calls.

**New files:**
- `internal/gh/pr.go` — PR command wrappers (list, checks, create, checkout)
- `internal/tui/views/proverlay.go` — PR list overlay with detail pane
- `internal/tui/views/prdetail.go` — PR detail renderer (right pane of overlay)
- `internal/tui/views/prcreateoverlay.go` — create PR form overlay

**Modified files:**
- `internal/tui/app/app.go` — PR messages, commands, handlers, `R` keybinding
- `internal/tui/views/header.go` — current branch PR status indicator
- `internal/tui/views/helpoverlay.go` — PR keybinding documentation

### 18.0 GH CLI PR Commands

**File**: `internal/gh/pr.go` (new file)

This file wraps all PR-related `gh` CLI commands. All commands run with the working
directory set to the repository path so `gh` can detect the correct GitHub remote. JSON
output is parsed with `encoding/json`. The `author` field in gh's JSON output is a nested
object (`{"login": "username"}`), so an intermediate struct handles the deserialization
before flattening into the exported `PullRequest` struct.

```go
package gh

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// PullRequest holds metadata about a single pull request.
type PullRequest struct {
	Number         int
	Title          string
	Body           string
	Author         string // GitHub login username
	State          string // OPEN, CLOSED, MERGED
	IsDraft        bool
	BaseRefName    string // target branch (e.g., "main")
	HeadRefName    string // source branch (e.g., "feature-x")
	ReviewDecision string // APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, ""
	Additions      int
	Deletions      int
	ChangedFiles   int
	CreatedAt      time.Time
	URL            string
}

// PRCheck holds the status of a single CI check on a PR.
type PRCheck struct {
	Name   string // check name (e.g., "CI", "lint")
	State  string // raw state (e.g., "SUCCESS", "FAILURE")
	Bucket string // normalized: pass, fail, pending, skipping, cancel
}

// ghPRJSON is the intermediate struct for deserializing gh pr list/view JSON.
// The author field is a nested object in gh's output, so it needs a sub-struct.
type ghPRJSON struct {
	Number         int    `json:"number"`
	Title          string `json:"title"`
	Body           string `json:"body"`
	Author         struct {
		Login string `json:"login"`
	} `json:"author"`
	State          string `json:"state"`
	IsDraft        bool   `json:"isDraft"`
	BaseRefName    string `json:"baseRefName"`
	HeadRefName    string `json:"headRefName"`
	ReviewDecision string `json:"reviewDecision"`
	Additions      int    `json:"additions"`
	Deletions      int    `json:"deletions"`
	ChangedFiles   int    `json:"changedFiles"`
	CreatedAt      string `json:"createdAt"`
	URL            string `json:"url"`
}

// ghCheckJSON is the intermediate struct for gh pr checks JSON output.
type ghCheckJSON struct {
	Name   string `json:"name"`
	State  string `json:"state"`
	Bucket string `json:"bucket"`
}

// convertPR flattens the nested ghPRJSON into a PullRequest.
func convertPR(j ghPRJSON) PullRequest {
	// The _ discards the parse error. If parsing fails, `created` is the
	// zero time (Jan 1, year 1), which formatPRDate handles by returning "".
	created, _ := time.Parse(time.RFC3339, j.CreatedAt)
	return PullRequest{
		Number:         j.Number,
		Title:          j.Title,
		Body:           j.Body,
		Author:         j.Author.Login,
		State:          j.State,
		IsDraft:        j.IsDraft,
		BaseRefName:    j.BaseRefName,
		HeadRefName:    j.HeadRefName,
		ReviewDecision: j.ReviewDecision,
		Additions:      j.Additions,
		Deletions:      j.Deletions,
		ChangedFiles:   j.ChangedFiles,
		CreatedAt:      created,
		URL:            j.URL,
	}
}

// ListPRs returns pull requests for the repository, filtered by state.
// State must be "open", "closed", "merged", or "all".
// Returns up to 30 PRs sorted by most recently updated.
func ListPRs(repoPath, state string) ([]PullRequest, error) {
	cmd := exec.Command("gh", "pr", "list",
		"--state", state,
		"--limit", "30",
		"--json", "number,title,body,author,state,isDraft,baseRefName,headRefName,"+
			"reviewDecision,additions,deletions,changedFiles,createdAt,url",
	)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return nil, fmt.Errorf("gh pr list failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return nil, fmt.Errorf("gh pr list failed: %w", err)
	}

	var raw []ghPRJSON
	if err := json.Unmarshal(out, &raw); err != nil {
		return nil, fmt.Errorf("failed to parse PR list JSON: %w", err)
	}

	prs := make([]PullRequest, len(raw))
	for i, r := range raw {
		prs[i] = convertPR(r)
	}
	return prs, nil
}

// GetPRChecks returns the CI check statuses for a specific PR.
// Returns an empty slice (not an error) if the PR has no CI checks.
func GetPRChecks(repoPath string, number int) ([]PRCheck, error) {
	cmd := exec.Command("gh", "pr", "checks",
		fmt.Sprintf("%d", number),
		"--json", "name,state,bucket",
	)
	cmd.Dir = repoPath

	// Use CombinedOutput (not Output) because gh pr checks exits 1 when
	// checks fail. Output() would return an error immediately on non-zero
	// exit code and discard the output. CombinedOutput captures stdout+stderr
	// regardless of exit code, so we can attempt JSON parsing first.
	out, err := cmd.CombinedOutput()

	// Attempt JSON parse regardless of exit code — gh returns valid JSON
	// even with exit code 1 (failed checks) when --json is used.
	var raw []ghCheckJSON
	if jsonErr := json.Unmarshal(out, &raw); jsonErr == nil {
		checks := make([]PRCheck, len(raw))
		for i, r := range raw {
			checks[i] = PRCheck{Name: r.Name, State: r.State, Bucket: r.Bucket}
		}
		return checks, nil
	}

	// JSON parsing failed — this is a real error (not just failed checks)
	if err != nil {
		return nil, fmt.Errorf("gh pr checks failed: %s", strings.TrimSpace(string(out)))
	}
	return []PRCheck{}, nil
}

// CreatePR creates a new pull request from the current branch.
// Returns the PR URL on success. The current branch must be pushed first.
func CreatePR(repoPath, title, body, base string, draft bool) (string, error) {
	args := []string{"pr", "create", "--title", title, "--body", body}
	if base != "" {
		args = append(args, "--base", base)
	}
	if draft {
		args = append(args, "--draft")
	}

	cmd := exec.Command("gh", args...)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("gh pr create failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return "", fmt.Errorf("gh pr create failed: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CreatePRFill creates a new pull request with title and body auto-filled from commits.
// The --fill flag uses the first commit's subject as the title and all commit messages
// as the body. Returns the PR URL on success.
func CreatePRFill(repoPath, base string, draft bool) (string, error) {
	args := []string{"pr", "create", "--fill"}
	if base != "" {
		args = append(args, "--base", base)
	}
	if draft {
		args = append(args, "--draft")
	}

	cmd := exec.Command("gh", args...)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("gh pr create --fill failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return "", fmt.Errorf("gh pr create --fill failed: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CheckoutPR checks out the branch associated with a pull request.
// If the branch doesn't exist locally, gh creates a local tracking branch.
// For fork PRs, gh adds the fork as a remote automatically.
func CheckoutPR(repoPath string, number int) error {
	cmd := exec.Command("gh", "pr", "checkout", fmt.Sprintf("%d", number))
	cmd.Dir = repoPath

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("gh pr checkout failed: %w\n%s", err, strings.TrimSpace(string(output)))
	}
	return nil
}

// GetCurrentBranchPR returns the open PR where the given branch is the head (source).
// Returns nil (not an error) if no open PR exists for this branch.
// This is used to show a PR indicator in the header for the current branch.
func GetCurrentBranchPR(repoPath, branch string) (*PullRequest, error) {
	if branch == "" {
		return nil, nil
	}

	cmd := exec.Command("gh", "pr", "list",
		"--head", branch,
		"--state", "open",
		"--limit", "1",
		"--json", "number,title,author,state,isDraft,baseRefName,headRefName,"+
			"reviewDecision,additions,deletions,changedFiles,createdAt,url",
	)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		// Silently return nil — network errors shouldn't block the header.
		// The user can always open the PR overlay to see PR info.
		return nil, nil
	}

	var raw []ghPRJSON
	if err := json.Unmarshal(out, &raw); err != nil {
		return nil, nil
	}

	if len(raw) == 0 {
		return nil, nil // no open PR for this branch
	}

	pr := convertPR(raw[0])
	return &pr, nil
}
```

The `GetCurrentBranchPR` function silently returns `nil` on errors instead of propagating
them. This is intentional — the current-branch PR indicator is a convenience feature, not
critical functionality. Network errors or rate limits shouldn't disrupt the header rendering.
The user can always open the PR overlay (`R`) for full PR information.

### 18.1 PR List View

**File**: `internal/tui/views/proverlay.go` (new file)

The PR overlay is a fullscreen modal with two panes: a scrollable PR list on the left
(40% width) and a detail view on the right (60% width). The list shows PR number, title,
and state label. Navigation uses `j`/`k` for cursor movement, `o` to cycle the state
filter, `Enter` to checkout the selected PR's branch, `c` to create a new PR, and `Esc`
to close.

**Layout:**

```
┌──────────────────────────────────────────────────────────────┐
│  Pull Requests              [Open]  Closed  Merged  All     │
├────────────────────────────────┬─────────────────────────────┤
│  ▸ #42  Fix login bug     OPEN │  Fix login bug  #42         │
│    #41  Add search      MERGED │  @leo • 2 days ago          │
│    #40  Refactor API     DRAFT │  feature-branch → main      │
│    #39  Update deps     CLOSED │                             │
│                                │  This PR fixes the login... │
│                                │                             │
│                                │  ─── Checks ────────────    │
│                                │  ✓ 3 passed  ✗ 1 failed    │
│                                │  ─── Reviews ───────────    │
│                                │  ✓ Approved                 │
│                                │                             │
│                                │  +23 −5 • 3 files changed   │
└────────────────────────────────┴─────────────────────────────┘
 j/k: navigate  o: filter  Enter: checkout  c: create  Esc: close
```

The overlay receives PR data from the app via `SetPRs()` and `SetChecks()` methods. It
does not call `gh` commands directly — all async operations are dispatched by the app in
response to messages emitted by the overlay.

```go
package views

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/gh"
)

// ── PR overlay messages ─────────────────────────────────

// PRCheckoutMsg is sent when the user presses Enter to checkout a PR's branch.
type PRCheckoutMsg struct {
	Number int
}

// PRCreateRequestMsg is sent when the user presses c to open the create PR form.
type PRCreateRequestMsg struct {
	BaseBranch string // suggested base branch for the new PR
}

// PROverlayCloseMsg is sent when the user presses Esc to close the overlay.
type PROverlayCloseMsg struct{}

// PRNeedChecksMsg is sent when the overlay needs CI checks loaded for a PR.
// The app handles this by dispatching the async gh pr checks command.
type PRNeedChecksMsg struct {
	Number int
}

// PRFilterChangeMsg is sent when the user cycles the state filter.
// The app handles this by reloading the PR list with the new filter.
type PRFilterChangeMsg struct {
	State string // "open", "closed", "merged", "all"
}

// ── PR overlay model ────────────────────────────────────

// PROverlayModel is a fullscreen overlay showing the PR list and detail pane.
type PROverlayModel struct {
	Visible      bool
	prs          []gh.PullRequest
	cursor       int
	filterState  string                  // "open", "closed", "merged", "all"
	loading      bool
	checks       map[int][]gh.PRCheck    // cached checks per PR number
	width        int
	height       int
	detailScroll int                     // scroll offset in the detail pane
	baseBranch   string                  // default base branch for create
}

// NewPROverlay creates a hidden PR overlay with default state.
func NewPROverlay() PROverlayModel {
	return PROverlayModel{
		filterState: "open",
		checks:      make(map[int][]gh.PRCheck),
	}
}

// Show opens the PR overlay in loading state. The app must dispatch the
// PR list load command separately. The overlay clears existing data and
// resets the cursor.
func (m *PROverlayModel) Show(baseBranch string, width, height int) {
	m.Visible = true
	m.loading = true
	m.prs = nil
	m.cursor = 0
	m.detailScroll = 0
	m.baseBranch = baseBranch
	m.width = width
	m.height = height
}

// Reshow makes the overlay visible again without resetting state.
// Used when returning from the create PR form without changes.
func (m *PROverlayModel) Reshow() {
	m.Visible = true
}

// SetPRs updates the PR list after loading completes.
func (m *PROverlayModel) SetPRs(prs []gh.PullRequest) {
	m.prs = prs
	m.loading = false
	m.cursor = 0
	m.detailScroll = 0
}

// SetChecks stores CI check results for a specific PR.
func (m *PROverlayModel) SetChecks(number int, checks []gh.PRCheck) {
	m.checks[number] = checks
}

// SelectedPR returns the currently selected PR, or nil if the list is empty.
func (m *PROverlayModel) SelectedPR() *gh.PullRequest {
	if len(m.prs) == 0 || m.cursor >= len(m.prs) {
		return nil
	}
	return &m.prs[m.cursor]
}

// Update handles key events for the PR overlay.
func (m PROverlayModel) Update(msg tea.Msg) (PROverlayModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "escape":
			m.Visible = false
			return m, func() tea.Msg { return PROverlayCloseMsg{} }

		case "j", "down":
			if m.cursor < len(m.prs)-1 {
				m.cursor++
				m.detailScroll = 0
				// Request checks if not cached for newly selected PR
				pr := m.prs[m.cursor]
				if _, ok := m.checks[pr.Number]; !ok {
					number := pr.Number
					return m, func() tea.Msg {
						return PRNeedChecksMsg{Number: number}
					}
				}
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.detailScroll = 0
				pr := m.prs[m.cursor]
				if _, ok := m.checks[pr.Number]; !ok {
					number := pr.Number
					return m, func() tea.Msg {
						return PRNeedChecksMsg{Number: number}
					}
				}
			}
			return m, nil

		case "enter":
			// Checkout the selected PR's branch
			if len(m.prs) > 0 {
				m.Visible = false
				number := m.prs[m.cursor].Number
				return m, func() tea.Msg {
					return PRCheckoutMsg{Number: number}
				}
			}
			return m, nil

		case "c":
			// Open create PR form
			m.Visible = false
			base := m.baseBranch
			return m, func() tea.Msg {
				return PRCreateRequestMsg{BaseBranch: base}
			}

		case "o":
			// Cycle state filter: open → closed → merged → all → open
			switch m.filterState {
			case "open":
				m.filterState = "closed"
			case "closed":
				m.filterState = "merged"
			case "merged":
				m.filterState = "all"
			default:
				m.filterState = "open"
			}
			m.loading = true
			m.prs = nil
			m.cursor = 0
			state := m.filterState
			return m, func() tea.Msg {
				return PRFilterChangeMsg{State: state}
			}

		case "J":
			// Scroll detail pane down
			m.detailScroll++
			return m, nil

		case "K":
			// Scroll detail pane up
			if m.detailScroll > 0 {
				m.detailScroll--
			}
			return m, nil
		}

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
	}

	return m, nil
}

// View renders the PR overlay as a fullscreen split-pane layout.
func (m PROverlayModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#58A6FF"))
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	textStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9"))

	// ── Filter tabs ──
	filterStates := []string{"open", "closed", "merged", "all"}
	var tabs []string
	for _, s := range filterStates {
		label := strings.ToUpper(s[:1]) + s[1:] // capitalize first letter
		if s == m.filterState {
			tabs = append(tabs, lipgloss.NewStyle().
				Bold(true).
				Foreground(lipgloss.Color("#FFFFFF")).
				Background(lipgloss.Color("#30363D")).
				Padding(0, 1).
				Render(label))
		} else {
			tabs = append(tabs, dimStyle.Copy().Padding(0, 1).Render(label))
		}
	}
	header := titleStyle.Render("Pull Requests") + "  " +
		lipgloss.JoinHorizontal(lipgloss.Center, tabs...)

	// ── Loading state ──
	if m.loading {
		loadingMsg := dimStyle.Render("Loading pull requests...")
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", loadingMsg)
		return lipgloss.NewStyle().
			Width(m.width).Height(m.height).
			Padding(1, 2).
			Render(content)
	}

	// ── Empty state ──
	if len(m.prs) == 0 {
		emptyMsg := dimStyle.Render("No " + m.filterState + " pull requests")
		hint := dimStyle.Render("Press c to create a pull request • Esc to close")
		content := lipgloss.JoinVertical(lipgloss.Left, header, "", emptyMsg, "", hint)
		return lipgloss.NewStyle().
			Width(m.width).Height(m.height).
			Padding(1, 2).
			Render(content)
	}

	// ── Calculate pane dimensions ──
	innerWidth := m.width - 4 // left+right padding
	leftWidth := innerWidth * 2 / 5
	rightWidth := innerWidth - leftWidth - 1 // 1 char for separator
	listHeight := m.height - 6               // header + hints + padding

	// ── Left pane: PR list ──
	var listItems []string
	for i, pr := range m.prs {
		if i >= listHeight {
			break
		}

		cursor := "  "
		if i == m.cursor {
			cursor = "▸ "
		}

		numberStr := dimStyle.Render(fmt.Sprintf("#%d", pr.Number))

		title := pr.Title
		maxTitleLen := leftWidth - 22 // space for cursor, number, state
		if maxTitleLen < 10 {
			maxTitleLen = 10
		}
		if len(title) > maxTitleLen {
			title = title[:maxTitleLen-1] + "…"
		}

		stateLabel := renderPRStateLabel(pr)

		line := fmt.Sprintf("%s%s  %s", cursor, numberStr, title)
		padLen := leftWidth - lipgloss.Width(line) - lipgloss.Width(stateLabel)
		if padLen < 1 {
			padLen = 1
		}
		line += strings.Repeat(" ", padLen) + stateLabel

		if i == m.cursor {
			line = lipgloss.NewStyle().
				Foreground(lipgloss.Color("#FFFFFF")).
				Background(lipgloss.Color("#1F2937")).
				Width(leftWidth).
				Render(line)
		} else {
			line = textStyle.Copy().Width(leftWidth).Render(line)
		}

		listItems = append(listItems, line)
	}

	// Pad list to fill height
	for len(listItems) < listHeight {
		listItems = append(listItems, strings.Repeat(" ", leftWidth))
	}

	leftPane := lipgloss.JoinVertical(lipgloss.Left, listItems...)

	// ── Right pane: PR detail ──
	var rightPane string
	if pr := m.SelectedPR(); pr != nil {
		checks := m.checks[pr.Number] // may be nil if not yet loaded
		rightPane = RenderPRDetail(*pr, checks, rightWidth, listHeight, m.detailScroll)
	}

	// ── Separator ──
	var sepLines []string
	for i := 0; i < listHeight; i++ {
		sepLines = append(sepLines, "│")
	}
	separator := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#30363D")).
		Render(strings.Join(sepLines, "\n"))

	// ── Compose layout ──
	mainContent := lipgloss.JoinHorizontal(lipgloss.Top,
		leftPane,
		separator,
		rightPane,
	)

	hint := dimStyle.Render("j/k: navigate  o: filter  Enter: checkout  c: create  J/K: scroll detail  Esc: close")

	content := lipgloss.JoinVertical(lipgloss.Left,
		header, "",
		mainContent, "",
		hint,
	)

	return lipgloss.NewStyle().
		Width(m.width).Height(m.height).
		Padding(1, 2).
		Render(content)
}

// renderPRStateLabel returns a colored state label for a pull request.
func renderPRStateLabel(pr gh.PullRequest) string {
	if pr.IsDraft {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Render("DRAFT")
	}
	switch pr.State {
	case "OPEN":
		return lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#3FB950")).
			Render("OPEN")
	case "CLOSED":
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#DA3633")).
			Render("CLOSED")
	case "MERGED":
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A371F7")).
			Render("MERGED")
	default:
		return pr.State
	}
}
```

**How the overlay works:**

- Pressing `R` in navigable mode calls `Show()` (sets `loading = true`) and the app
  dispatches `loadPRListCmd`. When the PR list arrives, the app calls `SetPRs()`.
- When the cursor moves (`j`/`k`), the overlay checks if CI checks are cached for the
  newly selected PR. If not, it emits `PRNeedChecksMsg` — the app dispatches the async
  load, and calls `SetChecks()` when results arrive.
- `o` cycles the filter state and emits `PRFilterChangeMsg`. The app dispatches a new
  `loadPRListCmd` with the updated filter. The overlay shows "Loading..." while waiting.
- `Enter` emits `PRCheckoutMsg` and hides the overlay. The app runs `gh pr checkout`.
- `c` emits `PRCreateRequestMsg` and hides the overlay. The app shows the create PR form.
- `J`/`K` (uppercase) scroll the detail pane without moving the list cursor. This is
  useful for reading long PR descriptions.
- The separator uses vertical bar characters (`│`) joined with newlines, rendering as a
  continuous vertical line between the two panes.

**Note on closure variable capture:** Throughout the overlay code, you will see patterns
like `number := pr.Number` followed by `func() tea.Msg { return Msg{Number: number} }`.
The local variable copy is required because Go closures capture variables by reference.
Without the copy, the closure could read a changed value by the time Bubbletea executes it.

### 18.2 PR Detail View (Status, Checks, Reviews)

**File**: `internal/tui/views/prdetail.go` (new file)

This file contains the `RenderPRDetail` function called by the PR overlay to render the
right pane. It shows the PR title, author, branches, body text, CI check summary with
individual check names, review decision, and file change stats. The detail pane supports
scrolling via the `detailScroll` parameter.

```go
package views

import (
	"fmt"
	"strings"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/gh"
)

// RenderPRDetail renders the detail view for a single pull request.
// checks may be nil if not yet loaded (shows "Loading checks..." placeholder).
// scroll is the vertical scroll offset (controlled by J/K in the overlay).
func RenderPRDetail(pr gh.PullRequest, checks []gh.PRCheck, width, height, scroll int) string {
	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#58A6FF"))
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	textStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9"))
	branchStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))
	greenStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))
	redStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#DA3633"))
	yellowStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#D29922"))

	var lines []string

	// ── Title + number ──
	titleText := pr.Title
	if len(titleText) > width-8 {
		titleText = titleText[:width-9] + "…"
	}
	lines = append(lines,
		titleStyle.Render(titleText)+"  "+dimStyle.Render(fmt.Sprintf("#%d", pr.Number)),
	)

	// ── Author + date ──
	dateStr := formatPRDate(pr.CreatedAt)
	lines = append(lines, dimStyle.Render(fmt.Sprintf("@%s • %s", pr.Author, dateStr)))

	// ── Branches ──
	lines = append(lines,
		branchStyle.Render(pr.HeadRefName)+
			dimStyle.Render(" → ")+
			branchStyle.Render(pr.BaseRefName),
	)

	// ── State label ──
	lines = append(lines, renderPRStateLabel(pr))
	lines = append(lines, "")

	// ── Body ──
	if pr.Body != "" {
		bodyLines := strings.Split(pr.Body, "\n")
		maxBodyLines := height - 18
		if maxBodyLines < 3 {
			maxBodyLines = 3
		}
		for i, bl := range bodyLines {
			if i >= maxBodyLines {
				lines = append(lines, dimStyle.Render("  ..."))
				break
			}
			if len(bl) > width-2 {
				bl = bl[:width-3] + "…"
			}
			lines = append(lines, textStyle.Render(bl))
		}
	} else {
		lines = append(lines, dimStyle.Render("No description"))
	}

	lines = append(lines, "")

	// ── Checks section ──
	sepWidth := width - 14
	if sepWidth < 0 {
		sepWidth = 0
	}
	lines = append(lines, dimStyle.Render("─── Checks "+strings.Repeat("─", sepWidth)))

	if checks == nil {
		lines = append(lines, dimStyle.Render("  Loading checks..."))
	} else if len(checks) == 0 {
		lines = append(lines, dimStyle.Render("  No checks"))
	} else {
		// Summary counts
		pass, fail, pending := 0, 0, 0
		for _, c := range checks {
			switch c.Bucket {
			case "pass":
				pass++
			case "fail":
				fail++
			case "pending":
				pending++
			}
		}

		var summary []string
		if pass > 0 {
			summary = append(summary, greenStyle.Render(fmt.Sprintf("✓ %d passed", pass)))
		}
		if fail > 0 {
			summary = append(summary, redStyle.Render(fmt.Sprintf("✗ %d failed", fail)))
		}
		if pending > 0 {
			summary = append(summary, yellowStyle.Render(fmt.Sprintf("⟳ %d pending", pending)))
		}
		lines = append(lines, "  "+strings.Join(summary, "  "))

		// Individual check names (up to 10)
		for i, c := range checks {
			if i >= 10 {
				lines = append(lines, dimStyle.Render(
					fmt.Sprintf("  ... and %d more", len(checks)-10),
				))
				break
			}
			icon := "·"
			style := dimStyle
			switch c.Bucket {
			case "pass":
				icon = "✓"
				style = greenStyle
			case "fail":
				icon = "✗"
				style = redStyle
			case "pending":
				icon = "⟳"
				style = yellowStyle
			case "skipping":
				icon = "⊘"
			case "cancel":
				icon = "⊘"
			}
			name := c.Name
			if len(name) > width-8 {
				name = name[:width-9] + "…"
			}
			lines = append(lines, style.Render(fmt.Sprintf("  %s %s", icon, name)))
		}
	}

	lines = append(lines, "")

	// ── Reviews section ──
	reviewSepWidth := width - 15
	if reviewSepWidth < 0 {
		reviewSepWidth = 0
	}
	lines = append(lines, dimStyle.Render("─── Reviews "+strings.Repeat("─", reviewSepWidth)))

	switch pr.ReviewDecision {
	case "APPROVED":
		lines = append(lines, greenStyle.Render("  ✓ Approved"))
	case "CHANGES_REQUESTED":
		lines = append(lines, redStyle.Render("  ✗ Changes Requested"))
	case "REVIEW_REQUIRED":
		lines = append(lines, yellowStyle.Render("  ⟳ Review Required"))
	default:
		lines = append(lines, dimStyle.Render("  No reviews"))
	}

	lines = append(lines, "")

	// ── File change stats ──
	lines = append(lines,
		greenStyle.Render(fmt.Sprintf("+%d", pr.Additions))+" "+
			redStyle.Render(fmt.Sprintf("-%d", pr.Deletions))+" "+
			dimStyle.Render(fmt.Sprintf("• %d files changed", pr.ChangedFiles)),
	)

	// ── Apply scroll offset ──
	if scroll > 0 {
		if scroll >= len(lines) {
			scroll = len(lines) - 1
		}
		if scroll < 0 {
			scroll = 0
		}
		lines = lines[scroll:]
	}

	// ── Truncate to available height ──
	if len(lines) > height {
		lines = lines[:height]
	}

	return lipgloss.NewStyle().
		Width(width).
		Padding(0, 1).
		Render(lipgloss.JoinVertical(lipgloss.Left, lines...))
}

// formatPRDate formats a time.Time as a human-readable relative or absolute date.
func formatPRDate(t time.Time) string {
	if t.IsZero() {
		return ""
	}
	diff := time.Since(t)
	switch {
	case diff < time.Minute:
		return "just now"
	case diff < time.Hour:
		m := int(diff.Minutes())
		if m == 1 {
			return "1 minute ago"
		}
		return fmt.Sprintf("%d minutes ago", m)
	case diff < 24*time.Hour:
		h := int(diff.Hours())
		if h == 1 {
			return "1 hour ago"
		}
		return fmt.Sprintf("%d hours ago", h)
	case diff < 30*24*time.Hour:
		d := int(diff.Hours() / 24)
		if d == 1 {
			return "1 day ago"
		}
		return fmt.Sprintf("%d days ago", d)
	default:
		return t.Format("Jan 2, 2006")
	}
}
```

**How the detail view works:**

- The detail view is a pure render function — it takes data in and returns a string. No
  state, no side effects. The PR overlay calls it on every `View()` render.
- **Checks section**: Shows a summary line with counts (✓ N passed, ✗ N failed, ⟳ N
  pending) followed by individual check names with icons. Limited to 10 individual checks
  to prevent overflow; shows "... and N more" for additional checks.
- **Reviews section**: Shows the aggregate `reviewDecision` from the GitHub API:
  `APPROVED`, `CHANGES_REQUESTED`, or `REVIEW_REQUIRED`. Individual reviewer details are
  not shown — for that level of detail, the user should use the terminal
  (`gh pr view <number>`).
- **Scroll support**: The `scroll` parameter offsets the rendered lines. The PR overlay
  controls scrolling via `J`/`K` keys. Lines beyond the available height are truncated.
- **Date formatting**: Uses relative times for recent PRs ("2 days ago") and absolute
  dates for older ones ("Jan 15, 2025"). This matches GitHub's PR list UI.

### 18.3 Create Pull Request

**File**: `internal/tui/views/prcreateoverlay.go` (new file)

The create PR form is a centered modal overlay with four fields: title (text input), body
(text input), base branch (text input, defaults to "main"), and draft toggle. Navigation
uses `Tab`/`Shift+Tab` to move between fields and `Enter` to advance or submit. The form
has three buttons: "Create" (uses provided title/body), "Fill & Create" (uses `--fill` to
auto-populate from commits), and "Cancel".

The body field is a single-line input for simplicity. Users who need multi-line PR
descriptions should use the terminal (`gh pr create` with editor) or edit the description
on GitHub after creation.

```go
package views

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// PRCreateConfirmMsg is sent when the user confirms PR creation.
type PRCreateConfirmMsg struct {
	Title string
	Body  string
	Base  string
	Draft bool
	Fill  bool // if true, use gh pr create --fill (ignores Title/Body)
}

// PRCreateCancelMsg is sent when the user cancels PR creation.
type PRCreateCancelMsg struct{}

// PRCreateOverlayModel is a form overlay for creating a new pull request.
type PRCreateOverlayModel struct {
	Visible     bool
	activeField int    // 0=title, 1=body, 2=base, 3=buttons
	selectedBtn int    // 0=Create, 1=Fill & Create, 2=Cancel
	title       string
	body        string
	baseBranch  string
	draft       bool
	width       int
	height      int
	errMsg      string // validation error
}

// NewPRCreateOverlay creates a hidden PR create overlay.
func NewPRCreateOverlay() PRCreateOverlayModel {
	return PRCreateOverlayModel{}
}

// Show opens the create PR form with the given default base branch.
func (m *PRCreateOverlayModel) Show(baseBranch string, width, height int) {
	m.Visible = true
	m.activeField = 0
	m.selectedBtn = 0
	m.title = ""
	m.body = ""
	m.baseBranch = baseBranch
	m.draft = false
	m.width = width
	m.height = height
	m.errMsg = ""
}

// Update handles key events for the create PR form.
func (m PRCreateOverlayModel) Update(msg tea.Msg) (PRCreateOverlayModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		key := msg.String()

		// Global keys (work in any field)
		switch key {
		case "escape":
			m.Visible = false
			return m, func() tea.Msg { return PRCreateCancelMsg{} }

		case "tab":
			m.activeField = (m.activeField + 1) % 4
			return m, nil

		case "shift+tab":
			// (+ 3) % 4 wraps backward: same as -1 but avoids Go negative modulo.
			m.activeField = (m.activeField + 3) % 4
			return m, nil

		case "ctrl+d":
			m.draft = !m.draft
			return m, nil
		}

		// Field-specific key handling
		switch m.activeField {
		case 0: // title input
			return m.handleTextInput(&m.title, key)
		case 1: // body input
			return m.handleTextInput(&m.body, key)
		case 2: // base branch input
			return m.handleTextInput(&m.baseBranch, key)
		case 3: // buttons
			return m.handleButtons(key)
		}
	}

	return m, nil
}

// handleTextInput processes key events for a text input field.
// `field` is a pointer to one of the model's string fields (title, body, or
// baseBranch). The pointer modifies the field on this copy of m, and the
// modified copy is returned — this is the standard Bubbletea value-receiver pattern.
func (m PRCreateOverlayModel) handleTextInput(field *string, key string) (PRCreateOverlayModel, tea.Cmd) {
	switch key {
	case "backspace":
		if len(*field) > 0 {
			*field = (*field)[:len(*field)-1]
		}
	case "enter":
		m.activeField++ // advance to next field
		if m.activeField > 3 {
			m.activeField = 3
		}
	default:
		// Accept printable characters and space.
		// Bubbletea sends "space" as a named key, not as " ".
		// ASCII 32–126 covers all printable characters (letters, digits, symbols).
		if key == "space" {
			*field += " "
		} else if len(key) == 1 && key[0] >= 32 && key[0] <= 126 {
			*field += key
		}
	}
	return m, nil
}

// handleButtons processes key events for the button row.
func (m PRCreateOverlayModel) handleButtons(key string) (PRCreateOverlayModel, tea.Cmd) {
	switch key {
	case "left", "h":
		if m.selectedBtn > 0 {
			m.selectedBtn--
		}
	case "right", "l":
		if m.selectedBtn < 2 {
			m.selectedBtn++
		}
	case "enter":
		m.errMsg = ""
		switch m.selectedBtn {
		case 0: // Create
			if strings.TrimSpace(m.title) == "" {
				m.errMsg = "Title is required"
				return m, nil
			}
			m.Visible = false
			title, body, base, draft := m.title, m.body, m.baseBranch, m.draft
			return m, func() tea.Msg {
				return PRCreateConfirmMsg{
					Title: title, Body: body, Base: base,
					Draft: draft, Fill: false,
				}
			}
		case 1: // Fill & Create
			m.Visible = false
			base, draft := m.baseBranch, m.draft
			return m, func() tea.Msg {
				return PRCreateConfirmMsg{
					Base: base, Draft: draft, Fill: true,
				}
			}
		case 2: // Cancel
			m.Visible = false
			return m, func() tea.Msg { return PRCreateCancelMsg{} }
		}
	}
	return m, nil
}

// View renders the create PR form as a centered modal.
func (m PRCreateOverlayModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#58A6FF"))
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	labelStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9")).Width(8)
	errStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#DA3633"))

	activeInputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#1F2937")).
		Padding(0, 1).
		Width(40)

	inactiveInputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9")).
		Background(lipgloss.Color("#0D1117")).
		Padding(0, 1).
		Width(40)

	activeBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#238636")).
		Padding(0, 2)

	inactiveBtnStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9")).
		Background(lipgloss.Color("#30363D")).
		Padding(0, 2)

	cancelActiveBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#DA3633")).
		Padding(0, 2)

	// Header
	header := titleStyle.Render("Create Pull Request")

	// Input field renderer
	renderField := func(label, value string, fieldIdx int) string {
		display := value
		if fieldIdx == m.activeField {
			display += "█"
		}
		if display == "" && fieldIdx != m.activeField {
			display = " "
		}
		style := inactiveInputStyle
		if fieldIdx == m.activeField {
			style = activeInputStyle
		}
		return labelStyle.Render(label+":") + " " + style.Render(display)
	}

	titleField := renderField("Title", m.title, 0)
	bodyField := renderField("Body", m.body, 1)
	baseField := renderField("Base", m.baseBranch, 2)

	// Draft toggle
	draftIcon := "[ ]"
	if m.draft {
		draftIcon = "[✓]"
	}
	draftLine := labelStyle.Render("Draft:") + " " +
		dimStyle.Render(draftIcon+" (Ctrl+D to toggle)")

	// Buttons
	btnLabels := []string{"Create", "Fill & Create", "Cancel"}
	var buttons []string
	for i, label := range btnLabels {
		if m.activeField == 3 && i == m.selectedBtn {
			if i == 2 {
				buttons = append(buttons, cancelActiveBtnStyle.Render(label))
			} else {
				buttons = append(buttons, activeBtnStyle.Render(label))
			}
		} else {
			buttons = append(buttons, inactiveBtnStyle.Render(label))
		}
	}
	buttonRow := lipgloss.JoinHorizontal(lipgloss.Center,
		buttons[0], " ", buttons[1], " ", buttons[2],
	)

	// Error message
	var errLine string
	if m.errMsg != "" {
		errLine = errStyle.Render(m.errMsg)
	}

	// Hints
	hint := dimStyle.Render("Tab: next field • Ctrl+D: toggle draft • Enter: submit • Esc: cancel")

	// Compose box content
	var lines []string
	lines = append(lines, header, "")
	lines = append(lines, titleField, bodyField, baseField, "")
	lines = append(lines, draftLine, "")
	if errLine != "" {
		lines = append(lines, errLine, "")
	}
	lines = append(lines, buttonRow, "", hint)

	boxContent := lipgloss.JoinVertical(lipgloss.Left, lines...)

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
```

**How the create form works:**

- The form has 4 navigation stops: title (0), body (1), base branch (2), and buttons (3).
  `Tab`/`Shift+Tab` cycles between them. `Enter` on a text field advances to the next field.
- Text input is simple: printable ASCII characters are appended, `Backspace` deletes the
  last character, and the cursor (█) shows which field is active.
- **"Create" button**: Validates that the title is non-empty, then emits
  `PRCreateConfirmMsg{Fill: false}` with the form data. The app runs
  `gh pr create --title ... --body ... --base ...`.
- **"Fill & Create" button**: Emits `PRCreateConfirmMsg{Fill: true}`. The app runs
  `gh pr create --fill --base ...` — gh auto-fills the title from the first commit's
  subject and the body from all commit messages. This is the fastest way to create a PR
  when the commit messages are well-written.
- **"Cancel" button** and `Esc`: Emit `PRCreateCancelMsg`. The app re-shows the PR overlay.
- **Draft toggle**: `Ctrl+D` toggles the draft flag from any field. The checkbox shows
  `[✓]` when draft is enabled. Draft PRs are created with `gh pr create --draft`.
- **Validation**: Only the title is validated (must be non-empty) for the "Create" button.
  "Fill & Create" skips validation since gh generates the title from commits.
- Button styling follows the same convention as the merge overlay: green for action buttons,
  red for cancel (when focused), gray for unfocused.

### 18.4 Checkout PR Branch

**File**: `internal/tui/app/app.go` (modify existing)

This section connects all PR functionality to the main app. The changes are:

1. New messages for PR operations
2. New async commands for PR list, checks, create, checkout, and current branch PR
3. New Model fields for PR state
4. PR overlay and create overlay integration in `handleMainKey()` and `Update()`
5. `R` keybinding to open the PR overlay
6. PR status indicator in header
7. Current branch PR detection on branch change

**New imports** — add `gh` package to the existing import block:

```go
	"github.com/LeoManrique/leogit/internal/gh"
```

The `gh` package is already imported for `gh.CheckAuth()` (Phase 2), so this import
already exists. No new import needed.

**New messages** — add after the existing message types:

```go
// prListResultMsg carries the loaded PR list from gh pr list.
type prListResultMsg struct {
	prs []gh.PullRequest
	err error
}

// prChecksResultMsg carries CI check statuses for a specific PR.
type prChecksResultMsg struct {
	number int
	checks []gh.PRCheck
	err    error
}

// prCheckoutCompleteMsg is sent after gh pr checkout completes.
type prCheckoutCompleteMsg struct {
	number int
	err    error
}

// prCreateCompleteMsg is sent after gh pr create completes.
type prCreateCompleteMsg struct {
	url string
	err error
}

// currentBranchPRMsg carries the open PR for the current branch (if any).
type currentBranchPRMsg struct {
	pr  *gh.PullRequest
	err error
}
```

**New async commands** — add after the existing command functions:

```go
// loadPRListCmd loads pull requests for the given state filter.
func loadPRListCmd(repoPath, state string) tea.Cmd {
	return func() tea.Msg {
		prs, err := gh.ListPRs(repoPath, state)
		return prListResultMsg{prs: prs, err: err}
	}
}

// loadPRChecksCmd loads CI check statuses for a specific PR.
func loadPRChecksCmd(repoPath string, number int) tea.Cmd {
	return func() tea.Msg {
		checks, err := gh.GetPRChecks(repoPath, number)
		return prChecksResultMsg{number: number, checks: checks, err: err}
	}
}

// checkoutPRCmd runs gh pr checkout asynchronously.
func checkoutPRCmd(repoPath string, number int) tea.Cmd {
	return func() tea.Msg {
		err := gh.CheckoutPR(repoPath, number)
		return prCheckoutCompleteMsg{number: number, err: err}
	}
}

// createPRCmd creates a PR with explicit title and body.
func createPRCmd(repoPath, title, body, base string, draft bool) tea.Cmd {
	return func() tea.Msg {
		url, err := gh.CreatePR(repoPath, title, body, base, draft)
		return prCreateCompleteMsg{url: url, err: err}
	}
}

// createPRFillCmd creates a PR with auto-filled title/body from commits.
func createPRFillCmd(repoPath, base string, draft bool) tea.Cmd {
	return func() tea.Msg {
		url, err := gh.CreatePRFill(repoPath, base, draft)
		return prCreateCompleteMsg{url: url, err: err}
	}
}

// loadCurrentBranchPRCmd checks if the current branch has an open PR.
// This runs once on branch change, not on every status poll.
func loadCurrentBranchPRCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		pr, err := gh.GetCurrentBranchPR(repoPath, branch)
		return currentBranchPRMsg{pr: pr, err: err}
	}
}
```

**Changes to the Model struct** — add PR state fields:

```go
	// Pull Requests
	prOverlay        views.PROverlayModel       // PR list overlay
	prCreateOverlay  views.PRCreateOverlayModel  // create PR form overlay
	checkingOutPR    bool                        // true while gh pr checkout is running
	creatingPR       bool                        // true while gh pr create is running
	currentBranchPR  *gh.PullRequest             // open PR for current branch (nil if none)
	prFilterState    string                      // current PR list filter ("open", etc.)
```

- `prOverlay` is the PR list + detail overlay shown when `R` is pressed.
- `prCreateOverlay` is the create PR form, shown when `c` is pressed in the PR overlay.
- `checkingOutPR` is `true` while `gh pr checkout` is running (shows spinner in header).
- `creatingPR` is `true` while `gh pr create` is running (shows spinner in header).
- `currentBranchPR` holds the open PR for the current branch, loaded on branch change.
  `nil` if no open PR exists. Used to show a PR indicator in the header.
- `prFilterState` remembers the last-used filter across overlay open/close cycles.

**Changes to `New()`** — initialize the PR overlays:

```go
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		// ... existing fields ...
		prOverlay:       views.NewPROverlay(),
		prCreateOverlay: views.NewPRCreateOverlay(),
		prFilterState:   "open",
	}
}
```

**Changes to `handleMainKey()`** — add PR overlay priority and `R` keybinding.

The PR overlays should be checked AFTER the merge overlay but BEFORE the branch dropdown.
Add these blocks between the merge overlay check and the branch dropdown check:

```go
	// ── PR create overlay ──
	// Checked before PR overlay because the create form sits on top of the PR list
	if m.prCreateOverlay.Visible {
		var cmd tea.Cmd
		m.prCreateOverlay, cmd = m.prCreateOverlay.Update(msg)
		return m, cmd
	}

	// ── PR overlay ──
	if m.prOverlay.Visible {
		var cmd tea.Cmd
		m.prOverlay, cmd = m.prOverlay.Update(msg)
		return m, cmd
	}
```

Add the `R` keybinding in the navigable mode section (alongside existing `B`, `S`, `F`,
`P`, `A`):

```go
	case "R":
		// Open PR overlay — load PR list for current filter state
		m.prOverlay.Show("main", m.width, m.height)
		return m, loadPRListCmd(m.repoPath, m.prFilterState)
```

The `R` key is uppercase to avoid conflicts with lowercase keys. The default base branch
is "main" — the user can change it in the create PR form if their repo uses a different
default branch.

**Changes to `Update()`** — add handlers for all PR-related messages. Add these cases
to the existing `switch msg := msg.(type)` block:

```go
	case views.PRCheckoutMsg:
		// User pressed Enter on a PR — checkout its branch
		m.checkingOutPR = true
		return m, checkoutPRCmd(m.repoPath, msg.Number)

	case views.PRCreateRequestMsg:
		// User pressed c in PR overlay — show create form
		m.prCreateOverlay.Show(msg.BaseBranch, m.width, m.height)
		return m, nil

	case views.PROverlayCloseMsg:
		// User pressed Esc in PR overlay — no action needed
		return m, nil

	case views.PRNeedChecksMsg:
		// PR overlay needs CI checks for a PR — dispatch async load
		return m, loadPRChecksCmd(m.repoPath, msg.Number)

	case views.PRFilterChangeMsg:
		// User cycled the state filter — reload PR list
		m.prFilterState = msg.State
		return m, loadPRListCmd(m.repoPath, msg.State)

	case views.PRCreateConfirmMsg:
		// User confirmed PR creation
		m.creatingPR = true
		if msg.Fill {
			return m, createPRFillCmd(m.repoPath, msg.Base, msg.Draft)
		}
		return m, createPRCmd(m.repoPath, msg.Title, msg.Body, msg.Base, msg.Draft)

	case views.PRCreateCancelMsg:
		// User cancelled PR creation — re-show PR overlay with existing data
		m.prOverlay.Reshow()
		return m, nil

	case prListResultMsg:
		if msg.err != nil {
			m.prOverlay.Visible = false
			m.errorModal = views.ShowError(
				"PR List Error",
				msg.err.Error(),
				false, nil, m.width, m.height,
			)
			return m, nil
		}
		m.prOverlay.SetPRs(msg.prs)
		// Auto-load checks for the first PR in the list
		if len(msg.prs) > 0 {
			return m, loadPRChecksCmd(m.repoPath, msg.prs[0].Number)
		}
		return m, nil

	case prChecksResultMsg:
		// Store checks in the overlay — errors are silently ignored
		// (the detail pane shows "Loading checks..." until data arrives)
		if msg.err == nil {
			m.prOverlay.SetChecks(msg.number, msg.checks)
		}
		return m, nil

	case prCheckoutCompleteMsg:
		m.checkingOutPR = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"PR Checkout Error",
				fmt.Sprintf("Failed to checkout PR #%d:\n\n%s", msg.number, msg.err.Error()),
				false, nil, m.width, m.height,
			)
			return m, nil
		}
		// Checkout succeeded — refresh status (branch has changed).
		// Also clear history cache since we're on a new branch.
		m.commitList.SetCommits(nil)
		m.selectedCommit = nil
		m.commitFiles.SetFiles(nil)
		m.commitDiffView.Clear()
		m.logExhausted = false
		return m, refreshStatusCmd(m.repoPath)

	case prCreateCompleteMsg:
		m.creatingPR = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"PR Create Error",
				msg.err.Error(),
				false, nil, m.width, m.height,
			)
			return m, nil
		}
		// Success — reopen PR overlay with refreshed list.
		// tea.Batch runs multiple commands concurrently (both start at once).
		m.prOverlay.Show("main", m.width, m.height)
		return m, tea.Batch(
			loadPRListCmd(m.repoPath, m.prFilterState),
			loadCurrentBranchPRCmd(m.repoPath, m.branchName),
		)

	case currentBranchPRMsg:
		// Update current branch PR indicator (silently ignore errors)
		if msg.err == nil {
			m.currentBranchPR = msg.pr
		}
		return m, nil
```

**Changes to `statusResultMsg` handler** — detect branch changes for PR status.

Add this check in the `statusResultMsg` handler, after the existing branch change
detection and status updates. The key insight is that `m.branchName` has already been
updated to the new value by this point, so we compare the old and new branch names:

```go
	case statusResultMsg:
		if msg.err != nil {
			// ... existing error handling ...
			return m, nil
		}

		// ... existing branch change detection ...
		newBranch := msg.status.BranchName
		branchChanged := m.branchName != newBranch

		// ... existing status field updates (m.branchName = newBranch, etc.) ...

		// ... existing postPullCheck, postMergeCheck, IsMerging checks ...

		// Load current branch PR on branch change
		// Only runs when the branch actually changes — not on every 2-second poll.
		// This avoids excessive gh API calls while keeping the header indicator current.
		if branchChanged {
			return m, tea.Batch(
				statusTickCmd(),
				loadCurrentBranchPRCmd(m.repoPath, m.branchName),
			)
		}
```

Note: the `branchChanged` variable already exists in the status handler from Phase 15
(branch switching). Phase 18 adds a `loadCurrentBranchPRCmd` to the batch of commands
dispatched when a branch change is detected. Place this block at the end of the
`statusResultMsg` handler. If the handler already returns inside a `branchChanged`
check, add `loadCurrentBranchPRCmd` to that existing `tea.Batch` instead of creating
a separate block.

**Changes to `View()`** — add PR overlay rendering. In the `stateMain` section, add the
PR overlay checks after the merge overlay and before `viewMain()`:

```go
		case stateMain:
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else if m.mergeOverlay.Visible {
				content = m.mergeOverlay.View()
			} else if m.prCreateOverlay.Visible {
				content = m.prCreateOverlay.View()
			} else if m.prOverlay.Visible {
				content = m.prOverlay.View()
			} else {
				content = m.viewMain()
			}
```

The PR create overlay is checked before the PR overlay because the create form conceptually
sits on top of the PR list. Both are fullscreen overlays that take over rendering.

**Changes to `viewMain()`** — pass PR state to header:

```go
	headerData := views.HeaderData{
		// ... existing fields ...
		CheckingOutPR:   m.checkingOutPR,
		CreatingPR:      m.creatingPR,
		CurrentBranchPR: m.currentBranchPR,
	}
```

**End-to-end PR list flow:**

1. User presses `R` in navigable mode
2. App calls `m.prOverlay.Show("main", ...)` — overlay enters loading state
3. App dispatches `loadPRListCmd(m.repoPath, "open")`
4. Overlay renders "Loading pull requests..."
5. `prListResultMsg{prs: [...]}` arrives
6. App calls `m.prOverlay.SetPRs(prs)` — list renders with first PR selected
7. App dispatches `loadPRChecksCmd(m.repoPath, prs[0].Number)` — loads checks for first PR
8. `prChecksResultMsg` arrives — app calls `m.prOverlay.SetChecks(...)` — detail pane shows checks
9. User presses `j` to move down — overlay emits `PRNeedChecksMsg` if checks not cached
10. App dispatches another `loadPRChecksCmd` — cycle repeats
11. User presses `o` — overlay cycles filter to "closed", emits `PRFilterChangeMsg`
12. App dispatches `loadPRListCmd(m.repoPath, "closed")` — list reloads

**End-to-end PR creation flow:**

1. User presses `c` in the PR overlay
2. Overlay hides, emits `PRCreateRequestMsg{BaseBranch: "main"}`
3. App shows `prCreateOverlay.Show("main", ...)`
4. User fills in title: "Fix login bug", body: "Fixes #42", base: "main", draft: false
5. User tabs to buttons, presses Enter on "Create"
6. Create overlay emits `PRCreateConfirmMsg{Title: "Fix login bug", Body: "Fixes #42", ...}`
7. App sets `m.creatingPR = true`, dispatches `createPRCmd`
8. Header shows `⟳ Creating PR...`
9. `gh pr create --title "Fix login bug" --body "Fixes #42" --base main` runs
10. `prCreateCompleteMsg{url: "https://github.com/..."}` arrives
11. App sets `m.creatingPR = false`, reopens PR overlay with refreshed list
12. Also dispatches `loadCurrentBranchPRCmd` to update header indicator

**End-to-end PR checkout flow:**

1. User selects PR #42 in the overlay, presses Enter
2. Overlay hides, emits `PRCheckoutMsg{Number: 42}`
3. App sets `m.checkingOutPR = true`, dispatches `checkoutPRCmd`
4. Header shows `⟳ Checking out PR...`
5. `gh pr checkout 42` runs (creates local branch if needed)
6. `prCheckoutCompleteMsg{number: 42}` arrives
7. App clears history cache, refreshes status
8. Status update picks up new branch — header shows new branch name
9. Next branch change detection triggers `loadCurrentBranchPRCmd` — header shows PR #42 indicator

### 18.5 PR Status Summary

**File**: `internal/tui/views/header.go` (modify existing)

Add three new fields to `HeaderData` for PR status in the header:

```go
type HeaderData struct {
	// ... existing fields ...
	CheckingOutPR   bool            // true while gh pr checkout is running
	CreatingPR      bool            // true while gh pr create is running
	CurrentBranchPR *gh.PullRequest // open PR for current branch (nil if none)
}
```

The `CurrentBranchPR` field uses a pointer (`*gh.PullRequest`) so it can be `nil` when
there is no open PR. A non-pointer struct would always exist as a zero value, making it
impossible to distinguish "no PR" from a PR with default fields. Since the header is in
the `views` package and already imports `gh` (indirectly through the models used in
other views), this import is available. If the `views` package does not yet import `gh`
directly in `header.go`, add it:

```go
import (
	// ... existing imports ...
	"github.com/LeoManrique/leogit/internal/gh"
)
```

In `RenderHeader()`, add PR indicators after the existing operation indicators
(`Switching`, `Fetching`, `Pulling`, `Merging`):

```go
	// PR indicators
	if data.CheckingOutPR {
		indicators = append(indicators, spinnerStyle.Render("⟳ Checking out PR..."))
	} else if data.CreatingPR {
		indicators = append(indicators, spinnerStyle.Render("⟳ Creating PR..."))
	} else if data.CurrentBranchPR != nil {
		pr := data.CurrentBranchPR
		prStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF"))
		reviewIcon := ""
		switch pr.ReviewDecision {
		case "APPROVED":
			reviewIcon = " ✓"
		case "CHANGES_REQUESTED":
			reviewIcon = " ✗"
		case "REVIEW_REQUIRED":
			reviewIcon = " ⟳"
		}
		prLabel := fmt.Sprintf("PR #%d%s", pr.Number, reviewIcon)
		indicators = append(indicators, prStyle.Render(prLabel))
	}
```

This adds a blue `PR #42` indicator next to the branch name when the current branch has
an open PR. The review decision icon provides at-a-glance status:
- `PR #42 ✓` — approved
- `PR #42 ✗` — changes requested
- `PR #42 ⟳` — review required
- `PR #42` — no reviews yet

The indicator is loaded once on branch change (not on every poll) and updates when:
- The user switches branches (status handler detects `branchChanged`)
- The user creates a PR (create complete handler dispatches reload)
- The user checks out a PR (checkout triggers status refresh → branch change → PR reload)

### 18.6 Help Overlay Update

**File**: `internal/tui/views/helpoverlay.go` (modify existing)

Add PR keybindings to the help overlay. Insert after the existing "Merge" section:

```go
		"",
		sectionStyle.Render("Pull Requests"),
		row("R", "Open pull requests overlay"),
		row("Enter", "Checkout PR branch (in PR overlay)"),
		row("c", "Create pull request (in PR overlay)"),
		row("o", "Cycle PR filter: Open/Closed/Merged/All"),
		row("j/k", "Navigate PR list"),
		row("J/K", "Scroll PR detail pane"),
```

These are added as a separate section because PR operations only work within the PR
overlay context (except `R` which opens it). The `Enter`, `c`, `o`, `j/k`, `J/K` keys
only have these meanings when the PR overlay is visible — in the normal app view, they
retain their usual behavior.

### 18.7 Test It

Build and run:
```bash
cd ~/leogit
go build ./...
go run ./cmd/leogit /path/to/github-hosted-repo
```

**Prerequisites**: You need a repository hosted on GitHub with `gh` authenticated
(`gh auth status` must return exit code 0). The repo should have some existing PRs for
testing the list/detail/checkout features. If you don't have PRs, you can create test
branches and PRs using the terminal.

**Setup**: Create test branches and a PR for testing:

```bash
cd /path/to/your-github-repo
git checkout -b test-pr-branch
echo "test change" > test-pr-file.txt
git add . && git commit -m "test: add file for PR testing"
git push -u origin test-pr-branch
gh pr create --title "Test PR for TUI" --body "This is a test PR" --base main --draft
```

**Test scenarios:**

1. **Open PR overlay**: Press `R` in navigable mode. The PR overlay should appear with
   "Loading pull requests..." briefly, then show the list of open PRs. The first PR should
   be selected (▸ cursor), and its detail should appear in the right pane.

2. **PR list navigation**: Press `j`/`k` to move through the PR list. The detail pane
   should update to show the selected PR's information. The cursor (▸) should track the
   selected item.

3. **PR detail content**: Select a PR and verify the detail pane shows:
   - Title and PR number (blue)
   - Author and relative date
   - Head → base branch names (green)
   - State label (OPEN/CLOSED/MERGED/DRAFT)
   - Body text (or "No description" if empty)
   - CI checks section (may show "Loading checks..." initially)
   - Review decision (Approved/Changes Requested/Review Required/No reviews)
   - File change stats (+N -N • N files changed)

4. **CI checks loading**: Select a PR that has CI checks (GitHub Actions, etc.). After a
   brief delay, the checks section should update from "Loading checks..." to show the
   check summary (✓ N passed, ✗ N failed) and individual check names with status icons.

5. **State filter cycling**: Press `o` to cycle through filters. The header should show
   the active filter: [Open] → [Closed] → [Merged] → [All] → [Open]. Each filter change
   should reload the PR list. Verify that closed/merged PRs appear with appropriate state
   labels and colors.

6. **Checkout PR branch**: Select a PR and press `Enter`. The overlay should close, the
   header should show `⟳ Checking out PR...` briefly. After completion, the branch name
   in the header should change to the PR's head branch. Press `Tab` to switch to History —
   the commit list should show the PR branch's commits.

7. **Create PR form**: Press `R` to open the PR overlay, then press `c`. The create form
   should appear with empty title/body fields and "main" as the default base branch. Use
   `Tab` to navigate between fields. Type a title, optionally a body, then tab to the
   buttons and press `Enter` on "Create".

8. **Create with --fill**: In the create form, tab to "Fill & Create" and press `Enter`.
   The PR should be created with the title/body auto-filled from commit messages. This
   bypasses the title validation (no title required since gh fills it).

9. **Create PR validation**: In the create form, leave the title empty and try to press
   Enter on "Create". The form should show "Title is required" error and not submit.

10. **Draft toggle**: In the create form, press `Ctrl+D` to toggle the draft checkbox.
    Create a PR with draft enabled and verify it appears as "DRAFT" in the PR list.

11. **Cancel create**: In the create form, press `Esc`. The PR overlay should reappear
    with the same PR list (not reloaded).

12. **PR header indicator**: Switch to a branch that has an open PR. The header should
    show a blue `PR #N` indicator next to the branch name. If the PR has been approved,
    it should show `PR #N ✓`. Switch to a branch without a PR — the indicator should
    disappear.

13. **Checking out PR spinner**: While a PR checkout is running, verify the header shows
    `⟳ Checking out PR...`. It should disappear when the checkout completes.

14. **Creating PR spinner**: While a PR is being created, verify the header shows
    `⟳ Creating PR...`. It should disappear when creation completes.

15. **PR list error handling**: Disconnect from the network and press `R`. The overlay
    should show a loading state, then an error modal should appear with the gh error message.

16. **Empty PR list**: Filter to a state with no PRs (e.g., "merged" on a repo with no
    merged PRs). The overlay should show "No merged pull requests" with a hint to create one.

17. **Detail pane scrolling**: Select a PR with a long description. Press `J` to scroll
    the detail pane down, `K` to scroll back up. The list cursor should not move while
    scrolling the detail.

18. **Window resize**: Resize the terminal while the PR overlay is visible. The two-pane
    layout should adapt to the new dimensions. Try a very narrow terminal — the PR titles
    should truncate with "…" rather than wrapping.

19. **PR overlay close**: Press `Esc` in the PR overlay. The overlay should close and
    return to the normal main view. The PR filter state should be preserved — pressing `R`
    again should show the same filter.

20. **Checkout updates PR indicator**: Check out a PR via the overlay. After the branch
    changes, the header should automatically show the PR indicator for the newly checked
    out branch.

**Summary**: Phase 18 adds pull request workflows via the `gh` CLI. Pressing `R` opens a
fullscreen PR overlay with a split-pane layout: PR list on the left (scrollable, with state
filter tabs cycled by `o`) and detail view on the right (title, author, body, CI checks,
review decision, file stats). CI checks are loaded asynchronously per-PR and cached. The
user can checkout a PR's branch with `Enter` (runs `gh pr checkout`), create a new PR
with `c` (opens a form overlay with title/body/base/draft fields), or use "Fill & Create"
to auto-populate from commit messages (`gh pr create --fill`). The header shows a PR
status indicator for the current branch: a blue `PR #N` label with a review decision icon
(✓/✗/⟳), loaded once on branch change via `gh pr list --head <branch>`. PR reviews
(`gh pr review`, `gh pr comment`) and CI reruns (`gh run rerun`) are terminal-deferred —
the TUI surfaces status information but advanced operations are performed in the embedded
terminal. Issue management (`gh issue list`, `gh issue create`) is also terminal-deferred.
