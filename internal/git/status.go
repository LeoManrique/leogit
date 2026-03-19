package git

import (
	"os/exec"
	"strconv"
	"strings"
)

// RepoStatus holds the branch metadata extracted from git status --porcelain=2 --branch.
// File entries are NOT parsed here — RawOutput is parsed separately for the file list.
type RepoStatus struct {
	Branch      string // current branch name (empty string if detached HEAD)
	OID         string // current commit SHA
	Upstream    string // upstream tracking branch (e.g. "origin/main"), empty if none
	Ahead       int    // commits ahead of upstream (0 if no upstream)
	Behind      int    // commits behind upstream (0 if no upstream)
	HasUpstream bool   // true if an upstream tracking branch is configured
	RawOutput   string // full command output, stored for file parsing
}

// GetStatus runs `git status --porcelain=2 --branch -z` and parses the branch header
// lines. This is the primary polling command — it runs every 2 seconds.
//
// The --no-optional-locks flag prevents git from taking any optional locks, which is
// important because this command runs frequently and we don't want it to block other
// git operations. TERM=dumb prevents any color/pager behavior.
func GetStatus(repoPath string) (RepoStatus, error) {
	cmd := exec.Command("git",
		"--no-optional-locks",
		"status",
		"--untracked-files=all",
		"--branch",
		"--porcelain=2",
		"-z",
	)
	cmd.Dir = repoPath
	// cmd.Environ() copies all current environment variables (PATH, HOME, etc.)
	// so git can still find its config. We append TERM=dumb to suppress colors/pager.
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	// cmd.Output() runs the command, waits for it to finish, and returns stdout.
	out, err := cmd.Output()
	if err != nil {
		return RepoStatus{}, err
	}

	output := string(out)
	result := RepoStatus{RawOutput: output}

	// With -z, all records (headers and file entries) are NUL-terminated.
	// Split on NUL to parse each record.
	for _, line := range strings.Split(output, "\x00") {
		// switch{} with no expression acts like if/else-if: each case is
		// evaluated top-to-bottom, and the first true case runs.
		switch {
		case strings.HasPrefix(line, "# branch.head "):
			result.Branch = strings.TrimPrefix(line, "# branch.head ")
			if result.Branch == "(detached)" {
				result.Branch = "" // empty string signals detached HEAD
			}

		case strings.HasPrefix(line, "# branch.oid "):
			result.OID = strings.TrimPrefix(line, "# branch.oid ")

		case strings.HasPrefix(line, "# branch.upstream "):
			result.Upstream = strings.TrimPrefix(line, "# branch.upstream ")
			result.HasUpstream = true

		case strings.HasPrefix(line, "# branch.ab "):
			// Format: "+<ahead> -<behind>" (e.g., "+3 -1")
			ab := strings.TrimPrefix(line, "# branch.ab ")
			parts := strings.Fields(ab)
			if len(parts) == 2 {
				// strconv.Atoi converts a string to int. The _ discards
				// the error — if parsing fails, the value defaults to 0.
				result.Ahead, _ = strconv.Atoi(strings.TrimPrefix(parts[0], "+"))
				result.Behind, _ = strconv.Atoi(strings.TrimPrefix(parts[1], "-"))
			}
		}
	}

	return result, nil
}

// HasConflicts returns true if any of the given file entries have a conflicted status.
func HasConflicts(entries []FileEntry) bool {
	for _, e := range entries {
		if e.Status == StatusConflicted {
			return true
		}
	}
	return false
}

// ConflictedFiles returns the paths of all conflicted files in the given entries.
func ConflictedFiles(entries []FileEntry) []string {
	var paths []string
	for _, e := range entries {
		if e.Status == StatusConflicted {
			paths = append(paths, e.Path)
		}
	}
	return paths
}
