package git

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// MergeResult describes the outcome of a merge operation.
type MergeResult struct {
	Success      bool     // true if the merge completed without conflicts
	FastForward  bool     // true if the merge was a fast-forward (no merge commit created)
	Conflicts    []string // paths of conflicted files (empty on success)
	ErrorMessage string   // raw error output from git (empty on success)
}

// MergeBranch merges the given branch into the current branch using --no-edit.
func MergeBranch(repoPath, branch string) MergeResult {
	cmd := exec.Command("git", "merge", "--no-edit", branch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	outStr := string(output)

	if err == nil {
		ff := strings.Contains(outStr, "Fast-forward") ||
			strings.Contains(outStr, "fast-forward")
		return MergeResult{Success: true, FastForward: ff}
	}

	// Check for conflicts
	if strings.Contains(outStr, "CONFLICT") || strings.Contains(outStr, "Automatic merge failed") {
		var conflicts []string
		for _, line := range strings.Split(outStr, "\n") {
			if strings.Contains(line, "Merge conflict in ") {
				idx := strings.Index(line, "Merge conflict in ")
				if idx >= 0 {
					path := strings.TrimSpace(line[idx+len("Merge conflict in "):])
					if path != "" {
						conflicts = append(conflicts, path)
					}
				}
			}
		}
		return MergeResult{
			Success:      false,
			Conflicts:    conflicts,
			ErrorMessage: outStr,
		}
	}

	return MergeResult{
		Success:      false,
		ErrorMessage: outStr,
	}
}

// MergeSquash performs a squash merge of the given branch.
func MergeSquash(repoPath, branch string) MergeResult {
	cmd := exec.Command("git", "merge", "--squash", branch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	outStr := string(output)

	if err == nil {
		return MergeResult{Success: true}
	}

	if strings.Contains(outStr, "CONFLICT") || strings.Contains(outStr, "Automatic merge failed") {
		var conflicts []string
		for _, line := range strings.Split(outStr, "\n") {
			if strings.Contains(line, "Merge conflict in ") {
				idx := strings.Index(line, "Merge conflict in ")
				if idx >= 0 {
					path := strings.TrimSpace(line[idx+len("Merge conflict in "):])
					if path != "" {
						conflicts = append(conflicts, path)
					}
				}
			}
		}
		return MergeResult{
			Success:      false,
			Conflicts:    conflicts,
			ErrorMessage: outStr,
		}
	}

	return MergeResult{
		Success:      false,
		ErrorMessage: outStr,
	}
}

// CommitSquashMerge finalizes a squash merge by committing the staged changes.
func CommitSquashMerge(repoPath string) error {
	cmd := exec.Command("git", "commit", "--no-edit", "--cleanup=strip")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git commit failed: %w\n%s", err, string(output))
	}
	return nil
}

// MergeAbort cancels an in-progress merge.
func MergeAbort(repoPath string) error {
	cmd := exec.Command("git", "merge", "--abort")
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git merge --abort failed: %w\n%s", err, string(output))
	}
	return nil
}

// IsMerging returns true if a merge is currently in progress.
func IsMerging(repoPath string) bool {
	gitDir := filepath.Join(repoPath, ".git")
	info, err := os.Stat(gitDir)
	if err != nil {
		return false
	}

	var mergeHeadPath string
	if info.IsDir() {
		mergeHeadPath = filepath.Join(gitDir, "MERGE_HEAD")
	} else {
		data, err := os.ReadFile(gitDir)
		if err != nil {
			return false
		}
		actualDir := strings.TrimSpace(strings.TrimPrefix(string(data), "gitdir: "))
		mergeHeadPath = filepath.Join(actualDir, "MERGE_HEAD")
	}

	_, err = os.Stat(mergeHeadPath)
	return err == nil
}

// GetMergeBase returns the SHA of the best common ancestor between two commits.
func GetMergeBase(repoPath, commitA, commitB string) (string, error) {
	cmd := exec.Command("git", "merge-base", commitA, commitB)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git merge-base: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CountCommitsToMerge returns the number of commits in targetBranch that are
// not in the current branch.
func CountCommitsToMerge(repoPath, targetBranch string) (int, error) {
	base, err := GetMergeBase(repoPath, "HEAD", targetBranch)
	if err != nil {
		return 0, err
	}

	cmd := exec.Command("git", "rev-list", "--count", base+".."+targetBranch)
	cmd.Dir = repoPath
	cmd.Env = append(cmd.Environ(), "TERM=dumb")

	out, err := cmd.Output()
	if err != nil {
		return 0, fmt.Errorf("git rev-list --count: %w", err)
	}

	count, err := strconv.Atoi(strings.TrimSpace(string(out)))
	if err != nil {
		return 0, fmt.Errorf("parse commit count: %w", err)
	}
	return count, nil
}
