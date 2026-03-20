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
	file       git.FileEntry  // the file currently being displayed
	fileDiff   *diff.FileDiff // parsed diff for the current file
	hasContent bool           // true once a diff has been loaded

	offset     int    // scroll position: index of the first visible line
	totalLines int    // total number of lines across all hunks
	width      int    // available pane width (inner, excluding borders)
	height     int    // available pane height in rows (inner, excluding borders and title)
	sideBySide bool   // true = side-by-side mode (stub), false = unified
	loading    bool   // true while waiting for the diff to load
	errMsg     string // non-empty if the diff command failed
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
	if fileDiff != nil {
		m.totalLines = fileDiff.TotalLines()
	} else {
		m.totalLines = 0
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

// Update handles key events for the diff viewer.
// Returns the updated model and an optional command.
func (m DiffViewModel) Update(msg tea.Msg) (DiffViewModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			m.offset++
			m.clampOffset()

		case "k", "up":
			m.offset--
			m.clampOffset()

		case "g":
			// Jump to top
			m.offset = 0

		case "G":
			// Jump to bottom
			m.offset = m.totalLines - m.height
			m.clampOffset()

		case "s":
			// Toggle unified / side-by-side
			m.sideBySide = !m.sideBySide

		case "d":
			// Page down (half screen)
			m.offset += m.height / 2
			m.clampOffset()

		case "u":
			// Page up (half screen)
			m.offset -= m.height / 2
			m.clampOffset()
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

	// Render only the visible window
	return render.RenderDiff(m.fileDiff, m.offset, m.height, m.width)
}
