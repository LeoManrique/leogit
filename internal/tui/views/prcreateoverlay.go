package views

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// ── PR Create messages ──────────────────────────────────

// PRCreateMsg is sent when the user confirms the PR creation form.
type PRCreateMsg struct {
	Title  string
	Body   string
	Base   string
	Draft  bool
	UseFill bool // true to use --fill instead of explicit title/body
}

// PRCreateCancelMsg is sent when the user cancels the PR creation form.
type PRCreateCancelMsg struct{}

// ── PR Create field focus ───────────────────────────────

type prCreateField int

const (
	prFieldTitle prCreateField = iota
	prFieldBody
	prFieldBase
	prFieldDraft
	prFieldSubmit
	prFieldFill
	prFieldCancel
)

// ── Model ───────────────────────────────────────────────

// PRCreateOverlayModel is a form overlay for creating a new pull request.
type PRCreateOverlayModel struct {
	Visible     bool
	title       string
	body        string
	base        string
	draft       bool
	activeField prCreateField
	width       int
	height      int
}

// NewPRCreateOverlay creates a hidden PR create overlay.
func NewPRCreateOverlay() PRCreateOverlayModel {
	return PRCreateOverlayModel{}
}

// Show opens the PR create form with a suggested base branch.
func (m *PRCreateOverlayModel) Show(baseBranch string, width, height int) {
	m.Visible = true
	m.title = ""
	m.body = ""
	m.base = baseBranch
	m.draft = false
	m.activeField = prFieldTitle
	m.width = width
	m.height = height
}

// Update handles key events for the PR create form.
func (m PRCreateOverlayModel) Update(msg tea.Msg) (PRCreateOverlayModel, tea.Cmd) {
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
		case "esc":
			m.Visible = false
			return m, func() tea.Msg { return PRCreateCancelMsg{} }

		case "tab":
			// Cycle through fields
			m.activeField = (m.activeField + 1) % 7
			return m, nil

		case "shift+tab":
			if m.activeField == 0 {
				m.activeField = prFieldCancel
			} else {
				m.activeField--
			}
			return m, nil

		case "enter":
			switch m.activeField {
			case prFieldDraft:
				m.draft = !m.draft
				return m, nil
			case prFieldSubmit:
				m.Visible = false
				return m, func() tea.Msg {
					return PRCreateMsg{
						Title: m.title,
						Body:  m.body,
						Base:  m.base,
						Draft: m.draft,
					}
				}
			case prFieldFill:
				m.Visible = false
				return m, func() tea.Msg {
					return PRCreateMsg{
						Base:    m.base,
						Draft:   m.draft,
						UseFill: true,
					}
				}
			case prFieldCancel:
				m.Visible = false
				return m, func() tea.Msg { return PRCreateCancelMsg{} }
			}
			return m, nil

		case "backspace":
			switch m.activeField {
			case prFieldTitle:
				if len(m.title) > 0 {
					m.title = m.title[:len(m.title)-1]
				}
			case prFieldBody:
				if len(m.body) > 0 {
					m.body = m.body[:len(m.body)-1]
				}
			case prFieldBase:
				if len(m.base) > 0 {
					m.base = m.base[:len(m.base)-1]
				}
			}
			return m, nil

		case "space":
			if m.activeField == prFieldDraft {
				m.draft = !m.draft
				return m, nil
			}
			// Fall through to default for text fields
			switch m.activeField {
			case prFieldTitle:
				m.title += " "
			case prFieldBody:
				m.body += " "
			case prFieldBase:
				m.base += " "
			}
			return m, nil

		default:
			key := msg.String()
			if len(key) == 1 {
				switch m.activeField {
				case prFieldTitle:
					m.title += key
				case prFieldBody:
					m.body += key
				case prFieldBase:
					m.base += key
				}
			}
			return m, nil
		}
	}

	return m, nil
}

// View renders the PR create form.
func (m PRCreateOverlayModel) View() string {
	if !m.Visible {
		return ""
	}

	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#58A6FF"))

	labelStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Width(12)

	inputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#C9D1D9"))

	activeInputStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FF7B72")).
		Bold(true)

	btnActive := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FFFFFF")).
		Background(lipgloss.Color("#238636")).
		Padding(0, 2)

	btnInactive := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#8B949E")).
		Background(lipgloss.Color("#21262D")).
		Padding(0, 2)

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#484F58")).
		Align(lipgloss.Center)

	renderField := func(label, value string, field prCreateField) string {
		display := value
		if field == m.activeField {
			display = activeInputStyle.Render(value + "█")
		} else {
			display = inputStyle.Render(value)
		}
		return labelStyle.Render(label) + display
	}

	var rows []string
	rows = append(rows, titleStyle.Render("Create Pull Request"))
	rows = append(rows, "")
	rows = append(rows, renderField("Title:", m.title, prFieldTitle))
	rows = append(rows, renderField("Body:", m.body, prFieldBody))
	rows = append(rows, renderField("Base:", m.base, prFieldBase))

	// Draft toggle
	draftLabel := "[ ] Draft"
	if m.draft {
		draftLabel = "[✓] Draft"
	}
	if m.activeField == prFieldDraft {
		rows = append(rows, labelStyle.Render("")+activeInputStyle.Render(draftLabel))
	} else {
		rows = append(rows, labelStyle.Render("")+inputStyle.Render(draftLabel))
	}

	rows = append(rows, "")

	// Buttons
	var btns []string
	if m.activeField == prFieldSubmit {
		btns = append(btns, btnActive.Render("Create"))
	} else {
		btns = append(btns, btnInactive.Render("Create"))
	}
	if m.activeField == prFieldFill {
		btns = append(btns, btnActive.Render("Auto-fill"))
	} else {
		btns = append(btns, btnInactive.Render("Auto-fill"))
	}
	if m.activeField == prFieldCancel {
		btns = append(btns, lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#FFFFFF")).
			Background(lipgloss.Color("#DA3633")).
			Padding(0, 2).Render("Cancel"))
	} else {
		btns = append(btns, btnInactive.Render("Cancel"))
	}
	rows = append(rows, strings.Join(btns, "  "))

	rows = append(rows, "")
	rows = append(rows, hintStyle.Render("Tab: next field • Enter: confirm • Esc: cancel"))

	content := strings.Join(rows, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#58A6FF")).
		Padding(1, 3).
		Width(min(70, m.width-4)).
		Render(content)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}
