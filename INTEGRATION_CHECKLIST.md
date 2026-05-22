# Integration & Testing Checklist

This document tracks integration testing and final verification steps for the Tauri migration.

## ✅ Architecture & Setup

- [x] Tauri v2 scaffolding complete
- [x] Rust backend (src-tauri/) fully configured
- [x] Frontend (src/) with Svelte 5 setup
- [x] Path aliases configured (vite.config.ts, tsconfig.json)
- [x] Build scripts configured (Makefile, package.json)
- [x] Dependencies resolved (npm, cargo)

## ✅ Rust Backend

### Core Modules
- [x] config.rs - Config loading/saving (TOML), state persistence (JSON)
- [x] git.rs - All git operations (27 functions)
- [x] diff.rs - Diff parsing and patch generation
- [x] gh.rs - GitHub PR management
- [x] ai.rs - Claude and Ollama integration

### Status: All modules compiling without errors ✓

## ✅ Frontend Components

### Layout & Navigation
- [x] App.svelte - Startup state machine
- [x] MainLayout.svelte - Two-column grid layout
- [x] Header.svelte - Branch info, refresh button
- [x] TabBar.svelte - Changes/History tab switching

### Changes Tab
- [x] FileList.svelte - Virtual scrolling, selection
- [x] DiffViewer.svelte - Syntax highlighting (Shiki), line selection
- [x] CommitMessage.svelte - AI generation, commit UI

### History Tab
- [x] CommitList.svelte - Virtual scroll, pagination
- [x] CommitDetail.svelte - Read-only commit metadata

### Overlays
- [x] RepoPicker.svelte - Fuzzy-filterable repo selection
- [x] BranchDropdown.svelte - Branch management (create, delete, switch)
- [x] MergeOverlay.svelte - Merge/squash merge UI
- [x] SettingsOverlay.svelte - Config UI
- [x] ErrorModal.svelte - Error display with retry
- [x] HelpOverlay.svelte - Keyboard shortcuts reference

### Utilities
- [x] Terminal.svelte - xterm.js integration
- [x] KeyboardShortcuts.svelte - Global hotkey handler

### Status: 16 components created ✓

## ⏳ Integration in Progress (Agent Tasks)

### Terminal PTY
- [ ] terminal.rs - Rust PTY command module
  - [ ] start_terminal(repo_path) → pid
  - [ ] write_terminal(pid, data)
  - [ ] resize_terminal(pid, cols, rows)
  - [ ] Terminal output streaming via Tauri events
  - [ ] Process management (HashMap of active PTYs)
  - [ ] Cleanup on app shutdown

### Background Polling & Lifecycle
- [ ] MainLayout.svelte updates:
  - [ ] Status polling every 2s
  - [ ] Auto-fetch on interval (configurable)
  - [ ] Focus detection (visibilitychange)
  - [ ] Immediate refresh on window focus
  - [ ] Error handling with retry
  - [ ] Cleanup on unmount

### Integration Tasks
- [ ] Register terminal commands in main.rs
- [ ] Update api/commands.ts with terminal wrappers
- [ ] Connect Terminal.svelte to PTY via Tauri events
- [ ] Connect KeyboardShortcuts to CommitMessage actions
- [ ] Connect HelpOverlay to UI

## 🧪 Testing Checklist

### Basic Operations
- [ ] App launches without errors
- [ ] Repo picker discovers repos
- [ ] Status tab shows files correctly
- [ ] File selection toggles
- [ ] Diff view shows syntax highlighting
- [ ] History tab shows commits

### Git Operations
- [ ] Stage/unstage files
- [ ] Create commits
- [ ] Switch branches
- [ ] Create branches
- [ ] Merge branches
- [ ] Fetch/pull operations
- [ ] Ahead/behind tracking

### Advanced Features
- [ ] AI commit message generation (Claude)
- [ ] AI commit message generation (Ollama)
- [ ] PR listing and checkout
- [ ] Terminal command execution
- [ ] Auto-fetch on interval
- [ ] Status refresh on focus

### Keyboard Shortcuts
- [ ] `Ctrl+Enter` - Commit
- [ ] `Ctrl+G` - Generate message
- [ ] `Ctrl+P` - Cycle provider
- [ ] `Ctrl+R` - Refresh
- [ ] `Ctrl+L` - Switch tab
- [ ] `` ` `` - Toggle terminal

### UI/UX
- [ ] Dark theme applies correctly
- [ ] Light theme applies correctly
- [ ] Layouts responsive on different window sizes
- [ ] Modals overlay correctly
- [ ] Error messages display
- [ ] Loading states show

### Performance
- [ ] Large repos (1000+ commits) load smoothly
- [ ] File lists scroll smoothly (100+ files)
- [ ] Diffs render quickly (large files)
- [ ] Memory usage reasonable (no leaks)
- [ ] CPU usage low at idle

## 🚀 Build & Deployment

- [ ] Verify build compiles without errors
- [ ] Verify build compiles without warnings
- [ ] Build creates executable
- [ ] App launches from executable
- [ ] App can be signed (macOS)
- [ ] App can be notarized (macOS)
- [ ] App works on Linux
- [ ] App works on Windows

## 📋 Code Quality

- [ ] No `allow_dead_code` attributes used
- [ ] All imports used (no dead code)
- [ ] Type safety verified (no `any` types)
- [ ] Error handling throughout
- [ ] Comments for non-obvious logic
- [ ] Keyboard accessible (a11y)

## 📚 Documentation

- [x] QUICKSTART.md - User guide
- [x] INTEGRATION_CHECKLIST.md - This file
- [x] IMPLEMENTATION.md - Technical architecture (Go version, needs update)
- [ ] Update IMPLEMENTATION.md for Tauri architecture
- [ ] API documentation (jsdoc/rustdoc)

## 🎯 Release Checklist

- [ ] All tests passing
- [ ] All keyboard shortcuts working
- [ ] README.md updated
- [ ] Version bump (0.0.1 → 0.1.0)
- [ ] Changelog written
- [ ] Release notes prepared
- [ ] Binary signed and notarized (if applicable)

---

**Last Updated**: 2026-05-21  
**Status**: ~90% complete (Terminal and polling tasks in progress)  
**Next**: Integrate terminal PTY and background polling when agent tasks complete
