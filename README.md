# leogit

A calm desktop Git client built with Tauri 2, Rust, and Svelte 5. Designed to get Git things done quickly.

## What it does

- **Stage and commit** via a checkbox file list with live unified diffs (native syntect syntax highlighting, optional side-by-side, optional whitespace-hidden).
- **AI commit messages** generated from the selected diff via the local `claude` CLI or a self-hosted Ollama instance.
- **Browse history** with a virtualized commit list, per-commit file diffs, trailers/SHA copy, and checkout of any past commit (detached HEAD).
- **Manage branches** (create / switch / delete) and **merge** (regular or squash) with conflict detection.
- **Push / pull** with ahead-behind badges, live transfer progress (an in-button progress fill plus git's own `Writing objects… MiB/s` line in the header), and force-push-with-lease (offered only when the branch has diverged) — or **publish a remote-less repo to GitHub** in one click via the GitHub CLI.
- **Open pull requests** through the GitHub CLI: list, check CI status, create, checkout.
- **Embedded terminal** docked at the bottom of the window, running the user's `$SHELL` in the repo directory.
- **Auto-fetch** + 2 s status polling so the UI stays in sync with anything the user does in another terminal.
- **Light / dark themes**, persistent layout, and a TOML config at `~/.config/leogit/config.toml`.

## Requirements

- `git` and (optionally) `gh` in `$PATH`.
- macOS 10.13+, Ubuntu 20.04+, or Windows 10+.
- Node.js 18+ and `pnpm` for development.
- Rust 1.95+ for building from source.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash
```

Auto-detects your platform: installs `leogit.app` into `/Applications` on macOS, or the AppImage into `~/.local/bin` with an app-menu launcher on Linux (x86_64; needs WebKitGTK 4.1 + FUSE 2 — on Arch, `sudo pacman -S webkit2gtk-4.1 fuse2`). Or grab a `.zip` / `.AppImage` from the [latest release](https://github.com/LeoManrique/leogit/releases/latest).

The installer also adds a `leogit [dir]` shell command to your shell config (zsh / bash / fish auto-detected). Run `leogit` to open the current repo or `leogit <path>` for another — it focuses the running window and switches repos instead of opening a duplicate, and running it from a subfolder opens the repo that contains it. Point it at a folder that isn't under version control yet and it offers to `git init` there and open the result. Open a new terminal (or `source` your shell rc) after installing to pick it up.

## Quick start

```bash
just install     # pnpm install in tauri-app
just dev         # launch dev build with hot reload
just build       # produce a debug bundle
just check       # type-check (svelte-check + cargo check)
```

(Or run the underlying `pnpm tauri …` commands directly — see `justfile`.)

> **Windows:** `just` runs every recipe through `sh`, which ships with [Git for Windows](https://git-scm.com/download/win). Add `C:\Program Files\Git\usr\bin` to your `PATH` (then open a new terminal) so `sh` — and the `rm`/`[`/inline-env bits the recipes use — resolve. Avoid the `bash` at `C:\Windows\System32\bash.exe`; that's the WSL launcher, not Git Bash.

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
