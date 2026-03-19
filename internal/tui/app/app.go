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

type statusTickMsg struct{}
type statusResultMsg struct {
	status git.RepoStatus
	err    error
}

type pushResultMsg struct{ err error }

// Fetch & Pull messages
type fetchTickMsg struct{}

// FetchCompleteMsg carries the result of a background or manual fetch.
type FetchCompleteMsg struct {
	Err          error
	OldAhead     int
	OldBehind    int
	NewAhead     int
	NewBehind    int
	AheadChanged bool
	Manual       bool
}

// PullCompleteMsg carries the result of a pull operation.
type PullCompleteMsg struct{ Err error }

// Branch messages
type branchListResultMsg struct {
	branches []git.BranchInfo
	err      error
}

type branchActionResultMsg struct {
	action string // "switch", "create", "delete", "rename"
	err    error
}

// History messages
type logResultMsg struct {
	commits []git.CommitInfo
	err     error
	append  bool // true for pagination (append to existing)
}

type commitFilesResultMsg struct {
	files []git.FileEntry
	err   error
}

type commitDiffResultMsg struct {
	file     git.FileEntry
	fileDiff *diff.FileDiff
	err      error
}

// Merge messages
type mergeCountResultMsg struct {
	branch string
	count  int
	err    error
}

type mergeResultMsg struct {
	result git.MergeResult
	squash bool
	branch string
}

type mergeAbortResultMsg struct{ err error }

// PR messages
type prListResultMsg struct {
	prs []gh.PullRequest
	err error
}

type prChecksResultMsg struct {
	number int
	checks []gh.PRCheck
	err    error
}

type prCheckoutResultMsg struct{ err error }

type prCreateResultMsg struct {
	url string
	err error
}

type prCurrentBranchResultMsg struct {
	pr *gh.PullRequest
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

func refreshStatusCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		status, err := git.GetStatus(repoPath)
		return statusResultMsg{status: status, err: err}
	}
}

func startTickCmd() tea.Cmd {
	return tea.Tick(2*time.Second, func(t time.Time) tea.Msg {
		return statusTickMsg{}
	})
}

func backgroundFetchCmd(repoPath string, oldAhead, oldBehind int, upstream string, manual bool) tea.Cmd {
	return func() tea.Msg {
		remote := git.GetRemote(repoPath)
		if remote == "" {
			return FetchCompleteMsg{
				Err: fmt.Errorf("no remote configured"), Manual: manual,
				OldAhead: oldAhead, OldBehind: oldBehind,
				NewAhead: oldAhead, NewBehind: oldBehind,
			}
		}

		err := git.Fetch(repoPath, remote)
		if err != nil {
			return FetchCompleteMsg{
				Err: err, Manual: manual,
				OldAhead: oldAhead, OldBehind: oldBehind,
				NewAhead: oldAhead, NewBehind: oldBehind,
			}
		}

		newAhead, newBehind, _ := git.GetAheadBehind(repoPath, upstream)
		return FetchCompleteMsg{
			OldAhead: oldAhead, OldBehind: oldBehind,
			NewAhead: newAhead, NewBehind: newBehind,
			AheadChanged: newAhead != oldAhead || newBehind != oldBehind,
			Manual:       manual,
		}
	}
}

func pullCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		remote := git.GetRemote(repoPath)
		if remote == "" {
			return PullCompleteMsg{Err: fmt.Errorf("no remote configured")}
		}
		return PullCompleteMsg{Err: git.Pull(repoPath, remote)}
	}
}

func startFetchTickCmd(intervalSecs int) tea.Cmd {
	if intervalSecs <= 0 {
		return nil
	}
	if intervalSecs < 30 {
		intervalSecs = 30
	}
	return tea.Tick(time.Duration(intervalSecs)*time.Second, func(t time.Time) tea.Msg {
		return fetchTickMsg{}
	})
}

func loadBranchesCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		branches, err := git.ListBranches(repoPath)
		return branchListResultMsg{branches: branches, err: err}
	}
}

func switchBranchCmd(repoPath, name string) tea.Cmd {
	return func() tea.Msg {
		err := git.SwitchBranch(repoPath, name)
		return branchActionResultMsg{action: "switch", err: err}
	}
}

func createBranchCmd(repoPath, name string) tea.Cmd {
	return func() tea.Msg {
		err := git.CreateBranch(repoPath, name, "")
		if err != nil {
			return branchActionResultMsg{action: "create", err: err}
		}
		// Auto-switch to the new branch
		err = git.SwitchBranch(repoPath, name)
		return branchActionResultMsg{action: "create", err: err}
	}
}

func deleteBranchCmd(repoPath, name string, isRemote bool) tea.Cmd {
	return func() tea.Msg {
		if isRemote {
			parts := strings.SplitN(name, "/", 2)
			if len(parts) == 2 {
				err := git.DeleteRemoteBranch(repoPath, parts[0], parts[1])
				return branchActionResultMsg{action: "delete", err: err}
			}
		}
		err := git.DeleteBranch(repoPath, name)
		return branchActionResultMsg{action: "delete", err: err}
	}
}

func renameBranchCmd(repoPath, oldName, newName string) tea.Cmd {
	return func() tea.Msg {
		err := git.RenameBranch(repoPath, oldName, newName)
		return branchActionResultMsg{action: "rename", err: err}
	}
}

func loadLogCmd(repoPath string, skip int, appendMode bool) tea.Cmd {
	return func() tea.Msg {
		commits, err := git.GetLog(repoPath, git.LogOptions{MaxCount: 50, Skip: skip})
		return logResultMsg{commits: commits, err: err, append: appendMode}
	}
}

func loadCommitFilesCmd(repoPath, sha string) tea.Cmd {
	return func() tea.Msg {
		files, err := git.GetCommitFiles(repoPath, sha)
		return commitFilesResultMsg{files: files, err: err}
	}
}

func loadCommitDiffCmd(repoPath, sha string, file git.FileEntry) tea.Cmd {
	return func() tea.Msg {
		raw, err := git.GetCommitDiff(repoPath, sha, file.Path)
		if err != nil {
			return commitDiffResultMsg{file: file, err: err}
		}
		parsed := diff.Parse(raw)
		return commitDiffResultMsg{file: file, fileDiff: parsed}
	}
}

func mergeCountCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		count, err := git.CountCommitsToMerge(repoPath, branch)
		return mergeCountResultMsg{branch: branch, count: count, err: err}
	}
}

func mergeBranchCmd(repoPath, branch string, squash bool) tea.Cmd {
	return func() tea.Msg {
		if squash {
			result := git.MergeSquash(repoPath, branch)
			if result.Success {
				// Finalize the squash merge
				if err := git.CommitSquashMerge(repoPath); err != nil {
					return mergeResultMsg{result: git.MergeResult{Success: false, ErrorMessage: err.Error()}, squash: true, branch: branch}
				}
			}
			return mergeResultMsg{result: result, squash: true, branch: branch}
		}
		result := git.MergeBranch(repoPath, branch)
		return mergeResultMsg{result: result, squash: false, branch: branch}
	}
}

func mergeAbortCmd(repoPath string) tea.Cmd {
	return func() tea.Msg {
		return mergeAbortResultMsg{err: git.MergeAbort(repoPath)}
	}
}

func loadPRsCmd(repoPath, state string) tea.Cmd {
	return func() tea.Msg {
		prs, err := gh.ListPRs(repoPath, state)
		return prListResultMsg{prs: prs, err: err}
	}
}

func loadPRChecksCmd(repoPath string, number int) tea.Cmd {
	return func() tea.Msg {
		checks, err := gh.GetPRChecks(repoPath, number)
		return prChecksResultMsg{number: number, checks: checks, err: err}
	}
}

func checkoutPRCmd(repoPath string, number int) tea.Cmd {
	return func() tea.Msg {
		return prCheckoutResultMsg{err: gh.CheckoutPR(repoPath, number)}
	}
}

func createPRCmd(repoPath, title, body, base string, draft, fill bool) tea.Cmd {
	return func() tea.Msg {
		var url string
		var err error
		if fill {
			url, err = gh.CreatePRFill(repoPath, base, draft)
		} else {
			url, err = gh.CreatePR(repoPath, title, body, base, draft)
		}
		return prCreateResultMsg{url: url, err: err}
	}
}

func loadCurrentBranchPRCmd(repoPath, branch string) tea.Cmd {
	return func() tea.Msg {
		pr, _ := gh.GetCurrentBranchPR(repoPath, branch)
		return prCurrentBranchResultMsg{pr: pr}
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
	authChecking bool

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
	branchName  string
	ahead       int
	behind      int
	hasUpstream bool

	// Changed files
	fileList components.FileListModel
	// Diff view
	diffView components.DiffViewModel

	// Commit message
	commitMsg   components.CommitMsgModel
	aiProviders []ai.CommitMessageProvider
	aiActiveIdx int

	// Push state
	pushing  bool
	upstream string

	// Fetch & Pull
	fetching      bool
	pulling       bool
	lastFetchTime time.Time
	remote        string
	postPullCheck bool

	// Embedded terminal
	terminal components.TerminalModel

	// Settings & theme
	showSettings bool
	settings     views.SettingsModel
	theme        render.Theme

	// Branch dropdown (Phase 15)
	branchDropdown views.BranchDropdownModel

	// History tab (Phase 16)
	commitList      components.CommitListModel
	historyFiles    components.FileListModel
	historyDiff     components.DiffViewModel
	selectedCommit  *git.CommitInfo
	historyLoaded   bool
	loadingMoreLogs bool

	// Merge overlay (Phase 17)
	mergeOverlay views.MergeOverlayModel

	// PR overlays (Phase 18)
	prOverlay       views.PROverlayModel
	prCreateOverlay views.PRCreateOverlayModel
	currentPR       *gh.PullRequest
}

// New creates the root model with the loaded config and optional repo path.
func New(cfg *config.Config, repoPath string) Model {
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
		terminal:     components.NewTerminal(""),
		theme:        render.CurrentTheme(cfg.Appearance.Theme),
		// Phase 15
		branchDropdown: views.NewBranchDropdown(),
		// Phase 16
		commitList:   components.NewCommitList(),
		historyFiles: components.NewFileList(),
		historyDiff:  components.NewDiffView(),
		// Phase 17
		mergeOverlay: views.NewMergeOverlay(),
		// Phase 18
		prOverlay:       views.NewPROverlay(),
		prCreateOverlay: views.NewPRCreateOverlay(),
	}
}

// Init is called once when the program starts. Kicks off the auth check.
func (m Model) Init() tea.Cmd {
	return checkAuthCmd
}

// ── Update ──────────────────────────────────────────────

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		if m.state == stateRepoPicker {
			var cmd tea.Cmd
			m.repoPicker, cmd = m.repoPicker.Update(msg)
			return m, cmd
		}
		if m.errorModal.Visible {
			m.errorModal, _ = m.errorModal.Update(msg)
		}
		if m.showSettings {
			m.settings, _ = m.settings.Update(msg)
		}
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
			m.remote = git.GetRemote(m.repoPath)
			return m, tea.Batch(
				refreshStatusCmd(m.repoPath),
				startTickCmd(),
				startFetchTickCmd(m.config.Git.FetchInterval),
				loadCurrentBranchPRCmd(m.repoPath, ""),
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
		m.remote = git.GetRemote(m.repoPath)
		return m, tea.Batch(
			refreshStatusCmd(m.repoPath),
			startTickCmd(),
			startFetchTickCmd(m.config.Git.FetchInterval),
		)

	case views.ErrorDismissedMsg:
		return m, nil

	case statusTickMsg:
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
		oldBranch := m.branchName
		m.branchName = msg.status.Branch
		m.ahead = msg.status.Ahead
		m.behind = msg.status.Behind
		m.hasUpstream = msg.status.HasUpstream
		m.upstream = msg.status.Upstream
		files := git.ParseFiles(msg.status.RawOutput)
		m.fileList.SetFiles(files)
		m.updateFileListSize()

		var cmds []tea.Cmd

		// Load current branch PR when branch changes
		if oldBranch != m.branchName && m.branchName != "" {
			cmds = append(cmds, loadCurrentBranchPRCmd(m.repoPath, m.branchName))
		}

		// Post-pull conflict detection
		if m.postPullCheck {
			m.postPullCheck = false
			conflicted := git.ConflictedFiles(files)
			if len(conflicted) > 0 && !m.errorModal.Visible {
				fileList := strings.Join(conflicted, "\n  ")
				m.errorModal = views.ShowError(
					"Merge Conflicts Detected",
					fmt.Sprintf(
						"The following files have conflicts:\n\n  %s\n\n"+
							"Resolve conflicts in the terminal (`):\n"+
							"  git mergetool\n"+
							"  # or edit files, then:\n"+
							"  git add <file>\n"+
							"  git commit",
						fileList,
					),
					false, nil, m.width, m.height,
				)
			}
		}

		if len(cmds) > 0 {
			return m, tea.Batch(cmds...)
		}
		return m, nil

	case tea.FocusMsg:
		if m.state == stateMain {
			return m, refreshStatusCmd(m.repoPath)
		}
		return m, nil

	case components.FileSelectedMsg:
		if m.activeTab == core.ChangesTab {
			m.diffView.SetLoading()
			return m, loadDiffCmd(m.repoPath, msg.File)
		}
		// History tab — load diff for the selected file in the selected commit
		if m.activeTab == core.HistoryTab && m.selectedCommit != nil {
			m.historyDiff.SetLoading()
			return m, loadCommitDiffCmd(m.repoPath, m.selectedCommit.SHA, msg.File)
		}
		return m, nil

	case components.DiffLoadedMsg:
		if msg.Err != nil {
			m.diffView.SetError(msg.Err.Error())
		} else {
			m.diffView.SetDiff(msg.File, msg.FileDiff)
		}
		return m, nil

	case components.AIGenerateMsg:
		if len(m.aiProviders) > 0 {
			provider := m.aiProviders[m.aiActiveIdx]
			selectedFiles := m.fileList.SelectedFiles()
			return m, generateCommitMsgCmd(m.repoPath, selectedFiles, provider)
		}
		return m, nil

	case components.AICycleProviderMsg:
		if len(m.aiProviders) > 0 {
			m.aiActiveIdx = (m.aiActiveIdx + 1) % len(m.aiProviders)
			provider := m.aiProviders[m.aiActiveIdx]
			m.commitMsg.SetAIProvider(provider.DisplayName())
		}
		return m, nil

	case components.AIResultMsg:
		if msg.Err != nil {
			m.commitMsg.SetAIError(msg.Err.Error())
			return m, nil
		}
		m.commitMsg.SetAIResult(msg.Title, msg.Description)
		return m, nil

	case components.CommitRequestMsg:
		selectedFiles := m.fileList.SelectedFiles()
		return m, commitCmd(m.repoPath, selectedFiles, msg.Summary, msg.Description)

	case components.CommitResultMsg:
		if msg.Err != nil {
			m.commitMsg.SetCommitError(msg.Err.Error())
			return m, nil
		}
		m.commitMsg.CommitSuccess()
		return m, refreshStatusCmd(m.repoPath)

	case pushResultMsg:
		m.pushing = false
		if msg.err != nil {
			m.errorModal = views.ShowError("Push Failed", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		return m, refreshStatusCmd(m.repoPath)

	// ── Fetch & Pull ──
	case fetchTickMsg:
		if m.state == stateMain && !m.fetching && m.remote != "" {
			m.fetching = true
			return m, backgroundFetchCmd(m.repoPath, m.ahead, m.behind, m.branchName+"@{upstream}", false)
		}
		return m, startFetchTickCmd(m.config.Git.FetchInterval)

	case FetchCompleteMsg:
		m.fetching = false
		m.lastFetchTime = time.Now()
		var cmds []tea.Cmd
		cmds = append(cmds, refreshStatusCmd(m.repoPath))
		cmds = append(cmds, startFetchTickCmd(m.config.Git.FetchInterval))
		if msg.Err != nil {
			if msg.Manual {
				m.errorModal = views.ShowError("Fetch Error", msg.Err.Error(), true,
					backgroundFetchCmd(m.repoPath, m.ahead, m.behind, m.branchName+"@{upstream}", true),
					m.width, m.height)
			}
			return m, tea.Batch(cmds...)
		}
		if msg.AheadChanged {
			m.errorModal = views.ShowError("Remote Updated", formatAheadBehindChange(msg), false, nil, m.width, m.height)
		}
		return m, tea.Batch(cmds...)

	case PullCompleteMsg:
		m.pulling = false
		m.postPullCheck = true
		cmd := refreshStatusCmd(m.repoPath)
		if msg.Err != nil {
			errMsg := msg.Err.Error()
			if strings.Contains(errMsg, "CONFLICT") || strings.Contains(errMsg, "conflict") {
				m.errorModal = views.ShowError("Merge Conflicts",
					"Pull completed with merge conflicts.\n\n"+
						"Conflicted files are marked with [!] in the file list.\n"+
						"Use the terminal (`) to resolve conflicts:\n\n"+
						"  git mergetool\n"+
						"  # or edit files manually, then:\n"+
						"  git add <resolved-file>\n"+
						"  git commit",
					false, nil, m.width, m.height)
			} else {
				m.errorModal = views.ShowError("Pull Error", errMsg, true, pullCmd(m.repoPath), m.width, m.height)
			}
			return m, cmd
		}
		return m, cmd

	// ── Branch actions ──
	case branchListResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("Branch Error", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		m.branchDropdown.SetBranches(msg.branches)
		return m, nil

	case branchActionResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("Branch Error", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		// Refresh status after any branch action
		var cmds []tea.Cmd
		cmds = append(cmds, refreshStatusCmd(m.repoPath))
		// Invalidate history cache
		m.historyLoaded = false
		return m, tea.Batch(cmds...)

	case views.BranchSwitchMsg:
		return m, switchBranchCmd(m.repoPath, msg.Name)

	case views.BranchCreateMsg:
		return m, createBranchCmd(m.repoPath, msg.Name)

	case views.BranchDeleteMsg:
		return m, deleteBranchCmd(m.repoPath, msg.Name, msg.IsRemote)

	case views.BranchRenameMsg:
		return m, renameBranchCmd(m.repoPath, msg.OldName, msg.NewName)

	case views.BranchDropdownClosedMsg:
		return m, nil

	// ── Merge ──
	case views.BranchMergeMsg:
		return m, mergeCountCmd(m.repoPath, msg.Name)

	case mergeCountResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("Merge Error", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		m.mergeOverlay.Show(msg.branch, m.branchName, msg.count, m.width, m.height)
		return m, nil

	case views.MergeConfirmMsg:
		return m, mergeBranchCmd(m.repoPath, msg.Branch, msg.Squash)

	case views.MergeCancelMsg:
		return m, nil

	case mergeResultMsg:
		if !msg.result.Success {
			if len(msg.result.Conflicts) > 0 {
				fileList := strings.Join(msg.result.Conflicts, "\n  ")
				m.errorModal = views.ShowError("Merge Conflicts",
					fmt.Sprintf("Merge produced conflicts:\n\n  %s\n\n"+
						"Resolve in the terminal (`) then commit.", fileList),
					false, nil, m.width, m.height)
			} else {
				m.errorModal = views.ShowError("Merge Failed", msg.result.ErrorMessage, false, nil, m.width, m.height)
			}
			return m, refreshStatusCmd(m.repoPath)
		}
		m.historyLoaded = false
		return m, refreshStatusCmd(m.repoPath)

	case mergeAbortResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("Abort Failed", msg.err.Error(), false, nil, m.width, m.height)
		}
		return m, refreshStatusCmd(m.repoPath)

	// ── History ──
	case logResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("Log Error", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		m.loadingMoreLogs = false
		if msg.append {
			m.commitList.AppendCommits(msg.commits)
		} else {
			m.commitList.SetCommits(msg.commits)
			m.historyLoaded = true
			// Auto-select first commit
			if c := m.commitList.SelectedCommit(); c != nil {
				m.selectedCommit = c
				return m, loadCommitFilesCmd(m.repoPath, c.SHA)
			}
		}
		return m, nil

	case commitFilesResultMsg:
		if msg.err != nil {
			return m, nil
		}
		m.historyFiles.SetFiles(msg.files)
		m.updateHistorySize()
		// Auto-select first file
		if len(msg.files) > 0 && m.selectedCommit != nil {
			m.historyDiff.SetLoading()
			return m, loadCommitDiffCmd(m.repoPath, m.selectedCommit.SHA, msg.files[0])
		}
		return m, nil

	case commitDiffResultMsg:
		if msg.err != nil {
			m.historyDiff.SetError(msg.err.Error())
		} else {
			m.historyDiff.SetDiff(msg.file, msg.fileDiff)
		}
		return m, nil

	case components.CommitSelectedMsg:
		m.selectedCommit = &msg.Commit
		m.historyDiff.Clear()
		return m, loadCommitFilesCmd(m.repoPath, msg.Commit.SHA)

	case components.LoadMoreCommitsMsg:
		if !m.loadingMoreLogs {
			m.loadingMoreLogs = true
			return m, loadLogCmd(m.repoPath, len(m.commitList.Commits), true)
		}
		return m, nil

	// ── PR ──
	case prListResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("PR Error", msg.err.Error(), false, nil, m.width, m.height)
			m.prOverlay.Visible = false
			return m, nil
		}
		m.prOverlay.SetPRs(msg.prs)
		// Load checks for first PR
		if pr := m.prOverlay.SelectedPR(); pr != nil {
			return m, loadPRChecksCmd(m.repoPath, pr.Number)
		}
		return m, nil

	case prChecksResultMsg:
		if msg.err == nil {
			m.prOverlay.SetChecks(msg.number, msg.checks)
		}
		return m, nil

	case views.PRCheckoutMsg:
		return m, checkoutPRCmd(m.repoPath, msg.Number)

	case prCheckoutResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("PR Checkout Failed", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		m.historyLoaded = false
		return m, refreshStatusCmd(m.repoPath)

	case views.PRCreateRequestMsg:
		m.prCreateOverlay.Show(msg.BaseBranch, m.width, m.height)
		return m, nil

	case views.PRCreateMsg:
		return m, createPRCmd(m.repoPath, msg.Title, msg.Body, msg.Base, msg.Draft, msg.UseFill)

	case views.PRCreateCancelMsg:
		m.prOverlay.Reshow()
		return m, nil

	case prCreateResultMsg:
		if msg.err != nil {
			m.errorModal = views.ShowError("PR Create Failed", msg.err.Error(), false, nil, m.width, m.height)
			return m, nil
		}
		m.errorModal = views.ShowError("PR Created", msg.url, false, nil, m.width, m.height)
		return m, loadCurrentBranchPRCmd(m.repoPath, m.branchName)

	case views.PROverlayCloseMsg:
		return m, nil

	case views.PRNeedChecksMsg:
		return m, loadPRChecksCmd(m.repoPath, msg.Number)

	case views.PRFilterChangeMsg:
		return m, loadPRsCmd(m.repoPath, msg.State)

	case prCurrentBranchResultMsg:
		m.currentPR = msg.pr
		return m, nil

	// ── Settings & Terminal ──
	case tea.KeyPressMsg:
		return m.handleKey(msg)

	case views.SettingsClosedMsg:
		m.showSettings = false
		return m, nil

	case views.SettingsChangedMsg:
		switch msg.Key {
		case "appearance.theme":
			m.theme = render.CurrentTheme(m.config.Appearance.Theme)
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

// updateFileListSize recalculates file list and diff dimensions from layout.
func (m *Model) updateFileListSize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	m.fileList.SetSize(dim.SidebarWidth-2, dim.FileListHeight-3)
	m.diffView.SetSize(dim.MainWidth-2, dim.DiffHeight-3)
	m.commitMsg.SetSize(dim.SidebarWidth-2, dim.CommitMsgHeight-3)
}

// updateHistorySize recalculates history tab component dimensions.
func (m *Model) updateHistorySize() {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
	// History tab: sidebar = commit list (full height), main = detail + files + diff
	m.commitList.SetSize(dim.SidebarWidth-2, dim.FileListHeight+dim.CommitMsgHeight-3)
	m.historyFiles.SetSize((dim.MainWidth/2)-2, dim.DiffHeight-6)
	m.historyDiff.SetSize((dim.MainWidth/2)-2, dim.DiffHeight-6)
}

// ── Key Handling ────────────────────────────────────────

func (m Model) handleKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	if msg.String() == "ctrl+c" {
		m.terminal.Close()
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

func (m Model) handleMainKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	// ── Overlays take priority ──
	if m.errorModal.Visible {
		var cmd tea.Cmd
		m.errorModal, cmd = m.errorModal.Update(msg)
		return m, cmd
	}

	if m.branchDropdown.Visible {
		var cmd tea.Cmd
		m.branchDropdown, cmd = m.branchDropdown.Update(msg)
		return m, cmd
	}

	if m.mergeOverlay.Visible {
		var cmd tea.Cmd
		m.mergeOverlay, cmd = m.mergeOverlay.Update(msg)
		return m, cmd
	}

	if m.prOverlay.Visible {
		var cmd tea.Cmd
		m.prOverlay, cmd = m.prOverlay.Update(msg)
		return m, cmd
	}

	if m.prCreateOverlay.Visible {
		var cmd tea.Cmd
		m.prCreateOverlay, cmd = m.prCreateOverlay.Update(msg)
		return m, cmd
	}

	if m.showHelp {
		if msg.String() == "?" || msg.String() == "esc" {
			m.showHelp = false
		}
		return m, nil
	}

	// ── Focused mode ──
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

		// Terminal resize
		if m.activePane == core.PaneTerminal {
			switch msg.String() {
			case "ctrl+shift+up":
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight+1)
				if dim.TerminalHeight > m.terminalHeight {
					m.terminalHeight = dim.TerminalHeight
					m.updateFileListSize()
					cmd := m.terminal.Resize(dim.MainWidth-2, dim.TerminalHeight-2)
					return m, cmd
				}
				return m, nil

			case "ctrl+shift+down":
				if m.terminalHeight <= 3 {
					return m, nil
				}
				m.terminalHeight--
				dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
				m.updateFileListSize()
				cmd := m.terminal.Resize(dim.MainWidth-2, dim.TerminalHeight-2)
				return m, cmd
			}
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

	// ── Navigable mode ──
	switch msg.String() {
	case "q":
		m.terminal.Close()
		m.quitting = true
		return m, tea.Quit

	case "?":
		m.showHelp = true
		return m, nil

	case "tab":
		if m.activeTab == core.ChangesTab {
			m.activeTab = core.HistoryTab
			// Load history if not loaded yet
			if !m.historyLoaded {
				m.updateHistorySize()
				return m, loadLogCmd(m.repoPath, 0, false)
			}
		} else {
			m.activeTab = core.ChangesTab
		}
		m.activePane = core.Pane1
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

			dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)
			innerW := dim.MainWidth - 2
			innerH := dim.TerminalHeight - 2

			if !m.terminal.Started() {
				cmd := m.terminal.Start(innerW, innerH)
				m.terminal.Focus()
				m.updateFileListSize()
				return m, cmd
			}

			m.terminal.Focus()
			cmd := m.terminal.Resize(innerW, innerH)
			m.updateFileListSize()
			return m, cmd
		} else {
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
			return m, nil
		}
		if m.branchName == "" {
			return m, nil
		}
		m.pushing = true
		return m, pushCmd(m.repoPath, m.branchName, m.upstream, m.hasUpstream)

	case "F":
		if !m.fetching && !m.pulling && m.remote != "" {
			m.fetching = true
			return m, backgroundFetchCmd(m.repoPath, m.ahead, m.behind, m.branchName+"@{upstream}", true)
		}
		return m, nil

	case "P":
		if !m.pulling && !m.fetching && m.remote != "" {
			m.pulling = true
			return m, pullCmd(m.repoPath)
		}
		return m, nil

	case "B":
		m.branchDropdown.Open(m.width, m.height)
		return m, loadBranchesCmd(m.repoPath)

	case "R":
		m.prOverlay.Show(m.branchName, m.width, m.height)
		return m, loadPRsCmd(m.repoPath, "open")

	case "A":
		// Abort merge (only when merging)
		if git.IsMerging(m.repoPath) {
			return m, mergeAbortCmd(m.repoPath)
		}
		return m, nil

	case "S":
		m.showSettings = true
		m.settings = views.NewSettings(m.config, m.width, m.height)
		return m, nil

	case "esc":
		return m, nil
	}

	// Forward to active pane
	return m.handlePaneKey(msg)
}

func (m Model) handlePaneKey(msg tea.KeyPressMsg) (Model, tea.Cmd) {
	switch m.activePane {

	case core.Pane1:
		if m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.fileList, cmd = m.fileList.Update(msg)
			return m, cmd
		}
		// History tab → Commit List
		var cmd tea.Cmd
		m.commitList, cmd = m.commitList.Update(msg)
		return m, cmd

	case core.Pane2:
		if m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.diffView, cmd = m.diffView.Update(msg)
			return m, cmd
		}
		// History tab → Changed Files in commit
		var cmd tea.Cmd
		m.historyFiles, cmd = m.historyFiles.Update(msg)
		if cmd != nil {
			// File selected in history → load its diff
			return m, cmd
		}
		return m, nil

	case core.Pane3:
		if m.activeTab == core.ChangesTab {
			var cmd tea.Cmd
			m.commitMsg, cmd = m.commitMsg.Update(msg)
			return m, cmd
		}
		// History tab → Diff Viewer
		var cmd tea.Cmd
		m.historyDiff, cmd = m.historyDiff.Update(msg)
		return m, cmd

	case core.PaneTerminal:
		var cmd tea.Cmd
		m.terminal, cmd = m.terminal.Update(msg)
		return m, cmd
	}

	return m, nil
}

// ── State Persistence ───────────────────────────────────

func (m *Model) saveRepoState() {
	state, err := config.LoadState()
	if err != nil {
		return
	}
	state.SetLastOpened(m.repoPath)
	_ = config.SaveState(state)
}

// ── View ────────────────────────────────────────────────

func (m Model) View() tea.View {
	var content string

	if m.quitting {
		content = ""
	} else {
		switch m.state {
		case stateAuthChecking, stateResolvingRepo, stateDiscoveringRepos:
			content = ""
		case stateAuthBlocked:
			content = m.viewAuthBlocker()
		case stateRepoPicker:
			content = m.repoPicker.View()
		case stateMain:
			if m.showSettings {
				content = m.settings.View()
			} else if m.prCreateOverlay.Visible {
				content = m.prCreateOverlay.View()
			} else if m.prOverlay.Visible {
				content = m.prOverlay.View()
			} else if m.mergeOverlay.Visible {
				content = m.mergeOverlay.View()
			} else if m.branchDropdown.Visible {
				content = m.branchDropdown.View()
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
	v.ReportFocus = true
	return v
}

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

func (m Model) viewMain() string {
	dim := layout.Calculate(m.width, m.height, m.terminalOpen, m.terminalHeight)

	// ── Header + Tab bar ──
	prNumber := 0
	prReview := ""
	if m.currentPR != nil {
		prNumber = m.currentPR.Number
		prReview = m.currentPR.ReviewDecision
	}

	headerData := views.HeaderData{
		RepoName:      git.RepoName(m.repoPath),
		BranchName:    m.branchName,
		Ahead:         m.ahead,
		Behind:        m.behind,
		HasUpstream:   m.hasUpstream,
		Pushing:       m.pushing,
		Fetching:      m.fetching,
		Pulling:       m.pulling,
		LastFetchTime: m.lastFetchTime,
		IsMerging:     git.IsMerging(m.repoPath),
		PRNumber:      prNumber,
		PRReview:      prReview,
	}
	header := views.RenderHeader(headerData, dim.Width)
	tabBar := views.RenderTabBar(m.activeTab, dim.Width)

	if m.activeTab == core.HistoryTab {
		return m.viewHistoryTab(header, tabBar, dim)
	}

	return m.viewChangesTab(header, tabBar, dim)
}

func (m Model) viewChangesTab(header, tabBar string, dim layout.Dimensions) string {
	// ── Sidebar column: pane 1 (top) + pane 3 (bottom) ──
	fileCount := len(m.fileList.Files)
	pane1Title := "Changed Files"
	if fileCount > 0 {
		pane1Title = fmt.Sprintf("Changed Files (%d)", fileCount)
	}
	pane1 := renderPane(pane1Title, m.fileList.View(), dim.SidebarWidth, dim.FileListHeight, m.activePane == core.Pane1)
	pane3 := renderPaneWithContent(
		core.PaneName(core.Pane3, m.activeTab),
		m.commitMsg.View(),
		dim.SidebarWidth, dim.CommitMsgHeight,
		m.activePane == core.Pane3,
	)
	sidebar := lipgloss.JoinVertical(lipgloss.Left, pane1, pane3)

	// ── Main column ──
	pane2 := renderPane("Diff", m.diffView.View(), dim.MainWidth, dim.DiffHeight, m.activePane == core.Pane2)
	mainCol := pane2
	if m.terminalOpen {
		var termContent string
		if m.terminal.Started() {
			termContent = m.terminal.View()
		} else {
			termContent = "Press ` to start terminal"
		}
		termPane := renderPaneWithContent("Terminal", termContent, dim.MainWidth, dim.TerminalHeight, m.activePane == core.PaneTerminal)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, pane2, termPane)
	}

	content := lipgloss.JoinHorizontal(lipgloss.Top, sidebar, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

func (m Model) viewHistoryTab(header, tabBar string, dim layout.Dimensions) string {
	// Sidebar = Commit List (full height)
	sidebarHeight := dim.FileListHeight + dim.CommitMsgHeight
	commitListPane := renderPane("Commit List", m.commitList.View(), dim.SidebarWidth, sidebarHeight, m.activePane == core.Pane1)

	// Main column: detail (top) + files + diff (bottom)
	detailHeight := 8
	remainingHeight := dim.DiffHeight - detailHeight
	if remainingHeight < 4 {
		remainingHeight = 4
	}

	detailContent := views.RenderCommitDetail(m.selectedCommit, dim.MainWidth-2)
	detailPane := renderPane("Commit Details", detailContent, dim.MainWidth, detailHeight, false)

	halfWidth := dim.MainWidth / 2
	filesPane := renderPane("Changed Files", m.historyFiles.View(), halfWidth, remainingHeight, m.activePane == core.Pane2)
	diffPane := renderPane("Diff", m.historyDiff.View(), dim.MainWidth-halfWidth, remainingHeight, m.activePane == core.Pane3)
	bottomRow := lipgloss.JoinHorizontal(lipgloss.Top, filesPane, diffPane)

	mainCol := lipgloss.JoinVertical(lipgloss.Left, detailPane, bottomRow)

	if m.terminalOpen {
		var termContent string
		if m.terminal.Started() {
			termContent = m.terminal.View()
		} else {
			termContent = "Press ` to start terminal"
		}
		termPane := renderPaneWithContent("Terminal", termContent, dim.MainWidth, dim.TerminalHeight, m.activePane == core.PaneTerminal)
		mainCol = lipgloss.JoinVertical(lipgloss.Left, detailPane, bottomRow, termPane)
	}

	content := lipgloss.JoinHorizontal(lipgloss.Top, commitListPane, mainCol)
	return lipgloss.JoinVertical(lipgloss.Left, header, tabBar, content)
}

// ── Pane Rendering ──────────────────────────────────────

func renderPane(title, content string, width, height int, focused bool) string {
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

// ── Async Commands ──────────────────────────────────────

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

func generateCommitMsgCmd(repoPath string, selectedFiles []git.FileEntry, provider ai.CommitMessageProvider) tea.Cmd {
	return func() tea.Msg {
		diff, err := git.GetSelectedDiff(repoPath, selectedFiles)
		if err != nil {
			return components.AIResultMsg{Err: err}
		}

		if strings.TrimSpace(diff) == "" {
			return components.AIResultMsg{
				Err: &ai.AIError{Code: ai.ErrEmptyDiff, Message: "no files selected — select files first"},
			}
		}

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

func commitCmd(repoPath string, selectedFiles []git.FileEntry, summary, description string) tea.Cmd {
	return func() tea.Msg {
		if len(selectedFiles) == 0 {
			return components.CommitResultMsg{Err: fmt.Errorf("no files selected — select files first")}
		}

		resetCmd := exec.Command("git", "reset", "HEAD")
		resetCmd.Dir = repoPath
		if out, err := resetCmd.CombinedOutput(); err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("resetting index: %s (%w)", string(out), err)}
		}

		if err := git.StageFiles(repoPath, selectedFiles); err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("staging selected files: %w", err)}
		}

		hasStaged, err := git.HasStagedChanges(repoPath)
		if err != nil {
			return components.CommitResultMsg{Err: fmt.Errorf("checking staged changes: %w", err)}
		}
		if !hasStaged {
			return components.CommitResultMsg{Err: fmt.Errorf("staging produced no changes")}
		}

		message := git.FormatCommitMessage(summary, description, nil)
		if err := git.Commit(repoPath, message); err != nil {
			return components.CommitResultMsg{Err: err}
		}

		return components.CommitResultMsg{Err: nil}
	}
}

func pushCmd(repoPath, branchName, upstream string, hasUpstream bool) tea.Cmd {
	return func() tea.Msg {
		var remote string
		if hasUpstream && upstream != "" {
			remote = git.RemoteFromUpstream(upstream)
		} else {
			var err error
			remote, err = git.GetDefaultRemote(repoPath)
			if err != nil {
				return pushResultMsg{err: fmt.Errorf("cannot push: %s", err)}
			}
		}

		opts := git.PushOptions{
			Remote:      remote,
			Branch:      branchName,
			SetUpstream: !hasUpstream,
		}

		if err := git.Push(repoPath, opts); err != nil {
			return pushResultMsg{err: err}
		}

		return pushResultMsg{err: nil}
	}
}

// ── Helpers ─────────────────────────────────────────────

func formatAheadBehindChange(msg FetchCompleteMsg) string {
	var parts []string

	if msg.NewBehind > msg.OldBehind {
		d := msg.NewBehind - msg.OldBehind
		parts = append(parts, fmt.Sprintf(
			"%d new commit(s) available to pull (now %d behind)",
			d, msg.NewBehind,
		))
	} else if msg.NewBehind < msg.OldBehind {
		parts = append(parts, fmt.Sprintf(
			"Now %d commit(s) behind (was %d)",
			msg.NewBehind, msg.OldBehind,
		))
	}

	if msg.NewAhead != msg.OldAhead {
		parts = append(parts, fmt.Sprintf(
			"Ahead count changed: %d → %d",
			msg.OldAhead, msg.NewAhead,
		))
	}

	if len(parts) == 0 {
		return "Remote tracking refs updated."
	}

	return strings.Join(parts, "\n")
}
