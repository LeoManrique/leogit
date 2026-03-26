package views

import (
	"fmt"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/core"
)

// HeaderData holds the information needed to render the header bar.
type HeaderData struct {
	RepoName      string
	BranchName    string
	Ahead         int
	Behind        int
	HasUpstream   bool
	Pushing       bool      // true while a push is in progress
	Fetching      bool      // true when a fetch is in progress
	Pulling       bool      // true when a pull is in progress
	LastFetchTime time.Time // when the last fetch completed (zero = never)
	IsMerging     bool      // true when repo is in a merge state
	PRNumber      int       // open PR number for current branch (0 = none)
	PRReview      string    // review decision: APPROVED, CHANGES_REQUESTED, etc.
}

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

	// ── Merge indicator ──
	if data.IsMerging {
		branchText += " " + lipgloss.NewStyle().
			Foreground(lipgloss.Color("#F85149")).
			Background(bg).
			Bold(true).
			Render("MERGING")
	}

	// ── Action button (context-aware) with status ──
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

	// ── PR indicator ──
	prText := ""
	if data.PRNumber > 0 {
		prIcon := prReviewIcon(data.PRReview)
		prText = sep + lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A371F7")).
			Background(bg).
			Padding(0, 1).
			Render(fmt.Sprintf("%s #%d", prIcon, data.PRNumber))
	}

	left := repoStyle.Render("⎇ "+data.RepoName) +
		sep +
		branchStyle.Render(branchText) +
		sep +
		actionText +
		prText

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

// formatDuration returns a human-readable short duration string.
func formatDuration(d time.Duration) string {
	if d < time.Minute {
		return fmt.Sprintf("%ds", int(d.Seconds()))
	}
	if d < time.Hour {
		return fmt.Sprintf("%dm", int(d.Minutes()))
	}
	return fmt.Sprintf("%dh", int(d.Hours()))
}

// prReviewIcon returns an icon for the PR review decision.
func prReviewIcon(decision string) string {
	switch decision {
	case "APPROVED":
		return "✓"
	case "CHANGES_REQUESTED":
		return "✗"
	case "REVIEW_REQUIRED":
		return "○"
	default:
		return "◌"
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
