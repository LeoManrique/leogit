package git

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

// Fetch runs `git fetch --prune` for the given remote.
// Returns nil on success, error on failure.
// Progress is not captured — the TUI shows a spinner while this runs.
func Fetch(repoPath, remote string) error {
	cmd := exec.Command("git", "fetch",
		"--prune",
		"--recurse-submodules=on-demand",
		remote,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git fetch failed: %w\n%s", err, string(output))
	}
	return nil
}

// Pull runs `git pull --ff` for the given remote.
// Returns nil on success. On merge conflict, returns an error whose message
// contains "CONFLICT" — the caller checks for this to trigger conflict detection.
func Pull(repoPath, remote string) error {
	cmd := exec.Command("git", "pull",
		"--ff",
		"--recurse-submodules",
		remote,
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		msg := string(output)
		return fmt.Errorf("git pull failed: %w\n%s", err, msg)
	}
	return nil
}

// GetAheadBehind runs `git rev-list --left-right --count HEAD...<upstream>`
// and returns the ahead and behind counts.
// Returns (0, 0, nil) if upstream is empty or the command fails.
func GetAheadBehind(repoPath, upstream string) (ahead, behind int, err error) {
	if upstream == "" {
		return 0, 0, nil
	}

	cmd := exec.Command("git", "rev-list",
		"--left-right",
		"--count",
		fmt.Sprintf("HEAD...%s", upstream),
		"--",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return 0, 0, fmt.Errorf("git rev-list failed: %w", err)
	}

	// Output format: "<ahead>\t<behind>\n"
	parts := strings.Fields(strings.TrimSpace(string(out)))
	if len(parts) != 2 {
		return 0, 0, fmt.Errorf("unexpected rev-list output: %q", string(out))
	}

	ahead, _ = strconv.Atoi(parts[0])
	behind, _ = strconv.Atoi(parts[1])
	return ahead, behind, nil
}

// GetRemote returns the name of the first configured remote (usually "origin").
// Returns an empty string if no remote is configured.
func GetRemote(repoPath string) string {
	cmd := exec.Command("git", "remote")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return ""
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) == 0 || lines[0] == "" {
		return ""
	}
	return lines[0]
}
