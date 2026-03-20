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
