package git

import (
	"fmt"
	"os/exec"
	"strings"
)

// StageFiles stages the given files using git update-index.
// Called at commit time to stage only the files the user has selected.
// Files are grouped by type (renamed old paths, normal files, deleted files)
// and processed in three passes with appropriate flags.
//
// The -z flag tells git to read NUL-terminated paths from stdin, which
// handles filenames with spaces or special characters correctly.
func StageFiles(repoPath string, files []FileEntry) error {
	var renamed []string // old paths of renamed files (need --force-remove)
	var normal []string  // normal modified/added files
	var deleted []string // deleted files (need --force-remove)

	for _, f := range files {
		switch {
		case f.Status == StatusRenamed && f.OrigPath != "":
			renamed = append(renamed, f.OrigPath)
			normal = append(normal, f.Path) // new path goes in normal batch
		case f.Status == StatusDeleted:
			deleted = append(deleted, f.Path)
		default:
			normal = append(normal, f.Path)
		}
	}

	// Pass 1: renamed old paths (--force-remove to remove the old index entry)
	if len(renamed) > 0 {
		if err := updateIndex(repoPath, renamed, true); err != nil {
			return fmt.Errorf("staging renamed files: %w", err)
		}
	}

	// Pass 2: normal files (new paths, modified, untracked)
	if len(normal) > 0 {
		if err := updateIndex(repoPath, normal, false); err != nil {
			return fmt.Errorf("staging files: %w", err)
		}
	}

	// Pass 3: deleted files (--force-remove to remove from index)
	if len(deleted) > 0 {
		if err := updateIndex(repoPath, deleted, true); err != nil {
			return fmt.Errorf("staging deleted files: %w", err)
		}
	}

	return nil
}

// updateIndex runs git update-index with -z --stdin, writing paths as NUL-terminated
// input. If forceRemove is true, adds --force-remove (needed for renames and deletes).
func updateIndex(repoPath string, paths []string, forceRemove bool) error {
	args := []string{"update-index", "--add", "--remove", "--replace", "-z", "--stdin"}
	if forceRemove {
		args = []string{"update-index", "--add", "--remove", "--force-remove", "--replace", "-z", "--stdin"}
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath

	// Write NUL-terminated paths to stdin
	input := strings.Join(paths, "\x00") + "\x00"
	cmd.Stdin = strings.NewReader(input)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s: %s", err, string(out))
	}
	return nil
}

// UnstageFiles removes the given files from the staging area using git reset HEAD.
// This works for all file types — it resets the index entry to match HEAD.
func UnstageFiles(repoPath string, files []FileEntry) error {
	if len(files) == 0 {
		return nil
	}

	args := []string{"reset", "HEAD", "--"}
	for _, f := range files {
		args = append(args, f.Path)
	}

	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("unstaging files: %s: %s", err, string(out))
	}
	return nil
}

// StageFile stages a single file (convenience wrapper around StageFiles).
func StageFile(repoPath string, file FileEntry) error {
	return StageFiles(repoPath, []FileEntry{file})
}

// UnstageFile unstages a single file (convenience wrapper around UnstageFiles).
func UnstageFile(repoPath string, file FileEntry) error {
	return UnstageFiles(repoPath, []FileEntry{file})
}

// ApplyPatchToIndex stages a partial patch using git apply --cached.
// The patch is piped to stdin. Flags:
//   - --cached: apply to the index (staging area), not the working tree
//   - --unidiff-zero: allow hunks with zero context lines (needed for single-line patches)
//   - --whitespace=nowarn: suppress whitespace warnings that would cause failures
func ApplyPatchToIndex(repoPath, patch string) error {
	if patch == "" {
		return nil
	}

	cmd := exec.Command("git",
		"apply",
		"--cached",
		"--unidiff-zero",
		"--whitespace=nowarn",
		"-",
	)
	cmd.Dir = repoPath
	cmd.Stdin = strings.NewReader(patch)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git apply --cached: %s: %s", err, string(out))
	}
	return nil
}

// ApplyPatchToWorkingTree applies a patch to the working tree (for discarding changes).
// Same as ApplyPatchToIndex but without --cached.
func ApplyPatchToWorkingTree(repoPath, patch string) error {
	if patch == "" {
		return nil
	}

	cmd := exec.Command("git",
		"apply",
		"--unidiff-zero",
		"--whitespace=nowarn",
		"-",
	)
	cmd.Dir = repoPath
	cmd.Stdin = strings.NewReader(patch)

	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("git apply: %s: %s", err, string(out))
	}
	return nil
}
