package git

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"time"
)

// CommitInfo holds metadata about a single commit from git log.
type CommitInfo struct {
	SHA            string    // full 40-character hash
	ShortSHA       string    // abbreviated hash (7 characters)
	Summary        string    // first line of commit message
	Body           string    // remaining commit message lines (may be empty)
	AuthorName     string
	AuthorEmail    string
	AuthorDate     time.Time
	CommitterName  string
	CommitterEmail string
	CommitterDate  time.Time
	Parents        []string // parent SHA(s); empty for root commits, 2+ for merges
	Trailers       string   // git trailers (e.g., "Signed-off-by: ...")
	Refs           string   // ref decorations (e.g., "HEAD -> main, origin/main")
}

// LogOptions controls which commits to fetch and how many.
type LogOptions struct {
	MaxCount int // maximum number of commits to return (default 50)
	Skip     int // number of commits to skip (for pagination)
}

// logFormat is the git log format string. Fields are separated by \x01 (SOH),
// records by \x00 (NULL). This handles multi-line bodies and trailers safely.
const logFormat = "%H%x01%h%x01%s%x01%b%x01%an%x01%ae%x01%ad%x01%cn%x01%ce%x01%cd%x01%P%x01%(trailers:unfold,only)%x01%D%x00"

// GetLog returns a list of commits from the repository's current branch.
// Commits are sorted newest-first (default git log order). Use opts.Skip
// for pagination — the app loads 50 commits at a time and fetches more
// as the user scrolls down.
func GetLog(repoPath string, opts LogOptions) ([]CommitInfo, error) {
	if opts.MaxCount == 0 {
		opts.MaxCount = 50
	}

	cmd := exec.Command("git",
		"log",
		"--date=raw",
		fmt.Sprintf("--max-count=%d", opts.MaxCount),
		fmt.Sprintf("--skip=%d", opts.Skip),
		"--format="+logFormat,
		"--no-show-signature",
		"--no-color",
		"--",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git log: %w", err)
	}

	return parseLogOutput(string(out))
}

// parseLogOutput splits the raw git log output into CommitInfo structs.
func parseLogOutput(raw string) ([]CommitInfo, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, nil
	}

	records := strings.Split(raw, "\x00")
	var commits []CommitInfo

	for _, record := range records {
		record = strings.TrimSpace(record)
		if record == "" {
			continue
		}

		fields := strings.SplitN(record, "\x01", 13)
		if len(fields) < 10 {
			continue
		}

		commit := CommitInfo{
			SHA:            fields[0],
			ShortSHA:       fields[1],
			Summary:        fields[2],
			Body:           strings.TrimSpace(fields[3]),
			AuthorName:     fields[4],
			AuthorEmail:    fields[5],
			AuthorDate:     parseRawDate(fields[6]),
			CommitterName:  fields[7],
			CommitterEmail: fields[8],
			CommitterDate:  parseRawDate(fields[9]),
		}

		if len(fields) > 10 {
			commit.Parents = splitParents(fields[10])
		}
		if len(fields) > 11 {
			commit.Trailers = strings.TrimSpace(fields[11])
		}
		if len(fields) > 12 {
			commit.Refs = strings.TrimSpace(fields[12])
		}

		commits = append(commits, commit)
	}

	return commits, nil
}

// parseRawDate parses a git raw date string ("1647360000 -0500") into time.Time.
func parseRawDate(raw string) time.Time {
	raw = strings.TrimSpace(raw)
	parts := strings.SplitN(raw, " ", 2)
	if len(parts) == 0 {
		return time.Time{}
	}

	unix, err := strconv.ParseInt(parts[0], 10, 64)
	if err != nil {
		return time.Time{}
	}

	return time.Unix(unix, 0)
}

// splitParents splits the space-separated parent SHA string into a slice.
func splitParents(raw string) []string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}
	return strings.Fields(raw)
}

// GetCommitFiles returns the list of files changed in a specific commit.
func GetCommitFiles(repoPath, sha string) ([]FileEntry, error) {
	cmd := exec.Command("git",
		"diff-tree",
		"--no-commit-id",
		"-r",
		"--name-status",
		sha,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git diff-tree: %w", err)
	}

	return parseDiffTree(string(out)), nil
}

// parseDiffTree parses the output of git diff-tree --name-status into FileEntry structs.
func parseDiffTree(raw string) []FileEntry {
	var files []FileEntry

	for _, line := range strings.Split(strings.TrimSpace(raw), "\n") {
		if line == "" {
			continue
		}

		parts := strings.SplitN(line, "\t", 3)
		if len(parts) < 2 {
			continue
		}

		statusCode := parts[0]
		entry := FileEntry{Path: parts[1]}

		switch {
		case statusCode == "M":
			entry.Status = StatusModified
		case statusCode == "A":
			entry.Status = StatusNew
		case statusCode == "D":
			entry.Status = StatusDeleted
		case strings.HasPrefix(statusCode, "R"):
			entry.Status = StatusRenamed
			if len(parts) > 2 {
				entry.OrigPath = parts[1]
				entry.Path = parts[2]
			}
		case strings.HasPrefix(statusCode, "C"):
			entry.Status = StatusModified // treat copies as modified
			if len(parts) > 2 {
				entry.Path = parts[2]
			}
		case statusCode == "T":
			entry.Status = StatusModified // treat type changes as modified
		default:
			entry.Status = StatusModified
		}

		files = append(files, entry)
	}

	return files
}

// RelativeDate formats a time.Time as a human-readable relative string.
func RelativeDate(t time.Time) string {
	if t.IsZero() {
		return ""
	}

	d := time.Since(t)

	switch {
	case d < time.Minute:
		return "just now"
	case d < time.Hour:
		m := int(d.Minutes())
		if m == 1 {
			return "1 minute ago"
		}
		return fmt.Sprintf("%d minutes ago", m)
	case d < 24*time.Hour:
		h := int(d.Hours())
		if h == 1 {
			return "1 hour ago"
		}
		return fmt.Sprintf("%d hours ago", h)
	case d < 30*24*time.Hour:
		days := int(d.Hours() / 24)
		if days == 1 {
			return "1 day ago"
		}
		return fmt.Sprintf("%d days ago", days)
	case d < 365*24*time.Hour:
		months := int(d.Hours() / 24 / 30)
		if months <= 1 {
			return "1 month ago"
		}
		return fmt.Sprintf("%d months ago", months)
	default:
		years := int(d.Hours() / 24 / 365)
		if years <= 1 {
			return "1 year ago"
		}
		return fmt.Sprintf("%d years ago", years)
	}
}

