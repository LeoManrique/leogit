package layout

// Dimensions holds the calculated sizes for all layout regions.
// All values are in terminal cells (characters wide, rows tall).
type Dimensions struct {
	Width  int // total terminal width
	Height int // total terminal height

	HeaderHeight int // always 1 row
	TabBarHeight int // always 1 row

	ContentTop    int // row where content area starts
	ContentHeight int // rows available for panes

	// Sidebar (left column)
	SidebarWidth    int
	FileListHeight  int // top of sidebar (flexible)
	CommitMsgHeight int // bottom of sidebar (8 rows fixed)

	// Main area (right column)
	MainWidth  int
	DiffHeight int // top of main (flexible)

	// Terminal (bottom of main, collapsible)
	TerminalHeight int // 0 when collapsed
}

const (
	headerRows      = 1
	tabBarRows      = 1
	minSidebarWidth = 25
	sidebarRatio    = 0.30
	commitMsgRows   = 8
	defaultTermRows = 10
	minTermRows     = 3
	minPaneRows     = 3
)

// Calculate computes layout dimensions from terminal size.
// terminalOpen/terminalRows control the collapsible terminal pane.
func Calculate(width, height int, terminalOpen bool, terminalRows int) Dimensions {
	d := Dimensions{
		Width:        width,
		Height:       height,
		HeaderHeight: headerRows,
		TabBarHeight: tabBarRows,
	}

	d.ContentTop = headerRows + tabBarRows
	d.ContentHeight = height - d.ContentTop
	if d.ContentHeight < 1 {
		d.ContentHeight = 1
	}

	// ── Horizontal: sidebar | main ──
	d.SidebarWidth = int(float64(width) * sidebarRatio)
	if d.SidebarWidth < minSidebarWidth {
		d.SidebarWidth = minSidebarWidth
	}
	if d.SidebarWidth > width/2 {
		d.SidebarWidth = width / 2
	}
	d.MainWidth = width - d.SidebarWidth
	if d.MainWidth < 1 {
		d.MainWidth = 1
	}

	// ── Terminal ──
	if terminalOpen {
		d.TerminalHeight = terminalRows
		if d.TerminalHeight < minTermRows {
			d.TerminalHeight = minTermRows
		}
		maxTerm := d.ContentHeight * 80 / 100 // 80% of content area, using integer math
		if d.TerminalHeight > maxTerm {
			d.TerminalHeight = maxTerm
		}
	}

	// ── Sidebar vertical: file list (top) + commit msg (bottom) ──
	d.CommitMsgHeight = commitMsgRows
	d.FileListHeight = d.ContentHeight - d.CommitMsgHeight
	if d.FileListHeight < minPaneRows {
		d.FileListHeight = minPaneRows
		d.CommitMsgHeight = d.ContentHeight - d.FileListHeight
	}

	// ── Main vertical: diff (top) + terminal (bottom) ──
	d.DiffHeight = d.ContentHeight - d.TerminalHeight
	if d.DiffHeight < minPaneRows {
		d.DiffHeight = minPaneRows
	}

	return d
}

// DefaultTerminalRows returns the default terminal height when first opened.
func DefaultTerminalRows() int {
	return defaultTermRows
}
