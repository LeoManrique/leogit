package config

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

type Config struct {
	Appearance    AppearanceConfig    `toml:"appearance"`
	Diff          DiffConfig          `toml:"diff"`
	AI            AIConfig            `toml:"ai"`
	Git           GitConfig           `toml:"git"`
	Confirmations ConfirmationsConfig `toml:"confirmations"`
	Repos         ReposConfig         `toml:"repos"`
}

type AppearanceConfig struct {
	Theme string `toml:"theme"`
}

type DiffConfig struct {
	SideBySide     bool `toml:"side_by_side"`
	HideWhitespace bool `toml:"hide_whitespace"`
	TabSize        int  `toml:"tab_size"`
	ContextLines   int  `toml:"context_lines"`
}

type AIConfig struct {
	Claude ClaudeConfig `toml:"claude"`
	Ollama OllamaConfig `toml:"ollama"`
}

type ClaudeConfig struct {
	Model       string `toml:"model"`
	Timeout     int    `toml:"timeout"`
	MaxDiffSize int    `toml:"max_diff_size"`
}

type OllamaConfig struct {
	Model       string `toml:"model"`
	ServerURL   string `toml:"server_url"`
	Timeout     int    `toml:"timeout"`
	MaxDiffSize int    `toml:"max_diff_size"`
}

type GitConfig struct {
	FetchInterval int `toml:"fetch_interval"`
}

type ConfirmationsConfig struct {
	DiscardChanges bool `toml:"discard_changes"`
	ForcePush      bool `toml:"force_push"`
	BranchDelete   bool `toml:"branch_delete"`
}

type ReposConfig struct {
	Mode        string   `toml:"mode"`
	ScanPaths   []string `toml:"scan_paths"`
	ScanDepth   int      `toml:"scan_depth"`
	ManualPaths []string `toml:"manual_paths"`
}

// newDefaultConfig returns a Config with every field set to its default value.
// `*Config` means "pointer to Config" -- the `&` below creates the struct and
// returns a pointer to it, so callers share the same instance (not a copy).
func newDefaultConfig() *Config {
	return &Config{
		Appearance: AppearanceConfig{
			Theme: "dark",
		},
		Diff: DiffConfig{
			SideBySide:     false,
			HideWhitespace: false,
			TabSize:        4,
			ContextLines:   3,
		},
		AI: AIConfig{
			Claude: ClaudeConfig{
				Model:       "sonnet",
				Timeout:     120,
				MaxDiffSize: 20_971_520, // 20MB
			},
			Ollama: OllamaConfig{
				Model:       "qwen2.5-coder",
				ServerURL:   "http://localhost:11434",
				Timeout:     120,
				MaxDiffSize: 52_428_800, // 50MB
			},
		},
		Git: GitConfig{
			FetchInterval: 300,
		},
		Confirmations: ConfirmationsConfig{
			DiscardChanges: true,
			ForcePush:      true,
			BranchDelete:   true,
		},
		Repos: ReposConfig{
			Mode:      "folders",
			ScanDepth: 1,
		},
	}
}

// configDir returns the leogit config directory using the OS-appropriate
// location: ~/Library/Application Support/leogit on macOS,
// %APPDATA%\leogit on Windows, $XDG_CONFIG_HOME/leogit on Linux.
func configDir() (string, error) {
	base, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, "leogit"), nil
}

// Path returns the config file path using the OS-appropriate directory
// (via configDir() defined in config.go — same package).
func Path() (string, error) {
	dir, err := configDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "config.toml"), nil
}

// Save writes the current config to the TOML file.
// Creates the parent directory if it doesn't exist.
func Save(cfg *Config) error {
	path, err := Path()
	if err != nil {
		return err
	}

	// Ensure the directory exists
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}

	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()

	encoder := toml.NewEncoder(f)
	return encoder.Encode(cfg)
}

func Load() (*Config, error) {
	cfg := newDefaultConfig()

	path, err := Path()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			// Create directory and default config
			dir := filepath.Dir(path)
			if mkErr := os.MkdirAll(dir, 0o755); mkErr != nil {
				return cfg, nil
			}
			if writeErr := writeDefaultConfig(path, cfg); writeErr != nil {
				return cfg, nil
			}
			return cfg, nil
		}
		return nil, err
	}

	if err := toml.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	return cfg, nil
}

// writeDefaultConfig writes a commented default config file so users
// can discover all available options without reading documentation.
func writeDefaultConfig(path string, cfg *Config) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return toml.NewEncoder(f).Encode(cfg)
}
