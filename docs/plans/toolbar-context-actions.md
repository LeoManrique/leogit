# Plan — Toolbar context actions (native client)

> Status: **Blocked on CM-0, handing over.** Two approaches to removing AppKit's
> display-mode menu were tried and neither worked; §4 records both, plus a
> logging trap that made the diagnostics useless. **Read §4 before trying
> anything.** The working tree holds inert code from those attempts — §4 says
> which files and what to do with each. CM-4, CM-5 and CM-6 are untouched, and
> the three original decisions are answered and folded into the work below.
> Produced from a three-way read of the native client, the Tauri client, and the
> GitHub Desktop source at `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop`
> (reference only — used to judge *how*, never as a source of new features, per
> [`cross-client-feature-parity.md`](cross-client-feature-parity.md) §1).
> Companion contract: [`FRONTEND.md`](../../FRONTEND.md) §10, which this plan
> amends — see §6.

## 1. The observation

Right-clicking the repo chip, the branch chip, or the sync button showed the
same three-item menu: *Icon and Text / Icon Only / Text Only*.

**Nothing in LeoGit put it there.** All three chips fell through to AppKit's
stock `NSToolbar` display-mode menu, which is the default on any toolbar item
that has not claimed the right-click. It is doubly wrong here: the toolbar is
**not customizable** (`.toolbar` is used without an `id:`,
`Screens/ContentView.swift:320`), so there is no customization palette the
display mode belongs to; and the title is **removed**
(`Screens/ContentView.swift:252`) with both chips setting
`.labelStyle(.titleAndIcon)` explicitly *because* the name is the control's
whole value — so the menu offered to undo the one decision those lines exist to
make.

So the work is not "fix the wrong menu". It is **claim the right-click on the
two chips that have something to say, and silence it on the one that does not.**

## 2. What the reference does

**GitHub Desktop.** Two shared builders, each attached to both the toolbar
button and the corresponding flyout rows:

- `generateRepositoryListContextMenu`
  (`app/src/ui/repositories-list/repository-list-item-context-menu.ts:25-78`),
  called from the toolbar button (`app/src/ui/app.tsx:3058`) **and** the list
  rows (`app/src/ui/repositories-list/repositories-list.tsx:290`). Items, in
  order: *Create/Change Alias*, *Remove Alias*, *Copy Repo Name*, *Copy Repo
  Path*, ─, *View on GitHub*, *Open in Shell*, *Reveal in Finder*, *Open in
  External Editor*, ─, *Remove…*.
- `generateBranchContextMenuItems`
  (`app/src/ui/branches/branch-list-item-context-menu.tsx:13-63`), called from
  the branch toolbar button (`app/src/ui/toolbar/branch-dropdown.tsx:311`)
  **and** the branch rows (`app/src/ui/branches/branch-list.tsx:290`). Toolbar
  gets: *Rename…*, *Copy Branch Name*, *View Branch on GitHub*, *View Pull
  Request on GitHub*, ─, *Delete…*. Rows get a subset: *Rename…*, *Copy Branch
  Name*, ─, *Delete…*.
- **The push/pull/fetch button has no context menu at all.**
  `app/src/ui/toolbar/push-pull-button.tsx` (699 lines) contains zero
  occurrences of `menu`, and `IPushPullButtonProps` has no `onContextMenu`. Its
  Fetch / Force-push flyout is a left-click `dropdownContentRenderer`, not a
  context menu.

Two structural lessons, independent of the item list:

1. **One builder, two call sites.** The chip and the rows never drift, because
   there is only one of each menu. This is the same argument `RepoPickerList`
   already makes for itself (`Screens/RepoPickerList.swift:38-47`), and it is
   why `RepoContextMenu` is a view rather than a menu written into each surface.
2. **Right-click on a chip acts on the *current* thing; left-click browses the
   others.** This is what makes a branch-chip context menu non-redundant even
   though left-click already opens a full branch menu: `BranchMenuContent`
   (`Screens/BranchMenu.swift:271`) is about *switching to* some other branch,
   and offers nothing that acts on the branch you are on.

**Tauri client.** Has a branch-row context menu already —
`views/BranchDropdown.svelte:319`, items *Delete…* / *Switch to Branch* /
*Merge into "{current}"…* — and no repo-row menu, and no menu on either header
chip. So a branch context menu is a **parity gap in the native client**, not a
new feature.

## 3. What each remaining item would cost

Verified against the code, not estimated from the label.

### Cheap — backend exists, front end does not

| Item | Backing | Missing |
|---|---|---|
| *Rename Branch…* | `core/src/git.rs:1841` (`git branch -m`) | **Everything above core.** A workspace-wide grep for `rename_branch` returns that one line: no `#[uniffi::export]` wrapper in `apps/swift-ui-app/ffi/src/lib.rs`, no Tauri shim, no `generate_handler!` entry, no TS wrapper. It is dead code. `views/BranchDropdown.svelte:197` even names the gap: *"the natural home for a rename when that lands."* Work is one export wrapper (the `delete_branch` one at `ffi/src/lib.rs:906-909` is the template), a sheet modelled on `CreateBranchSheet`, and the Tauri half for parity. |

### Expensive — needs a core change crossing both clients

| Item | Why |
|---|---|
| *View on GitHub* / *View Branch on GitHub* | `parse_owner_repo` (`core/src/git.rs:2668`) **discards the host** — `let (_host, path) = after_user.split_once('/')?` — so `RepoIdentifier` carries only `owner` and `name`. Building a URL today means hardcoding `github.com`, which is wrong for GitLab and every self-hosted remote. Correct fix: add `host` to `RepoIdentifier`, which changes an FFI record shape and touches `repoIdentifiers.ts` too. `open_url` itself already exists and is already strict (`core/src/os.rs:77`, https-only, metacharacter-rejecting). |

### Declined — no honest equivalent in LeoGit

| Item | Why not |
|---|---|
| *Remove…* | LeoGit **discovers** repositories by walking scan folders; it has no registered list to remove from. `RepoDirectoryStore` has `noteOpened` but no remove counterpart, and `Screens/RepoPickerList.swift:174-179` already argues the case deliberately: a repository missing from the list means the *scan paths* are wrong, and that is a Settings edit which also holds next launch. A per-repo "remove" would be a lie that a rescan undoes. |
| *Create/Change/Remove Alias* | LeoGit has no alias concept. Row labels come from the remote (`Stores/RepoIdentifierStore.swift`). Adding aliases is a feature, not a context action. |
| *Open in External Editor* | No editor configuration exists in either client — no `open_in_editor` command, no setting. `Open with Default Program` is file-scoped and meaningless on a directory. |
| *Delete Branch…* on the branch chip | GitHub Desktop offers it because it checks out the default branch first. LeoGit's `deletableBranches` (`Screens/BranchMenu.swift:317`) excludes the checked-out branch, as git itself does — and the chip menu only ever targets that branch, so the row could never be anything but permanently disabled. |

## 4. CM-0 — two approaches tried, neither works. Handing over.

**The display-mode menu is still there on all three chips.** Everything in this
section failed; it is written down so the next attempt does not repeat it.

### Attempt 1 — SwiftUI `.contextMenu` on the toolbar items

`.contextMenu { … }` on `RepoSwitcher`'s button and `BranchMenu`'s menu, and
`.contextMenu {}` on `SyncControls` for the suppression case.

**Result: no effect of any kind.** AppKit's menu still appears on all three
chips. No SwiftUI menu appeared, nothing doubled up, no warning was emitted —
the modifier is silently discarded. Consistent with what
`SyncControls.splitButton` already documents: macOS bridges a toolbar control's
label to a *system control*, so there is no SwiftUI view in the item to hang a
menu on.

So there is no one-line CM-1, and CM-2/CM-3 have no SwiftUI route.

### Attempt 2 — `NSToolbar.allowsDisplayModeCustomization = false`

The API is real and is the right one on paper: `NSToolbar.h:132` in
`MacOSX26.5.sdk`, macOS 15+, and its own documentation names this exact case —
disable it "only when the functionality or legibility of your toolbar could not
be improved by another display mode." With the title removed and both chips
carrying their names as the whole point of the control, Icon Only leaves two
unlabelled glyphs where the only statement of where you are used to be.

SwiftUI exposes no equivalent, so it was reached through an
`NSViewRepresentable` background view (`WindowAccessor`), setting the flag on
every window in `NSApp.windows`. Attached first to the repository screen just
after `.toolbar`, then moved to the root beside `.trackWindowVisibility`.

**Result: no effect either time.** And critically, **it is unproven whether the
code ran at all** — see Attempt 3. Two candidates remain open:

- (a) the accessor's callback never fires, so the flag is never set;
- (b) it fires and sets the flag, and SwiftUI's toolbar ignores or reinstates it.

Deciding between these is the next agent's first job, and neither is ruled out.

### Attempt 3 — capturing the diagnostic output. **This is a trap; read it.**

The instrumentation was run by launching the binary directly with stdout
redirected to a file. Only Rust's `[discover]` line ever appeared — **no Swift
`print` output at all**, including lines proven to be in the binary
(`strings … | grep "\[toolbar\]"` finds them).

The cause is not the toolbar. Rust's stdout is a `LineWriter` and flushes on
every newline regardless of TTY; Swift's `print` goes through C `stdout`, which
is **fully buffered when stdout is not a TTY**, and `SIGTERM` discards the
buffer. So every capture taken that way was empty of Swift logs by construction
and says nothing about whether the code ran.

Run it from a real terminal, or under a pty (`script -q /dev/null <binary>`),
before drawing any conclusion from missing `[toolbar]` lines.

### Ruled out: a SwiftUI-native answer

The complete `toolbar*` modifier family in `MacOSX26.5.sdk`'s
`SwiftUI.swiftinterface` is `toolbar`, `toolbarBackground`,
`toolbarBackgroundVisibility`, `toolbarColorScheme`, `toolbarItemHidden`,
`toolbarRole`, `toolbarTitleDisplayMode`, `toolbarVisibility`. None covers the
item display mode. AppKit is the only route.

### State of the working tree

Nothing is committed. All of this builds clean with no warnings.

| File | State |
|---|---|
| `Screens/RepoContextMenu.swift` | **New, correct, currently unattached.** The repo menu's items — *Show/Hide Terminal* / ─ / *Copy Repository Path* / *Copy Repository Name* / ─ / *Reveal in Finder*, in the house order. Reveal needs no core change: `reveal_path` joins the relative path onto the repo root, and joining `""` leaves the root. This is CM-4's content whatever happens to the chips, since picker rows are ordinary SwiftUI views where `.contextMenu` does work. Worth keeping. |
| `Screens/ToolbarChrome.swift` | **New, inert.** Attempt 2, plus a `[toolbar]` diagnostic dump that has never been seen to print. |
| `Services/WindowAccessor.swift` | **New.** `WindowAccessor` lifted out of `BackgroundSchedulingPolicy` (which still uses it, unchanged in behaviour) so Attempt 2 could share it, and taught to re-report on every SwiftUI update. Sound either way. |
| `Screens/BranchMenu.swift`, `RepoSwitcher.swift`, `SyncControls.swift` | **Modified, inert.** The three `.contextMenu` calls from Attempt 1, and the `terminal:`/`onNotice:` parameters `RepoSwitcher` gained to feed one. Their comments describe behaviour that does not happen — **delete them or make them true; do not leave them as they are.** |
| `Screens/ContentView.swift` | **Modified.** Applies `withFixedToolbarDisplayMode()` at the root, and passes `RepoSwitcher`'s two new parameters. |

## 5. What is next

### First: settle CM-0

1. **Run the app from a real terminal or a pty and read the `[toolbar]` line.**
   Attempt 3 above is why there has never been one to read. That line says
   whether `window.toolbar` exists at all, how many items it has, and whether
   each item hosts a real `NSView` or is system-drawn — which is what decides
   between the two routes below.
2. If the flag is being set and ignored, the remaining lever is the item's own
   view; if there is no `NSToolbar` on any window, both AppKit routes are dead
   and Route A is the only one left.

### Then: how the chips get real actions

Suppressing AppKit's menu would settle the sync button — the user's third ask,
and the reference's own behaviour — but it does not by itself put anything on
the other two chips.

- **Route A — put the actions in the chips' existing left-click menus.** The
  branch chip is already a `Menu`, so a current-branch section at the top of
  `BranchMenuContent` costs nothing and is *more* discoverable than a hidden
  right-click. The repo chip's popover would take a footer row. No AppKit
  reaching, nothing to break on the next SDK. Diverges from GitHub Desktop's
  shape, which is allowed: the reference informs *how*, never *what*. **It also
  works whether or not CM-0 is ever solved**, which is its strongest argument
  after two failed attempts.
- **Route B — hang an `NSMenu` on the item's hosted view.** Literal right-click
  parity. Requires that SwiftUI gave the item a real `NSView`, and that the item
  can be identified without depending on ordering — which is the part that would
  age badly, since SwiftUI generates the identifiers.

*Recommendation: A.* Long-term correctness is the standing priority here, and B
depends on SwiftUI internals that are neither documented nor promised.

### Findings that change the order

- **CM-3 is thin until CM-5 lands.** With *Delete…* correctly declined (§3) and
  *Rename…* unreachable, the branch chip's menu is one item most of the time.
  It is honest and it closes the parity gap, but *Rename Branch…* is what gives
  it substance — which promotes CM-5 above CM-4.
- **CM-4 is no longer free.** `RepoContextMenu` reports a failed hand-off
  through an `onNotice` closure, the way `ChangesSidebar` does. The toolbar chip
  had a channel to hand it (`store.errorMessage`); the picker rows do not.
  `RepoPickerList` has two hosts — the switcher popover and `WelcomeView` — and
  Welcome has **no error surface at all**. So CM-4 needs a route decided first:
  thread `onNotice` through both hosts, or give `RepoPickerList` a local inline
  notice reusing its own `discoveryFailure` strip (`RepoPickerList.swift:261`).
  *Recommendation: the local strip.* The failure belongs where it happened, the
  shape already exists, and it leaves `WelcomeView`'s API untouched.

### Order

1. **CM-5 — make `rename_branch` reachable.** Export wrapper → sheet → Tauri
   shim → TS wrapper, then the item joins CM-3's menu. Independent of the
   right-click test: the sheet is reachable from `BranchMenuContent` either way.
   *The only item here that touches Rust, and the only one that is a fully
   written core function no user can reach.*
2. **CM-4 — picker rows get `RepoContextMenu`.** Attach to `RepoListRow`
   (`Screens/RepoPickerList.swift:374`) — rows are a `LazyVStack` of `Button`s,
   not a `List`, so a plain `.contextMenu` applies, not
   `contextMenu(forSelectionType:)`. Pass `terminal: nil`: the dock is `cwd`-ed
   to the *open* repository, so offering it on a row that is not open would
   either lie or force a switch. **Ports to `RepoDropdown.svelte` in the same
   change** (answered decision), noting `ContextMenu.svelte:47-51` first — it
   must not render inside an ancestor with a `transform`.
3. **CM-6 — host-aware remote URLs, then *View on GitHub*.** Add `host` to
   `RepoIdentifier`, thread it through `repoIdentifiers.ts` and
   `RepoIdentifierStore`, then add the item to CM-2 and CM-3. Real but not
   urgent, and the one item that can ship *wrong* rather than missing.
4. **Doc updates**, held behind the test rather than written ahead of it: a
   contract edit describing menus that lost to AppKit would be a lie. See §6.

## 6. Contract amendment

`FRONTEND.md` §10 is titled *Row context actions* and specifies only changed-file
rows and commit rows. It says nothing about repository rows, branch rows, or
toolbar chips — which is why the two clients could diverge here without either
being wrong.

Whatever ships from this plan needs §10 extended to state, for both clients,
which surfaces carry a context menu and what each offers. `DESIGN.md` takes the
user-facing half of the same statement. Both are queued behind the test in §5.

## 7. Non-goals

- No changes to the left-click behavior of any of the three chips.
- No repository removal, aliases, or external-editor integration (§3).
- No toolbar customization. The toolbar stays fixed; CM-1 removes the affordance
  that implied otherwise.
