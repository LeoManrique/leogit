# leogit

A desktop Git client with a clean visual interface, built with Tauri v2, Rust, and Svelte 5.

## Features

- **Changes view**: Stage/unstage lines of code with visual diff viewer
- **History view**: Browse commits with full details and commit file lists
- **Branch management**: Create, switch, delete, rename branches with merge options
- **Pull requests**: List, create, view CI checks, checkout PR branches via GitHub CLI
- **AI commit messages**: Generate commit title + description from staged diff (Claude or Ollama)
- **Embedded terminal**: Shell access in repo directory via backtick key
- **Auto-fetch**: Configurable background fetch with ahead/behind indicators
- **Syntax highlighting**: Colored diffs with language detection
- **Settings**: Theme (light/dark), AI provider, fetch interval, per-repo state

## Requirements

- **macOS/Linux/Windows**: Tauri v2 requires OS support (10.13+, Ubuntu 20.04+, Windows 10+)
- **Node.js 18+**: Frontend build and dev server
- **Rust toolchain**: Backend build (cargo)
- **git** and **gh** in `$PATH`: Version control and GitHub integration

## Quick Start

```bash
pnpm install
pnpm tauri dev
```

See `QUICKSTART.md` for detailed setup instructions.

## Architecture

- **Backend** (`src-tauri/`): Rust commands for git ops, AI, config, PTY
- **Frontend** (`src/`): Svelte 5 SPA with TypeScript, reactive stores, CSS custom properties
- **IPC Bridge**: Tauri invoke for type-safe async command dispatch
- **Config**: TOML at `~/.config/leogit/config.toml`

See `IMPLEMENTATION.md` for detailed architecture and development guide.
