package views

import (
	"fmt"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// RenderCommitDetail renders the commit metadata panel.
func RenderCommitDetail(commit *git.CommitInfo, width int) string {
	if commit == nil {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Render("Select a commit to view details")
	}

	labelStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Width(10)

	valueStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	shaStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#D29922"))

	refStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	var rows []string

	// Summary
	summaryStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF"))
	rows = append(rows, summaryStyle.Render(commit.Summary))

	// Refs
	if commit.Refs != "" {
		rows = append(rows, refStyle.Render("("+commit.Refs+")"))
	}

	rows = append(rows, "")

	// SHA
	rows = append(rows, labelStyle.Render("Commit")+" "+shaStyle.Render(commit.SHA))

	// Author
	rows = append(rows, labelStyle.Render("Author")+" "+
		valueStyle.Render(fmt.Sprintf("%s <%s>", commit.AuthorName, commit.AuthorEmail)))

	// Date
	rows = append(rows, labelStyle.Render("Date")+" "+
		valueStyle.Render(commit.AuthorDate.Format("2006-01-02 15:04:05")+" ("+git.RelativeDate(commit.AuthorDate)+")"))

	// Parents
	if len(commit.Parents) > 0 {
		rows = append(rows, labelStyle.Render("Parents")+" "+
			shaStyle.Render(strings.Join(commit.Parents, " ")))
	}

	// Body
	if commit.Body != "" {
		rows = append(rows, "")
		bodyStyle := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E"))
		rows = append(rows, bodyStyle.Render(commit.Body))
	}

	// Trailers
	if commit.Trailers != "" {
		rows = append(rows, "")
		trailerStyle := lipgloss.NewStyle().
			Foreground(lipgloss.Color("#A371F7"))
		rows = append(rows, trailerStyle.Render(commit.Trailers))
	}

	return strings.Join(rows, "\n")
}
