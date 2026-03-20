package diff

import (
	"regexp"
	"strconv"
	"strings"
)

// LineType classifies a line within a diff hunk.
type LineType int

const (
	LineContext   LineType = iota // unchanged line (prefix: " ")
	LineAdd                       // added line (prefix: "+")
	LineDelete                    // deleted line (prefix: "-")
	LineHunk                      // hunk header line (prefix: "@@")
	LineNoNewline                 // "\ No newline at end of file"
)

// Line represents a single line within a diff hunk.
type Line struct {
	Text      string   // the full line text including the +/-/space prefix
	Content   string   // the line text WITHOUT the +/-/space prefix (for syntax highlighting)
	Type      LineType // context, add, delete, hunk header, or no-newline marker
	OldLineNo int      // line number in the old file (0 if not applicable, e.g., added lines)
	NewLineNo int      // line number in the new file (0 if not applicable, e.g., deleted lines)
}

// HunkHeader holds the parsed line numbers from a @@ hunk header.
type HunkHeader struct {
	OldStart int // start line in old file
	OldCount int // number of lines in old file (default 1 if omitted)
	NewStart int // start line in new file
	NewCount int // number of lines in new file (default 1 if omitted)
}

// Hunk represents a single hunk within a file diff — a contiguous group of changes
// surrounded by context lines.
type Hunk struct {
	Header HunkHeader // parsed @@ line numbers
	Lines  []Line     // all lines in this hunk, including the @@ header line itself
}

// FileDiff represents the complete parsed diff for a single file.
type FileDiff struct {
	OldPath    string // path in the old version (from "--- a/...")
	NewPath    string // path in the new version (from "+++ b/...")
	FileHeader string // raw metadata lines before the first hunk (diff --git, index, ---, +++)
	Hunks      []Hunk // parsed hunks
}

// TotalLines returns the total number of displayable lines across all hunks.
// This is used to calculate scroll height.
func (d *FileDiff) TotalLines() int {
	total := 0
	for _, h := range d.Hunks {
		total += len(h.Lines)
	}
	return total
}

// AllLines returns a flat slice of all lines across all hunks, in order.
// This is used by the diff viewer for rendering and scrolling.
func (d *FileDiff) AllLines() []Line {
	var lines []Line
	for _, h := range d.Hunks {
		lines = append(lines, h.Lines...)
	}
	return lines
}

// hunkHeaderRegex matches the @@ line in a unified diff.
// Format: @@ -oldStart[,oldCount] +newStart[,newCount] @@[ optional section heading]
var hunkHeaderRegex = regexp.MustCompile(
	`^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$`,
)

// parseHunkHeader extracts line numbers from a @@ header string.
func parseHunkHeader(line string) (HunkHeader, bool) {
	matches := hunkHeaderRegex.FindStringSubmatch(line)
	if matches == nil {
		return HunkHeader{}, false
	}

	h := HunkHeader{
		OldStart: atoi(matches[1]),
		OldCount: 1, // default if omitted
		NewStart: atoi(matches[3]),
		NewCount: 1, // default if omitted
	}

	if matches[2] != "" {
		h.OldCount = atoi(matches[2])
	}
	if matches[4] != "" {
		h.NewCount = atoi(matches[4])
	}

	return h, true
}

// atoi converts a string to int, returning 0 on failure.
func atoi(s string) int {
	n, _ := strconv.Atoi(s)
	return n
}

// Parse takes raw unified diff output (from git diff) and parses it into a FileDiff.
// Returns nil if the input is empty or contains no hunks.
func Parse(raw string) *FileDiff {
	if strings.TrimSpace(raw) == "" {
		return nil
	}

	lines := strings.Split(raw, "\n")

	result := &FileDiff{}
	var headerLines []string
	var currentHunk *Hunk
	oldLine, newLine := 0, 0
	inHeader := true

	for _, line := range lines {
		// ── Hunk header ──
		if strings.HasPrefix(line, "@@") {
			inHeader = false

			header, ok := parseHunkHeader(line)
			if !ok {
				continue
			}

			// Save previous hunk
			if currentHunk != nil {
				result.Hunks = append(result.Hunks, *currentHunk)
			}

			oldLine = header.OldStart
			newLine = header.NewStart

			currentHunk = &Hunk{
				Header: header,
				Lines: []Line{{
					Text:    line,
					Content: line,
					Type:    LineHunk,
				}},
			}
			continue
		}

		// ── File header (everything before the first @@) ──
		if inHeader {
			// Extract old/new paths from --- and +++ lines
			if strings.HasPrefix(line, "--- ") {
				path := strings.TrimPrefix(line, "--- ")
				path = strings.TrimPrefix(path, "a/") // git prefixes with a/
				result.OldPath = path
			} else if strings.HasPrefix(line, "+++ ") {
				path := strings.TrimPrefix(line, "+++ ")
				path = strings.TrimPrefix(path, "b/") // git prefixes with b/
				result.NewPath = path
			}
			headerLines = append(headerLines, line)
			continue
		}

		// ── Inside a hunk — classify each line ──
		if currentHunk == nil {
			continue
		}

		if len(line) == 0 {
			// Empty line in diff = context line with empty content.
			// This happens because strings.Split turns a blank line into "",
			// but in unified diff format it would normally be " " (space prefix).
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      " ",
				Content:   "",
				Type:      LineContext,
				OldLineNo: oldLine,
				NewLineNo: newLine,
			})
			oldLine++
			newLine++
			continue
		}

		switch line[0] {
		case '+':
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   line[1:],
				Type:      LineAdd,
				OldLineNo: 0, // added lines have no old line number
				NewLineNo: newLine,
			})
			newLine++

		case '-':
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   line[1:],
				Type:      LineDelete,
				OldLineNo: oldLine,
				NewLineNo: 0, // deleted lines have no new line number
			})
			oldLine++

		case '\\':
			// "\ No newline at end of file" — display but don't count
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:    line,
				Content: line,
				Type:    LineNoNewline,
			})

		default:
			// Context line (starts with space)
			content := line
			if len(line) > 0 && line[0] == ' ' {
				content = line[1:]
			}
			currentHunk.Lines = append(currentHunk.Lines, Line{
				Text:      line,
				Content:   content,
				Type:      LineContext,
				OldLineNo: oldLine,
				NewLineNo: newLine,
			})
			oldLine++
			newLine++
		}
	}

	// Save the last hunk
	if currentHunk != nil {
		result.Hunks = append(result.Hunks, *currentHunk)
	}

	result.FileHeader = strings.Join(headerLines, "\n")

	if len(result.Hunks) == 0 {
		return nil
	}

	return result
}
