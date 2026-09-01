# leogit

A calm desktop Git client built with Tauri 2, Rust, and Svelte 5. Designed to get Git things done quickly.

## What it does

- **Stage and commit** via a checkbox file list with live diffs — unified or side by side, switched from the diff's own header and remembered (native syntect syntax highlighting, optional whitespace-hidden). Shift-click or shift-arrow to select several rows, Space to include or exclude the lot, and a tri-state header checkbox that says how many of them are going in. Right-click a file to discard it — one row or a whole selection, and never-committed files go to the Trash, not oblivion — add it or its extension to `.gitignore`, copy its path, or hand it to Finder.
- **AI commit messages** generated from the selected diff via the local `claude` CLI or a self-hosted Ollama instance. Each provider keeps its own model, so switching between them never needs a Settings trip.
- **Browse history** with a virtualized commit list — keyboard-navigable, the newest commit selected for you — plus per-commit file diffs, SHA and tag copy, and checkout of any past commit (detached HEAD). Right-click the last commit to amend it (its message and co-authors reload into the composer) or undo it, keeping its changes.
- **Manage branches** from one menu in both clients — switch, create, delete, and **merge** (regular or squash, with a commit-count preview, conflicts reported as git wrote them, and an abort that is reachable even for a merge you started in the terminal). The list is re-read whenever the menu opens, so a branch made outside the app is there.
- **Sync from one adaptive button** whose face is whatever the repository needs next — Publish, Publish branch, Pull, Push, Fetch — with both ahead-behind counts on it, live transfer progress (an in-button fill plus git's own `Writing objects… MiB/s` line in the header), Fetch and force-push-with-lease (only on a genuinely diverged branch) under its chevron, and the same ladder on ⌘P. Or **publish a remote-less repo to GitHub** in one click via the GitHub CLI.
- **Embedded terminal** docked under the diff pane (⌃`), running the user's `$SHELL` in the repo directory — links open on ⌘/Ctrl-click, `vim` and `tmux` can set the system clipboard through OSC 52 (never read it), and nothing the shell prints is lost, including a shell that dies on its own startup file.
- **Auto-fetch** + a status poll that keeps the UI in sync with anything the user does in another terminal, on a cadence that follows the window: 2 s while you are in it, slower while you are not, and nothing at all published when nothing changed.
- **Light / dark themes**, a layout and window frame that reopen where you left them, and a TOML config in the platform config dir (`~/Library/Application Support/leogit/` on macOS, `~/.config/leogit/` on Linux) — edited from a Settings window in either client that applies each change as you make it, with no Save button and nothing to lose by closing.

## Requirements

- `git` and (optionally) `gh` in `$PATH`.
- macOS 26+, Ubuntu 20.04+, or Windows 10+.
- Node.js 18+ and `pnpm` for development.
- Rust 1.85+ for building from source (the workspace is on the 2024 edition).

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash
```

Auto-detects your platform: installs `LeoGit.app` into `/Applications` on macOS, or the AppImage into `~/.local/bin` with an app-menu launcher on Linux (arm64 or amd64; needs FUSE 2 — on Arch, `sudo pacman -S fuse2`). Or grab a `.zip` / `.AppImage` from the [latest release](https://github.com/LeoManrique/leogit/releases/latest).

A release holds one artifact per platform, each built from whichever client covers it: **macOS runs the native SwiftUI app, Linux and Windows run the Tauri one.** Same version, same features — see [FRONTEND.md](FRONTEND.md) §8 for the handful of places they present the same behaviour differently.

Re-run the same command to upgrade. leogit checks GitHub Releases at launch and, when a newer version exists, shows an **Update** chip in the header that hands you that command (or the installer download on Windows) — nothing downloads or restarts itself.

The installer also adds a `leogit [dir]` shell command to your shell config (zsh / bash / fish auto-detected). Run `leogit` to open the current repo or `leogit <path>` for another — it focuses the running window and switches repos instead of opening a duplicate, and running it from a subfolder opens the repo that contains it. Point it at a folder that isn't under version control yet and it offers to `git init` there and open the result. Open a new terminal (or `source` your shell rc) after installing to pick it up.

## Quick start

```bash
just install     # pnpm install in apps/tauri-app
just dev         # launch dev build with hot reload
just build       # produce a debug bundle
just check       # type-check (svelte-check + cargo check --workspace)
just mac-run     # build and launch the native macOS app (needs Xcode + xcodegen)
                 # add --no-build to relaunch the last build without rebuilding
just bundle      # build this platform's release bundle
just mac-install # install a Release build into /Applications (macOS)
just release     # build and publish a GitHub release; pass x.y.z to bump first
```

(Or run the underlying `pnpm tauri …` commands directly — see `justfile`.)

> **Windows:** `just` runs every recipe through `sh`, which ships with [Git for Windows](https://git-scm.com/download/win). Add `C:\Program Files\Git\usr\bin` to your `PATH` (then open a new terminal) so `sh` — and the `rm`/`[`/inline-env bits the recipes use — resolve. Avoid the `bash` at `C:\Windows\System32\bash.exe`; that's the WSL launcher, not Git Bash.

On first run the app scans the configured paths (default: `~/Dev`, `~/dev`, `~/code`, `~/Code`, `~/Projects`, `~/src`) for git repos, then either opens the previously used one or shows a picker.

## Repository layout

A Cargo workspace: all logic lives once in `core/`, and each client is a thin shell over it.

```
core/                    # leogit-core — Tauri-free Rust logic (git, diff, highlight,
                         #   terminal, config, gh, ai, …). The one host seam is
                         #   events::EventSink (streaming git progress + PTY output).
apps/
├── tauri-app/
│   ├── src/             # Svelte 5 frontend (TypeScript, runes)
│   └── src-tauri/       # Tauri host: one #[tauri::command] shim per core fn
└── swift-ui-app/        # Native macOS client (SwiftUI, Swift 6)
    ├── ffi/             # leogit-ffi: UniFFI bridge over the same core/
    └── Sources/LeoGit/  # App, Screens, Stores, IPC, Design
scripts/                 # release pipeline: build, deploy_release, install_local,
                         #   cleanup_releases (Python, sharing _common/_version/_build)
                         #   + install.sh, the curlable installer
Cargo.toml               # workspace root (target/ and Cargo.lock live here)
```

The SwiftUI macOS client links the same `core/` via UniFFI, so both clients run identical
logic — only the marshaling differs. It needs Xcode and `brew install xcodegen`; build it with
`just mac-run`. It currently covers open a repo → changes with per-file syntax-highlighted
diffs → commit (multi-select checkbox file list + message composer, AI-generated messages via the claude
CLI or Ollama sharing the Tauri client's config) → branches (switch / create / delete
and merge or squash-merge, with conflict + abort handling) → sync (pull / push / fetch with
live transfer progress, publish-branch first push, force-push-with-lease, and one-click
publish of a remote-less repo to GitHub, plus auto-fetch, status polling, and resync
on app re-activation) → history → an embedded terminal (SwiftTerm, ⌃` to toggle, fed by
the same core PTY as the Tauri client, staying open with the exit code when the shell
dies) → cloning (your GitHub repos via `gh` or any URL, with live progress) —
with a repo picker — the same searchable list in the toolbar switcher and on the screen
shown before a repository opens, rows named by their remote with dirty / pull / push
indicators — that restores the last opened repo at launch, or opens a folder handed to it
from outside (`open -a LeoGit <dir>`, the installed `leogit` command, a drop on the Dock
icon, Finder's Open With — one that isn't a repository yet is offered one), a menu bar
carrying the app's shortcuts, a release check with the same quiet
update chip, and a native Settings window
(⌘,) editing the same shared config. Every Tauri flow is ported, and it is what macOS
releases ship. Building it also needs Xcode's Metal Toolchain component
(`xcodebuild -downloadComponent MetalToolchain`, once) for SwiftTerm's shaders.

The pre-monorepo design is preserved on the `legacy/classic-design` branch (== tag `v0.1.32`);
recall it with `git worktree add ../leogit-classic legacy/classic-design`.

See [DESIGN.md](DESIGN.md) for user-facing features and flows, [TECHNICAL.md](TECHNICAL.md) for architecture, [STYLE.md](STYLE.md) for the visual design language, [FRONTEND.md](FRONTEND.md) for the frontend contract shared by the Tauri and SwiftUI clients, and [ROADMAP.md](ROADMAP.md) for what's next.

## License

MIT (see LICENSE if present).
