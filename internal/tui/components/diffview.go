package components

import (
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/diff"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/render"
)

// DiffLoadedMsg is sent when a diff has been fetched and parsed for a file.
// The app receives this from the async command and forwards it to the diff viewer.
type DiffLoadedMsg struct {
	File     git.FileEntry  // the file this diff belongs to
	FileDiff *diff.FileDiff // parsed diff (nil if the file has no diff, e.g., binary)
	Err      error          // non-nil if the git diff command failed
}

// DiffViewModel displays a scrollable, syntax-highlighted diff in a pane.
type DiffViewModel struct {
	file       git.FileEntry
	fileDiff   *diff.FileDiff
	hasContent bool

	offset     int
	totalLines int
	width      int
	height     int
	sideBySide bool
	loading    bool
	errMsg     string

	// Line selection
	cursor    int                // highlighted line index (in flat AllLines array)
	selection diff.DiffSelection // tracks which lines are selected for staging
	allLines  []diff.Line        // cached flat line array for quick access
}

// NewDiffView creates an empty diff viewer. Content is set via SetDiff().
func NewDiffView() DiffViewModel {
	return DiffViewModel{}
}

// SetDiff updates the diff viewer with a new parsed diff.
// Resets the scroll position to the top.
func (m *DiffViewModel) SetDiff(file git.FileEntry, fileDiff *diff.FileDiff) {
	m.file = file
	m.fileDiff = fileDiff
	m.hasContent = true
	m.loading = false
	m.errMsg = ""
	m.offset = 0
	m.cursor = 0
	if fileDiff != nil {
		m.totalLines = fileDiff.TotalLines()
		m.allLines = fileDiff.AllLines()
		// Default: all lines selected (everything is included in the commit by default)
		m.selection = diff.NewDiffSelection(fileDiff, diff.SelectAll)
	} else {
		m.totalLines = 0
		m.allLines = nil
		m.selection = diff.DiffSelection{}
	}
}

// SetError sets an error state for the diff viewer.
func (m *DiffViewModel) SetError(errMsg string) {
	m.errMsg = errMsg
	m.loading = false
	m.hasContent = true
	m.fileDiff = nil
	m.totalLines = 0
	m.offset = 0
}

// SetLoading puts the diff viewer into a loading state.
func (m *DiffViewModel) SetLoading() {
	m.loading = true
	m.errMsg = ""
}

// SetSize updates the available rendering dimensions.
func (m *DiffViewModel) SetSize(width, height int) {
	m.width = width
	m.height = height
}

// IsSideBySide returns whether side-by-side mode is active.
func (m DiffViewModel) IsSideBySide() bool {
	return m.sideBySide
}

// clampOffset ensures the scroll offset stays within valid bounds.
// maxOffset = totalLines - height means the last line sits at the bottom of
// the visible area. If the diff is shorter than the pane, maxOffset is 0.
func (m *DiffViewModel) clampOffset() {
	maxOffset := m.totalLines - m.height
	if maxOffset < 0 {
		maxOffset = 0
	}
	if m.offset > maxOffset {
		m.offset = maxOffset
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

func (m DiffViewModel) Update(msg tea.Msg) (DiffViewModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < m.totalLines-1 {
				m.cursor++
			}
			// Auto-scroll to keep cursor visible
			if m.cursor >= m.offset+m.height {
				m.offset = m.cursor - m.height + 1
			}
			m.clampOffset()

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
			}
			if m.cursor < m.offset {
				m.offset = m.cursor
			}
			m.clampOffset()

		case "g":
			m.cursor = 0
			m.offset = 0

		case "G":
			if m.totalLines > 0 {
				m.cursor = m.totalLines - 1
			}
			m.offset = m.totalLines - m.height
			m.clampOffset()

		case "s":
			m.sideBySide = !m.sideBySide

		case "d":
			m.offset += m.height / 2
			m.cursor += m.height / 2
			if m.cursor >= m.totalLines {
				m.cursor = m.totalLines - 1
			}
			m.clampOffset()

		case "u":
			m.offset -= m.height / 2
			m.cursor -= m.height / 2
			if m.cursor < 0 {
				m.cursor = 0
			}
			m.clampOffset()

		case "space":
			// Toggle selection on the current line
			if m.totalLines > 0 && m.selection.IsSelectable(m.cursor) {
				m.selection = m.selection.WithToggle(m.cursor)
			}

		case "h":
			// Toggle selection for the entire hunk containing the cursor
			if m.totalLines > 0 && m.fileDiff != nil {
				start, count := hunkRange(m.fileDiff, m.cursor)
				if count > 0 {
					// Check if majority of hunk is selected — if so, deselect all; otherwise select all
					selectedInHunk := 0
					selectableInHunk := 0
					for i := start; i < start+count; i++ {
						if m.selection.IsSelectable(i) {
							selectableInHunk++
							if m.selection.IsSelected(i) {
								selectedInHunk++
							}
						}
					}
					// If half or fewer lines are selected, select all; otherwise deselect all.
					// This gives a natural "toggle" feel for the hunk.
					selectAll := selectedInHunk <= selectableInHunk/2
					m.selection = m.selection.WithRangeSelection(start, count, selectAll)
				}
			}

		}
	}

	return m, nil
}

// View renders the diff viewer content.
func (m DiffViewModel) View() string {
	if m.loading {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Loading diff...")
	}

	if m.errMsg != "" {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#F85149")).
			Render("Error: " + m.errMsg)
	}

	if !m.hasContent {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("Select a file to view its diff")
	}

	if m.fileDiff == nil || m.totalLines == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8B949E")).
			Italic(true).
			Render("No changes in this file")
	}

	// Render the visible window with cursor and selection indicators.
	// Selection is in-memory — ● means the line will be included in the commit.
	return render.RenderDiffWithSelection(
		m.fileDiff, m.allLines, m.selection,
		m.offset, m.height, m.width, m.cursor,
	)
}

// HunkRange returns the flat line index range for the hunk containing the given line index.
// Returns (startIdx, count) for use with WithRangeSelection.
func hunkRange(fileDiff *diff.FileDiff, flatIdx int) (int, int) {
	if fileDiff == nil {
		return 0, 0
	}

	offset := 0
	for _, hunk := range fileDiff.Hunks {
		hunkLen := len(hunk.Lines)
		if flatIdx >= offset && flatIdx < offset+hunkLen {
			return offset, hunkLen
		}
		offset += hunkLen
	}
	return 0, 0
}
