package git

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// DiscoverRepos scans the given paths for git repositories up to maxDepth levels deep.
// Returns a sorted list of absolute paths to discovered repos.
func DiscoverRepos(scanPaths []string, maxDepth int) []string {
	var repos []string
	seen := make(map[string]bool)

	for _, scanPath := range scanPaths {
		// Expand tilde to home directory
		expanded := ExpandTilde(scanPath)

		// Resolve to absolute path
		abs, err := filepath.Abs(expanded)
		if err != nil {
			continue
		}

		// Check if the scan path itself exists
		info, err := os.Stat(abs)
		if err != nil || !info.IsDir() {
			continue
		}

		// Walk the directory tree up to maxDepth
		scanForRepos(abs, abs, maxDepth, seen, &repos)
	}

	sort.Strings(repos)
	return repos
}

// scanForRepos recursively searches for git repos starting at dir.
// root is the original scan path, used to calculate current depth.
func scanForRepos(dir, root string, maxDepth int, seen map[string]bool, repos *[]string) {
	// Calculate current depth relative to root
	rel, err := filepath.Rel(root, dir)
	if err != nil {
		return
	}

	depth := 0
	if rel != "." {
		depth = strings.Count(rel, string(filepath.Separator)) + 1
	}

	// Don't go deeper than maxDepth
	if depth > maxDepth {
		return
	}

	// Check if this directory is a git repo (depth > 0 skips the scan root itself)
	if depth > 0 && IsGitRepo(dir) {
		absPath, err := filepath.Abs(dir)
		if err != nil {
			return
		}
		if !seen[absPath] {
			seen[absPath] = true
			*repos = append(*repos, absPath)
		}
		return // Don't scan inside a git repo for nested repos
	}

	// Read directory entries
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}

	for _, entry := range entries {
		name := entry.Name()

		// Skip hidden directories
		if strings.HasPrefix(name, ".") {
			continue
		}

		// Follow symlinks: resolve the entry to check if it's a directory
		fullPath := filepath.Join(dir, name)
		info, err := os.Stat(fullPath) // os.Stat follows symlinks
		if err != nil || !info.IsDir() {
			continue
		}

		scanForRepos(fullPath, root, maxDepth, seen, repos)
	}
}

// ExpandTilde replaces a leading ~ with the user's home directory.
func ExpandTilde(path string) string {
	if !strings.HasPrefix(path, "~") {
		return path
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return path
	}

	if path == "~" {
		return home
	}

	if strings.HasPrefix(path, "~/") {
		return filepath.Join(home, path[2:])
	}

	return path
}
