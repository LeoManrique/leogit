package diff

import (
	"fmt"
	"strings"
)

// GeneratePatch creates a unified diff patch from a FileDiff and DiffSelection.
// Only selected Add/Delete lines are included. Unselected deletes become context.
// Unselected adds are skipped entirely.
//
// Returns the patch as a string suitable for piping to `git apply --cached`.
// Returns empty string if no selectable lines are selected.
func GeneratePatch(fileDiff *FileDiff, selection DiffSelection) string {
	if fileDiff == nil || selection.NoneSelected() {
		return ""
	}

	var patch strings.Builder

	// Write the file header (diff --git, index, ---, +++ lines)
	patch.WriteString(fileDiff.FileHeader)
	patch.WriteString("\n")

	// Track the flat line index across all hunks
	flatIdx := 0

	for _, hunk := range fileDiff.Hunks {
		hunkPatch := generateHunkPatch(hunk, selection, &flatIdx)
		if hunkPatch != "" {
			patch.WriteString(hunkPatch)
		}
	}

	result := patch.String()
	if strings.TrimSpace(result) == strings.TrimSpace(fileDiff.FileHeader) {
		// No hunks were generated — nothing to apply
		return ""
	}

	return result
}

// generateHunkPatch generates the patch for a single hunk given the selection.
// Returns empty string if the hunk has no selected changes.
// flatIdx is a pointer to the current position in the flat line array,
// advanced as lines are processed.
func generateHunkPatch(hunk Hunk, selection DiffSelection, flatIdx *int) string {
	var lines []string
	oldCount := 0
	newCount := 0
	hasChanges := false

	for _, line := range hunk.Lines {
		idx := *flatIdx
		*flatIdx++

		switch line.Type {
		case LineHunk:
			// Skip the original hunk header — we'll generate a new one
			continue

		case LineContext:
			lines = append(lines, line.Text)
			oldCount++
			newCount++

		case LineAdd:
			if selection.IsSelected(idx) {
				lines = append(lines, line.Text)
				newCount++
				hasChanges = true
			}
			// Unselected adds: skip entirely (don't include in patch)

		case LineDelete:
			if selection.IsSelected(idx) {
				lines = append(lines, line.Text)
				oldCount++
				hasChanges = true
			} else {
				// Unselected deletes: convert to context line.
				// The line exists in the old file, so the patch must account for it.
				// Note: line.Content omits the prefix char (e.g. "foo" not "-foo"),
				// so " " + Content turns a delete into a context line (" foo").
				contextLine := " " + line.Content
				lines = append(lines, contextLine)
				oldCount++
				newCount++
			}

		case LineNoNewline:
			lines = append(lines, line.Text)
		}
	}

	if !hasChanges {
		return ""
	}

	// Generate the new hunk header with recalculated counts
	header := fmt.Sprintf("@@ -%d,%d +%d,%d @@\n",
		hunk.Header.OldStart, oldCount,
		hunk.Header.NewStart, newCount,
	)

	return header + strings.Join(lines, "\n") + "\n"
}

// GenerateInversePatch creates a reverse patch for discarding selected changes.
// Selected Add lines become Delete lines, selected Delete lines become Add lines.
// This is used by "discard changes" — applied without --cached to modify the working tree.
func GenerateInversePatch(fileDiff *FileDiff, selection DiffSelection) string {
	if fileDiff == nil || selection.NoneSelected() {
		return ""
	}

	var patch strings.Builder
	patch.WriteString(fileDiff.FileHeader)
	patch.WriteString("\n")

	flatIdx := 0

	for _, hunk := range fileDiff.Hunks {
		hunkPatch := generateInverseHunkPatch(hunk, selection, &flatIdx)
		if hunkPatch != "" {
			patch.WriteString(hunkPatch)
		}
	}

	result := patch.String()
	if strings.TrimSpace(result) == strings.TrimSpace(fileDiff.FileHeader) {
		return ""
	}

	return result
}

// generateInverseHunkPatch generates the inverse patch for a single hunk.
// Adds become deletes and deletes become adds (for discarding changes).
func generateInverseHunkPatch(hunk Hunk, selection DiffSelection, flatIdx *int) string {
	var lines []string
	oldCount := 0
	newCount := 0
	hasChanges := false

	for _, line := range hunk.Lines {
		idx := *flatIdx
		*flatIdx++

		switch line.Type {
		case LineHunk:
			continue

		case LineContext:
			lines = append(lines, line.Text)
			oldCount++
			newCount++

		case LineAdd:
			if selection.IsSelected(idx) {
				// Inverse: add becomes delete
				lines = append(lines, "-"+line.Content)
				oldCount++
				hasChanges = true
			} else {
				// Unselected add: stays as context in the inverse patch
				lines = append(lines, " "+line.Content)
				oldCount++
				newCount++
			}

		case LineDelete:
			if selection.IsSelected(idx) {
				// Inverse: delete becomes add
				lines = append(lines, "+"+line.Content)
				newCount++
				hasChanges = true
			} else {
				// Unselected delete: convert to context
				lines = append(lines, " "+line.Content)
				oldCount++
				newCount++
			}

		case LineNoNewline:
			lines = append(lines, line.Text)
		}
	}

	if !hasChanges {
		return ""
	}

	// Inverse patch: swap old/new start positions
	header := fmt.Sprintf("@@ -%d,%d +%d,%d @@\n",
		hunk.Header.NewStart, oldCount,
		hunk.Header.OldStart, newCount,
	)

	return header + strings.Join(lines, "\n") + "\n"
}
