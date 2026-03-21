package app

import (
	"fmt"
	"os/exec"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/LeoManrique/leogit/internal/tui/render"

	"github.com/LeoManrique/leogit/internal/ai"
	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/core"
	"github.com/LeoManrique/leogit/internal/diff"
	"github.com/LeoManrique/leogit/internal/gh"
	"github.com/LeoManrique/leogit/internal/git"
	"github.com/LeoManrique/leogit/internal/tui/components"
	"github.com/LeoManrique/leogit/internal/tui/layout"
	"github.com/LeoManrique/leogit/internal/tui/views"
)

// ── Messages ────────────────────────────────────────────

type authResultMsg struct{ authenticated bool }
type repoResolvedMsg struct{ path string }
type reposDiscoveredMsg struct{ repos []string }

// statusTickMsg is sent every 2 seconds by the polling timer.
type statusTickMsg struct{}

// statusResultMsg carries the result of a git status command back to Update.
type statusResultMsg struct {
	status git.RepoStatus
	err    error
}

// pushResultMsg is sent when the async git push completes (success or error).
type pushResultMsg struct {
	err error
}

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

// refreshStatusCmd runs git status asynchronously and returns the result.
func refreshStatusCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		status, err := git.GetStatus(repoPath)
		return statusResultMsg{status: status, err: err}
	}
}

// startTickCmd starts the 2-second polling timer.
func startTickCmd() tea.Cmd {
	return tea.Tick(2*time.Second, func(t time.Time) tea.Msg {
		return statusTickMsg{}
	})
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

	// Branch & status
	branchName  string // current branch from git status
	ahead       int    // commits ahead of upstream
	behind      int    // commits behind upstream
	hasUpstream bool   // whether an upstream tracking branch is configured

	// Changed files
	fileList components.FileListModel
	// Diff view
	diffView components.DiffViewModel

	// Commit message
	commitMsg   components.CommitMsgModel
	aiProviders []ai.CommitMessageProvider // available providers
	aiActiveIdx int                        // index into aiProviders

	// Push state
	pushing  bool   // true while push is in progress (prevents double-push)
	upstream string // upstream tracking ref (e.g., "origin/main"), empty if none

	// Embedded terminal
	terminal components.TerminalModel // PTY + bubbleterm component

	// Settings & theme
	showSettings bool                // true when the settings overlay is visible
	settings     views.SettingsModel // settings overlay state
	theme        render.Theme        // active color palette
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
	// Build the AI provider list from config
	claudeProvider := ai.NewClaudeProvider(
		cfg.AI.Claude.Model,
		cfg.AI.Claude.Timeout,
		cfg.AI.Claude.MaxDiffSize,
	)
	ollamaProvider := ai.NewOllamaProvider(
		cfg.AI.Ollama.Model,
		cfg.AI.Ollama.ServerURL,
		cfg.AI.Ollama.Timeout,
		cfg.AI.Ollama.MaxDiffSize,
	)

	providers := []ai.CommitMessageProvider{claudeProvider, ollamaProvider}

	return Model{
		config:       cfg,
		cliPath:      repoPath,
		state:        stateAuthChecking,
		authChecking: true,
		activeTab:    core.ChangesTab,
		activePane:   core.Pane1,
		focusMode:    core.Navigable,
		fileList:     components.NewFileList(),
		commitMsg:    components.NewCommitMsg(),
		aiProviders:  providers,
		aiActiveIdx:  0,
		// Terminal
		terminal: components.NewTerminal(""), // repoPath set later when repo is resolved
		// Theme
		theme: render.CurrentTheme(cfg.Appearance.Theme),
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
		// Keep settings dimensions in sync
		if m.showSettings {
			m.settings, _ = m.settings.Update(msg)
		}
		// Update layout and resize terminal when in main state
		if m.state == stateMain {
			m.updateFileListSize()
			if m.terminalOpen && m.terminal.Started() {
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
				innerW := dim.MainWidth - 2
				innerH := dim.TerminalHeight - 2
				cmd := m.terminal.Resize(innerW, innerH)
				return m, cmd
			}
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
			m.terminal = components.NewTerminal(m.repoPath)
			m.saveRepoState()
			// Start polling: fetch initial status + start the 2s tick timer
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
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
		m.terminal = components.NewTerminal(m.repoPath)
		m.saveRepoState()
		// Start polling: fetch initial status + start the 2s tick timer
		return m, tea.Batch(
			refreshStatusCmd(m.repoPath),
			startTickCmd(),
		)

	case views.ErrorDismissedMsg:
		return m, nil

	case statusTickMsg:
		// Timer fired — refresh status and restart the timer.
		if m.state == stateMain {
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
			)
		}
		return m, nil

	case statusResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Git Status Error",
				"Failed to read repository status: "+msg.err.Error(),
				true,
				refreshStatusCmd(m.repoPath),
				m.width, m.height,
			)
			return m, nil
		}
		// Update branch info
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		m.upstream = msg.status.Upstream //save upstream for push
		// Parse and update changed files
		files := git.ParseFiles(msg.status.RawOutput)
		m.fileList.SetFiles(files)
		m.updateFileListSize()
		return m, nil

	case tea.FocusMsg:
		// Terminal gained focus — trigger immediate refresh
		if m.state == stateMain {
			return m, refreshStatusCmd(m.repoPath)
		}
		return m, nil

	case components.FileSelectedMsg:
		// A file was selected from the list — load its diff.
		m.diffView.SetLoading()
		return m, loadDiffCmd(m.repoPath, msg.File)

	case components.DiffLoadedMsg:
		if msg.Err != nil {
			m.diffView.SetError(msg.Err.Error())
		} else {
			m.diffView.SetDiff(msg.File, msg.FileDiff)
		}
		return m, nil

	case components.AIGenerateMsg:
		// User pressed ctrl+g — run the active AI provider with selected files
		if len(m.aiProviders) > 0 {
			provider := m.aiProviders[m.aiActiveIdx]
			selectedFiles := m.fileList.SelectedFiles()
			return m, generateCommitMsgCmd(m.repoPath, selectedFiles, provider)
		}
		return m, nil

	case components.AICycleProviderMsg:
		// User pressed ctrl+p — cycle to the next AI provider
		if len(m.aiProviders) > 0 {
			m.aiActiveIdx = (m.aiActiveIdx + 1) % len(m.aiProviders)
			provider := m.aiProviders[m.aiActiveIdx]
			m.commitMsg.SetAIProvider(provider.DisplayName())
		}
		return m, nil

	case components.AIResultMsg:
		// AI generation completed
		if msg.Err != nil {
			m.commitMsg.SetAIError(msg.Err.Error())
			return m, nil
		}
		m.commitMsg.SetAIResult(msg.Title, msg.Description)
		return m, nil

	case components.CommitRequestMsg:
		// User pressed ctrl+x or ctrl+enter — stage selected files and commit
		selectedFiles := m.fileList.SelectedFiles()
		return m, commitCmd(m.repoPath, selectedFiles, msg.Summary, msg.Description)

	case components.CommitResultMsg:
		if msg.Err != nil {
			m.commitMsg.SetCommitError(msg.Err.Error())
			return m, nil
		}
		// Commit succeeded — clear fields and refresh status.
		// asynchronously and sends a statusResultMsg to update the file list.
		m.commitMsg.CommitSuccess()
		return m, refreshStatusCmd(m.repoPath)

	case pushResultMsg:
		m.pushing = false
		if msg.err != nil {
			m.errorModal = views.ShowError(
				"Push Failed",
				msg.err.Error(),
				false, // not retryable — user should inspect the error
				nil,
				m.width, m.height,
			)
			return m, nil
		}
		// Push succeeded — refresh status to update ahead/behind counts.
		// refreshStatusCmd triggers a new `git status --porcelain=2` call, which will
		// report `# branch.ab +0 -0` after a successful push, so the statusResultMsg
		// handler sets ahead=0 and the header label switches from "↑ Push" to "↻ Fetch".
		return m, refreshStatusCmd(m.repoPath)

	case tea.KeyPressMsg:
		return m.handleKey(msg)
	// Forward bubbleterm internal messages (PTY output, ticks)
	// These must reach the terminal even when it's not focused.
	case views.SettingsClosedMsg:
		m.showSettings = false
		return m, nil

	case views.SettingsChangedMsg:
		// React to specific setting changes
		switch msg.Key {
		case "appearance.theme":
			m.theme = render.CurrentTheme(m.config.Appearance.Theme)
		case "git.fetch_interval":
			// The next tick will pick up the new interval automatically
			// because startTickCmd reads from m.config.Git.FetchInterval.
			// No action needed here — the timer restarts on its own.
		}
		return m, nil
	default:
		if m.terminalOpen && m.terminal.Started() {
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			if cmd != nil {
				return m, cmd
			}
		}
	}
	return m, nil
}

// updateFileListSize recalculates and sets the file list dimensions from the current layout.
// Inner dimensions = pane dimensions minus borders (2) and title line (1).
//
// Why -2 for width? The border draws 1 character on the left and 1 on the right,
// so the usable inner width is SidebarWidth minus 2.
//
// Why -3 for height? The border takes 2 rows (top + bottom) and the pane title
// takes 1 row, leaving FileListHeight minus 3 rows for actual file entries.
func (m *Model) updateFileListSize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	m.fileList.SetSize(dim.SidebarWidth-2, dim.FileListHeight-3)
	// Diff viewer: subtract border (2) and title line (1) from pane dimensions
	m.diffView.SetSize(dim.MainWidth-2, dim.DiffHeight-3)
	m.commitMsg.SetSize(dim.SidebarWidth-2, dim.CommitMsgHeight-3) // -3 for border + title
}

// ── Key Handling ────────────────────────────────────────

// handleKey processes key messages based on current state.
func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// Ctrl+C always quits, regardless of state or focus mode
	if msg.String() == "ctrl+c" {
		m.terminal.Close() // clean up PTY subprocess
		m.quitting = true
		return m, tea.Quit
	}

	// Settings overlay intercepts all keys when visible
	if m.showSettings {
		var cmd tea.Cmd
		m.settings, cmd = m.settings.Update(msg)
		return m, cmd
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
// Priority: error modal → help overlay → focused mode → navigable keybindings → pane keys.
func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Error modal takes priority over everything ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	// ── Help overlay ──
	if m.showHelp {
		if msg.String() == "?" || msg.String() == "esc" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode — only Esc escapes ──
	if m.focusMode == core.Focused {
		if msg.String() == "esc" {
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.terminal.Blur()
			} else {
				m.commitMsg.Blur()
			}
			return m, nil
		}

		// Terminal resize: Ctrl+Shift+Up/Down
		if m.activePane == core.PaneTerminal {
			switch msg.String() {
			case "ctrl+shift+up":
				// Grow terminal by 1 row. We ask layout.Calculate to try +1,
				// then check if it actually increased — because Calculate clamps
				// to the 80% max defined earlier. If clamped, the requested
				// height equals the current height and no resize happens.
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight+1)
				if dim.TerminalHeight > m.terminalHeight {
					m.terminalHeight = dim.TerminalHeight
					m.updateFileListSize()
					innerW := dim.MainWidth - 2
					innerH := dim.TerminalHeight - 2
					cmd := m.terminal.Resize(innerW, innerH)
					return m, cmd
				}
				return m, nil

			case "ctrl+shift+down":
				// Shrink terminal by 1 row
				if m.terminalHeight <= 3 { // minTermRows
					return m, nil
				}
				m.terminalHeight--
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
				m.updateFileListSize()
				innerW := dim.MainWidth - 2
				innerH := dim.TerminalHeight - 2
				cmd := m.terminal.Resize(innerW, innerH)
				return m, cmd
			}
		}

		// Esc always unfocuses
		if msg.String() == "esc" {
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.terminal.Blur()
			} else {
				m.commitMsg.Blur()
			}
			return m, nil
		}

		// Terminal pane: ALL keys go to PTY
		if m.activePane == core.PaneTerminal {
			var cmd tea.Cmd
			m.terminal, cmd = m.terminal.Update(msg)
			return m, cmd
		}

		// Commit message pane
		if m.activePane == core.Pane3 && m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}

		return m, nil
	}

	// ── Navigable mode keybindings ──
	switch msg.String() {
	case "q":
		m.terminal.Close() // clean up PTY subprocess
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
		if m.activeTab == core.ChangesTab {
			m.focusMode = core.Focused
			m.commitMsg.Focus()
		}
		return m, nil

	case "`":
		m.terminalOpen = !m.terminalOpen
		if m.terminalOpen {
			if m.terminalHeight == 0 {
				m.terminalHeight = layout.DefaultTerminalRows()
			}
			m.activePane = core.PaneTerminal
			m.focusMode = core.Focused

			// Calculate the terminal pane's inner dimensions
			dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
			innerW := dim.MainWidth - 2      // subtract border: 1 left + 1 right = 2
			innerH := dim.TerminalHeight - 2 // same: 1 top + 1 bottom = 2

			// Lazy start: spawn the PTY on first open
			if !m.terminal.Started() {
				cmd := m.terminal.Start(innerW, innerH)
				m.terminal.Focus()
				m.updateFileListSize()
				return m, cmd
			}

			// Already started — just focus and resize to current dimensions
			m.terminal.Focus()
			cmd := m.terminal.Resize(innerW, innerH)
			m.updateFileListSize()
			return m, cmd
		} else {
			// Closing the terminal pane
			m.terminal.Blur()
			m.focusMode = core.Navigable
			if m.activePane == core.PaneTerminal {
				m.activePane = core.Pane1
			}
		}
		m.updateFileListSize()
		return m, nil

	case "p":
		// Push to remote
		if m.pushing {
			return m, nil // already pushing
		}
		if m.branchName == "" {
			// Detached HEAD — can't push.
			// When HEAD is detached, `git status --porcelain=2` reports `# branch.oid <sha>`
			// but the `# branch.head` line contains `(detached)` instead of a branch name.
			// The status parser sets `Branch` to "" in that case, so an empty
			// branchName here means we're in detached HEAD state and there's no branch to push.
			return m, nil
		}
		m.pushing = true
		return m, pushCmd(m.repoPath, m.branchName, m.upstream, m.hasUpstream)

	case "S":
		// Open settings overlay
		m.showSettings = true
		m.settings = views.NewSettings(m.config, m.width, m.height)
		return m, nil

	case "esc":
		// Already navigable — Esc is a no-op
		return m, nil
	}

	// ── Forward unhandled keys to the active pane ──
	// This routes j/k/enter/space/a/g/G to the correct pane component.
	// Keys the pane doesn't handle are silently dropped (no-op).
	return m.handlePaneKey(msg)
}

// handlePaneKey forwards key events to the component that owns the active pane.
// Each pane's component handles its own navigation and selection keys.
func (m Model) handlePaneKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.activePane {

	case core.Pane1:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 1 = Changed Files list
			var cmd tea.Cmd
			m.fileList, cmd = m.fileList.Update(msg)
			return m, cmd
		}
		// History tab → Pane 1 = Commit List
		return m, nil

	case core.Pane2:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 2 = Diff Viewer
			var cmd tea.Cmd
			m.diffView, cmd = m.diffView.Update(msg)
			return m, cmd
		}
		// History tab → Changed Files in commit
		return m, nil

	case core.Pane3:
		if m.activeTab == core.ChangesTab {
			// Changes tab → Pane 3 = Commit Message
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}
		// History tab → Diff Viewer
		return m, nil

	case core.PaneTerminal:
		// Terminal pane: forward keys to bubbleterm
		var cmd tea.Cmd
		m.terminal, cmd = m.terminal.Update(msg)
		return m, cmd
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
func (m Model) View() tea.View {
	var content string

	if m.quitting {
		content = ""
	} else {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			content = "" // brief blank screen during async operations
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			if m.showSettings {
				content = m.settings.View()
			} else if m.errorModal.Visible {
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
	v.ReportFocus = true // enables tea.FocusMsg / tea.BlurMsg
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
	// ── Header + Tab bar ──
	headerData := views.HeaderData{
		RepoName:    git.RepoName(m.repoPath),
		BranchName:  m.branchName,
		Ahead:       m.ahead,
		Behind:      m.behind,
		HasUpstream: m.hasUpstream,
		Pushing:     m.pushing, // show pushing state in header
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	var pane1Content string
	var pane1Title string
	if m.activeTab == core.ChangesTab {
		// Changes tab → Pane 1 = Changed Files list with file count in title
		fileCount := len(m.fileList.Files)
		if fileCount > 0 {
			pane1Title = fmt.Sprintf("Changed Files (%d)", fileCount)
		} else {
			pane1Title = "Changed Files"
		}
		pane1Content = m.fileList.View()
	} else {
		// History tab → Pane 1 = Commit List
		pane1Title = core.PaneName(core.Pane1, m.activeTab)
		pane1Content = "(commit list)"
	}

	pane1 := renderPane(
		pane1Title,
		pane1Content,
		dim.SidebarWidth, dim.FileListHeight,
		m.activePane == core.Pane1,
	)
	pane3 := renderPaneWithContent(
		core.PaneName(core.Pane3, m.activeTab),
		m.commitMsg.View(),
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column: pane 2 (top) + terminal (bottom, if open) ──
	var pane2Content string
	var pane2Title string
	if m.activeTab == core.ChangesTab {
		pane2Title = "Diff"
		pane2Content = m.diffView.View()
	} else {
		pane2Title = core.PaneName(core.Pane2, m.activeTab)
		pane2Content = "(commit list)"
	}
	pane2 := renderPane(
		pane2Title,
		pane2Content,
		dim.MainWidth, dim.DiffHeight,
		m.activePane == core.Pane2,
	)
	mainCol := pane2
	if m.terminalOpen {
		var termContent string
		if m.terminal.Started() {
			termContent = m.terminal.View()
		} else {
			termContent = "Press ` to start terminal"
		}
		termPane := renderPaneWithContent(
			"Terminal",
			termContent,
			dim.MainWidth, dim.TerminalHeight,
			m.activePane == core.PaneTerminal,
		)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	// ── Compose ──
	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// renderPane draws a bordered box with a title and content.
// Active panes get a blue border; inactive panes get a gray border.
// The content string manages its own styling — renderPane does NOT
// apply any foreground color to it (unlike the earlier version which grayed out placeholders).
func renderPane(title, content string, width, height int, focused bool) string {
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

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}

// renderPaneThemed draws a bordered box using theme colors.
// This replaces the hardcoded colors in renderPane().
func renderPaneThemed(title, content string, width, height int, focused bool, theme render.Theme) string {
	borderColor := theme.BorderInactive
	titleColor := theme.TextSecondary
	if focused {
		borderColor = theme.BorderActive
		titleColor = theme.BorderActive
	}

	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}

func renderPaneWithContent(title, content string, width, height int, focused bool) string {
	borderColor := lipgloss.Color("#484F58")
	titleColor := lipgloss.Color("#8B949E")
	if focused {
		borderColor = lipgloss.Color("#58A6FF")
		titleColor = lipgloss.Color("#58A6FF")
	}

	innerW := width - 2
	innerH := height - 2
	if innerW < 1 {
		innerW = 1
	}
	if innerH < 1 {
		innerH = 1
	}

	titleLine := lipgloss.NewStyle().Bold(true).Foreground(titleColor).Render(title)

	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(borderColor).
		Width(innerW).
		Height(innerH).
		Render(titleLine + "\n" + content)
}

// loadDiffCmd runs git diff for a file and returns the parsed result.
// It returns a tea.Cmd, which is a function that Bubbletea runs on a separate
// goroutine. When the function finishes, its return value (a tea.Msg) is sent
// back to Update(). This keeps the UI responsive while git runs.
func loadDiffCmd(repoPath string, file git.FileEntry) tea.Cmd {
	return func() tea.Msg {
		raw, err := git.GetDiff(repoPath, file)
		if err != nil {
			return components.DiffLoadedMsg{File: file, Err: err}
		}
		parsed := diff.Parse(raw)
		return components.DiffLoadedMsg{File: file, FileDiff: parsed}
	}
}

// generateCommitMsgCmd runs the active AI provider asynchronously.
// It builds the diff from the currently selected files (in-memory selection),
// not from git's staging area.
func generateCommitMsgCmd(repoPath string, selectedFiles []git.FileEntry, provider ai.CommitMessageProvider) tea.Cmd {
	return func() tea.Msg {
		// Get the combined diff of selected files
		diff, err := git.GetSelectedDiff(repoPath, selectedFiles)
		if err != nil {
			return components.AIResultMsg{Err: err}
		}

		if strings.TrimSpace(diff) == "" {
			return components.AIResultMsg{
				Err: &ai.AIError{Code: ai.ErrEmptyDiff, Message: "no files selected — select files first"},
			}
		}

		// Generate the commit message
		msg, err := provider.GenerateCommitMessage(diff)
		if err != nil {
			return components.AIResultMsg{Err: err}
		}

		return components.AIResultMsg{
			Title:       msg.Title,
			Description: msg.Description,
		}
	}
}

// commitCmd stages the selected files and runs git commit asynchronously.
// This is the ONLY place where leogit modifies git's staging area (index).
// Flow: reset index → stage selected files → commit → (git status refresh on success)
func commitCmd(repoPath string, selectedFiles []git.FileEntry, summary, description string) tea.Cmd {
	return func() tea.Msg {
		if len(selectedFiles) == 0 {
			return components.CommitResultMsg{Err: fmt.Errorf("no files selected — select files first")}
		}

		// Step 1: Reset the index to HEAD (clear any external staging)
		// This ensures only what the user selected in leogit gets committed.
		resetCmd := exec.Command("git", "reset", "HEAD")
		resetCmd.Dir = repoPath
		if out, err := resetCmd.CombinedOutput(); err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("resetting index: %s (%w)", string(out), err)}
		}

		// Step 2: Stage the selected files
		if err := git.StageFiles(repoPath, selectedFiles); err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("staging selected files: %w", err)}
		}

		// Step 3: Verify staging succeeded
		hasStaged, err := git.HasStagedChanges(repoPath)
		if err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("checking staged changes: %w", err)}
		}
		if !hasStaged {
			return components.CommitResultMsg{Err: fmt.Errorf("staging produced no changes")}
		}

		// Step 4: Format and execute the commit
		message := git.FormatCommitMessage(summary, description, nil)

		if err := git.Commit(repoPath, message); err != nil {
			return components.CommitResultMsg{Err: err}
		}

		return components.CommitResultMsg{Err: nil}
	}
}

// pushCmd runs git push asynchronously.
// It determines the remote from the upstream ref (if set) or falls back to the default
// remote. If there's no upstream, --set-upstream is added to create one.
func pushCmd(repoPath, branchName, upstream string, hasUpstream bool) tea.Cmd {
	// Bubbletea runs tea.Cmd functions in a background goroutine so the push
	// doesn't block the UI thread (the TUI stays responsive and can render
	// "Pushing..." in the header). The returned closure captures the outer
	// function's parameters (repoPath, branchName, etc.) so they're available
	// when the goroutine executes. When the closure returns a tea.Msg, bubbletea
	// delivers it back to Update() on the main thread.
	return func() tea.Msg {
		// Determine the remote to push to
		var remote string
		if hasUpstream && upstream != "" {
			remote = git.RemoteFromUpstream(upstream)
		} else {
			// No upstream — discover the default remote
			var err error
			remote, err = git.GetDefaultRemote(repoPath)
			if err != nil {
				return pushResultMsg{err: fmt.Errorf("cannot push: %s", err)}
			}
		}

		// Build push options
		opts := git.PushOptions{
			Remote:      remote,
			Branch:      branchName,
			SetUpstream: !hasUpstream, // auto-set upstream on first push
		}

		// Execute the push
		if err := git.Push(repoPath, opts); err != nil {
			return pushResultMsg{err: err}
		}

		return pushResultMsg{err: nil}
	}
}
