# Plan — Toolbar context actions

> Status: **CM-0 to CM-3 done in both clients and confirmed by the owner.** The
> repository and branch chips open their own right-click menus, and nothing else in
> the toolbar opens one. The native client's stock display-mode menu is switched off. The
> owner scoped the menus to three copy items: *Copy Repo Name* and *Copy Repo Path* on
> the repository chip, *Copy Branch Name* on the branch chip. CM-4 to CM-6 are open and
> not scheduled. §4 records how the native right-click was won, including the approaches
> that cannot work, so nobody tries them again.
> Produced from a three-way read of the native client, the Tauri client, and the
> GitHub Desktop source at `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop`
> (reference only — used to judge *how*, never as a source of new features, per
> [`cross-client-feature-parity.md`](cross-client-feature-parity.md) §1).
> Companion contract: [`FRONTEND.md`](../../FRONTEND.md) §10, which now states the
> toolbar chips' context actions for both clients.

| Item | What | State |
|---|---|---|
| CM-0 | A native toolbar chip can own its right-click at all | Done — §4 |
| CM-1 | No display-mode menu anywhere in the toolbar; the sync button shows nothing | Done, both clients |
| CM-2 | Repository chip: *Copy Repo Name*, *Copy Repo Path* | Done, both clients |
| CM-3 | Branch chip: *Copy Branch Name* (disabled when HEAD is detached) | Done, both clients |
| CM-4 | Repo picker rows get the repository chip's menu | Open |
| CM-5 | *Rename Branch…* made reachable, then added to the branch chip | Open |
| CM-6 | Host-aware remote URLs, then *View on GitHub* | Open |

## 1. The observation

Right-clicking the repo chip, the branch chip, or the sync button in the native
client showed the same menu: *Icon and Text / Icon Only*.

Nothing in LeoGit put it there. It is AppKit's stock `NSToolbar` display-mode
menu. It is doubly wrong here:
- **The toolbar is not customizable.** `.toolbar` is used without an `id:`, so there is
  no customization palette for a display mode to belong to.
- **The title is removed.** Both chips set `.labelStyle(.titleAndIcon)` explicitly
  *because* the name is the control's whole value, so the menu offered to undo the one
  decision those lines exist to make.

In the Tauri client the same right-click fell through to the webview's own menu
(*Reload*), which knows nothing about repositories or branches.

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
  `app/src/ui/toolbar/push-pull-button.tsx` contains no `menu`, and
  `IPushPullButtonProps` has no `onContextMenu`.

Two structural lessons, independent of the item list:

1. **One builder, two call sites.** The chip and the rows never drift, because
   there is only one of each menu. This is why `RepoContextMenu` is a view rather
   than a menu written into each surface.
2. **Right-click on a chip acts on the *current* thing; left-click browses the
   others.** This is what makes a branch-chip context menu non-redundant even
   though left-click already opens a full branch menu: `BranchMenuContent` is
   about *switching to* some other branch.

## 3. What each remaining item would cost

Verified against the code, not estimated from the label.

### Cheap — backend exists, front end does not

| Item | Backing | Missing |
|---|---|---|
| *Rename Branch…* (CM-5) | `core/src/git.rs` `rename_branch` (`git branch -m`) | **Everything above core.** No `#[uniffi::export]` wrapper in `apps/swift-ui-app/ffi/src/lib.rs`, no Tauri shim, no `generate_handler!` entry, no TS wrapper. The work is one export wrapper (the `delete_branch` one is the template), a sheet modelled on `CreateBranchSheet`, and the Tauri half. |

### Expensive — needs a core change crossing both clients

| Item | Why |
|---|---|
| *View on GitHub* / *View Branch on GitHub* (CM-6) | `parse_owner_repo` (`core/src/git.rs`) **discards the host**, so `RepoIdentifier` carries only `owner` and `name`. Building a URL today means hardcoding `github.com`, which is wrong for GitLab and every self-hosted remote. The correct fix is to add `host` to `RepoIdentifier`, which changes an FFI record shape and touches `repoIdentifiers.ts` too. `open_url` already exists and is strict (`core/src/os.rs`, https-only). |

### Declined — no honest equivalent in LeoGit

| Item | Why not |
|---|---|
| *Remove…* | LeoGit **discovers** repositories by walking scan folders; it has no registered list to remove from. A repository missing from the list means the *scan paths* are wrong, and that is a Settings edit which also holds next launch. A per-repo "remove" would be a lie that a rescan undoes. |
| *Create/Change/Remove Alias* | LeoGit has no alias concept. Row labels come from the remote. Adding aliases is a feature, not a context action. |
| *Open in External Editor* | No editor configuration exists in either client. |
| *Delete Branch…* on the branch chip | GitHub Desktop checks out the default branch first. LeoGit's `deletableBranches` excludes the checked-out branch, as git itself does, and the chip menu only ever targets that branch, so the item could never be anything but permanently disabled. |

## 4. CM-0 — how a native toolbar chip owns its right-click

### Why SwiftUI alone cannot do it

AppKit routes a right-click through `NSWindow`'s context-click hit test, and
`NSToolbarView` overrides it: it takes every context click inside its bounds for
itself unless the view under the pointer is one of its few recognised menu owners.
SwiftUI hosts each `ToolbarItem` in a real `ToolbarItemHostingView`, so the chip is
**not** a system control. But the hosting view is never asked for a menu, and neither is
anything inside it. Every one of these was tried in a replica of this toolbar and shows
the display-mode menu:
- `.contextMenu` on the `Button`, on the `Menu`, on the split `Menu(primaryAction:)`,
  or on a wrapper around any of them;
- every `ButtonStyle` / `MenuStyle`, and non-`Button` content with `onTapGesture`
  (which also breaks left-click);
- `.toolbar(id:)`, which also adds *Customize Toolbar…*;
- `ToolbarItemGroup`, `.principal`, `.contentMarginsRemoved()`;
- an `NSViewRepresentable` whose view sets `menu` or overrides `menu(for:)`;
- `NSToolbarItem.view.menu` and `menuFormRepresentation`.

A hand-built `NSToolbar` with `NSHostingView` items was also tried: AppKit keeps
the click there too, so owning the toolbar does not help. Moving the chips out of
the toolbar (a titlebar accessory, or content under a hidden titlebar) gives them
ordinary SwiftUI menus, but it stops being the stock toolbar.

### What ships: `.toolbarContextMenu` (`Design/ToolbarContextMenu.swift`)

The click is claimed where every click passes before the window dispatches it: one
local `NSEvent` monitor for right-clicks and control-clicks. Each chip carries a
zero-hit anchor view as its `.background`. SwiftUI sizes the anchor to the whole
toolbar item, which is exactly the glass capsule. The monitor opens the menu of the
anchor whose rectangle holds the click and ends the event there. Every other click
goes on to AppKit unchanged. The menu is the chip's SwiftUI content, rendered through
`NSHostingMenu`.

The same anchor sets `NSToolbar.allowsDisplayModeCustomization = false`, and this is
what leaves the sync button and the empty bar with no menu. **It has to be the
anchor.** SwiftUI installs a new `NSToolbar`, with the flag back on, every time the
repository screen replaces Welcome. A window hook on the screen (the first attempt)
fires before that toolbar exists, so it sets the flag on nothing. An anchor lives
inside a toolbar item, so it only ever reaches a window that has its toolbar.

Public API only. One alternative also works and was not taken: an overlay whose
class returns a `defaultMenu`, which `NSToolbarView` lets through. That relies on an
undocumented AppKit rule, where the monitor relies on documented event dispatch.

### How it was proven

A scratchpad replica of this toolbar: the repo `Button` with its popover, the branch
`Menu`, the split sync `Menu`, the counts text, the conditional update chip, and a
Welcome → repository swap. The harness sent synthetic clicks through
`NSApp.postEvent` / `NSApp.sendEvent` and watched `NSMenu.didBeginTrackingNotification`
for which menu opened. The shipped file was copied into it verbatim and built in
Swift 6 mode. The owner then confirmed it in the app.

One trap cost an earlier attempt its diagnostics. Swift's `print` goes through C
`stdout`, which is fully buffered when stdout is not a TTY, and `SIGTERM` discards the
buffer. So a capture taken by redirecting a launched binary to a file shows no Swift
output at all. Capture under a pty (`script -q /dev/null <binary>`) or log to a
flushed file.

### The Tauri half

`Header.svelte` opens the shared `ContextMenu` from each chip's `oncontextmenu`, and
cancels every other `contextmenu` event in the header, so the webview's *Reload*
never appears over the toolbar.

## 5. What is next (unscheduled)

1. **CM-5 — make `rename_branch` reachable.** Export wrapper, then the sheet, the Tauri
   shim and the TS wrapper; then the item joins the branch chip's menu. It is the one
   fully written core function no user can reach.
2. **CM-4 — picker rows get `RepoContextMenu`.** Attach it to `RepoListRow`, whose rows
   are a `LazyVStack` of `Button`s, so a plain `.contextMenu` applies. Port it to
   `RepoDropdown.svelte` in the same change, noting `ContextMenu.svelte`'s rule first: it
   must not render inside an ancestor with a `transform`.
3. **CM-6 — host-aware remote URLs, then *View on GitHub*** on both chips. It is the one
   item that can ship *wrong* rather than missing.

## 6. Non-goals

- No changes to the left-click behavior of any of the three chips.
- No repository removal, aliases, or external-editor integration (§3).
- No toolbar customization. The toolbar stays fixed, and the display-mode menu, the one
  affordance that implied otherwise, is off.
