# leogit: Functional Design

What the user sees and does.

## Product intent

A desktop Git client for people who already know git on the command line and want a calm surface for what they repeat all day: staging, committing, browsing history, switching branches, syncing. Every operation is a real `git` call against the on-disk repo, so you can shell out at any moment and both sides stay in sync.

## Clients

macOS releases ship the native SwiftUI client. Windows and Linux ship the Tauri client. Both implement every flow below and share one config and state, so each resumes where the other left off. Where they present something differently, [FRONTEND.md](FRONTEND.md) §8 says how and why. ⌘ below means Ctrl off macOS.

## Top-level flows

### 1. Launch and repo discovery

On first launch the app writes a default config and scans `~/Dev`, `~/dev`, `~/code`, `~/Code`, `~/Projects` and `~/src` (used whenever `scan_paths` is empty), 3 levels deep. A subtree stops at its first `.git`.

What opens, first match wins:

1. A folder named from outside the app, even outside the scan paths.
2. The last opened repo, if it's still a repo. It's listed whether or not the walk found it.
3. The only repo discovered.
4. The repo picker.

**Picker.** The same list and filter as the header's repo switcher, with *Clone Repository…* in the footer. Settings stays reachable, so finding nothing is never a dead end.

- An empty list says one of three things: *Looking for repositories…*, *No repositories found* with the searched folders in full, or *No matching repositories* under a filter. The dead ends offer *Choose Folders to Search*.
- Discovery re-runs when the switcher opens and when scan paths or depth change. A re-walk never auto-enters a repo, even if only one now matches.
- A failed re-walk keeps the rows and adds *Couldn't search for repositories.* with Retry. Only a very first walk may show an error screen.
- Deliberately no "open this folder" action. A repo missing from the list means the scan paths are wrong, and fixing them also holds next launch.

**Open from the terminal.** The curl installer adds a `leogit [dir]` shell function to zsh, bash or fish, like `code .`.

- `leogit` opens the current directory, `leogit <path>` that one. A subfolder opens the repo containing it.
- A running app focuses its window and switches repo. There's never a second window.
- A path that isn't a directory prints `leogit: no such directory: <path>` and opens nothing.
- On macOS, `open -a LeoGit <dir>`, a Dock drop and Finder's *Open With* do the same.

**Not a repository yet.** A named folder outside version control raises *Create a repository here?* in any phase, noting that files stay put and nothing gets committed. Confirming runs `git init` (`main` unless `init.defaultBranch` says otherwise) and opens it. Cancelling leaves you where you were.

### 2. Main view

A permanent split: sidebar on the left, content on the right, terminal docked under the content.

| Tab | Sidebar | Content |
|---|---|---|
| Changes (shows a file count) | File list, commit composer resizable from its top edge | Diff of the active file |
| History | Commit list | Commit detail, commit files, per-file diff |

On a clean tree the composer stays, with *No changes* in the list. A just-opened repo's list stays blank until its first status arrives. The first changed file opens itself.

**Header:** repo chip, branch chip, git's live progress line, then the actions. Right-clicking the repo chip offers *Copy Repo Name* and *Copy Repo Path*, the branch chip *Copy Branch Name*.

**Repo switcher:** a filter over all known repos, with *Clone Repository…* in the footer.

- Rows use the remote's repo name, else the folder name, with a muted `owner/` only where two rows share a name.
- The filter is loose on names (`gpm` finds `git-projects-manager`) and strict on paths, only below the scan folder, since shared ancestry would match everything.
- Sort: last opened by you (default) or A to Z.
- Each row shows a dirty dot, exactly when its Changes tab would list a file, plus `↓N` and `↑N`.
- Rows can't be picked during a push, pull or publish, since nothing would report that transfer after a switch.

### 3. Staging and committing

There's no staging area: the checked list is the commit.

**File list**
- Every change, untracked included, with a status letter and a checkbox, under a tri-state header reading *3 of 12 files included*.
- Everything starts checked. An unchecked file stays unchecked across polls. If it leaves the list, the opt-out lasts a little while, so a formatter rewriting a file can't silently re-include it.
- Embedded repo: blue ↪ badge, commits as a link, always asks first. Dirty submodule: muted ↪, can't be checked.
- Highlight and checkboxes are separate: pointing vs. what the commit will contain. Shift-click and shift-arrow extend, ⌘-click flips a row, ⌘A takes all. Space toggles the highlighted rows: include all if any is excluded, else exclude all.
- Right-click: Discard Changes…, Ignore File, Ignore All .ext Files, Copy File Path, Copy Relative File Path, Reveal, Open with Default Program. Over a multi-selection, only Discard N Selected Changes….
- Discard always confirms and states the outcome: restored from HEAD, or moved to the OS Trash if never committed.

**Diff pane**
- Unified or side by side, from the diff's own header, remembered. Lines always wrap.
- A different diff starts at line one. The same diff re-read keeps its offset. Loading never blanks the pane.
- Nothing to show gets a specific reason: matches the commit, whitespace hidden, or changed with no lines. Binary files and dirty submodules get their own messages.
- A diff over 4 MB, or with a line over 5,000 characters, is withheld with *Show Diff Anyway*. The bypass holds for that diff and resets on another file or commit.
- Copying yields the file's own lines, no gutters or prefixes.

**Composer**
- Summary with a soft 72-character counter, plus Description. With one file checked, the placeholder is *Create/Update/Delete \<file\>* and committing untyped uses it.
- Commit and Generate lock each other out, so a late AI result can't overwrite a cleared composer.
- **Amend** (from History) prefills message and co-authors, shows *Stop Amending*, and allows message-only amends. It ends whenever HEAD stops being that commit.
- **While a rebase, cherry-pick or revert is stopped**, the fields lock and the button becomes *Continue Rebase* (etc.). It stages only the conflicted files, ignoring the checkboxes, and refuses by name over a leftover conflict marker. A merge is the exception: you conclude it with an ordinary commit.

### 4. AI commit message generation

- The composer has a provider picker (Claude or Ollama), the same setting as Settings, and Generate. Each provider remembers its own model.
- Generate sends the checked files' diff and fills summary and description, overwriting what's there.
- Claude uses the local `claude` CLI, default model `sonnet`, diff cap 20 MB. Ollama uses `http://localhost:11434` or `ollama_server_url`, default model `tavernari/git-commit-message:latest`, cap 50 MB. Timeout defaults to 120 s.
- Generate is disabled when the provider can't answer, signed-out CLI included, and a strip under Description says why. An unfinished or unreadable check leaves it enabled.
- If a command would fix it, the app offers it spelled out (`Run claude auth login`) and types it into the embedded terminal. Deliberately never drives sign-in itself.

### 5. History browsing

**List.** Newest first from HEAD, loading more as you scroll. The newest commit is selected on arrival. **Selection** is a set, using the file list's gestures, non-adjacent allowed, always at least one. Right-clicking outside it selects that commit first.

**Commit menu**

| Item | Offered on |
|---|---|
| Amend Last Commit… | HEAD |
| Undo Last Commit (runs at once, message returns to the composer) | HEAD, if unpushed |
| Check Out Commit… | Any non-HEAD commit |
| Cherry-pick, Reorder | One or several commits |
| Squash N Commits… | Two or more |
| Copy SHA, Copy Tag | One commit |

**Check out** warns about detached HEAD first. The branch chip then reads `Detached at <sha>` and sync is disabled. Picking a branch returns you.

**Cherry-pick** copies commits onto a local branch you choose, oldest first, leaving you there with the new commits selected. *Abort Cherry-pick…* undoes the picks and returns you to the original branch.

**Squash** folds the selection into the oldest selected commit, which keeps its author and date. Unselected commits replay on top. The dialog prefills a message from all of them and warns if a force push follows. Disabled with a merge commit between the tip and the oldest selected.

**Reorder** has no dialog: the list asks where. An insertion line appears, `↑` `↓` move it, `Enter` applies, `Esc` or any click cancels. It never stops below a merge commit. Commits move as one block in their existing order. It asks first only if pushed commits get rewritten.

**All three**
- Disabled during an operation, on a detached HEAD, or during another write. Uncommitted tracked changes block them, by name.
- A conflict moves you to Changes with git's text, then Continue or Abort. Any other failure (hook, signing key, untracked file in the way) rolls back on its own.
- **Undo:** one line under the header, also after a conflict you resolved and continued. It restores the very commits the branch had, with no prompt, and refuses rather than touch uncommitted work. The offer ends on dismissal, a newer action, a repo switch, a restart, or the branch moving.

### 6. Branches

- The branch menu carries New Branch…, Merge into "*current*"…, Abort… (only during an operation, named for it) and Delete Branch…, over local then remote branches. It re-reads the list on every open.
- Picking a remote branch creates or switches to the local tracking branch rather than detaching.
- **Merge** previews the commit count and offers Merge or Squash & Merge. Nothing to bring in reads *already up to date*. A conflict shows git's text, with conflicted files on Changes.
- **Operation in progress**, from here or a terminal: the chip adds `· merging`, `· rebasing`, `· cherry-picking` or `· reverting`. Abort confirms, then returns branch and tree to where they were.
- **Delete:** local non-current branches only, confirmed, warning that unmerged commits are lost.
- **One repository write at a time, across the window.** While one runs, anything starting another is disabled. Reading is never blocked.
- Rename and delete-on-remote are deliberately absent for now.

### 7. Remote synchronization

**One adaptive button.** It proposes the first that applies:

| Face | When |
|---|---|
| Publish | No remote |
| Publish Branch | Remote but no upstream |
| Force Push | Diverged from its own past (amend, rebase, squash of pushed commits) |
| Pull | Behind |
| Push | Ahead |
| Fetch | In sync |

Pull outranks push, so the push git would reject is never offered. Both counts are always visible. Detached HEAD shows a disabled Push.

- A rewrite is told from someone else's push via the branch's reflog, so a terminal rewrite counts. Any doubt stays Pull.
- The chevron offers Fetch (all remotes), plus Pull under Force Push and Force Push (with Lease)… under a diverged Pull.
- **Force push** always confirms, naming git's actual upstream, and stays open through a refusal. It never removes a commit this branch didn't once contain. It does remove every commit the branch moved away from, whoever wrote them, and the dialog says so.
- **Progress:** the face reads `Pushing…` etc., with an aggregate bar and git's line verbatim. One network operation at a time, everything else stays usable. Failures show git's text.
- **Publish:** name from the folder (`owner/name` targets an org), description, private by default, via `gh`.
- No Refresh button: polling covers it, and ⌘R forces a reload.

**Background**
- Auto-fetch every 30 s by default, skipped while text or the terminal has the keyboard.
- Status polls often while the window is focused and less when it isn't. Outside changes show up on their own, the open diff included. Waking the window resyncs at once, with a fetch.
- Other recent repos' badges refresh in the background, more often the more recently you used them.
- Offline: no banners, no hangs. Remote commands fail with an inline error, and background work pauses until connectivity returns.

### 8. Embedded terminal

- Docked under the content, never the sidebar. Toggle with ⌃\` or the header, `+` replaces the session, ✕ kills it. First expand spawns the shell at the repo root.
- Shell: your pick in Settings, else the best found. On Windows, Git Bash, PowerShell 7, Windows PowerShell, then Command Prompt. On Unix, `$SHELL`, zsh, bash, fish, then sh. The header names what launched.
- Showing the panel takes keyboard focus, collapsing hands it back and keeps the session. Switching repos kills it, so shells never cross repos.
- A clean exit collapses the panel. Anything else stays up with `[Process exited with code N]` in red, so a broken `.zshrc` can be read.
- 1,000 lines of scrollback. ⌘-click opens URLs. Terminal programs can set your clipboard but never read it.

### 9. Settings

Sections: Appearance, Git, Diff, Repository Discovery, Terminal, AI Commit Messages. macOS follows the system appearance, so it has no Appearance section.

- No Save button. Toggles write on click, fields on leaving or Return. Out-of-range values correct themselves. A failed write says so and reverts the control.
- Each write names only its field, so a change from the other client survives. Changes apply at once, and the config is re-read when the window reactivates.
- **Scan paths** sit behind Edit and Done. Done re-walks.
- An unreadable config shows only the failure, no form of defaults pretending to be yours.

### 10. Cloning a repository

Opened from *Clone Repository…* in either list. Two tabs and a shared destination.

- **GitHub:** your non-archived repos via `gh` (200 max), filterable, sorted by recently pushed or name. A missing or signed-out `gh` shows inline with Retry.
- **URL:** a full git URL or `owner/name`. Never hangs on a credential prompt.
- **Destination:** defaults to the last clone folder, then the first scan path, then `~/Dev`, with a live preview.
- During a clone: progress bar and git's line, inputs frozen, no cancel. On success it opens the repo, which keeps its row even outside the scan paths.

### 11. Update available

A silent check once per launch. The **Update vX.Y.Z** chip appears only once the release has a build for your platform. It offers *Copy Update Command* and *View Release on GitHub* on macOS and Linux, *Download from GitHub* on Windows, plus *Dismiss for This Session*. Nothing installs or restarts by itself.

## Keyboard surface

| Key | Action | Scope |
|---|---|---|
| ⌘Enter | Commit, Amend or Continue | Changes tab, no dialog open |
| ⌘Enter | Squash | Squash dialog |
| ⌘G | Generate message | Changes tab, no dialog open |
| ⌘P | Run the sync button's proposal | Anywhere |
| ⌘R | Reload status, history, branches | Anywhere |
| ⌘1 / ⌘2 | Show Changes / History | No text focus |
| ⌘, | Settings | No text focus |
| ⌃\` | Toggle terminal | Anywhere, including inside it. Ctrl on every OS, since macOS owns ⌘\` |
| ↑ ↓, Shift+↑ ↓, ⌘A | Move, extend, take all rows | File or commit list |
| Space | Toggle highlighted files | File list |
| ↑ ↓, Enter, Esc | Move line, apply, cancel | History, during Reorder |
| ↑ ↓, Home, End | Move the composer edge by 16, or to min/max | Resize handle focused |
| Esc | Close only the frontmost dialog, menu or popover | Anywhere |
| ⌘L, ⌘B, ? | Toggle tab, branch menu, Help | Tauri only, no text focus |
| Home, End, ⌘↑, ⌘↓ | First / last row | Tauri only, list focused |
| ⇧⌘O, ⇧⌘N | Clone Repository…, New Branch… | macOS only |

Inside the focused terminal on Tauri, app chords step aside (`Ctrl+P`, `Ctrl+R`, `Esc` belong to the shell). Only the toggle reaches the app, so the panel can always be closed.

## Persistence model

| What | Where |
|---|---|
| Settings: theme, AI, scan paths, diff prefs and layout, fetch interval | `config.toml` |
| Last opened repo, recent repos, last clone folder, both sort toggles | `repos-state.json` |
| Window size and position, composer height | The client's own storage |
| Sidebar and commit-files widths | Tauri's own storage. Per session on macOS |
| Unchecked files, active file and commit, terminal, Undo offer | Memory, gone on quit |

Both files live in the platform config dir: `~/Library/Application Support/leogit/` on macOS, `~/.config/leogit/` on Linux, `%APPDATA%\leogit\` on Windows.

Nothing about your code leaves the machine except the diff sent to the chosen AI provider when you press Generate.
