package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// BranchInfo holds metadata about a single branch.
type BranchInfo struct {
	Name      string // short name (e.g., "main", "feature-x")
	IsRemote  bool   // true for remote-tracking branches (e.g., "origin/main")
	IsCurrent bool   // true if this is the currently checked-out branch
}

// ListBranches returns all local and remote branches, sorted by most recent commit.
// The current branch is marked with IsCurrent = true.
// Remote branches have IsRemote = true and names like "origin/main".
func ListBranches(repoPath string) ([]BranchInfo, error) {
	// Get the current branch name first
	currentCmd := exec.Command("git", "branch", "--show-current")
	currentCmd.Dir = repoPath
	currentCmd.Env = append(currentCmd.Environ(), "TERM=dumb")
	currentOut, _ := currentCmd.Output()
	currentBranch := strings.TrimSpace(string(currentOut))

	// List all branches (local + remote) sorted by most recent commit
	cmd := exec.Command("git", "branch",
		"--all",
		"--format=%(refname:short)",
		"--sort=-committerdate",
	)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git branch --all failed: %w", err)
	}

	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	var branches []BranchInfo

	seen := make(map[string]bool)

	for _, line := range lines {
		name := strings.TrimSpace(line)
		if name == "" {
			continue
		}

		// Skip HEAD pointer entries like "origin/HEAD -> origin/main"
		if strings.Contains(name, "->") {
			continue
		}

		isRemote := strings.Contains(name, "/")

		if isRemote {
			parts := strings.SplitN(name, "/", 2)
			if len(parts) == 2 && seen[parts[1]] {
				continue
			}
		} else {
			seen[name] = true
		}

		branches = append(branches, BranchInfo{
			Name:      name,
			IsRemote:  isRemote,
			IsCurrent: !isRemote && name == currentBranch,
		})
	}

	return branches, nil
}

// CreateBranch creates a new branch at the given start point (or HEAD if empty).
// Does NOT switch to the new branch.
func CreateBranch(repoPath, name, startPoint string) error {
	args := []string{"branch", name}
	if startPoint != "" {
		args = append(args, startPoint)
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch failed: %w\n%s", err, string(output))
	}
	return nil
}

// SwitchBranch checks out the given branch.
// For remote branches (e.g., "origin/feature-x"), git auto-creates a local tracking branch.
func SwitchBranch(repoPath, branch string) error {
	cmd := exec.Command("git", "checkout", branch, "--")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git checkout failed: %w\n%s", err, string(output))
	}
	return nil
}

// DeleteBranch force-deletes a local branch.
// Returns an error if you try to delete the currently checked-out branch.
func DeleteBranch(repoPath, name string) error {
	cmd := exec.Command("git", "branch", "-D", name)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch -D failed: %w\n%s", err, string(output))
	}
	return nil
}

// DeleteRemoteBranch deletes a branch on the remote.
// The branch parameter should be the short name (e.g., "feature-x"), not "origin/feature-x".
func DeleteRemoteBranch(repoPath, remote, branch string) error {
	cmd := exec.Command("git", "push", remote, ":"+branch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git push delete failed: %w\n%s", err, string(output))
	}
	return nil
}

// RenameBranch renames a branch from oldName to newName.
// If oldName is empty, renames the currently checked-out branch.
func RenameBranch(repoPath, oldName, newName string) error {
	args := []string{"branch", "-m"}
	if oldName != "" {
		args = append(args, oldName)
	}
	args = append(args, newName)

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git branch -m failed: %w\n%s", err, string(output))
	}
	return nil
}
