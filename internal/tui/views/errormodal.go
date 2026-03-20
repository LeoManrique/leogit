package views

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// ErrorDismissedMsg is sent when the user dismisses the error modal.
// This is a Bubbletea "message" — a plain struct used as a signal. When the modal
// is dismissed, it returns a tea.Cmd (a function that returns this message), which
// Bubbletea will execute and deliver back to the root model's Update method.
type ErrorDismissedMsg struct{}

// ErrorModalModel is a centered modal that displays errors.
type ErrorModalModel struct {
	Title     string  // e.g., "Git Error", "Network Error"
	Message   string  // the error message
	Retryable bool    // true = show Retry + Dismiss, false = show OK only
	RetryCmd  tea.Cmd // command to run when user clicks Retry
	Visible   bool

	retryFocused bool // which button is highlighted (true = Retry)
	width        int
	height       int
}

// Update handles input when the modal is visible.
func (m ErrorModalModel) Update(msg tea.Msg) (ErrorModalModel, tea.Cmd) {
	if !m.Visible {
		return m, nil
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil

	case tea.KeyPressMsg:
		switch msg.String() {
		case "escape", "q":
			m.Visible = false
			return m, func() tea.Msg { return ErrorDismissedMsg{} }

		case "enter":
			if m.Retryable && m.retryFocused && m.RetryCmd != nil {
				m.Visible = false
				return m, m.RetryCmd
			}
			m.Visible = false
			return m, func() tea.Msg { return ErrorDismissedMsg{} }

		case "tab":
			if m.Retryable {
				m.retryFocused = !m.retryFocused
			}
			return m, nil
		}
	}

	return m, nil
}

// View renders the error modal centered on screen.
func (m ErrorModalModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#F85149")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9")).
		Align(lipgloss.Center)

	activeBtnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#58A6FF")).
		Padding(0, 2)

	inactiveBtnStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(lipgloss.Color("#21262D")).
		Padding(0, 2)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	title := titleStyle.Render(m.Title)
	message := messageStyle.Render(m.Message)

	var buttons string
	if m.Retryable {
		retryBtn := inactiveBtnStyle.Render("Retry")
		dismissBtn := inactiveBtnStyle.Render("Dismiss")
		if m.retryFocused {
			retryBtn = activeBtnStyle.Render("Retry")
		} else {
			dismissBtn = activeBtnStyle.Render("Dismiss")
		}
		buttons = retryBtn + "  " + dismissBtn
	} else {
		buttons = activeBtnStyle.Render("OK")
	}

	hint := hintStyle.Render("Enter to confirm • Esc to dismiss")

	content := strings.Join([]string{title, "", message, "", buttons, "", hint}, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#F85149")).
		Padding(1, 3).
		Align(lipgloss.Center).
		MaxWidth(60)

	box := boxStyle.Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// ShowError creates a visible error modal ready to display.
func ShowError(title, message string, retryable bool, retryCmd tea.Cmd, width, height int) ErrorModalModel {
	return ErrorModalModel{
		Title:        title,
		Message:      message,
		Retryable:    retryable,
		RetryCmd:     retryCmd,
		Visible:      true,
		retryFocused: true,
		width:        width,
		height:       height,
	}
}
