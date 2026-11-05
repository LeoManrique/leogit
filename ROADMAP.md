# LeoGit Roadmap

Open work, by theme, in no particular order. Shipped work leaves this file. Much of it is GitHub Desktop parity, so "GHD" below means that app.

## Sync and remotes

- [ ] No-upstream ahead/behind ignores `pushRemote` and `pushDefault`, so counts can describe a remote the next push won't reach. Fixing it costs a `git config` spawn on the poll, or teaching the config reader `branch.*`. A decision first.
- [ ] Force-push sheets name `status.upstream`, not the push target. Only wrong from the chevron's *Force Push (with Lease)…* under a Pull face, with `pushRemote` or `pushDefault` set.
- [ ] Push with an explicit `<branch>:<upstream branch>` refspec. `git push <remote> <branch>` creates a stray remote branch when the upstream has another name, and `remote.<name>.push` remaps it. `sync_ladder::force_push_lease` must then use the upstream name. Until then, Force Push is proposed only where names match.
- [ ] Decide what the sync button is during an open merge, cherry-pick or revert: as is, disabled, or Pull and Force Push withheld. The rule goes in `sync_ladder::propose`.
- [ ] "Last fetched…" caption on the sync button. Both clients already record it (`repoSync.ts`, `FetchCooldown`).
- [ ] Force-push confirm: say how many remote commits get discarded.
- [ ] Plain-language push rejection: "the remote has commits you don't have, pull first", over the raw non-fast-forward text.
- [ ] Tauri: re-read status after a failed push or fetch, as a failed pull does.
- [ ] Say what a merge or pull did ("Merged x into y", "Pulled 3 commits") in the status strip, not a toast.

## Commit composer

- [ ] Grow the composer on focus so summary and description both show.
- [ ] After Generate, scroll the description to the top. Long output shows its tail.
- [ ] Confirm before Generate replaces a message the user typed.
- [ ] AI generation respects line selection. `get_selected_diff` sends whole files, so this is core work, and it lands with per-line staging.
- [ ] Commit button: name the branch, and say why it's disabled.
- [ ] Hint past 50 summary characters (GHD's lightbulb).
- [ ] Setting to hide the /72 summary counter.
- [ ] Co-author input with `@` autocomplete; pills saved as `Co-Authored-By` trailers.
- [ ] Commit warnings for repo rules and branch protection. An over-long subject only colors the counter.
- [ ] Push-protection / secret-scanning dialog with optional bypass-with-reason.
- [ ] Oversized-files warning with one-click "Add to Git LFS".

## Changes and diff

- [ ] Finish per-line staging in Tauri, then port it to SwiftUI. Selection dots and `showSelection` are scaffolded, `diff::generate_patch` is tested. Needs the selection store, per-hunk checkboxes, patch-based commits, a brighter `.selection-dot`, and the `DiffSelection` `usize` UniFFI fix.
- [ ] Line-level discard from the diff.
- [ ] Copy selected diff lines in Tauri. `copyDiffText` is wrapped in `commands.ts`, but nothing calls it yet.
- [ ] Bulk ignore for a multi-row selection.
- [ ] List-level context menu: Discard All, Stash All.
- [ ] File-row menu: Ignore Folder, and Open in Editor (needs the editor picker).
- [ ] Filter input on the changes list, with a "committing only filtered files" confirmation.
- [ ] Retry dialog for files git couldn't discard.
- [ ] Cap the Changes tab count at 300+, as GHD does.
- [ ] Hide-whitespace toggle in the diff header. It only exists in Settings.
- [ ] Tauri multi-line diff copy (DF-6(T) in the [parity plan](docs/plans/cross-client-feature-parity.md)). Core's `copy_diff_text` has no caller, and a side-by-side drag-select interleaves columns.
- [ ] Extract DiffViewer's unified/split control as a component, so the next segmented control reuses it.

## History

- [ ] Interactive rebase: edit, drop, drag-and-drop. `history_rewrite/replay.rs` takes any todo. Drag reuses reorder's placement model.
- [ ] An undo after a hand-made commit in the middle of a conflicted cherry-pick removes that commit too (reflog keeps it). Telling them apart needs a list of what the pick made, which nothing keeps.
- [ ] Commit search: author, message regex, path, date range.
- [ ] Blame: inline in the diff, plus a dedicated view.
- [ ] File history jump from the diff viewer.
- [ ] Read-only reflog browser.
- [ ] Revert from the commit menu, and cherry-pick into the current branch (needs browsing another branch's history).
- [ ] Drag a commit onto the branch dropdown to cherry-pick.
- [ ] Commit menu: Copy Full Message.
- [ ] Commit menu: Create Branch from Commit. `create_branch` already takes a start point.
- [ ] Commit menu: View on GitHub. Needs the remote's host, see the toolbar item.
- [ ] Tag chips in the commit detail card. Only the list shows them.
- [ ] Expand a long commit body past the card's 140 pt scroll cap.
- [ ] Commit detail file rows get a context menu: Copy Path, later View on GitHub.
- [ ] Undo banner in Changes for the latest unpushed commit. Today only History's menu offers it.
- [ ] Warn when Undo Last Commit lands on a dirty tree, since the undone files mix with uncommitted ones.
- [ ] Tags: view, push, delete, with an unpushed indicator and counter.

## Branches and conflicts

- [ ] Branch rename and delete-on-remote, built on both clients at once. Rename goes in the row menu, "also delete on the remote" in the delete dialog, backed by a combined core `delete_branch(name, include_remote)`.
- [ ] Base-branch picker in the create-branch form.
- [ ] Resolve conflicts in-app: ours/theirs per hunk, mark resolved. Today that means the terminal.
- [ ] A way out of conflicts no operation owns: squash merge, `cherry-pick -n`, `stash pop`, `git am`. Needs a status fact ("unmerged entries, no operation") and an escape: `git reset --merge`, or `git am --abort` for the last.
- [ ] Conflict modal auto-opened when a repo opens mid-operation.
- [ ] Native delete and abort confirmations can't say "another operation is running". They're `confirmationDialog`s with no live state, so make them sheets.
- [ ] "Local Changes Will Be Overwritten" dialog with one-click Stash-and-Continue.

## Stash

- [ ] Stash backend and UI: `git stash push -m`, `list`, `apply`, `pop`, `drop`.
- [ ] Stash all changes, and Stash-and-switch-branch dialogs.
- [ ] Sidebar entry when a stash exists, opening a stash diff with Restore / Discard.

## Repositories

- [ ] Full submodule support: clean pointer commits, nested diffs, and one app-wide decision on when the app updates a submodule's files. Undo names `git submodule update`, the other actions say nothing.
- [ ] Multi-repo workspace in a single window.
- [ ] Per-repo settings dialog: Remote, Ignored Files, Git Config, Fork Settings.
- [ ] Live `.gitignore` editor inside repo settings.
- [ ] Per-repo `user.name` / `user.email` override editor.
- [ ] Tauri: merge the rows of `RepoDropdown` and `RepoPicker` into one view, as `RepoPickerList.swift` does (P-32 in the [re-skin plan](docs/plans/tauri-reskin.md)).

## Toolbar, menus and shortcuts

- [ ] [Rest of the toolbar context actions](docs/plans/toolbar-context-actions.md): Rename Branch, the repo menu on picker rows, View on GitHub. That last one waits on `RepoIdentifier` carrying the host, since a URL without it is wrong for non-GitHub remotes.
- [ ] Reveal the repository in Finder / Explorer from the repo chip and app menus. File rows have it.
- [ ] Tauri: shortcut modifier follows the platform, ⌘ on macOS, Ctrl elsewhere. Today `, R P B L G Enter` answer to both, so the terminal swallows them. The Help overlay's "Ctrl/Cmd" label goes with it. One platform test must feed the window handlers and xterm's key handler. Revisit when noticed.
- [ ] Full menu bar. Missing: `Cmd+T`, `Cmd+Shift+M/H/E`, `Cmd+Shift+R/D`, `Cmd+Shift+B`, `Ctrl+H`, `Cmd+8/9`, `Cmd+0/=/-`.
- [ ] In-app menubar in the title bar on Windows and Linux.
- [ ] "Show Keyboard Shortcuts" menu entry, richer than Help.
- [ ] Menu items: Show Logs, User Guides, Report Issue, Contact Support.
- [ ] Custom keybindings in Settings, saved to `config.toml`.
- [ ] Resizable toolbar buttons (`Cmd-8/9`).
- [ ] Custom window controls on Windows and Linux, with full-screen and zoom.
- [ ] Add the branch to the OS window title, which names only the repo.
- [ ] Terminal chord as a visible hint on the collapsed strip. Tooltip covers it, so decline unless a new user misses it.
- [ ] Persist the native sidebar width. `HSplitView` exposes no position, so use an `NSSplitView` bridge with `autosaveName` or a hand-rolled handle.
- [ ] Aria-live region announcing async results ("Pulled 3 commits").
- [ ] Icon registry draws glyphs near-square where SF Symbols share a cap height, so wide symbols run narrow (P-33 in the re-skin plan).

## Settings

- [ ] Tauri: "System" theme that tracks OS changes. Today it's Dark or Light.
- [ ] Accounts: sign in and out of dotcom and Enterprise, list accounts.
- [ ] Integrations: editor, shell, custom-integration override.
- [ ] Git: per-account and per-repo identity, default branch, edit-global-config link.
- [ ] Prompts: confirm on discard, force push, undo commit, message override, plus `uncommittedChangesStrategy` (Move / Stash / Ask).
- [ ] "Don't ask again" checkboxes on destructive confirms, backed by Prompts.
- [ ] Advanced: usage tracking, repo indicators, OpenSSH on Windows, external credential helper.
- [ ] Accessibility: underline links, diff check marks.

## Integrations

- [ ] External editor picker with auto-detection and custom override.
- [ ] External shell picker (Terminal, iTerm, Hyper, Windows Terminal, …) with a custom form.
- [ ] "Open in WSL" on Windows for `\\wsl$\` repos.
- [ ] `x-leogit://` protocol handler for clone and OAuth-callback links.
- [ ] Desktop notifications for CI failure.
- [ ] Notification permission prompt and Configure Notifications panel.
- [ ] "Create GitHub Project" via `gh project create`. Check `check_auth` at the point of use, since sign-in happens outside the app.
- [ ] Plugin system: stable command surface and a manifest format.

## Onboarding and help

- [ ] Install-Git flow when `git` isn't on `$PATH`.
- [ ] Tutorial overlay: create branch, edit file, commit, publish.
- [ ] About dialog with version, architecture, manual update check, release-notes link.
- [ ] Release Notes dialog from a JSON changelog, grouped New / Fixed / Improved.
- [ ] Crash window (separate process) with friendly error and Report Issue.
- [ ] OS-no-longer-supported banner.
- [ ] Recovery dialog for a global git-config lock file.
- [ ] Localization scaffolding. No strings catalog yet.
- [ ] Marketing site.

## Performance and engineering

- [ ] Lean highlight request ([I/O plan](docs/plans/io-and-network-efficiency.md) §7 Q6). Tauri renders HTML from `content`, so the win means moving span-building into the WebView. Trace a 3,000-row diff before deciding.
- [ ] SSH `ControlMaster` for background sweeps. Unmeasurable until there's an SSH remote to test.
- [ ] `PathText` resize debounce. Trace a sidebar drag with ~40 changed files first, users could feel this change.
- [ ] Token cache for highlighting, only if rapid file-switching feels heavy. Key on blob OID, working tree on (mtime, len).
- [ ] Tauri diff virtualization, only if large diffs still feel heavy after the size guard. Wrapped rows vary in height, so measure and cache as GHD's `CellMeasurer` does. Native needs nothing.
- [ ] Clippy pedantic backlog: `git.rs`, `diff.rs`, `gh.rs`, `config.rs`, `highlight.rs`. No new warnings, fix old ones in any function you touch.
- [ ] Drop the `@typescript/typescript6` alias once TS 7.1 ships the programmatic API and svelte-check supports it.
- [ ] Connectivity observer in `core::net` (WS-K in the parity plan): three OS backends and a new `CoreEvent`. Needs a Linux machine.
- [ ] Re-skin's on-target checks ([plan](docs/plans/tauri-reskin.md) §8) need a Windows and a Linux build.

## Release

- [ ] Developer ID signing and notarization on macOS. Bundles are ad-hoc signed.
- [ ] In-app auto-update. Needs a signed release feed. Today the app detects a release and hands over the command or link.
- [ ] Installing-Update view and Move-to-Applications prompt on macOS, once auto-update lands.
- [ ] CI build matrix: macOS, Linux, Windows on push.
- [ ] Opt-in crash reporting through a privacy-respecting endpoint.
