package git

import (
	"path/filepath"
	"strings"
)

// FileStatus represents the user-facing status of a changed file.
type FileStatus int

const (
	StatusNew        FileStatus = iota // untracked or newly added
	StatusModified                     // modified in worktree or index
	StatusDeleted                      // deleted
	StatusRenamed                      // renamed or moved
	StatusConflicted                   // unmerged / merge conflict
)

// Icon returns the short icon string for display in the file list.
func (s FileStatus) Icon() string {
	switch s {
	case StatusNew:
		return "[+]"
	case StatusModified:
		return "[M]"
	case StatusDeleted:
		return "[-]"
	case StatusRenamed:
		return "[R]"
	case StatusConflicted:
		return "[!]"
	default:
		return "[?]"
	}
}

// Label returns the full human-readable label.
func (s FileStatus) Label() string {
	switch s {
	case StatusNew:
		return "New"
	case StatusModified:
		return "Modified"
	case StatusDeleted:
		return "Deleted"
	case StatusRenamed:
		return "Renamed"
	case StatusConflicted:
		return "Conflicted"
	default:
		return "Unknown"
	}
}

// FileEntry represents a single changed file from git status.
type FileEntry struct {
	Path     string     // file path relative to repo root
	OrigPath string     // original path (only set for renames, empty otherwise)
	Status   FileStatus // user-facing status category
	Staged   bool       // true if the file has changes in the index (staging area)
	XY       string     // raw 2-character status code from porcelain v2
}

// DisplayName returns the filename (last path component) for display.
func (f FileEntry) DisplayName() string {
	return filepath.Base(f.Path)
}

// DisplayDir returns the directory portion of the path, or empty string if at repo root.
func (f FileEntry) DisplayDir() string {
	dir := filepath.Dir(f.Path)
	if dir == "." {
		return ""
	}
	return dir + "/"
}

// ParseFiles extracts FileEntry items from the RawOutput of a git status command.
// The input must be from `git status --porcelain=2 --branch -z`.
//
// Parsing strategy:
// 1. Skip header lines (start with "# ", newline-terminated)
// 2. Split the remaining content by NUL (\x00) to get entry segments
// 3. Parse each segment based on its type prefix (1, 2, u, ?)
// 4. For rename entries (type 2), consume the NEXT segment as the original path
func ParseFiles(rawOutput string) []FileEntry {
	if rawOutput == "" {
		return nil
	}

	// Skip header lines. They start with "# " and are newline-terminated.
	// Everything after the last header line is file entries.
	rest := rawOutput
	for {
		nl := strings.Index(rest, "\n")
		if nl == -1 {
			break
		}
		line := rest[:nl]
		if !strings.HasPrefix(line, "# ") {
			break
		}
		rest = rest[nl+1:]
	}

	if rest == "" {
		return nil
	}

	// Split by NUL to get entry segments.
	// Each entry is NUL-terminated, so the last element after split is empty.
	segments := strings.Split(rest, "\x00")

	var entries []FileEntry
	i := 0
	for i < len(segments) {
		seg := segments[i]
		if seg == "" {
			i++
			continue
		}

		switch {
		case strings.HasPrefix(seg, "1 "):
			// Type 1: ordinary changed entry
			if e := parseOrdinaryEntry(seg); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "2 "):
			// Type 2: rename/copy entry
			// The NEXT NUL-separated segment is the original (old) path
			origPath := ""
			if i+1 < len(segments) {
				origPath = segments[i+1]
				// Why i++ here? Rename entries are unique: they produce TWO
				// NUL-separated segments (newpath\0oldpath\0) instead of one.
				// This extra increment skips past the oldpath segment so the
				// outer loop's own i++ doesn't try to parse it as a new entry.
				i++ // consume the extra segment
			}
			if e := parseRenameEntry(seg, origPath); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "u "):
			// Type u: unmerged (conflict) entry
			if e := parseUnmergedEntry(seg); e != nil {
				entries = append(entries, *e)
			}

		case strings.HasPrefix(seg, "? "):
			// Type ?: untracked file (just a path, no metadata)
			path := strings.TrimPrefix(seg, "? ")
			entries = append(entries, FileEntry{
				Path:   path,
				Status: StatusNew,
				Staged: false,
				XY:     "??",
			})
		}

		i++
	}

	return entries
}

// parseOrdinaryEntry parses a type-1 (ordinary changed) entry.
// Format: "1 XY sub mH mI mW hH hI <path>"
// 9 fields total — use SplitN to handle paths with spaces.
func parseOrdinaryEntry(line string) *FileEntry {
	// SplitN splits into at most 9 pieces, so the 9th piece captures everything
	// remaining — including spaces. For example:
	//   Input:  "1 M. N... 100644 100644 100644 abc123 def456 my cool file.go"
	//   fields[0..7] get the first 8 space-separated tokens, and fields[8]
	//   becomes "my cool file.go" (unsplit) — exactly the file path we need.
	fields := strings.SplitN(line, " ", 9)
	if len(fields) < 9 {
		return nil
	}

	xy := fields[1]
	path := fields[8]

	return &FileEntry{
		Path:   path,
		Status: statusFromXY(xy),
		Staged: isStagedXY(xy),
		XY:     xy,
	}
}

// parseRenameEntry parses a type-2 (rename/copy) entry.
// Format: "2 XY sub mH mI mW hH hI Xscore <newpath>"
// 10 fields total. origPath is the next NUL-separated segment.
func parseRenameEntry(line string, origPath string) *FileEntry {
	fields := strings.SplitN(line, " ", 10)
	if len(fields) < 10 {
		return nil
	}

	xy := fields[1]
	path := fields[9]

	return &FileEntry{
		Path:     path,
		OrigPath: origPath,
		Status:   StatusRenamed,
		Staged:   isStagedXY(xy),
		XY:       xy,
	}
}

// parseUnmergedEntry parses a type-u (unmerged/conflict) entry.
// Format: "u XY sub m1 m2 m3 mW h1 h2 h3 <path>"
// 11 fields total.
func parseUnmergedEntry(line string) *FileEntry {
	fields := strings.SplitN(line, " ", 11)
	if len(fields) < 11 {
		return nil
	}

	xy := fields[1]
	path := fields[10]

	return &FileEntry{
		Path:   path,
		Status: StatusConflicted,
		Staged: false, // conflicts are not considered "staged"
		XY:     xy,
	}
}

// statusFromXY maps the 2-character XY status code to a FileStatus.
//
// X = status in the index (staging area)
// Y = status in the worktree
//
// Priority order: Conflicted → New → Renamed → Deleted → Modified
func statusFromXY(xy string) FileStatus {
	if len(xy) != 2 {
		return StatusModified
	}
	x, y := xy[0], xy[1]

	// Unmerged states: any U, or both added (AA), or both deleted (DD)
	if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
		return StatusConflicted
	}

	// Untracked
	if x == '?' {
		return StatusNew
	}

	// Added to index (new file)
	if x == 'A' {
		return StatusNew
	}

	// Renamed in index
	if x == 'R' {
		return StatusRenamed
	}

	// Deleted (either in index or worktree)
	if x == 'D' || y == 'D' {
		return StatusDeleted
	}

	// Modified (M in either position, C for copied, or any other combination)
	return StatusModified
}

// isStagedXY returns true if the file has changes in the index (staging area).
// X represents the index status:
//   - '.' means unmodified in the index
//   - '?' means untracked
//   - '!' means ignored
//
// Any other X value means the file has staged changes.
func isStagedXY(xy string) bool {
	if len(xy) != 2 {
		return false
	}
	x := xy[0]
	return x != '.' && x != '?' && x != '!'
}
