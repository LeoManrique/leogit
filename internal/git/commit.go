package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// Commit creates a git commit with the given message.
// The message is piped to `git commit -F -` via stdin.
// The message should already be fully formatted (summary + description + trailers).
func Commit(repoPath string, message string) error {
	cmd := exec.Command("git", "commit", "-F", "-")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")
	cmd.Stdin = strings.NewReader(message)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git commit: %s (%w)", strings.TrimSpace(string(out)), err)
	}
	return nil
}

// HasStagedChanges checks whether the index has any staged changes.
// Returns true if there are staged changes, false if the index matches HEAD.
// Uses `git diff --cached --quiet` — exit code 1 means changes exist.
func HasStagedChanges(repoPath string) (bool, error) {
	cmd := exec.Command("git", "diff", "--cached", "--quiet")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	err := cmd.Run()
	if err != nil {
		// Exit code 1 means there ARE staged changes (diff found).
		// Type assertion: err.(*exec.ExitError) checks if the error is specifically
		// an ExitError (process exited with non-zero code). The "ok" bool tells us
		// if the assertion succeeded. This is needed because cmd.Run() returns a
		// generic error interface — we need the concrete ExitError to read ExitCode().
		if exitErr, ok := err.(*exec.ExitError); ok {
			if exitErr.ExitCode() == 1 {
				return true, nil
			}
		}
		return false, fmt.Errorf("git diff --cached --quiet: %w", err)
	}
	// Exit code 0 means no staged changes (index matches HEAD)
	return false, nil
}

// FormatCommitMessage builds a full commit message string from its parts.
// The result follows git commit message conventions:
//   - Line 1: summary
//   - Line 2: blank (if description or trailers follow)
//   - Line 3+: description body
//   - Blank line before trailers
//   - Co-authored-by trailers (one per line)
func FormatCommitMessage(summary, description string, coAuthors []string) string {
	var parts []string

	parts = append(parts, summary)

	if description != "" || len(coAuthors) > 0 {
		parts = append(parts, "") // blank line after summary
	}

	if description != "" {
		parts = append(parts, description)
	}

	if len(coAuthors) > 0 {
		if description != "" {
			// Extra blank line to separate description body from trailers.
			// When there's no description, the blank line after the summary
			// (added above) already provides the required separation.
			parts = append(parts, "") // blank line before trailers
		}
		for _, author := range coAuthors {
			parts = append(parts, fmt.Sprintf("Co-authored-by: %s", author))
		}
	}

	return strings.Join(parts, "\n")
}
