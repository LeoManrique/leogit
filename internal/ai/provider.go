package ai

import (
	"encoding/json"
	"fmt"
	"strings"
)

// CommitMessageProvider is the interface that AI commit message generators implement.
type CommitMessageProvider interface {
	// ID returns the provider identifier ("claude" or "ollama").
	ID() string

	// DisplayName returns the human-readable name (e.g., "Claude", "Ollama").
	DisplayName() string

	// IsAvailable checks whether the provider can be used (binary exists, server reachable).
	IsAvailable() (bool, error)

	// GenerateCommitMessage takes a diff of the selected files and returns a commit message.
	// The diff is the combined output of `git diff HEAD` for each selected file.
	GenerateCommitMessage(diff string) (*CommitMessage, error)
}

// CommitMessage holds a generated commit message with title and description.
type CommitMessage struct {
	Title       string `json:"title"`       // ≤50 chars, imperative mood
	Description string `json:"description"` // what changed and why
}

// Error codes for AI generation failures.
const (
	ErrEmptyDiff       = "EMPTY_DIFF"       // no files selected
	ErrDiffTooLarge    = "DIFF_TOO_LARGE"   // exceeds provider's size limit
	ErrTimeout         = "TIMEOUT"          // provider didn't respond in time
	ErrCLIError        = "CLI_ERROR"        // claude CLI non-zero exit
	ErrConnectionError = "CONNECTION_ERROR" // can't reach Ollama server
	ErrModelNotFound   = "MODEL_NOT_FOUND"  // Ollama model not pulled
	ErrAPIError        = "API_ERROR"        // Ollama HTTP 500+
	ErrInvalidResponse = "INVALID_RESPONSE" // unparseable JSON
)

// AIError is a structured error with a code and message.
type AIError struct {
	Code    string
	Message string
}

func (e *AIError) Error() string {
	return fmt.Sprintf("[%s] %s", e.Code, e.Message)
}

// PromptTemplate is the shared prompt used by both providers.
// The placeholder {DIFF} is replaced with the actual diff of selected files.
const PromptTemplate = `You are a Git commit message generator. Analyze the provided git diff
and generate a commit message.

Return ONLY valid JSON in this exact format:
{"title": "≤50 char summary in imperative mood", "description": "what changed and why"}

Rules:
- Title MUST be ≤50 characters, imperative mood ("Add", "Fix", "Update")
- Description explains what and why, not how
- Return ONLY the JSON object

Git diff:
` + "```diff\n{DIFF}\n```"

// BuildPrompt creates the full prompt by inserting the diff into the template.
func BuildPrompt(diff string) string {
	return strings.Replace(PromptTemplate, "{DIFF}", diff, 1)
}

// ParseCommitMessage extracts a CommitMessage from a JSON string.
// Handles field normalization: accepts title/summary/subject/message for title,
// and description/body/details for description.
// Strips markdown code fences if present.
func ParseCommitMessage(raw string) (*CommitMessage, error) {
	// Strip markdown code fences (```json ... ```)
	cleaned := raw
	cleaned = strings.TrimSpace(cleaned)
	if strings.HasPrefix(cleaned, "```") {
		lines := strings.Split(cleaned, "\n")
		// Remove first line (```json) and last line (```)
		// A fenced block needs at least 3 lines: opening fence, one or more
		// content lines, and closing fence. Fewer than 3 means the fences
		// are incomplete, so we leave the string as-is.
		if len(lines) >= 3 {
			cleaned = strings.Join(lines[1:len(lines)-1], "\n")
		}
	}
	cleaned = strings.TrimSpace(cleaned)

	// Parse into a generic map for field normalization
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(cleaned), &parsed); err != nil {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: fmt.Sprintf("failed to parse JSON: %s", err),
		}
	}

	// Normalize title field: accept title, summary, subject, message
	title := ""
	for _, key := range []string{"title", "summary", "subject", "message"} {
		if v, ok := parsed[key]; ok {
			if s, ok := v.(string); ok && s != "" {
				title = s
				break
			}
		}
	}

	// Normalize description field: accept description, body, details
	description := ""
	for _, key := range []string{"description", "body", "details"} {
		if v, ok := parsed[key]; ok {
			if s, ok := v.(string); ok && s != "" {
				description = s
				break
			}
		}
	}

	if title == "" {
		return nil, &AIError{
			Code:    ErrInvalidResponse,
			Message: "no title/summary field found in response",
		}
	}

	// Truncate title to 50 chars if the AI didn't follow instructions
	if len(title) > 50 {
		title = title[:50]
	}

	return &CommitMessage{
		Title:       title,
		Description: description,
	}, nil
}
