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

// Load reads the config file and merges it into defaults.
// If the file does not exist, it creates the config directory and writes
// a default config file so the user can find and edit it later.
func Load() (*Config, error) {
	cfg := newDefaultConfig()

	dir, err := configDir()
	if err != nil {
		return nil, err
	}

	path := filepath.Join(dir, "config.toml")

	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			// Create the config directory and write a default config file
			// so the user knows where to find it and can edit it.
			if mkErr := os.MkdirAll(dir, 0o755); mkErr != nil {
				return cfg, nil // silently use defaults if we can't create the dir
			}
			if writeErr := writeDefaultConfig(path, cfg); writeErr != nil {
				return cfg, nil // silently use defaults if we can't write
			}
			return cfg, nil
		}
		return nil, err
	}

	// Unmarshal decodes the TOML data into `cfg`. Because `cfg` already has
	// defaults, only the fields present in the file get overwritten.
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
