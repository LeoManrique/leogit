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
	v.AltScreen = true                    // takes over the full terminal; restores previous content on exit
	v.MouseMode = tea.MouseModeCellMotion // enables mouse click/scroll events
	return v
}
