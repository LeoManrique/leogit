package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"time"
)

// ReposState is the top-level structure of repos-state.json.
type ReposState struct {
	LastOpened string               `json:"last_opened"`
	Repos      map[string]RepoState `json:"repos"`
}

// RepoState tracks per-repo persistent state.
type RepoState struct {
	LastOpenedAt time.Time `json:"last_opened_at"`
	LastBranch   string    `json:"last_branch"`
}

// statePath returns the path to repos-state.json.
// Uses configDir() from config.go (same package) to get the OS-appropriate directory.
func statePath() (string, error) {
	dir, err := configDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "repos-state.json"), nil
}

// LoadState reads repos-state.json. If the file does not exist, returns an empty state.
func LoadState() (*ReposState, error) {
	path, err := statePath()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return &ReposState{Repos: make(map[string]RepoState)}, nil
		}
		return nil, err
	}

	var state ReposState
	if err := json.Unmarshal(data, &state); err != nil {
		// If the file is corrupt, start fresh rather than crashing
		return &ReposState{Repos: make(map[string]RepoState)}, nil
	}

	if state.Repos == nil {
		state.Repos = make(map[string]RepoState)
	}

	return &state, nil
}

// SaveState writes repos-state.json. Creates the config directory if needed.
func SaveState(state *ReposState) error {
	path, err := statePath()
	if err != nil {
		return err
	}

	// Ensure the config directory exists
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}

	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(path, data, 0o644)
}

// SetLastOpened updates the state to record that a repo was opened now.
func (s *ReposState) SetLastOpened(repoPath string) {
	s.LastOpened = repoPath
	s.Repos[repoPath] = RepoState{
		LastOpenedAt: time.Now().UTC(),
		LastBranch:   s.Repos[repoPath].LastBranch, // preserve existing branch if any
	}
}
