package views

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/gh"
)

// ── PR overlay messages ─────────────────────────────────

// PRCheckoutMsg is sent when the user presses Enter to checkout a PR's branch.
type PRCheckoutMsg struct {
	Number int
}

// PRCreateRequestMsg is sent when the user presses c to open the create PR form.
type PRCreateRequestMsg struct {
	BaseBranch string
}

// PROverlayCloseMsg is sent when the user presses Esc to close the overlay.
type PROverlayCloseMsg struct{}

// PRNeedChecksMsg is sent when the overlay needs CI checks loaded for a PR.
type PRNeedChecksMsg struct {
	Number int
}

// PRFilterChangeMsg is sent when the user cycles the state filter.
type PRFilterChangeMsg struct {
	State string // "open", "closed", "merged", "all"
}

// ── PR overlay model ────────────────────────────────────

// PROverlayModel is a fullscreen overlay showing the PR list and detail pane.
type PROverlayModel struct {
	Visible      bool
	prs          []gh.PullRequest
	cursor       int
	filterState  string
	loading      bool
	checks       map[int][]gh.PRCheck
	width        int
	height       int
	detailScroll int
	baseBranch   string
}

// NewPROverlay creates a hidden PR overlay with default state.
func NewPROverlay() PROverlayModel {
	return PROverlayModel{
		filterState: "open",
		checks:      make(map[int][]gh.PRCheck),
	}
}

// Show opens the PR overlay in loading state.
func (m *PROverlayModel) Show(baseBranch string, width, height int) {
	m.Visible = true
	m.loading = true
	m.prs = nil
	m.cursor = 0
	m.detailScroll = 0
	m.baseBranch = baseBranch
	m.width = width
	m.height = height
}

// Reshow makes the overlay visible again without resetting state.
func (m *PROverlayModel) Reshow() {
	m.Visible = true
}

// SetPRs updates the PR list after loading completes.
func (m *PROverlayModel) SetPRs(prs []gh.PullRequest) {
	m.prs = prs
	m.loading = false
	m.cursor = 0
	m.detailScroll = 0
}

// SetChecks caches CI check results for a PR.
func (m *PROverlayModel) SetChecks(number int, checks []gh.PRCheck) {
	m.checks[number] = checks
}

// SelectedPR returns the currently highlighted PR, or nil if empty.
func (m PROverlayModel) SelectedPR() *gh.PullRequest {
	if len(m.prs) == 0 || m.cursor >= len(m.prs) {
		return nil
	}
	return &m.prs[m.cursor]
}

// Update handles key events for the PR overlay.
func (m PROverlayModel) Update(msg tea.Msg) (PROverlayModel, tea.Cmd) {
	if !m.Visible {
		return m, nil
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch msg.String() {
		case "esc":
			m.Visible = false
			return m, func() tea.Msg { return PROverlayCloseMsg{} }

		case "j", "down":
			if m.cursor < len(m.prs)-1 {
				m.cursor++
				m.detailScroll = 0
				// Request checks for new selection
				if pr := m.SelectedPR(); pr != nil {
					if _, ok := m.checks[pr.Number]; !ok {
						num := pr.Number
						return m, func() tea.Msg { return PRNeedChecksMsg{Number: num} }
					}
				}
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.detailScroll = 0
				if pr := m.SelectedPR(); pr != nil {
					if _, ok := m.checks[pr.Number]; !ok {
						num := pr.Number
						return m, func() tea.Msg { return PRNeedChecksMsg{Number: num} }
					}
				}
			}
			return m, nil

		case "enter":
			if pr := m.SelectedPR(); pr != nil {
				num := pr.Number
				m.Visible = false
				return m, func() tea.Msg { return PRCheckoutMsg{Number: num} }
			}
			return m, nil

		case "c":
			base := m.baseBranch
			m.Visible = false
			return m, func() tea.Msg { return PRCreateRequestMsg{BaseBranch: base} }

		case "o":
			// Cycle filter state
			states := []string{"open", "closed", "merged", "all"}
			for i, s := range states {
				if s == m.filterState {
					m.filterState = states[(i+1)%len(states)]
					break
				}
			}
			m.loading = true
			state := m.filterState
			return m, func() tea.Msg { return PRFilterChangeMsg{State: state} }

		case "J":
			m.detailScroll++
			return m, nil

		case "K":
			if m.detailScroll > 0 {
				m.detailScroll--
			}
			return m, nil
		}
	}

	return m, nil
}

// View renders the PR overlay.
func (m PROverlayModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	// Title with filter tabs
	filterLabels := map[string]string{"open": "Open", "closed": "Closed", "merged": "Merged", "all": "All"}
	var filterParts []string
	for _, s := range []string{"open", "closed", "merged", "all"} {
		label := filterLabels[s]
		if s == m.filterState {
			filterParts = append(filterParts, lipgloss.NewStyle().
				Bold(true).Foreground(lipgloss.Color("#FFFFFF")).
				Render("["+label+"]"))
		} else {
			filterParts = append(filterParts, lipgloss.NewStyle().
				Foreground(lipgloss.Color("#484F58")).
				Render(label))
		}
	}
	title := titleStyle.Render("Pull Requests") + "  " + strings.Join(filterParts, "  ")

	if m.loading {
		content := title + "\n\nLoading..."
		return m.renderBox(content, hintStyle)
	}

	if len(m.prs) == 0 {
		content := title + "\n\nNo pull requests found."
		return m.renderBox(content, hintStyle)
	}

	// Split: 40% list, 60% detail
	listWidth := m.width * 2 / 5
	if listWidth < 30 {
		listWidth = 30
	}
	detailWidth := m.width - listWidth - 10 // account for borders/padding

	// PR list
	cursorBg := lipgloss.NewStyle().
		Background(lipgloss.Color("#264F78")).
		Foreground(lipgloss.Color("#FFFFFF"))

	visible := m.height - 8
	if visible < 5 {
		visible = 5
	}

	var listLines []string
	offset := 0
	if m.cursor >= visible {
		offset = m.cursor - visible + 1
	}
	end := offset + visible
	if end > len(m.prs) {
		end = len(m.prs)
	}

	for i := offset; i < end; i++ {
		pr := m.prs[i]
		stateIcon := prStateIcon(pr.State, pr.IsDraft)
		line := fmt.Sprintf("  %s #%-4d %s", stateIcon, pr.Number, truncate(pr.Title, listWidth-20))
		if i == m.cursor {
			line = cursorBg.Render(line)
		}
		listLines = append(listLines, line)
	}
	listContent := strings.Join(listLines, "\n")

	// Detail pane
	detailContent := m.renderDetail(detailWidth)

	// Compose layout
	listPane := lipgloss.NewStyle().Width(listWidth).Render(listContent)
	detailPane := lipgloss.NewStyle().Width(detailWidth).Render(detailContent)

	body := lipgloss.JoinHorizontal(lipgloss.Top, listPane, " │ ", detailPane)
	content := title + "\n\n" + body

	return m.renderBox(content, hintStyle)
}

func (m PROverlayModel) renderDetail(width int) string {
	pr := m.SelectedPR()
	if pr == nil {
		return ""
	}

	titleStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#FFFFFF"))
	metaStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	branchStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))
	bodyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9"))

	var lines []string
	lines = append(lines, titleStyle.Render(fmt.Sprintf("%s  #%d", pr.Title, pr.Number)))
	lines = append(lines, metaStyle.Render(fmt.Sprintf("@%s • %s", pr.Author, formatPRDate(pr.CreatedAt))))
	lines = append(lines, branchStyle.Render(pr.HeadRefName)+" → "+branchStyle.Render(pr.BaseRefName))
	lines = append(lines, "")

	// Body
	if pr.Body != "" {
		body := pr.Body
		if len(body) > 500 {
			body = body[:500] + "..."
		}
		lines = append(lines, bodyStyle.Render(body))
		lines = append(lines, "")
	}

	// Checks
	if checks, ok := m.checks[pr.Number]; ok && len(checks) > 0 {
		lines = append(lines, metaStyle.Render("── Checks ──"))
		pass, fail, pending := 0, 0, 0
		for _, c := range checks {
			switch c.Bucket {
			case "pass":
				pass++
			case "fail":
				fail++
			default:
				pending++
			}
		}
		checkSummary := ""
		if pass > 0 {
			checkSummary += lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950")).Render(fmt.Sprintf("✓ %d passed", pass))
		}
		if fail > 0 {
			if checkSummary != "" {
				checkSummary += "  "
			}
			checkSummary += lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149")).Render(fmt.Sprintf("✗ %d failed", fail))
		}
		if pending > 0 {
			if checkSummary != "" {
				checkSummary += "  "
			}
			checkSummary += lipgloss.NewStyle().Foreground(lipgloss.Color("#D29922")).Render(fmt.Sprintf("○ %d pending", pending))
		}
		lines = append(lines, checkSummary)
		lines = append(lines, "")
	}

	// Review
	if pr.ReviewDecision != "" {
		lines = append(lines, metaStyle.Render("── Reviews ──"))
		reviewIcon := prReviewIcon(pr.ReviewDecision)
		reviewLabel := strings.ReplaceAll(pr.ReviewDecision, "_", " ")
		lines = append(lines, fmt.Sprintf("%s %s", reviewIcon, reviewLabel))
		lines = append(lines, "")
	}

	// Stats
	statsStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	lines = append(lines, statsStyle.Render(fmt.Sprintf(
		"+%d -%d • %d files changed",
		pr.Additions, pr.Deletions, pr.ChangedFiles,
	)))

	// Apply scroll
	if m.detailScroll > 0 && m.detailScroll < len(lines) {
		lines = lines[m.detailScroll:]
	}

	return strings.Join(lines, "\n")
}

func (m PROverlayModel) renderBox(content string, hintStyle lipgloss.Style) string {
	hints := hintStyle.Render("j/k: navigate • o: filter • Enter: checkout • c: create • Esc: close")

	boxContent := content + "\n\n" + hints

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Width(min(m.width-4, 100)).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

func prStateIcon(state string, isDraft bool) string {
	if isDraft {
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E")).Render("◌")
	}
	switch state {
	case "OPEN":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950")).Render("●")
	case "MERGED":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#A371F7")).Render("●")
	case "CLOSED":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149")).Render("●")
	default:
		return "○"
	}
}

func truncate(s string, maxLen int) string {
	if maxLen <= 0 {
		return ""
	}
	if len(s) > maxLen {
		return s[:maxLen-1] + "…"
	}
	return s
}

func formatPRDate(t interface{ Format(string) string }) string {
	return t.Format("Jan 2, 2006")
}
