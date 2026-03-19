package diff

// SelectionState represents the default state of all lines.
type SelectionState int

const (
	SelectAll  SelectionState = iota // all lines selected by default
	SelectNone                       // no lines selected by default
)

// DiffSelection tracks which diff lines are selected for committing or discarding.
// Uses a DefaultState + diverging set for space efficiency.
type DiffSelection struct {
	DefaultState    SelectionState // initial state for all lines
	DivergingLines  map[int]bool   // lines that differ from DefaultState (key = flat line index)
	SelectableLines map[int]bool   // only Add/Delete lines are selectable (key = flat line index)
}

// NewDiffSelection creates a DiffSelection for a parsed diff.
// Scans all lines to build the SelectableLines set.
// defaultState determines whether lines start selected (default: all selected)
// or unselected (user manually picks lines to include).
func NewDiffSelection(fileDiff *FileDiff, defaultState SelectionState) DiffSelection {
	sel := DiffSelection{
		DefaultState:    defaultState,
		DivergingLines:  make(map[int]bool),
		SelectableLines: make(map[int]bool),
	}

	if fileDiff == nil {
		return sel
	}

	// Build the selectable lines set — only Add and Delete lines can be toggled
	idx := 0
	for _, hunk := range fileDiff.Hunks {
		for _, line := range hunk.Lines {
			if line.Type == LineAdd || line.Type == LineDelete {
				sel.SelectableLines[idx] = true
			}
			idx++
		}
	}

	return sel
}

// IsSelected returns whether the line at the given flat index is selected.
func (s DiffSelection) IsSelected(lineIdx int) bool {
	_, diverges := s.DivergingLines[lineIdx]
	if s.DefaultState == SelectAll {
		return !diverges // selected unless it diverges
	}
	return diverges // not selected unless it diverges
}

// IsSelectable returns whether the line at the given flat index can be toggled.
func (s DiffSelection) IsSelectable(lineIdx int) bool {
	return s.SelectableLines[lineIdx]
}

// WithLineSelection sets a single line's selection state.
// Only works on selectable lines — non-selectable lines are ignored.
func (s DiffSelection) WithLineSelection(lineIdx int, selected bool) DiffSelection {
	if !s.SelectableLines[lineIdx] {
		return s
	}

	// Create a copy of the diverging set
	newDiverging := make(map[int]bool, len(s.DivergingLines))
	for k, v := range s.DivergingLines {
		newDiverging[k] = v
	}

	// wantsDiverge: true when the requested state differs from the default.
	// E.g. DefaultState=SelectAll + selected=false => diverges => add to set.
	// E.g. DefaultState=SelectAll + selected=true  => matches  => remove from set.
	wantsDiverge := (s.DefaultState == SelectAll) != selected
	if wantsDiverge {
		newDiverging[lineIdx] = true
	} else {
		delete(newDiverging, lineIdx)
	}

	return DiffSelection{
		DefaultState:    s.DefaultState,
		DivergingLines:  newDiverging,
		SelectableLines: s.SelectableLines,
	}
}

// WithToggle toggles a single line's selection.
func (s DiffSelection) WithToggle(lineIdx int) DiffSelection {
	return s.WithLineSelection(lineIdx, !s.IsSelected(lineIdx))
}

// WithRangeSelection sets selection for a range of lines (used for hunk-level toggle).
// Only affects selectable lines in the range [from, from+count).
func (s DiffSelection) WithRangeSelection(from, count int, selected bool) DiffSelection {
	result := s
	for i := from; i < from+count; i++ {
		result = result.WithLineSelection(i, selected)
	}
	return result
}

// SelectedCount returns the number of currently selected selectable lines.
func (s DiffSelection) SelectedCount() int {
	count := 0
	for idx := range s.SelectableLines {
		if s.IsSelected(idx) {
			count++
		}
	}
	return count
}

// SelectableCount returns the total number of selectable lines.
func (s DiffSelection) SelectableCount() int {
	return len(s.SelectableLines)
}

// AllSelected returns true if every selectable line is selected.
func (s DiffSelection) AllSelected() bool {
	return s.SelectedCount() == s.SelectableCount()
}

// NoneSelected returns true if no selectable lines are selected.
func (s DiffSelection) NoneSelected() bool {
	return s.SelectedCount() == 0
}
