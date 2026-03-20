package git

import (
	"os"
	"path/filepath"
)

// IsGitRepo checks if the given path is a git repository.
// It looks for a .git directory or file (worktrees use a .git file).
func IsGitRepo(path string) bool {
	info, err := os.Stat(filepath.Join(path, ".git"))
	if err != nil {
		return false
	}
	// .git can be a directory (normal repo) or a file (worktree/submodule)
	return info.IsDir() || info.Mode().IsRegular()
}

// RepoName extracts a display name from a repo path.
// It returns the last component of the path (e.g., "/home/leo/Dev/my-project" → "my-project").
func RepoName(path string) string {
	return filepath.Base(path)
}
