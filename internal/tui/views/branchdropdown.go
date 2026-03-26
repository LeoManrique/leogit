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
type BranchDeleteMsg struct {
	Name     string
	IsRemote bool
}

// BranchRenameMsg is sent when the user confirms a rename.
type BranchRenameMsg struct {
	OldName string
	NewName string
}

// BranchMergeMsg is sent when the user presses 'm' on a branch.
type BranchMergeMsg struct {
	Name string
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
	allBranches []git.BranchInfo
	filtered    []git.BranchInfo
	filter      string
	cursor      int
	width       int
	height      int
	Visible     bool

	mode       branchMode
	inputText  string
	renameFrom string
}

// NewBranchDropdown creates a new hidden branch dropdown.
func NewBranchDropdown() BranchDropdownModel {
	return BranchDropdownModel{}
}

// SetBranches replaces the branch list and resets the filter.
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

func (m BranchDropdownModel) updateBrowse(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "esc":
		if m.filter != "" {
			m.filter = ""
			m.applyFilter()
			return m, nil
		}
		m.Visible = false
		return m, func() tea.Msg { return BranchDropdownClosedMsg{} }

	case "enter":
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
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
		m.mode = branchModeCreate
		m.inputText = ""
		return m, nil

	case "d":
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
			if branch.IsCurrent {
				return m, nil
			}
			m.Visible = false
			return m, func() tea.Msg {
				return BranchDeleteMsg{Name: branch.Name, IsRemote: branch.IsRemote}
			}
		}
		return m, nil

	case "r":
		if len(m.filtered) > 0 && m.cursor < len(m.filtered) {
			branch := m.filtered[m.cursor]
			if branch.IsRemote {
				return m, nil
			}
			m.mode = branchModeRename
			m.renameFrom = branch.Name
			m.inputText = branch.Name
		}
		return m, nil

	case "m":
		if len(m.filtered) > 0 {
			branch := m.filtered[m.cursor]
			if branch.IsCurrent {
				return m, nil
			}
			m.Visible = false
			return m, func() tea.Msg {
				return BranchMergeMsg{Name: branch.Name}
			}
		}
		return m, nil

	case "backspace":
		if len(m.filter) > 0 {
			m.filter = m.filter[:len(m.filter)-1]
			m.applyFilter()
		}
		return m, nil

	default:
		key := msg.String()
		if len(key) == 1 && key != " " {
			m.filter += key
			m.applyFilter()
		}
		return m, nil
	}
}

func (m BranchDropdownModel) updateCreate(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "esc":
		m.mode = branchModeBrowse
		m.inputText = ""
		return m, nil

	case "enter":
		name := strings.TrimSpace(m.inputText)
		if name != "" {
			m.Visible = false
			return m, func() tea.Msg {
				return BranchCreateMsg{Name: name}
			}
		}
		return m, nil

	case "backspace":
		if len(m.inputText) > 0 {
			m.inputText = m.inputText[:len(m.inputText)-1]
		}
		return m, nil

	default:
		key := msg.String()
		if len(key) == 1 {
			m.inputText += key
		}
		return m, nil
	}
}

func (m BranchDropdownModel) updateRename(msg tea.KeyPressMsg) (BranchDropdownModel, tea.Cmd) {
	switch msg.String() {
	case "esc":
		m.mode = branchModeBrowse
		m.inputText = ""
		return m, nil

	case "enter":
		newName := strings.TrimSpace(m.inputText)
		if newName != "" && newName != m.renameFrom {
			oldName := m.renameFrom
			m.Visible = false
			return m, func() tea.Msg {
				return BranchRenameMsg{OldName: oldName, NewName: newName}
			}
		}
		return m, nil

	case "backspace":
		if len(m.inputText) > 0 {
			m.inputText = m.inputText[:len(m.inputText)-1]
		}
		return m, nil

	default:
		key := msg.String()
		if len(key) == 1 {
			m.inputText += key
		}
		return m, nil
	}
}

// ── Filter ──────────────────────────────────────────────

func (m *BranchDropdownModel) applyFilter() {
	if m.filter == "" {
		m.filtered = m.allBranches
	} else {
		lower := strings.ToLower(m.filter)
		m.filtered = nil
		for _, b := range m.allBranches {
			if strings.Contains(strings.ToLower(b.Name), lower) {
				m.filtered = append(m.filtered, b)
			}
		}
	}
	if m.cursor >= len(m.filtered) {
		m.cursor = max(0, len(m.filtered)-1)
	}
}

// ── View ────────────────────────────────────────────────

func (m BranchDropdownModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	currentStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	remoteStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E"))

	cursorStyle := lipgloss.NewStyle().
		Background(lipgloss.Color("#264F78")).
		Foreground(lipgloss.Color("#FFFFFF"))

	normalStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	inputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FF7B72")).
		Bold(true)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	// Title + mode
	var title string
	switch m.mode {
	case branchModeCreate:
		title = titleStyle.Render("Create Branch")
	case branchModeRename:
		title = titleStyle.Render("Rename Branch")
	default:
		title = titleStyle.Render("Branches")
	}

	var rows []string
	rows = append(rows, title)

	// Filter or input display
	if m.mode == branchModeCreate || m.mode == branchModeRename {
		rows = append(rows, "")
		rows = append(rows, "Name: "+inputStyle.Render(m.inputText+"█"))
		rows = append(rows, "")
		rows = append(rows, hintStyle.Render("Enter: confirm • Esc: cancel"))
	} else {
		if m.filter != "" {
			rows = append(rows, "Filter: "+inputStyle.Render(m.filter))
		}
		rows = append(rows, "")

		// Branch list
		visible := m.height - 10
		if visible < 5 {
			visible = 5
		}
		offset := 0
		if m.cursor >= visible {
			offset = m.cursor - visible + 1
		}
		end := offset + visible
		if end > len(m.filtered) {
			end = len(m.filtered)
		}

		for i := offset; i < end; i++ {
			b := m.filtered[i]
			var line string

			prefix := "  "
			if b.IsCurrent {
				prefix = "✓ "
			}

			if b.IsCurrent {
				line = prefix + currentStyle.Render(b.Name)
			} else if b.IsRemote {
				line = prefix + remoteStyle.Render(b.Name)
			} else {
				line = prefix + normalStyle.Render(b.Name)
			}

			if i == m.cursor {
				line = cursorStyle.Render(line)
			}

			rows = append(rows, line)
		}

		rows = append(rows, "")
		rows = append(rows, hintStyle.Render("j/k: nav • Enter: switch • c: create • d: delete • r: rename • m: merge • Esc: close"))
	}

	content := strings.Join(rows, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Width(min(60, m.width-4)).
		Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
