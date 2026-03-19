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

// viewMain renders the normal app content (expanded later).
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
