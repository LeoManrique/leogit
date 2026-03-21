package components

import (
	"os"
	"os/exec"

	tea "charm.land/bubbletea/v2"
	"github.com/creack/pty"
	"github.com/taigrr/bubbleterm"
)

// TerminalModel manages an embedded terminal pane with a PTY subprocess.
type TerminalModel struct {
	term     *bubbleterm.Model // terminal emulator (ANSI parser + screen buffer)
	ptmx     *os.File          // PTY master file (read/write to the subprocess)
	cmd      *exec.Cmd         // shell subprocess
	repoPath string            // working directory for the shell
	width    int               // current terminal width in columns
	height   int               // current terminal height in rows
	started  bool              // whether the PTY has been started
}

// NewTerminal creates a new terminal component. The PTY is not started yet —
// call Start() to spawn the shell subprocess.
func NewTerminal(repoPath string) TerminalModel {
	return TerminalModel{
		repoPath: repoPath,
	}
}

// Start spawns a PTY subprocess running the user's default shell.
// The shell opens in the repo root directory with the given initial size.
// Returns a tea.Cmd that can be used to initialize the bubbleterm component.
func (m *TerminalModel) Start(width, height int) tea.Cmd {
	if m.started {
		return nil
	}

	m.width = width
	m.height = height

	// Determine the shell to run
	shell := os.Getenv("SHELL")
	if shell == "" {
		shell = "/bin/sh"
	}

	// Create the shell command
	m.cmd = exec.Command(shell)
	m.cmd.Dir = m.repoPath
	m.cmd.Env = append(os.Environ(), "TERM=xterm-256color")

	// Start the PTY with the specified size
	var err error
	m.ptmx, err = pty.StartWithSize(m.cmd, &pty.Winsize{
		Rows: uint16(height),
		Cols: uint16(width),
	})
	if err != nil {
		// If PTY fails, the terminal pane will show nothing.
		// This can happen on unsupported platforms.
		return nil
	}

	// Create the bubbleterm emulator connected to the PTY.
	// The PTY master file is both the reader (shell output) and writer (user input).
	m.term, err = bubbleterm.NewWithPipes(width, height, m.ptmx, m.ptmx)
	if err != nil {
		m.ptmx.Close()
		return nil
	}

	m.started = true

	// Return the bubbleterm Init command to start reading PTY output
	return m.term.Init()
}

// Started returns whether the PTY subprocess has been started.
func (m *TerminalModel) Started() bool {
	return m.started
}

// Update handles messages for the terminal component.
// When focused, key events are forwarded to the PTY via bubbleterm.
func (m TerminalModel) Update(msg tea.Msg) (TerminalModel, tea.Cmd) {
	if !m.started || m.term == nil {
		return m, nil
	}

	termModel, cmd := m.term.Update(msg)
	m.term = termModel.(*bubbleterm.Model)
	return m, cmd
}

// View renders the terminal screen buffer as a string.
// Returns empty string if the terminal hasn't been started.
func (m TerminalModel) View() string {
	if !m.started || m.term == nil {
		return ""
	}
	return m.term.View().Content
}

// Focus tells bubbleterm to start capturing key events.
// While focused, all keypresses are forwarded to the PTY subprocess.
func (m *TerminalModel) Focus() {
	if m.term != nil {
		m.term.Focus()
	}
}

// Blur tells bubbleterm to stop capturing key events.
// Keypresses will no longer be forwarded to the PTY.
func (m *TerminalModel) Blur() {
	if m.term != nil {
		m.term.Blur()
	}
}

// Focused returns whether the terminal is currently capturing key events.
func (m *TerminalModel) Focused() bool {
	if m.term == nil {
		return false
	}
	return m.term.Focused()
}

// Resize updates the terminal dimensions and notifies the PTY subprocess.
// This is called when the terminal pane height changes (user resize or
// Ctrl+Shift+Up/Down) or when the window is resized.
func (m *TerminalModel) Resize(width, height int) tea.Cmd {
	if width < 1 || height < 1 {
		return nil
	}
	m.width = width
	m.height = height

	// Notify the PTY of the new size so the shell can reflow its output
	if m.ptmx != nil {
		_ = pty.Setsize(m.ptmx, &pty.Winsize{
			Rows: uint16(height),
			Cols: uint16(width),
		})
	}

	// Notify bubbleterm of the new size so it can resize its screen buffer
	if m.term != nil {
		return m.term.Resize(width, height)
	}
	return nil
}

// Close shuts down the PTY subprocess and terminal emulator.
// This should be called when the app exits.
func (m *TerminalModel) Close() {
	if m.term != nil {
		m.term.Close()
	}
	if m.ptmx != nil {
		m.ptmx.Close()
	}
	if m.cmd != nil && m.cmd.Process != nil {
		m.cmd.Process.Kill()
		m.cmd.Wait()
	}
}
