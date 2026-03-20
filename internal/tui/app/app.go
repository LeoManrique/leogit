package app

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

// authResultMsg carries the result of an auth check back to the Update loop.
type authResultMsg struct {
	authenticated bool
}

// repoResolvedMsg is sent when the startup flow has determined which repo to open.
// If path is empty, no repo could be resolved (need to show picker).
type repoResolvedMsg struct {
	path string
}

// reposDiscoveredMsg carries the list of discovered repos for the picker.
type reposDiscoveredMsg struct {
	repos []string
}

// ── Commands ────────────────────────────────────────────

// checkAuthCmd runs the auth check asynchronously.
func checkAuthCmd() tea.Msg {
	return authResultMsg{authenticated: gh.CheckAuth()}
}

// resolveRepoCmd tries CLI arg → last opened → gives up (empty string).
func resolveRepoCmd(cliPath string) tea.Cmd {
	return func() tea.Msg {
		// 1. CLI argument
		if cliPath != "" && git.IsGitRepo(cliPath) {
			return repoResolvedMsg{path: cliPath}
		}

		// 2. Last opened from state file
		state, err := config.LoadState()
		if err == nil && state.LastOpened != "" && git.IsGitRepo(state.LastOpened) {
			return repoResolvedMsg{path: state.LastOpened}
		}

		// 3. No repo found — the picker will be shown
		return repoResolvedMsg{path: ""}
	}
}

// discoverReposCmd scans for repos based on the config.
func discoverReposCmd(cfg *config.Config) tea.Cmd {
	return func() tea.Msg {
		var repos []string

		switch cfg.Repos.Mode {
		case "manual":
			// Use manually listed paths, validate each one
			for _, p := range cfg.Repos.ManualPaths {
				expanded := git.ExpandTilde(p)
				if git.IsGitRepo(expanded) {
					repos = append(repos, expanded)
				}
			}
		default: // "folders" or any unrecognized mode
			repos = git.DiscoverRepos(cfg.Repos.ScanPaths, cfg.Repos.ScanDepth)
		}

		return reposDiscoveredMsg{repos: repos}
	}
}

// ── App State ───────────────────────────────────────────

// appState tracks which screen the app is on.
type appState int

const (
	stateAuthChecking     appState = iota // waiting for first auth check
	stateAuthBlocked                      // auth failed, showing blocker
	stateResolvingRepo                    // trying CLI arg / last opened
	stateDiscoveringRepos                 // scanning for repos
	stateRepoPicker                       // showing the repo picker
	stateMain                             // repo is open, normal UI
)

// Model is the root Bubbletea model for the entire application.
type Model struct {
	config       *config.Config
	cliPath      string // original CLI arg (may be empty)
	repoPath     string // resolved repo path (set once a repo is chosen)
	width        int
	height       int
	quitting     bool
	state        appState
	authChecking bool // prevents spamming concurrent auth checks

	repoPicker views.RepoPickerModel
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

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
		return m, nil

	case authResultMsg:
		m.authChecking = false
		if msg.authenticated {
			// Auth passed → resolve repo
			m.state = stateResolvingRepo
			return m, resolveRepoCmd(m.cliPath)
		}
		m.state = stateAuthBlocked
		return m, nil

	case repoResolvedMsg:
		if msg.path != "" {
			// We have a repo — open it
			m.repoPath = msg.path
			m.state = stateMain
			m.saveRepoState()
			return m, nil
		}
		// No repo found — discover repos for the picker
		m.state = stateDiscoveringRepos
		return m, discoverReposCmd(m.config)

	case reposDiscoveredMsg:
		m.repoPicker = views.NewRepoPicker(msg.repos)
		// Forward the current window size to the picker
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

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}

	return m, nil
}

// handleKey processes key messages based on current state.
// Returns (Model, tea.Cmd) not (tea.Model, tea.Cmd) — Go auto-converts because Model satisfies tea.Model.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.state {

	case stateAuthBlocked:
		if msg.String() == "ctrl+c" {
			m.quitting = true
			return m, tea.Quit
		}
		// Any other key → re-check auth
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
		switch msg.String() {
		case "q", "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	}

	return m, nil
}

// saveRepoState records the opened repo in repos-state.json.
func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return // silently ignore — state persistence is best-effort
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state) // best-effort
}

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
			content = m.viewMain()
		}
	}

	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	return v
}

// viewAuthBlocker renders a fullscreen centered message telling the user to log in.
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

	title := titleStyle.Render("Authentication Required")

	message := messageStyle.Render(
		"GitHub authentication is required to use this app.\n\nRun the following command in your terminal:",
	)

	command := commandStyle.Render("gh auth login")

	hint := hintStyle.Render("Press any key to retry • Ctrl+C to quit")

	boxContent := strings.Join([]string{
		title,
		"",
		message,
		"",
		"  " + command,
		"",
		hint,
	}, "\n")

	boxStyle := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#FF0000")).
		Padding(1, 3).
		Align(lipgloss.Center)

	box := boxStyle.Render(boxContent)

	fullscreen := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return fullscreen.Render(box)
}

// viewMain renders the normal app content (expanded in later phases).
func (m Model) viewMain() string {
	repoDisplay := m.repoPath
	if repoDisplay == "" {
		repoDisplay = "(none)"
	}

	lines := []string{
		"leogit",
		"",
		fmt.Sprintf("Config loaded: %s", m.config.Appearance.Theme),
		fmt.Sprintf("Repo: %s (%s)", git.RepoName(m.repoPath), m.repoPath),
		fmt.Sprintf("Terminal: %dx%d", m.width, m.height),
		"",
		"Press q to quit",
	}
	content := strings.Join(lines, "\n")

	style := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center)

	return style.Render(content)
}
