package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// PushOptions configures the git push command.
type PushOptions struct {
	Remote         string // remote name (e.g., "origin")
	Branch         string // local branch name to push
	SetUpstream    bool   // --set-upstream: create tracking relationship
	ForceWithLease bool   // --force-with-lease: safe force push
}

// Push runs `git push` with the given options.
// The command is: git push [--progress] [--set-upstream] [--force-with-lease] <remote> <branch>
// Progress output goes to stderr; CombinedOutput captures both stdout and stderr.
func Push(repoPath string, opts PushOptions) error {
	args := []string{"push", "--progress"}

	if opts.SetUpstream {
		args = append(args, "--set-upstream")
	}
	if opts.ForceWithLease {
		args = append(args, "--force-with-lease")
	}

	args = append(args, opts.Remote, opts.Branch)

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	// cmd.Environ() copies the current process's environment variables (PATH, HOME,
	// SSH_AUTH_SOCK, etc.) so git inherits them — without this, git wouldn't find
	// SSH keys or the git binary itself. We then append TERM=dumb which prevents
	// git from using a pager (like `less`) or ANSI color codes in its output.
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git push: %s (%w)", strings.TrimSpace(string(out)), err)
	}
	return nil
}

// GetDefaultRemote returns the name of the first configured remote.
// In most repositories this is "origin". If no remotes are configured,
// an error is returned.
func GetDefaultRemote(repoPath string) (string, error) {
	cmd := exec.Command("git", "remote")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git remote: %w", err)
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) == 0 || lines[0] == "" {
		return "", fmt.Errorf("no remotes configured")
	}
	return lines[0], nil
}

// RemoteFromUpstream extracts the remote name from an upstream tracking ref.
// For example, "origin/main" returns "origin", "upstream/feature" returns "upstream".
// If the format is unexpected, it returns "origin" as a fallback.
func RemoteFromUpstream(upstream string) string {
	if idx := strings.Index(upstream, "/"); idx > 0 {
		return upstream[:idx]
	}
	return "origin"
}
