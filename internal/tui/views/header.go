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
	Pushing     bool // true while a push is in progress
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
