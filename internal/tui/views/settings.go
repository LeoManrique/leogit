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
	Get func(*config.Config) string
	Set func(*config.Config, string)
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
	case "esc", "S":
		// Close settings view
		return m, func() tea.Msg { return SettingsClosedMsg{} }

	case "j", "down":
		m.moveDown()
		return m, nil

	case "k", "up":
		m.moveUp()
		return m, nil

	case "space":
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
	case "esc":
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
