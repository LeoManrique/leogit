package app

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/core"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/layout"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

type authResultMsg struct{ authenticated bool }
type repoResolvedMsg struct{ path string }
type reposDiscoveredMsg struct{ repos []string }

// ── Commands ────────────────────────────────────────────

func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}
		return repoResolvedMsg{path: ""}
	}
}

func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string
		switch cfg.Repos.Mode {
		case "manual":
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default:
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}
		return reposDiscoveredMsg{repos: repos}
	}
}

// ── App State ───────────────────────────────────────────

type appState int

const (
	stateAuthChecking appState = iota
	stateAuthBlocked
	stateResolvingRepo
	stateDiscoveringRepos
	stateRepoPicker
	stateMain
)

// ── Model ───────────────────────────────────────────────

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config   *config.Config
	cliPath  string // original CLI arg (may be empty)
	repoPath string // resolved repo path (set once a repo is chosen)
	width    int
	height   int
	quitting bool

	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel

	// Layout & focus
	activeTab      core.Tab
	activePane     core.Pane
	focusMode      core.FocusMode
	terminalOpen   bool
	terminalHeight int
	showHelp       bool
	errorModal     views.ErrorModalModel
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// ── Update ──────────────────────────────────────────────

// Update handles all incoming messages.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to picker if active
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		// Keep error modal dimensions in sync
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			return m, nil
		}
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		m.repoPicker, _ = m.repoPicker.Update(tea.WindowSizeMsg{
			Width: m.width, Height: m.height,
		})
		m.state = stateRepoPicker
		return m, nil

	case views.RepoSelectedMsg:
		m.repoPath = msg.Path
		m.state = stateMain
		m.saveRepoState()
		return m, nil

	case views.ErrorDismissedMsg:
		// Modal already hidden by the modal's own Update — nothing to do
		return m, nil

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// ── Key Handling ────────────────────────────────────────

// handleKey processes key messages based on current state.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// Ctrl+C always quits, regardless of state or focus mode
	if msg.String() == "ctrl+c" {
		m.quitting = true
		return m, tea.Quit
	}

	switch m.state {

	case stateAuthBlocked:
		// Any key → re-check auth
		if !m.authChecking {
			m.authChecking = true
			return m, checkAuthCmd
		}
		return m, nil

	case stateRepoPicker:
		var cmd tea.Cmd
		m.repoPicker, cmd = m.repoPicker.Update(msg)
		return m, cmd

	case stateMain:
		return m.handleMainKey(msg)
	}

	return m, nil
}

// handleMainKey processes keys when the app is in the main layout state.
// Priority: error modal → help overlay → focused mode → navigable keybindings.
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "escape" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "escape" {
			m.focusMode = core.Navigable
			return m, nil
		}
		// TODO: forward to active pane (commit msg, terminal)
		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1 // reset to first pane on tab switch
		return m, nil

	case "1":
		m.activePane = core.Pane1
		return m, nil

	case "2":
		m.activePane = core.Pane2
		return m, nil

	case "3":
		m.activePane = core.Pane3
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
		} else if m.activePane == core.PaneTerminal {
			m.activePane = core.Pane1
		}
		return m, nil

	case "escape":
		// Already navigable — Esc is a no-op
		return m, nil
	}

	return m, nil
}

// ── State Persistence ───────────────────────────────────

// saveRepoState records the opened repo in repos-state.json (best-effort).
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state)
}

// ── View ────────────────────────────────────────────────

// View renders the current state to the terminal.
//
// Returns tea.View (Bubbletea v2 API). The View struct lets us
// declaratively set terminal features like AltScreen and MouseMode.
func (m Model) View() tea.View {
	var content string

	if !m.quitting {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			// brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			// Overlays take over the full screen when active
			if m.errorModal.Visible {
				content = m.errorModal.View()
			} else if m.showHelp {
				content = views.RenderHelpOverlay(m.width, m.height)
			} else {
				content = m.viewMain()
			}
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	return v
}

// viewAuthBlocker renders the fullscreen auth message
func (m Model) viewAuthBlocker() string {
	titleStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#FF0000")).
		Align(lipgloss.Center)

	messageStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FFFFFF")).
		Align(lipgloss.Center)

	commandStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#00FF00"))

	hintStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#666666")).
		Align(lipgloss.Center)

	boxContent := strings.Join([]string{
		titleStyle.Render("Authentication Required"),
		"",
		messageStyle.Render("GitHub authentication is required to use this app.\n\nRun the following command in your terminal:"),
		"",
		"  " + commandStyle.Render("gh auth login"),
		"",
		hintStyle.Render("Press any key to retry • Ctrl+C to quit"),
	}, "\n")

	box := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center).
		Render(boxContent)

	return lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(box)
}

// viewMain renders the full layout with header, tab bar, and panes.
func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar ──
	header := views.RenderHeader(git.RepoName(m.repoPath), "", dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	pane1 := renderPane(
		core.PaneName(core.Pane1, m.activeTab),
		"(file list)",
		dim.SidebarWidth, dim.FileListHeight,
		m.activePane == core.Pane1,
	)
	pane3 := renderPane(
		core.PaneName(core.Pane3, m.activeTab),
		"(commit message)",
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	pane2 := renderPane(
		core.PaneName(core.Pane2, m.activeTab),
		"(diff viewer)",
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
	mainCol := pane2
	if m.terminalOpen {
		termPane := renderPane(
			"Terminal",
			"(terminal)",
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// renderPane draws a bordered box with a title and placeholder content.
// Active panes get a blue border; inactive panes get a gray border.
func renderPane(title, placeholder string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	// Border adds 2 to width and 2 to height, so subtract to hit target outer size
	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)
	body := lipgloss.NewStyle().Foreground(lipgloss.Color("#484F58")).Render(placeholder)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + body)
}
