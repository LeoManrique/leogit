package components

import (
	"strings"

	"charm.land/bubbles/v2/textarea"
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// ── Messages ────────────────────────────────────────────

// CommitRequestMsg is sent when the user presses ctrl+enter or ctrl+x to commit.
// The app handles the actual git commit.
type CommitRequestMsg struct {
	Summary     string
	Description string
}

// AIGenerateMsg is sent when the user presses ctrl+g to generate a commit message.
type AIGenerateMsg struct{}

// AICycleProviderMsg is sent when the user presses ctrl+p to cycle AI providers.
type AICycleProviderMsg struct{}

// AIResultMsg is sent when AI generation completes (success or error).
type AIResultMsg struct {
	Title       string
	Description string
	Err         error
}

// ── Focus field ─────────────────────────────────────────

type commitField int

const (
	fieldSummary commitField = iota
	fieldDescription
)

// ── Model ───────────────────────────────────────────────

// CommitMsgModel is the commit message pane component (Pane 3 in Changes tab).
// It contains a summary text input and a description textarea.
type CommitMsgModel struct {
	summary     textinput.Model
	description textarea.Model
	activeField commitField

	// AI generation state
	aiLoading  bool   // true while AI is generating
	aiProvider string // display name of active provider (e.g., "Claude", "Ollama")
	aiError    string // last AI error message (cleared on next attempt)

	// Commit state
	commitError string // last commit error message (cleared on next attempt)
	committing  bool   // true while commit is in progress

	width   int
	height  int
	focused bool
}

// CommitResultMsg is sent when the git commit completes (success or error).
type CommitResultMsg struct {
	Err error
}

// NewCommitMsg creates a new commit message component with default settings.
func NewCommitMsg() CommitMsgModel {
	// Summary: single-line input, max 72 chars
	ti := textinput.New()
	ti.Placeholder = "Summary (required)"
	ti.CharLimit = 72
	ti.SetWidth(30) // will be resized by SetSize

	// Description: multi-line textarea
	ta := textarea.New()
	ta.Placeholder = "Description"
	ta.SetWidth(30)
	ta.SetHeight(3) // compact — fits in the sidebar pane
	ta.ShowLineNumbers = false
	ta.CharLimit = 0 // no limit on description length

	return CommitMsgModel{
		summary:     ti,
		description: ta,
		activeField: fieldSummary,
		aiProvider:  "Claude",
	}
}

// SetSize updates the component dimensions.
// Called when the window resizes or the layout recalculates.
func (m *CommitMsgModel) SetSize(width, height int) {
	m.width = width
	m.height = height

	innerW := width - 2 // leave space for padding
	if innerW < 10 {
		innerW = 10
	}
	m.summary.SetWidth(innerW)

	// Description gets remaining height: total - summary(1) - button bar(1) - gaps(1)
	descHeight := height - 3
	if descHeight < 1 {
		descHeight = 1
	}
	m.description.SetWidth(innerW)
	m.description.SetHeight(descHeight)
}

// Focus activates the commit message pane for text input.
func (m *CommitMsgModel) Focus() {
	m.focused = true
	m.activeField = fieldSummary
	m.summary.Focus()
	m.description.Blur()
}

// Blur deactivates the commit message pane.
func (m *CommitMsgModel) Blur() {
	m.focused = false
	m.summary.Blur()
	m.description.Blur()
}

// Summary returns the current summary text.
func (m CommitMsgModel) Summary() string {
	return strings.TrimSpace(m.summary.Value())
}

// Description returns the current description text.
func (m CommitMsgModel) Description() string {
	return strings.TrimSpace(m.description.Value())
}

// IsEmpty returns true if both summary and description are empty.
func (m CommitMsgModel) IsEmpty() bool {
	return m.Summary() == "" && m.Description() == ""
}

// Clear resets both fields to empty and commit state
func (m *CommitMsgModel) Clear() {
	m.summary.SetValue("")
	m.description.SetValue("")
	m.activeField = fieldSummary
	m.aiError = ""
	m.commitError = ""
	m.committing = false
	if m.focused {
		m.summary.Focus()
		m.description.Blur()
	}
}

// SetCommitError records a commit error (validation or git error).
func (m *CommitMsgModel) SetCommitError(errMsg string) {
	m.committing = false
	m.commitError = errMsg
}

// CommitSuccess clears the fields and resets state after a successful commit.
func (m *CommitMsgModel) CommitSuccess() {
	m.committing = false
	m.commitError = ""
	m.Clear()
}

// SetAIResult fills the fields with the AI-generated commit message.
func (m *CommitMsgModel) SetAIResult(title, description string) {
	m.summary.SetValue(title)
	m.description.SetValue(description)
	m.aiLoading = false
	m.aiError = ""
}

// SetAIError records an AI generation error.
func (m *CommitMsgModel) SetAIError(errMsg string) {
	m.aiLoading = false
	m.aiError = errMsg
}

// SetAIProvider updates the displayed provider name.
func (m *CommitMsgModel) SetAIProvider(name string) {
	m.aiProvider = name
}

// ── Update ──────────────────────────────────────────────

// Update handles key messages for the commit message pane.
func (m CommitMsgModel) Update(msg tea.Msg) (CommitMsgModel, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		key := msg.String()

		switch key {
		case "tab":
			// Toggle between summary and description
			if m.activeField == fieldSummary {
				m.activeField = fieldDescription
				m.summary.Blur()
				m.description.Focus()
			} else {
				m.activeField = fieldSummary
				m.description.Blur()
				m.summary.Focus()
			}
			return m, nil

		case "ctrl+g":
			// Trigger AI commit message generation
			if !m.aiLoading {
				m.aiLoading = true
				m.aiError = ""
				return m, func() tea.Msg { return AIGenerateMsg{} }
			}
			return m, nil

		case "ctrl+p":
			// Cycle AI provider
			return m, func() tea.Msg { return AICycleProviderMsg{} }

		case "ctrl+x", "ctrl+enter":
			// Request commit (handled by app)
			if m.committing {
				return m, nil // already committing
			}
			summary := m.Summary()
			if summary == "" {
				m.commitError = "summary is required"
				return m, nil
			}
			m.commitError = ""
			m.committing = true
			return m, func() tea.Msg {
				return CommitRequestMsg{
					Summary:     summary,
					Description: m.Description(),
				}
			}
		}
	}

	// Forward to the active field — any key not caught by the switch above
	// (regular characters, backspace, arrows, etc.) falls through to here
	// and is forwarded to the active Bubbles text component for typing.
	var cmd tea.Cmd
	if m.activeField == fieldSummary {
		m.summary, cmd = m.summary.Update(msg)
	} else {
		m.description, cmd = m.description.Update(msg)
	}
	return m, cmd
}

// ── View ────────────────────────────────────────────────

// View renders the commit message pane.
func (m CommitMsgModel) View() string {
	if m.width < 5 || m.height < 2 {
		return ""
	}

	var sections []string

	// Summary field
	sections = append(sections, m.summary.View())

	// Description field
	sections = append(sections, m.description.View())

	// Button bar: [AI: Provider] [ctrl+g Generate] [ctrl+x or ctrl+enter Commit]
	buttonBar := m.renderButtonBar()
	sections = append(sections, buttonBar)

	return strings.Join(sections, "\n")
}

// renderButtonBar renders the bottom bar with AI provider and action hints.
func (m CommitMsgModel) renderButtonBar() string {
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E"))
	activeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#58A6FF"))
	errorStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#F85149"))
	successStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950"))

	var parts []string

	// AI provider indicator
	providerLabel := dimStyle.Render("AI:") + " " + activeStyle.Render(m.aiProvider)
	parts = append(parts, providerLabel)

	// Status: commit error > committing > AI loading > AI error > hints
	if m.commitError != "" {
		errText := m.commitError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else if m.committing {
		parts = append(parts, successStyle.Render("Committing..."))
	} else if m.aiLoading {
		parts = append(parts, activeStyle.Render("⟳ Generating..."))
	} else if m.aiError != "" {
		errText := m.aiError
		if len(errText) > 30 {
			errText = errText[:27] + "..."
		}
		parts = append(parts, errorStyle.Render(errText))
	} else {
		parts = append(parts, dimStyle.Render("^g:generate ^p:provider"))
	}

	return strings.Join(parts, " ")
}
