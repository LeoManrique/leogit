package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/LeoManrique/leogit/internal/config"
	"github.com/LeoManrique/leogit/internal/tui/app"
)

func main() {
	// Optional first argument: path to a git repository
	var repoPath string
	if len(os.Args) > 1 {
		repoPath = os.Args[1]
	}

	// Load configuration (missing file is fine — defaults are used)
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error loading config: %v\n", err)
		os.Exit(1)
	}

	// Create and run the TUI
	// AltScreen, mouse mode, and focus reporting are set declaratively
	// in View() via tea.View fields (Bubbletea v2 pattern).
	model := app.New(cfg, repoPath)
	p := tea.NewProgram(model)

	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "Error running app: %v\n", err)
		os.Exit(1)
	}
}
