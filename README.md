# leogit

A calm desktop Git client built with Tauri 2, Rust, and Svelte 5. Designed to feel like the macOS apps it sits next to: dense, tabular, quick, no marketing pills.

## What it does

- **Stage and commit** via a checkbox file list with live unified diffs (native syntect syntax highlighting, optional side-by-side, optional whitespace-hidden).
- **AI commit messages** generated from the selected diff via the local `claude` CLI or a self-hosted Ollama instance.
- **Browse history** with a virtualized commit list, per-commit file diffs, and trailers/SHA copy.
- **Manage branches** (create / switch / delete) and **merge** (regular or squash) with conflict detection.
- **Open pull requests** through the GitHub CLI: list, check CI status, create, checkout.
- **Embedded terminal** docked at the bottom of the window, running the user's `$SHELL` in the repo directory.
- **Auto-fetch** + 2 s status polling so the UI stays in sync with anything the user does in another terminal.
- **Light / dark themes**, persistent layout, and a TOML config at `~/.config/leogit/config.toml`.

## Requirements

- `git` and (optionally) `gh` in `$PATH`.
- macOS 10.13+, Ubuntu 20.04+, or Windows 10+.
- Node.js 18+ and `pnpm` for development.
- Rust 1.95+ for building from source.

## Quick start

```bash
just install     # pnpm install in tauri-app
just dev         # launch dev build with hot reload
just build       # produce a debug bundle
just check       # type-check (svelte-check + cargo check)
```

(Or run the underlying `pnpm tauri …` commands directly — see `justfile`.)

On first run the app scans the configured paths (default: `~/Dev`, `~/dev`, `~/code`, `~/Code`, `~/Projects`, `~/src`) for git repos, then either opens the previously used one or shows a picker.

## Repository layout

```
tauri-app/
├── src/           # Svelte 5 frontend (TypeScript, runes)
└── src-tauri/     # Rust backend (Tauri commands)
```

See [DESIGN.md](DESIGN.md) for user-facing features and flows, [TECHNICAL.md](TECHNICAL.md) for architecture, [FRONTEND.md](FRONTEND.md) for the visual design language, and [ROADMAP.md](ROADMAP.md) for what's next.

## License

MIT (see LICENSE if present).
