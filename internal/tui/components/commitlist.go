package components

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// CommitSelectedMsg is sent when the user presses Enter on a commit in the list.
type CommitSelectedMsg struct {
	Index  int
	Commit git.CommitInfo
}

// LoadMoreCommitsMsg is sent when the cursor approaches the bottom of the loaded
// commits, signaling that the app should load the next batch via pagination.
type LoadMoreCommitsMsg struct{}

const loadMoreThreshold = 5

// CommitListModel displays a scrollable list of commits with SHA, summary, and date.
type CommitListModel struct {
	Commits []git.CommitInfo
	cursor  int
	offset  int
	width   int
	height  int
}

// NewCommitList creates an empty commit list.
func NewCommitList() CommitListModel {
	return CommitListModel{}
}

// SetCommits replaces the entire commit list and resets the cursor if out of bounds.
func (m *CommitListModel) SetCommits(commits []git.CommitInfo) {
	m.Commits = commits
	if m.cursor >= len(m.Commits) {
		m.cursor = max(0, len(m.Commits)-1)
	}
	m.clampOffset()
}

// AppendCommits adds commits to the end of the list (for pagination).
func (m *CommitListModel) AppendCommits(commits []git.CommitInfo) {
	m.Commits = append(m.Commits, commits...)
}

// SetSize updates the available rendering dimensions.
func (m *CommitListModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.clampOffset()
}

// SelectedCommit returns the currently highlighted commit, or nil if empty.
func (m CommitListModel) SelectedCommit() *git.CommitInfo {
	if len(m.Commits) == 0 || m.cursor >= len(m.Commits) {
		return nil
	}
	return &m.Commits[m.cursor]
}

// Update handles navigation keys when the commit list pane is focused.
func (m CommitListModel) Update(msg tea.Msg) (CommitListModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < len(m.Commits)-1 {
				m.cursor++
				m.clampOffset()
			}
			if len(m.Commits) > 0 && m.cursor >= len(m.Commits)-loadMoreThreshold {
				return m, func() tea.Msg {
					return LoadMoreCommitsMsg{}
				}
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.clampOffset()
			}
			return m, nil

		case "g":
			m.cursor = 0
			m.clampOffset()
			return m, nil

		case "G":
			m.cursor = max(0, len(m.Commits)-1)
			m.clampOffset()
			if len(m.Commits) > 0 && m.cursor >= len(m.Commits)-loadMoreThreshold {
				return m, func() tea.Msg {
					return LoadMoreCommitsMsg{}
				}
			}
			return m, nil

		case "enter":
			if len(m.Commits) > 0 && m.cursor < len(m.Commits) {
				commit := m.Commits[m.cursor]
				idx := m.cursor
				return m, func() tea.Msg {
					return CommitSelectedMsg{Index: idx, Commit: commit}
				}
			}
			return m, nil
		}
	}
	return m, nil
}

// View renders the commit list.
func (m CommitListModel) View() string {
	if len(m.Commits) == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Render("No commits loaded")
	}

	shaStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#D29922"))

	summaryStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	dateStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58"))

	refStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#3FB950")).
		Bold(true)

	cursorBg := lipgloss.NewStyle().
		Background(lipgloss.Color("#264F78")).
		Foreground(lipgloss.Color("#FFFFFF"))

	end := m.offset + m.height
	if end > len(m.Commits) {
		end = len(m.Commits)
	}

	var lines []string
	for i := m.offset; i < end; i++ {
		c := m.Commits[i]

		sha := shaStyle.Render(c.ShortSHA)
		date := dateStyle.Render(git.RelativeDate(c.AuthorDate))
		dateLen := len(git.RelativeDate(c.AuthorDate))

		// Truncate summary to fit
		maxSummary := m.width - 10 - dateLen
		if maxSummary < 10 {
			maxSummary = 10
		}
		summary := c.Summary
		if len(summary) > maxSummary {
			summary = summary[:maxSummary-1] + "…"
		}

		ref := ""
		if c.Refs != "" {
			ref = " " + refStyle.Render("("+c.Refs+")")
		}

		line := sha + " " + summaryStyle.Render(summary) + ref
		// Right-align date
		lineWidth := lipgloss.Width(line)
		pad := m.width - lineWidth - dateLen - 1
		if pad < 1 {
			pad = 1
		}
		line += fmt.Sprintf("%*s%s", pad, "", date)

		if i == m.cursor {
			line = cursorBg.Render(line)
		}

		lines = append(lines, line)
	}

	return strings.Join(lines, "\n")
}

func (m *CommitListModel) clampOffset() {
	if m.height <= 0 {
		return
	}
	if m.cursor >= m.offset+m.height {
		m.offset = m.cursor - m.height + 1
	}
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.offset < 0 {
		m.offset = 0
	}
}
