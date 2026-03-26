package views

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// MergeConfirmMsg is sent when the user confirms a merge from the overlay.
type MergeConfirmMsg struct {
	Branch string
	Squash bool
}

// MergeCancelMsg is sent when the user cancels the merge overlay.
type MergeCancelMsg struct{}

// MergeOverlayModel is a confirmation dialog for merge operations.
type MergeOverlayModel struct {
	Visible     bool
	branch      string
	intoBranch  string
	commitCount int
	selectedBtn int
	width       int
	height      int
}

// NewMergeOverlay creates a hidden merge overlay.
func NewMergeOverlay() MergeOverlayModel {
	return MergeOverlayModel{}
}

// Show opens the merge confirmation overlay.
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
			case 0:
				branch := m.branch
				return m, func() tea.Msg {
					return MergeConfirmMsg{Branch: branch, Squash: false}
				}
			case 1:
				branch := m.branch
				return m, func() tea.Msg {
					return MergeConfirmMsg{Branch: branch, Squash: true}
				}
			default:
				return m, func() tea.Msg {
					return MergeCancelMsg{}
				}
			}

		case "esc":
			m.Visible = false
			return m, func() tea.Msg {
				return MergeCancelMsg{}
			}
		}
	}

	return m, nil
}

// View renders the merge confirmation overlay.
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
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	btnActive := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#238636")).
		Padding(0, 2)

	btnInactive := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(lipgloss.Color("#21262D")).
		Padding(0, 2)

	btnCancel := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#F85149")).
		Background(lipgloss.Color("#21262D")).
		Padding(0, 2)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	// Title
	title := titleStyle.Render("Merge Branch")

	// Description
	commitWord := "commits"
	if m.commitCount == 1 {
		commitWord = "commit"
	}
	desc := textStyle.Render(fmt.Sprintf(
		"Merge %d %s from %s into %s?",
		m.commitCount, commitWord,
		branchStyle.Render(m.branch),
		branchStyle.Render(m.intoBranch),
	))

	// Buttons
	labels := []string{"Merge", "Squash & Merge", "Cancel"}
	var btns []string
	for i, label := range labels {
		if i == m.selectedBtn {
			btns = append(btns, btnActive.Render(label))
		} else if i == 2 {
			btns = append(btns, btnCancel.Render(label))
		} else {
			btns = append(btns, btnInactive.Render(label))
		}
	}
	btnRow := strings.Join(btns, "  ")

	hints := hintStyle.Render("←/→: select • Enter: confirm • Esc: cancel")

	content := title + "\n\n" + desc + "\n\n" + btnRow + "\n\n" + hints

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
