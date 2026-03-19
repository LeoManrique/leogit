package components

import (
	"fmt"
	"image/color"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/git"
)

// FileSelectedMsg is sent when the user presses Enter on a file in the list.
// This is used to show the diff for the selected file.
type FileSelectedMsg struct {
	Index int
	File  git.FileEntry
}

// FileListModel displays a scrollable list of changed files with status icons.
type FileListModel struct {
	Files    []git.FileEntry // current list of changed files
	cursor   int             // index of the highlighted file
	offset   int             // scroll offset (first visible index)
	width    int             // available width for rendering (inner, excluding borders)
	height   int             // available height in rows (inner, excluding borders and title)
	selected map[string]bool // in-memory selection state: path → selected (true = will be committed)
}

// NewFileList creates an empty file list. Files are set via SetFiles().
func NewFileList() FileListModel {
	return FileListModel{
		selected: make(map[string]bool),
	}
}

// SetFiles replaces the file list contents and resets the cursor if out of bounds.
// Called every time a statusResultMsg arrives with fresh git status data.
func (m *FileListModel) SetFiles(files []git.FileEntry) {
	m.Files = files
	if m.cursor >= len(m.Files) {
		m.cursor = max(0, len(m.Files)-1)
	}
	// New files default to selected. Files already in the map keep their state.
	for _, f := range files {
		if _, exists := m.selected[f.Path]; !exists {
			m.selected[f.Path] = true // default: selected for commit
		}
	}
	// Clean up paths that no longer exist in the file list
	validPaths := make(map[string]bool, len(files))
	for _, f := range files {
		validPaths[f.Path] = true
	}
	for path := range m.selected {
		if !validPaths[path] {
			delete(m.selected, path)
		}
	}
	m.clampOffset()
}

// SetSize updates the available rendering dimensions.
// Called when the terminal resizes or the layout changes.
func (m *FileListModel) SetSize(width, height int) {
	m.width = width
	m.height = height
	m.clampOffset()
}

// SelectedFile returns the currently highlighted file, or nil if the list is empty.
func (m FileListModel) SelectedFile() *git.FileEntry {
	if len(m.Files) == 0 || m.cursor >= len(m.Files) {
		return nil
	}
	return &m.Files[m.cursor]
}

// Update handles navigation keys when the file list pane is focused.
// Only called when the file list pane (Pane 1 on Changes tab) is the active pane.
func (m FileListModel) Update(msg tea.Msg) (FileListModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		switch msg.String() {
		case "j", "down":
			if m.cursor < len(m.Files)-1 {
				m.cursor++
				m.clampOffset()
			}
			return m, nil

		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
				m.clampOffset()
			}
			return m, nil

		case "g":
			// Jump to top of the list
			m.cursor = 0
			m.clampOffset()
			return m, nil

		case "G":
			// Jump to bottom of the list
			if len(m.Files) > 0 {
				m.cursor = len(m.Files) - 1
				m.clampOffset()
			}
			return m, nil

		case "enter":
			// Select the current file → triggers diff view
			if len(m.Files) > 0 && m.cursor < len(m.Files) {
				file := m.Files[m.cursor]
				// This is how Bubbletea commands work: we return a function
				// (tea.Cmd) that produces a tea.Msg. Bubbletea calls this
				// function asynchronously, and when it returns, the resulting
				// message is fed back into Update(). This pattern lets you
				// run work in the background without blocking the UI.
				return m, func() tea.Msg {
					return FileSelectedMsg{Index: m.cursor, File: file}
				}
			}
			return m, nil

		case "space":
			// Toggle selection for the current file (in-memory only, no git commands)
			if len(m.Files) > 0 && m.cursor < len(m.Files) {
				path := m.Files[m.cursor].Path
				m.selected[path] = !m.IsSelected(path)
			}
			return m, nil

		case "a":
			// Select all or deselect all — toggle based on current state.
			// If all files are selected, deselect all. Otherwise, select all.
			if len(m.Files) > 0 {
				allSelected := !m.AnyDeselected()
				for _, f := range m.Files {
					m.selected[f.Path] = !allSelected
				}
			}
			return m, nil
		}
	}

	return m, nil
}

// View renders the file list as a string that fits within the configured width/height.
func (m FileListModel) View() string {
	if len(m.Files) == 0 {
		return lipgloss.NewStyle().
			Foreground(lipgloss.Color("#484F58")).
			Italic(true).
			Render("No changed files")
	}

	// Calculate the visible window based on scroll offset
	visibleHeight := m.height
	if visibleHeight <= 0 {
		visibleHeight = 10 // fallback if size not yet set
	}

	end := m.offset + visibleHeight
	if end > len(m.Files) {
		end = len(m.Files)
	}

	// ── Styles ──────────────────────────────────────────
	cursorStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#264F78"))

	selectedStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))
	deselectedStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58"))

	statusColors := map[git.FileStatus]color.Color{
		git.StatusNew:        lipgloss.Color("#3FB950"), // green
		git.StatusModified:   lipgloss.Color("#D29922"), // yellow
		git.StatusDeleted:    lipgloss.Color("#F85149"), // red
		git.StatusRenamed:    lipgloss.Color("#58A6FF"), // blue
		git.StatusConflicted: lipgloss.Color("#F85149"), // red
	}

	dirStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58"))

	// ── Render each visible row ─────────────────────────
	var lines []string
	for i := m.offset; i < end; i++ {
		file := m.Files[i]

		// Selection indicator: ● selected (will be committed), ○ excluded
		staging := deselectedStyle.Render("○")
		if m.IsSelected(file.Path) {
			staging = selectedStyle.Render("●")
		}

		// Status icon [M], [+], [-], [R], [!] with color
		iconColor := statusColors[file.Status]
		icon := lipgloss.NewStyle().
			Foreground(iconColor).
			Bold(true).
			Render(file.Status.Icon())

		// File name (bright) + directory path (dim)
		name := file.DisplayName()
		dir := file.DisplayDir()
		dirText := ""
		if dir != "" {
			dirText = " " + dirStyle.Render(dir)
		}

		// Rename arrow: ← old_path
		rename := ""
		if file.OrigPath != "" {
			rename = " " + dirStyle.Render("← "+file.OrigPath)
		}

		// Assemble the line: " ● [M] filename  dir/  ← oldname"
		line := fmt.Sprintf(" %s %s %s%s%s", staging, icon, name, dirText, rename)

		if i == m.cursor {
			// Highlight the entire row with a blue background
			// Pad to full width for a solid highlight bar
			padded := line + strings.Repeat(" ", max(0, m.width-lipgloss.Width(line)))
			lines = append(lines, cursorStyle.Render(padded))
		} else {
			lines = append(lines, line)
		}
	}

	return strings.Join(lines, "\n")
}

// clampOffset adjusts the scroll offset to keep the cursor visible within the viewport.
//
// Virtual scrolling concept: the full file list may have 100 items, but the
// screen only shows (say) 20 rows. We maintain a "window" defined by m.offset
// (the index of the first visible item) and visibleHeight (how many items fit
// on screen). As the cursor moves, we slide this window so the cursor is always
// inside it — scrolling down when the cursor passes the bottom edge, and
// scrolling up when it passes the top edge.
func (m *FileListModel) clampOffset() {
	visibleHeight := m.height
	if visibleHeight <= 0 {
		visibleHeight = 10
	}

	// Scroll down if cursor is below the visible window
	if m.cursor >= m.offset+visibleHeight {
		m.offset = m.cursor - visibleHeight + 1
	}

	// Scroll up if cursor is above the visible window
	if m.cursor < m.offset {
		m.offset = m.cursor
	}

	// Clamp to valid range
	if m.offset < 0 {
		m.offset = 0
	}
	maxOffset := len(m.Files) - visibleHeight
	if maxOffset < 0 {
		maxOffset = 0
	}
	if m.offset > maxOffset {
		m.offset = maxOffset
	}
}

// IsSelected returns whether the file at the given path is selected for commit.
func (m FileListModel) IsSelected(path string) bool {
	selected, exists := m.selected[path]
	return !exists || selected // default to selected if not in map
}

// SelectedFiles returns all files that are currently selected for commit.
func (m FileListModel) SelectedFiles() []git.FileEntry {
	var result []git.FileEntry
	for _, f := range m.Files {
		if m.IsSelected(f.Path) {
			result = append(result, f)
		}
	}
	return result
}

// AnyDeselected returns true if any file has been deselected.
func (m FileListModel) AnyDeselected() bool {
	for _, f := range m.Files {
		if !m.IsSelected(f.Path) {
			return true
		}
	}
	return false
}
