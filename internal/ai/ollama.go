package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// OllamaProvider generates commit messages using a local Ollama server.
type OllamaProvider struct {
	Model       string        // model name (default: "tavernari/git-commit-message:latest")
	ServerURL   string        // Ollama server URL (default: "http://localhost:11434")
	Timeout     time.Duration // request timeout (default: 120s)
	MaxDiffSize int           // max diff size in bytes (default: 50MB)
}

// NewOllamaProvider creates an Ollama provider with settings from the config.
func NewOllamaProvider(model, serverURL string, timeoutSecs int, maxDiffSize int) *OllamaProvider {
	if model == "" {
		model = "tavernari/git-commit-message:latest"
	}
	if serverURL == "" {
		serverURL = "http://localhost:11434"
	}
	if timeoutSecs <= 0 {
		timeoutSecs = 120
	}
	if maxDiffSize <= 0 {
		maxDiffSize = 52_428_800 // 50MB
	}

	return &OllamaProvider{
		Model:       model,
		ServerURL:   strings.TrimRight(serverURL, "/"),
		Timeout:     time.Duration(timeoutSecs) * time.Second,
		MaxDiffSize: maxDiffSize,
	}
}

func (p *OllamaProvider) ID() string          { return "ollama" }
func (p *OllamaProvider) DisplayName() string { return "Ollama" }

// IsAvailable checks whether the Ollama server is reachable by hitting GET /api/tags.
func (p *OllamaProvider) IsAvailable() (bool, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "GET", p.ServerURL+"/api/tags", nil)
	if err != nil {
		return false, nil
	}

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return false, nil // server not reachable — not an error, just unavailable
	}
	defer resp.Body.Close()

	return resp.StatusCode == http.StatusOK, nil
}

// GenerateCommitMessage sends the selected files' diff to Ollama and returns a parsed commit message.
func (p *OllamaProvider) GenerateCommitMessage(diff string) (*CommitMessage, error) {
	if strings.TrimSpace(diff) == "" {
		return nil, &AIError{Code: ErrEmptyDiff, Message: "no files selected"}
	}

	if len(diff) > p.MaxDiffSize {
		return nil, &AIError{
			Code:    ErrDiffTooLarge,
			Message: fmt.Sprintf("diff is %d bytes (max %d)", len(diff), p.MaxDiffSize),
		}
	}

	prompt := BuildPrompt(diff)

	// Build the request body
	reqBody := ollamaRequest{
		Model:  p.Model,
		Prompt: prompt,
		Stream: false,
		Format: "json",
	}

	bodyBytes, err := json.Marshal(reqBody)
	if err != nil {
		return nil, fmt.Errorf("marshaling request: %w", err)
	}

	// Send POST /api/generate with timeout
	ctx, cancel := context.WithTimeout(context.Background(), p.Timeout)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "POST",
		p.ServerURL+"/api/generate",
		bytes.NewReader(bodyBytes),
	)
	if err != nil {
		return nil, &AIError{Code: ErrConnectionError, Message: err.Error()}
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return nil, &AIError{Code: ErrTimeout, Message: "Ollama request timed out"}
		}
		return nil, &AIError{Code: ErrConnectionError, Message: err.Error()}
	}
	defer resp.Body.Close()

	// Read response body
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, &AIError{Code: ErrAPIError, Message: "reading response: " + err.Error()}
	}

	// Check for HTTP errors
	if resp.StatusCode != http.StatusOK {
		// Check for model not found (404)
		if resp.StatusCode == http.StatusNotFound {
			return nil, &AIError{
				Code:    ErrModelNotFound,
				Message: fmt.Sprintf("model %q not found — run: ollama pull %s", p.Model, p.Model),
			}
		}
		return nil, &AIError{
			Code:    ErrAPIError,
			Message: fmt.Sprintf("HTTP %d: %s", resp.StatusCode, string(respBody)),
		}
	}

	// Parse the Ollama response: {"response": "<content>", ...}
	var ollamaResp ollamaResponse
	if err := json.Unmarshal(respBody, &ollamaResp); err != nil {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: fmt.Sprintf("parsing Ollama response: %s", err),
		}
	}

	if ollamaResp.Response == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "empty response from Ollama",
		}
	}

	return ParseCommitMessage(ollamaResp.Response)
}

// ── Request/Response types ──────────────────────────────

type ollamaRequest struct {
	Model  string `json:"model"`
	Prompt string `json:"prompt"`
	Stream bool   `json:"stream"`
	Format string `json:"format"`
}

type ollamaResponse struct {
	Response string `json:"response"`
}
