package render

import (
	"bytes"
	"fmt"
	"path/filepath"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/alecthomas/chroma/v2"
	"github.com/alecthomas/chroma/v2/formatters"
	"github.com/alecthomas/chroma/v2/lexers"
	"github.com/alecthomas/chroma/v2/styles"

	"github.com/LeoManrique/leogit/internal/diff"
)

// DiffColors holds the lipgloss styles for each diff line type.
var DiffColors = struct {
	Add       lipgloss.Style
	Delete    lipgloss.Style
	Hunk      lipgloss.Style
	Context   lipgloss.Style
	LineNoOld lipgloss.Style
	LineNoNew lipgloss.Style
	NoNewline lipgloss.Style
}{
	Add:       lipgloss.NewStyle().Foreground(lipgloss.Color("#2EA043")), // green
	Delete:    lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149")), // red
	Hunk:      lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF")), // cyan/blue
	Context:   lipgloss.NewStyle().Foreground(lipgloss.Color("#C9D1D9")), // light gray
	LineNoOld: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")), // dim gray
	LineNoNew: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")), // dim gray
	NoNewline: lipgloss.NewStyle().Foreground(lipgloss.Color("#6E7681")).Italic(true),
}

// chromaStyle is the Chroma color theme used for syntax highlighting within diff lines.
// "monokai" works well on dark terminal backgrounds. This is applied on top of the
// diff coloring — context lines get full Chroma colors, while add/delete lines get
// tinted green/red.
var chromaStyle = styles.Get("monokai")

// chromaFormatter is the Chroma formatter for 256-color terminal output.
// TTY256 is widely supported and maps RGB colors to the nearest 256 index.
var chromaFormatter = formatters.Get("terminal256")

// highlightLine runs Chroma syntax highlighting on a single line of code.
// The lexer is determined by the file extension. Returns the ANSI-colored string.
// If highlighting fails for any reason, returns the plain text.
func highlightLine(content string, lexer chroma.Lexer) string {
	if lexer == nil || content == "" {
		return content
	}

	iterator, err := lexer.Tokenise(nil, content)
	if err != nil {
		return content
	}

	var buf bytes.Buffer
	err = chromaFormatter.Format(&buf, chromaStyle, iterator)
	if err != nil {
		return content
	}

	// Chroma may add a trailing newline — strip it for inline use
	result := buf.String()
	result = strings.TrimRight(result, "\n")
	return result
}

// getLexer returns the Chroma lexer for a file path, or nil if none matches.
// Uses the file extension to determine the language.
func getLexer(filePath string) chroma.Lexer {
	// Try matching by filename first (handles special files like Makefile, Dockerfile)
	lexer := lexers.Match(filepath.Base(filePath))
	if lexer != nil {
		return lexer
	}
	// Fallback: try by extension
	ext := filepath.Ext(filePath)
	if ext != "" {
		lexer = lexers.Get(ext)
	}
	return lexer
}

// gutterWidth is the width of each line number column in the gutter.
const gutterWidth = 5

// formatLineNo formats a line number into a fixed-width string for the gutter.
// Line number 0 means "not applicable" — renders as blank spaces.
func formatLineNo(n int) string {
	if n == 0 {
		return strings.Repeat(" ", gutterWidth)
	}
	return fmt.Sprintf("%*d", gutterWidth, n)
}

// RenderDiffLine renders a single diff line with gutter (line numbers) and
// syntax-highlighted content. Returns a single styled string.
func RenderDiffLine(line diff.Line, lexer chroma.Lexer, width int) string {
	// ── Gutter: old line | new line | prefix ──
	oldNo := DiffColors.LineNoOld.Render(formatLineNo(line.OldLineNo))
	newNo := DiffColors.LineNoNew.Render(formatLineNo(line.NewLineNo))

	// Gutter separator
	sep := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")).Render("│")

	// ── Content ──
	var prefix string
	var styledContent string

	switch line.Type {
	case diff.LineHunk:
		// Hunk headers get full cyan styling, no syntax highlighting
		return DiffColors.Hunk.Render(line.Text)

	case diff.LineAdd:
		prefix = DiffColors.Add.Render("+")
		// For added lines, apply Chroma highlighting then tint green
		highlighted := highlightLine(line.Content, lexer)
		styledContent = DiffColors.Add.Render(highlighted)

	case diff.LineDelete:
		prefix = DiffColors.Delete.Render("-")
		highlighted := highlightLine(line.Content, lexer)
		styledContent = DiffColors.Delete.Render(highlighted)

	case diff.LineContext:
		prefix = " "
		// Context lines get full Chroma syntax highlighting
		styledContent = highlightLine(line.Content, lexer)
		if styledContent == line.Content {
			// Chroma didn't highlight — use default context color
			styledContent = DiffColors.Context.Render(line.Content)
		}

	case diff.LineNoNewline:
		return DiffColors.NoNewline.Render(line.Text)
	}

	return oldNo + sep + newNo + sep + prefix + styledContent
}

// RenderDiff renders the visible portion of a diff for the diff viewer pane.
// It takes the parsed diff, the scroll offset, the number of visible rows, and
// the pane width. Returns the rendered string.
//
// Parameters:
//   - fileDiff: the parsed diff (from diff.Parse)
//   - offset: scroll position (index of the first visible line)
//   - visibleRows: how many lines fit in the pane
//   - width: pane width in columns
//
// Returns the rendered string for the visible window.
func RenderDiff(fileDiff *diff.FileDiff, offset, visibleRows, width int) string {
	if fileDiff == nil {
		return ""
	}

	allLines := fileDiff.AllLines()
	total := len(allLines)

	if total == 0 {
		return ""
	}

	// Clamp offset
	if offset < 0 {
		offset = 0
	}
	if offset >= total {
		offset = total - 1
	}

	// Determine the visible window
	end := offset + visibleRows
	if end > total {
		end = total
	}

	// Get the lexer for this file (based on the new path, falling back to old path)
	filePath := fileDiff.NewPath
	if filePath == "" || filePath == "/dev/null" {
		filePath = fileDiff.OldPath
	}
	lexer := getLexer(filePath)

	// Render each visible line
	var rendered []string
	for i := offset; i < end; i++ {
		rendered = append(rendered, RenderDiffLine(allLines[i], lexer, width))
	}

	return strings.Join(rendered, "\n")
}
