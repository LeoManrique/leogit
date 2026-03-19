package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// ClaudeProvider generates commit messages using the Claude CLI tool.
type ClaudeProvider struct {
	Model       string        // claude model name (default: "haiku")
	Timeout     time.Duration // subprocess timeout (default: 120s)
	MaxDiffSize int           // max diff size in bytes (default: 20MB)
	binaryPath  string        // cached path to the claude binary
}

// NewClaudeProvider creates a Claude provider with settings from the config.
func NewClaudeProvider(model string, timeoutSecs int, maxDiffSize int) *ClaudeProvider {
	if model == "" {
		model = "haiku"
	}
	if timeoutSecs <= 0 {
		timeoutSecs = 120
	}
	if maxDiffSize <= 0 {
		maxDiffSize = 20_971_520 // 20MB
	}

	return &ClaudeProvider{
		Model:       model,
		Timeout:     time.Duration(timeoutSecs) * time.Second,
		MaxDiffSize: maxDiffSize,
	}
}

func (p *ClaudeProvider) ID() string          { return "claude" }
func (p *ClaudeProvider) DisplayName() string { return "Claude" }

// IsAvailable checks whether the Claude CLI binary exists on the system.
func (p *ClaudeProvider) IsAvailable() (bool, error) {
	path, err := p.findBinary()
	if err != nil {
		return false, nil
	}
	p.binaryPath = path
	return true, nil
}

// GenerateCommitMessage runs the Claude CLI with the selected files' diff and returns
// a parsed commit message.
func (p *ClaudeProvider) GenerateCommitMessage(diff string) (*CommitMessage, error) {
	if strings.TrimSpace(diff) == "" {
		return nil, &AIError{Code: ErrEmptyDiff, Message: "no files selected"}
	}

	if len(diff) > p.MaxDiffSize {
		return nil, &AIError{
			Code:    ErrDiffTooLarge,
			Message: fmt.Sprintf("diff is %d bytes (max %d)", len(diff), p.MaxDiffSize),
		}
	}

	// Ensure we have the binary path
	if p.binaryPath == "" {
		path, err := p.findBinary()
		if err != nil {
			return nil, &AIError{Code: ErrCLIError, Message: "claude CLI not found"}
		}
		p.binaryPath = path
	}

	prompt := BuildPrompt(diff)

	// Run claude CLI with timeout
	ctx, cancel := context.WithTimeout(context.Background(), p.Timeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, p.binaryPath,
		"--print",
		"--output-format", "json",
		"--model", p.Model,
	)
	cmd.Stdin = strings.NewReader(prompt)

	out, err := cmd.Output()
	if ctx.Err() == context.DeadlineExceeded {
		return nil, &AIError{Code: ErrTimeout, Message: "Claude CLI timed out"}
	}
	if err != nil {
		return nil, &AIError{
			Code:    ErrCLIError,
			Message: fmt.Sprintf("claude CLI error: %s", err),
		}
	}

	// Parse the Claude CLI JSON response
	// Format: {"type":"result","result":"<content>"}
	var cliResponse struct {
		Type   string `json:"type"`
		Result string `json:"result"`
	}
	if err := json.Unmarshal(out, &cliResponse); err != nil {
		// Try parsing the raw output directly as a commit message
		return ParseCommitMessage(string(out))
	}

	if cliResponse.Result == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "empty result from Claude CLI",
		}
	}

	return ParseCommitMessage(cliResponse.Result)
}

// findBinary searches for the claude binary in known locations.
// Returns the first path that exists as an executable.
func (p *ClaudeProvider) findBinary() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		home = ""
	}

	// Check well-known install locations first
	candidates := []string{}
	if home != "" {
		candidates = append(candidates,
			filepath.Join(home, ".local", "share", "pnpm", "claude"),
			filepath.Join(home, ".local", "bin", "claude"),
		)
	}
	candidates = append(candidates,
		"/usr/local/bin/claude",
		"/usr/bin/claude",
	)

	for _, path := range candidates {
		if info, err := os.Stat(path); err == nil && !info.IsDir() {
			return path, nil
		}
	}

	// Fallback: ask the user's login shell.
	// Using $SHELL with -l (login) flag loads the user's full PATH from
	// their shell profile (~/.zshrc, ~/.bashrc, etc.), which catches
	// binaries installed in non-standard locations like nvm/homebrew paths
	// that aren't in the default system PATH.
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}
	out, err := exec.Command(shell, "-l", "-c", "which claude").Output()
	if err == nil {
		path := strings.TrimSpace(string(out))
		if path != "" {
			return path, nil
		}
	}

	return "", fmt.Errorf("claude binary not found")
}
