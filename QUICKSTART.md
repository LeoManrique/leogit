# leogit — Quick Start Guide

A modern visual Git client built with Tauri, Svelte, and Rust.

## Prerequisites

- **Git** (system git, must be in `$PATH`)
- **GitHub CLI** (`gh` command, optional but recommended for PR features)
- **Node.js** 18+ (tested with 24.12.0)
- **Rust** 1.95.0+
- **macOS**, **Linux**, or **Windows** (Tauri v2 supports all platforms)

## Installation

```bash
# Clone or enter the repo
cd leogit

# Install dependencies
pnpm install

# Verify everything compiles
pnpm check
cargo check --manifest-path src-tauri/Cargo.toml
```

## Development

### Option 1: Build and Run (Recommended for testing)
```bash
# Build frontend
pnpm run build:frontend

# Build app (creates .app on macOS)
pnpm tauri build --debug

# Or run release build
pnpm tauri build --release
```

### Option 2: Development Server (with live reload)
```bash
pnpm tauri dev
```
Opens app window with Vite dev server on http://localhost:5173.

## First Run

1. **Authenticate GitHub (if using PR features)**
   ```bash
   gh auth login
   ```
   Follow prompts. leogit will detect when authenticated.

2. **Open leogit**
   - App will auto-discover Git repos in /Users and /home directories (3 levels deep)
   - Or select a specific repo from the picker

3. **Try the main workflows**
   - **Changes tab**: Select files, review diffs, commit
   - **History tab**: View commit log, inspect commits
   - **AI commit messages**: Ctrl+G (requires Claude CLI or Ollama)
   - **Branches**: Create, switch, merge branches
   - **PRs**: Create, view, check CI status

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + Enter` | Commit staged changes |
| `Ctrl/Cmd + G` | Generate commit message with AI |
| `Ctrl/Cmd + P` | Cycle AI provider (Claude ↔ Ollama) |
| `Ctrl/Cmd + R` | Force refresh status |
| `Ctrl/Cmd + L` | Switch between Changes/History tabs |
| `` ` `` | Toggle embedded terminal |
| `Escape` | Close dialogs |

## Configuration

Settings are saved to `~/.config/leogit/config.toml`:

```toml
theme = "dark"                    # "dark" or "light"
auto_fetch = true                 # Auto-fetch every fetch_interval_ms
fetch_interval_ms = 30000         # 30 seconds
ai_provider = "claude"            # "claude" or "ollama"
syntax_highlighting = true        # Syntax highlight diffs
```

AI Providers:
- **Claude**: Requires `claude` CLI installed and authenticated
- **Ollama**: Requires local Ollama running on http://localhost:11434

## Features

### ✅ Implemented
- [x] Visual file status with diffs
- [x] Commit history with metadata
- [x] Branch management (create, switch, delete, merge)
- [x] Pull request workflow (list, checks, create, checkout)
- [x] AI commit message generation (Claude, Ollama)
- [x] Syntax-highlighted diffs
- [x] Embedded terminal
- [x] Auto-fetch with ahead/behind tracking
- [x] Dark and light themes

### 🎯 Future
- [ ] Stash management
- [ ] Rebase interactive UI
- [ ] Search commits
- [ ] Blame/Annotate
- [ ] Custom keybindings
- [ ] Plugin system

## Troubleshooting

### "GitHub authentication required"
```bash
gh auth login
```

### Terminal not working
- Terminal requires a PTY (pseudo-terminal)
- Linux/macOS: should work out of the box
- Windows: requires Windows Terminal or similar

### Git operations slow
- Check `fetch_interval_ms` in settings
- Try disabling `auto_fetch` if on slow network
- Run on a repo with fewer commits for testing

### AI commit message fails
- Verify Claude CLI: `claude --version`
- Or verify Ollama: `curl http://localhost:11434/api/tags`
- Check `ai_provider` in settings

## Development

### Project Structure
```
leogit/
  src-tauri/              Rust backend (Tauri commands)
    src/commands/         Git, GitHub, diff, AI, config commands
  src/                    Svelte frontend
    lib/components/       Reusable UI components
    lib/views/           Full-page views and overlays
    lib/stores/          State management
    lib/api/             Typed Tauri command wrappers
```

### Adding a Feature
1. Add Tauri command in `src-tauri/src/commands/*.rs`
2. Register command in `src-tauri/src/main.rs`
3. Add command wrapper in `src/lib/api/commands.ts`
4. Use in components via stores and API

### Building Release
```bash
# macOS
pnpm tauri build --release

# Notarization (macOS, optional)
# Configure in src-tauri/tauri.conf.json

# Windows
pnpm tauri build --release

# Linux
pnpm tauri build --release
```

## Support

- **Issues**: Check `POST_IMPLEMENTATION_TODO.md` for known limitations
- **Code**: See `IMPLEMENTATION.md` for architecture and design decisions
- **Test**: `pnpm check` (TypeScript), `cargo check` (Rust)

## License

MIT (See LICENSE if present)
