package views

import (
	"path/filepath"
	"strconv"
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
	allRepos []string // all discovered repo paths
	filtered []string // repos matching the current filter
	filter   string   // current search text
	cursor   int      // index in filtered list
	width    int
	height   int
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

		case "esc":
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
				"  (" + strconv.Itoa(m.cursor+1) + "/" + strconv.Itoa(len(m.filtered)) + ")",
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
