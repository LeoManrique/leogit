package git

import (
	"os/exec"
)

// GetDiff runs the appropriate git diff command for a file and returns the raw
// NOTE: FileEntry (with Status field) is defined in internal/git/files.go.
// unified diff output. The command chosen depends on the file's status:
//   - Untracked files use --no-index against /dev/null to show the full file as added
//   - All tracked files diff the working tree against HEAD
//
// We always diff against HEAD (not the index) because leogit uses in-memory selection
// for commit staging. Git's index is only modified at commit time.
func GetDiff(repoPath string, file FileEntry) (string, error) {
	var args []string

	switch {
	case file.Status == StatusNew:
		// Untracked file: compare /dev/null with the file to show it as entirely new.
		// --no-index tells git to diff two paths outside the index.
		args = []string{
			"diff",
			"--no-ext-diff",
			"--patch-with-raw",
			"--no-color",
			"--no-index",
			"--", "/dev/null", file.Path,
		}

	default:
		// Tracked file: diff the working tree against HEAD.
		args = []string{
			"diff",
			"--no-ext-diff",
			"--patch-with-raw",
			"--no-color",
			"HEAD",
			"--", file.Path,
		}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		// git diff --no-index exits with code 1 when differences exist.
		// This is expected for untracked files — only fail if there's no output.
		if file.Status == StatusNew && len(out) > 0 {
			return string(out), nil
		}
		return "", err
	}

	return string(out), nil
}

// GetDiffWhitespaceIgnored runs the same diff as GetDiff but with the -w flag
// to ignore all whitespace changes. Used when the user enables "hide whitespace".
func GetDiffWhitespaceIgnored(repoPath string, file FileEntry) (string, error) {
	// For simplicity, get the normal diff command args and inject -w.
	// We duplicate the logic rather than adding a parameter to keep GetDiff clean.
	var args []string

	switch {
	case file.Status == StatusNew:
		args = []string{
			"diff", "--no-ext-diff", "--patch-with-raw", "--no-color",
			"--no-index", "-w",
			"--", "/dev/null", file.Path,
		}
	default:
		args = []string{
			"diff", "--no-ext-diff", "--patch-with-raw", "--no-color",
			"HEAD", "-w",
			"--", file.Path,
		}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		if file.Status == StatusNew && len(out) > 0 {
			return string(out), nil
		}
		return "", err
	}

	return string(out), nil
}

// GetCommitDiff returns the diff for a specific file in a specific commit.
// Used by the History tab to show what changed in a selected commit.
func GetCommitDiff(repoPath, sha, filePath string) (string, error) {
	cmd := exec.Command("git",
		"log", sha,
		"-m", "-1", "--first-parent",
		"--patch-with-raw",
		"--format=",
		"--no-color",
		"-z",
		"--", filePath,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return string(out), nil
}
