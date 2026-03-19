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
		row("S", "Open settings"),
		row("B", "Open branch picker"),
		row("R", "Open pull requests"),
		row("F", "Fetch from remote"),
		row("P", "Pull from remote"),
		row("p", "Push to remote"),
		row("A", "Abort merge (when merging)"),
		row("Esc", "Unfocus pane / dismiss modal"),
		row("Ctrl+C", "Quit (always works)"),
		"",
		sectionStyle.Render("Pane Navigation (navigable)"),
		row("j / k", "Navigate up / down"),
		row("Enter", "Focus pane / select item"),
		"",
		sectionStyle.Render("Branch Picker (B)"),
		row("j / k", "Navigate branches"),
		row("Enter", "Switch to branch"),
		row("c", "Create new branch"),
		row("d", "Delete branch"),
		row("r", "Rename branch"),
		row("m", "Merge branch into current"),
		row("Esc", "Close picker"),
		"",
		sectionStyle.Render("History Tab"),
		row("1", "Commit list"),
		row("2", "Changed files"),
		row("3", "Diff viewer"),
		row("Enter", "Select commit / file"),
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
