package gh

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// PullRequest holds metadata about a single pull request.
type PullRequest struct {
	Number         int
	Title          string
	Body           string
	Author         string // GitHub login username
	State          string // OPEN, CLOSED, MERGED
	IsDraft        bool
	BaseRefName    string // target branch (e.g., "main")
	HeadRefName    string // source branch (e.g., "feature-x")
	ReviewDecision string // APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, ""
	Additions      int
	Deletions      int
	ChangedFiles   int
	CreatedAt      time.Time
	URL            string
}

// PRCheck holds the status of a single CI check on a PR.
type PRCheck struct {
	Name   string // check name (e.g., "CI", "lint")
	State  string // raw state (e.g., "SUCCESS", "FAILURE")
	Bucket string // normalized: pass, fail, pending, skipping, cancel
}

// ghPRJSON is the intermediate struct for deserializing gh pr list/view JSON.
type ghPRJSON struct {
	Number         int    `json:"number"`
	Title          string `json:"title"`
	Body           string `json:"body"`
	Author         struct {
		Login string `json:"login"`
	} `json:"author"`
	State          string `json:"state"`
	IsDraft        bool   `json:"isDraft"`
	BaseRefName    string `json:"baseRefName"`
	HeadRefName    string `json:"headRefName"`
	ReviewDecision string `json:"reviewDecision"`
	Additions      int    `json:"additions"`
	Deletions      int    `json:"deletions"`
	ChangedFiles   int    `json:"changedFiles"`
	CreatedAt      string `json:"createdAt"`
	URL            string `json:"url"`
}

type ghCheckJSON struct {
	Name   string `json:"name"`
	State  string `json:"state"`
	Bucket string `json:"bucket"`
}

func convertPR(j ghPRJSON) PullRequest {
	created, _ := time.Parse(time.RFC3339, j.CreatedAt)
	return PullRequest{
		Number:         j.Number,
		Title:          j.Title,
		Body:           j.Body,
		Author:         j.Author.Login,
		State:          j.State,
		IsDraft:        j.IsDraft,
		BaseRefName:    j.BaseRefName,
		HeadRefName:    j.HeadRefName,
		ReviewDecision: j.ReviewDecision,
		Additions:      j.Additions,
		Deletions:      j.Deletions,
		ChangedFiles:   j.ChangedFiles,
		CreatedAt:      created,
		URL:            j.URL,
	}
}

// ListPRs returns pull requests for the repository, filtered by state.
func ListPRs(repoPath, state string) ([]PullRequest, error) {
	cmd := exec.Command("gh", "pr", "list",
		"--state", state,
		"--limit", "30",
		"--json", "number,title,body,author,state,isDraft,baseRefName,headRefName,"+
			"reviewDecision,additions,deletions,changedFiles,createdAt,url",
	)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return nil, fmt.Errorf("gh pr list failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return nil, fmt.Errorf("gh pr list failed: %w", err)
	}

	var raw []ghPRJSON
	if err := json.Unmarshal(out, &raw); err != nil {
		return nil, fmt.Errorf("failed to parse PR list JSON: %w", err)
	}

	prs := make([]PullRequest, len(raw))
	for i, r := range raw {
		prs[i] = convertPR(r)
	}
	return prs, nil
}

// GetPRChecks returns the CI check statuses for a specific PR.
func GetPRChecks(repoPath string, number int) ([]PRCheck, error) {
	cmd := exec.Command("gh", "pr", "checks",
		fmt.Sprintf("%d", number),
		"--json", "name,state,bucket",
	)
	cmd.Dir = repoPath

	out, err := cmd.CombinedOutput()

	var raw []ghCheckJSON
	if jsonErr := json.Unmarshal(out, &raw); jsonErr == nil {
		checks := make([]PRCheck, len(raw))
		for i, r := range raw {
			checks[i] = PRCheck{Name: r.Name, State: r.State, Bucket: r.Bucket}
		}
		return checks, nil
	}

	if err != nil {
		return nil, fmt.Errorf("gh pr checks failed: %s", strings.TrimSpace(string(out)))
	}
	return []PRCheck{}, nil
}

// CreatePR creates a new pull request from the current branch.
func CreatePR(repoPath, title, body, base string, draft bool) (string, error) {
	args := []string{"pr", "create", "--title", title, "--body", body}
	if base != "" {
		args = append(args, "--base", base)
	}
	if draft {
		args = append(args, "--draft")
	}

	cmd := exec.Command("gh", args...)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("gh pr create failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return "", fmt.Errorf("gh pr create failed: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CreatePRFill creates a new pull request with title and body auto-filled from commits.
func CreatePRFill(repoPath, base string, draft bool) (string, error) {
	args := []string{"pr", "create", "--fill"}
	if base != "" {
		args = append(args, "--base", base)
	}
	if draft {
		args = append(args, "--draft")
	}

	cmd := exec.Command("gh", args...)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("gh pr create --fill failed: %s", strings.TrimSpace(string(exitErr.Stderr)))
		}
		return "", fmt.Errorf("gh pr create --fill failed: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

// CheckoutPR checks out the branch associated with a pull request.
func CheckoutPR(repoPath string, number int) error {
	cmd := exec.Command("gh", "pr", "checkout", fmt.Sprintf("%d", number))
	cmd.Dir = repoPath

	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("gh pr checkout failed: %w\n%s", err, strings.TrimSpace(string(output)))
	}
	return nil
}

// GetCurrentBranchPR returns the open PR where the given branch is the head (source).
// Returns nil (not an error) if no open PR exists for this branch.
func GetCurrentBranchPR(repoPath, branch string) (*PullRequest, error) {
	if branch == "" {
		return nil, nil
	}

	cmd := exec.Command("gh", "pr", "list",
		"--head", branch,
		"--state", "open",
		"--limit", "1",
		"--json", "number,title,author,state,isDraft,baseRefName,headRefName,"+
			"reviewDecision,additions,deletions,changedFiles,createdAt,url",
	)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err != nil {
		return nil, nil
	}

	var raw []ghPRJSON
	if err := json.Unmarshal(out, &raw); err != nil {
		return nil, nil
	}

	if len(raw) == 0 {
		return nil, nil
	}

	pr := convertPR(raw[0])
	return &pr, nil
}
