package core

// FocusMode determines whether global shortcuts are active.
type FocusMode int

const (
	// Navigable is the default mode. Global shortcuts (q, ?, Tab, 1/2/3) work.
	// j/k navigate within the active pane.
	Navigable FocusMode = iota

	// Focused means all keystrokes go to the active pane. Global shortcuts
	// are blocked. Esc returns to Navigable.
	Focused
)

// Tab identifies which tab is shown.
type Tab int

const (
	ChangesTab Tab = iota
	HistoryTab
)

// Pane identifies a UI pane by its positional number.
type Pane int

const (
	PaneNone     Pane = iota
	Pane1             // Changes: Changed Files  | History: Commit List
	Pane2             // Changes: Diff Viewer     | History: Changed Files
	Pane3             // Changes: Commit Message  | History: Diff Viewer
	PaneTerminal      // Terminal/Log pane (both tabs)
)

// PaneName returns the display name for a pane in the given tab.
func PaneName(p Pane, t Tab) string {
	if p == PaneTerminal {
		return "Terminal"
	}
	if t == ChangesTab {
		switch p {
		case Pane1:
			return "Changed Files"
		case Pane2:
			return "Diff Viewer"
		case Pane3:
			return "Commit Message"
		}
	} else {
		switch p {
		case Pane1:
			return "Commit List"
		case Pane2:
			return "Changed Files"
		case Pane3:
			return "Diff Viewer"
		}
	}
	return ""
}
