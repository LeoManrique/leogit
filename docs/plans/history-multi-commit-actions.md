# Plan — History multi-commit actions (cherry-pick, squash, reorder)

> Status: **WS-A (a selection that is a set, `5ff2a4c`), WS-B (operations in
> progress, `d0b68df`) and WS-C (cherry-pick, the preflight and the window-wide
> write gate, `ff0e195`) are built, confirmed and committed. WS-D (force push
> recommended) is built in both clients, 2026-09-20, and awaits the owner's
> visual check; WS-E (the rewrite driver and squash) is next — do not start it
> before that check.** The owner's decisions
> are marked **Decided**; the ones this plan made on its own are marked
> **Proposed** and are open to change until their workstream starts. One question
> is open and is the owner's — whether network transfers and repository writes
> exclude each other (§5.2) — and nothing in WS-E … WS-G waits on it; §9 records
> the standing decision on where rewriting runs. §3 describes
> the code as it stands *after* WS-D, and §4.2 – §4.6, §5.1 – §5.3 record what
> the next workstreams inherit.
> Produced from a three-way read of the native client, the Tauri client, and
> the GitHub Desktop source at
> `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop` — whose history and
> multi-commit code is upstream's in every region §2 cites (the fork's changes
> to `app-store.ts` and `dispatcher.ts` sit above them, so **line numbers are
> this fork's**, not upstream's). The reference is used to judge *how*; the
> feature itself is LeoGit's own, promised by `ROADMAP.md`'s open items
> *Rebase (interactive UI)* and *Revert, and cherry-pick into the current
> branch* — the part of it this plan ships being cherry-pick onto *another*
> branch (roadmap items are named
> by title throughout — its line numbers move with every entry). The
> git mechanics in §4 were run against throwaway repositories, and the plan was
> then checked adversarially against both code bases and git's release notes —
> §4.8 lists what was verified and what was not.
> Companion contract: [`FRONTEND.md`](../../FRONTEND.md) §3, §5, §6 and §8,
> which this plan amends — see §8.

## 1. The feature

Select several commits in History, right-click, and get three actions on the
whole selection:

- **Cherry-pick N Commits…** — copy them onto another branch.
- **Squash N Commits…** — fold them into one commit, with a message you edit.
- **Reorder N Commits…** — move them to another place in the branch.

The work is three layers, in this order: **a selection that is a set** (built,
WS-A), **a core that can detect, continue and abort a multi-step git
operation** (built, WS-B), **and then the three actions on top.** The core
still *starts* no rebase and no cherry-pick, and has no stash and no undo
beyond `undo_last_commit` (`core/src/git.rs`).

## 2. What the reference does

**Selection** is built (WS-A) to the reference's gestures. One thing differs
and matters to the actions: the reference stores SHAs in click order and
re-orders them by history only where it matters
(`app/src/lib/stores/app-store.ts:7506-7544`), while both LeoGit clients hand
an action its commits already in list order, newest first.

**Menu** (`app/src/ui/history/commit-list.tsx:930-956`). With more than one
commit selected the menu is exactly the three items and nothing else — every
single-commit item disappears. The only menu-level gates are "an operation is
already running" and "squash/reorder are off while comparing another branch"
(`:871-891`, fed by `compare.tsx:281-282`). Everything else — merge commits in
the range, a dirty tree — is handled *after* the click. The single-commit menu
also carries *Reorder Commit* (`:795-801`) and *Cherry-pick Commit…*
(`:845-850`), and no single-commit squash.

**Cherry-pick** (`app/src/lib/git/cherry-pick.ts:174-179`). A choose-branch
dialog (the current branch is not a valid target), then check out the target
and run one `git cherry-pick <shas, oldest first> --empty=keep -m 1` — with
`'-m 1'` passed as a single argument containing a space, which works by
accident of git's option parser and is not copied. Undo is `reset --hard` of
the target to its old tip, then a checkout back.

**Squash** (`app/src/lib/git/squash.ts:33-171`, the todo at `:69-131`). A
commit-message dialog pre-filled from the selected commits, then a generated
todo file fed to `git rebase -i` through `sequence.editor`, with the message
injected through
`GIT_EDITOR='cat "<file>" >'`. The right-clicked commit is the target: the
todo gathers every selected commit at the target's position, buffering the
unselected commits in between so they replay after it. `--root` when the
range reaches the first commit (`app/src/lib/git/rebase.ts:580-583`).

**Reorder** (`app/src/lib/git/reorder.ts:28-152`). **No dialog.** The menu
item (`commit-list.tsx:948-954`) arms an insertion mode in the list — ↑/↓ move
an insertion point, ⏎ confirms, Esc cancels (`list.tsx:565-614`), with an
on-screen hint (`commit-list.tsx:631-664`) — and lands in the same code path as
drag-and-drop. Same todo shape (`reorder.ts:58-131`), all `pick`.

**Shared.** A dirty tree stops the operation at a dialog
(`app-store.ts:7580-7598`) that offers **Stash Changes and Continue**
(`local-changes-overwritten-dialog.tsx:110-156`) — and the stash is never
popped back afterwards. A merge commit anywhere in the replayed range refuses
squash and reorder (`app/src/lib/git/rev-list.ts:190-201`). A warning before
rewriting pushed commits (`dispatcher.ts:430-464`), the branch remembered as
force-push-recommended afterwards (`:3733-3739`, cherry-pick excluded), and an
Undo banner that does `reset --hard` to the pre-operation tip and refuses when
the tree is dirty or the branch changed (`app-store.ts:7856-7978`).

Four things the reference gets wrong or leaves fragile, which §4 does not
copy:

1. **A squash that conflicts before the squashed commit exists loses the
   message the user typed.** Continue runs under `GIT_EDITOR=':'`
   (`rebase.ts:437-443`, `dispatcher.ts:1369-1386`), so the custom message
   only lands if the rebase reaches the squash without stopping. (A conflict
   in the picks replayed *after* it is harmless.)
2. **The pushed-commits warning fires on someone else's push.** It counts
   `lastRetained..upstream`, which is non-empty whenever the upstream has
   moved — even if none of *your* replayed commits were ever pushed.
3. **Force-push-recommended is an in-memory `Map`**
   (`repository-state-cache.ts:340`). A rewrite done in the terminal, or
   before a restart, is invisible to it.
4. **Paths are spliced into a shell string** (`sequence.editor=cat "<path>" >`).

It also bundles its own git. **LeoGit runs the system git** (`README.md`
Requirements) and supports recent versions of it only, so every flag below is
git's current spelling, and §4.7 sets the floor.

## 3. What exists today

**Native History list** — `Screens/HistorySidebar.swift` is a stock
`List(commits, selection: $selection)` over a `Set<String>`
(`ContentView`'s `historySelection`), so shift-click, ⌘-click, shift-arrow and
⌘A are AppKit's, not ours; `selectedSha` beside it is the one commit the detail
pane shows, derived from the set. Its `.contextMenu(forSelectionType:)` puts
the ids in list order with `ListSelection.targets` and builds `rowMenu(for:)`
for one target and, for several, just `cherryPickItem(for:)` — **that
`targets.count > 1` branch is where WS-E and WS-F add Squash and Reorder**, and
`canStartHistoryAction` beside it is the one menu-time gate (no operation, not
detached, no write in flight) every such item disables on. A selection made in
code is scrolled into view by the list's `.onChange(of: selectedSha)`. The
rules are `Services/ListSelection.swift`, generic
over any `Identifiable` row and shared with the file lists: `activeID` (the
row a pane shows), `reseated` (prune, then first row if nothing is left) and
`targets`. Both sidebars and the commit detail's file list apply them through
one modifier,
`maintainsSelection(_:showing:of:)` (`Design/ListSelectionMaintenance.swift`).

**Tauri History list** — `components/CommitList.svelte` is a hand-rolled
virtual list (`ROW_HEIGHT = 44`) whose props are `selection: ListSelection`
(a key set plus an anchor) and `activeSha`, and whose one output is
`onSelect(selection, activeCommit)`. The selection lives in
`repoState.historySelection`; `MainLayout.svelte`'s `selectCommits` is the one
writer of it and of `activeCommit`, and the History re-seat effect beside it
prunes and re-seats on the sha list changing. The rules are
`utils/listSelection.ts` — pure functions (`applyGesture`, `reseated`,
`contextTargets`, `activeKey`, `selectKeys` for a selection made in code, the
key and click helpers) shared with `FileList.svelte`. `openContextMenu` keeps
the targets, in list order and newest first, on the menu's own state, and
`menuItems` is `[cherryPickItem]` for several targets and `singleCommitItems`
for one — **that array is where WS-E and WS-F add Squash and Reorder.** The
item's `enabled` / `title` come from the `historyActionsBlocked` prop, the one
menu-time gate, worded in `MainLayout.svelte`. `ContextMenu.svelte` items are
`aria-disabled`, not `disabled`, so that `title` can show.

**History actions** (WS-C) — `core/src/history_rewrite.rs`:
`rewrite_preflight(repo, replayed_from)` → `RewritePreflight { blocked,
rewrites_pushed }`, `cherry_pick_commits(repo, shas, target)` → `RewriteResult`,
and the private `switch_to` (a checkout judged by where HEAD ends up) and
`return_to` (abort the sequence, then switch back). Both commands are on both
bridges (`ffi/src/lib.rs`, *history actions*;
`src-tauri/src/shims/history_rewrite.rs`). Client side: `MainLayout.svelte`'s
*History actions* section (`requestCherryPick` → preflight → arm the branch
popover; `runCherryPick`; `selectPickedCommits`; `cherryPickReturn`), and
natively `Stores/HistoryActionStore.swift` (`refusal(replayedFrom:)`,
`cherryPick`, `CherryPickOutcome`, `cherryPickReturn`) with
`ContentView.requestCherryPick` / `finishCherryPick` and
`Screens/CherryPickSheet.swift`. **A new action copies that shape: preflight on
the menu click, then the dialog, then one core call, then reload → select
`RewriteResult.selection` or go to Changes on a conflict.**

**The write gate** (WS-C) — one slot per window for every repository write:
`stores/repoWrite.ts` (`beginRepoWrite(kind)` / `endRepoWrite()`,
`isHeldByAnother`, `REPO_BUSY_MESSAGE`; add the new action's name to
`RepoWriteKind`) and `Stores/RepositoryWriteGate.swift` (`claim()` →
`Claim?`, `release(_:)`, `busyMessage`). **Every History action must claim
it**, report a refused claim as its own outcome (never success), and give any
dialog that can sit open under another write the `blocked` prop /
`WriteBlockedNote`. Stores keep *cannot start* (`isBlocked`, the gate) apart
from *I am working* (`isRunning`, their own).

**The operation in progress** (WS-B) — `core/src/operation.rs`:
`OperationInProgress { Merge, Rebase, CherryPick, Revert }`, the filesystem
probe `in_progress(git_dir)` that fills `RepoStatus.operation` on every status
read, `continue_operation` → `OperationOutcome { success, conflicts,
error_message }` and `abort_operation` → `Option<String>` (whatever git
printed). Both are on both bridges (`ffi/src/lib.rs`, *the operation in
progress*; `src-tauri/src/shims/operation.rs`). `core/src/git_version.rs`
holds `FLOOR`, the `git --version` parser and `require_floor()`, whose one
caller is `rewrite_preflight`. `core/src/test_support.rs` (`git`, `git_stdout`,
`git_stopping` for a command expected to stop, `init_test_repo`, `commit_file`,
`subjects`, and `conflicting_repo()`) is what every core test module builds its
repositories with; `history_rewrite.rs`'s own `linear_repo()` and
`failing_hook()` are the fixtures to reuse for a rewrite. `git.rs` lends
`pub(crate)` `git_cmd`, `run_git`, `run_git_optional` (exit 1 is `None`),
`run_git_combined`, `run_git_combined_with_env` (the extra-environment sibling
the rewrite driver needs), `git_dir`, `current_branch`, `has_commits`,
`is_object_id`, `ls_files_unmerged` and `git_add`.

**The pattern to copy in the core** is still merge (`merge_branch` in
`core/src/git.rs`): `run_git_combined` so a failure is data, `MergeResult
{ success, conflicts, error_message }` with git's own text verbatim.
`RepoStatus.unpushed_shas` and `CommitInfo.parents` already answer "is it
pushed" and "is it a merge" on the client.

**The clients' operation surfaces** (WS-B) — one words table per client
(`utils/operationWords.ts`, `Services/OperationWords.swift`: noun, Title,
gerund, and `continuesFromComposer`, false for a merge only) instead of a
`switch` per site; a new operation is one row there. The header chip appends
the gerund (`Header.svelte`; `menuLabel` in `BranchMenu.swift`). Abort is the
branch menu's footer item, and its confirmation captures the operation *when
asked*, so a poll that lands mid-dialog cannot reword it (`MainLayout.svelte`
`requestAbort` / `abortOperation`; `BranchMenu.swift` `pendingAbort`).
Continue lives in the composer and claims the window's write gate like every
other write (`CommitMessage.svelte` `handleContinue`;
`CommitStore.continueOperation` → `ChangesSidebar.continueOperation` →
`CommitComposer`'s `continuing:` / `onContinue:`). Abort of a cherry-pick begun
in History also switches back to the branch it came from
(`cherryPickReturn`, client memory dropped by one rule on the status read). After a continue **both
clients re-read first and report second**, because a continue that stops again
has still moved the repository.

**Sync** (WS-D) — `core/src/sync_ladder.rs` holds `SyncProposal`, the ladder
`propose(status, rewritten_away)` and the reflog probe
`upstream_was_rewritten_away`; `get_status` (`git.rs`) wires them. A branch
diverged by its own rewrite proposes **`ForcePush`**, anything else diverged
proposes *Pull*, and each keeps the other under its chevron. **A squash or a
reorder of pushed commits needs nothing more from the sync button**: the
rebase leaves the old tip in the branch's reflog, and the next status read
proposes Force Push by itself
(`sync_proposes_force_push_after_a_rebase_of_pushed_commits`). Each client
states a rung's presentation once, over the whole enum — `SYNC_FACES` in
`Header.svelte`, `Services/SyncFace.swift` — so a new rung is a compile error
at every site; `utils/assertNever.ts` is there for any other `switch` over a
core union. `git.rs` also lends `run_git_with_stdin` (feed git a list on
stdin, judge the exit status yourself).

**Surfaces** — one sheet slot per window on the native side, driven by
`RootSheet` (the enum at the foot of `ContentView.swift`, which now has a
`.cherryPick` case — a squash-message or reorder sheet is another case there);
`ConfirmDialog.svelte` and the one-off dialogs on the Tauri side, all with the
`blocked` prop; the branch dropdown's *picking mode* (`STYLE.md`, *Branch
picker*), which `BranchDropdown.svelte` enters from outside when
`cherryPickCount` is set (`mode` is derived from it, so a second externally
armed mode is one more prop and one more `MenuMode`); and a text-only notice
banner (`reportNotice` in `stores/repo.ts` and its native counterpart).

## 4. Core design

The core lives beside `git.rs`, which is already 4 000+ lines, in modules
split by responsibility: `core/src/operation.rs` (built — detect, continue,
abort), `core/src/git_version.rs` (built — the floor),
`core/src/sync_ladder.rs` (built — the sync proposal and the reflog test), and
`core/src/history_rewrite.rs` for everything that *starts* an operation — the
preflight and cherry-pick are built; the rewrite driver, squash, reorder and
undo go there too. It reuses the `pub(crate)` helpers §3 lists.

### 4.1 Operation in progress (OP-1) — built (WS-B)

`RepoStatus.operation: Option<OperationInProgress>`; the probe table and the
reason for its order are `TECHNICAL.md`, *Operation in progress*. What a later
workstream has to know:

- **There is a `Revert` variant the plan did not foresee.** Revert shares
  `sequencer/` with cherry-pick, `git cherry-pick --abort` silently aborts a
  revert, and `cherry-pick --continue` can silently complete one — so the two
  must be told apart (first word of `sequencer/todo`, else `REVERT_HEAD`).
- **Rebase is probed first**, because `--rebase-merges` leaves `MERGE_HEAD`
  beside `rebase-merge/` and `git merge --abort` there strands the rebase.
- **`git am` reads as no operation** (`rebase-apply/applying`): every
  `git rebase` command refuses there, so naming it would offer buttons that
  cannot work. A `stash pop` conflict and a bisect also create none of the
  probe files, and `AUTO_MERGE` appears in every conflicted state, so it is
  never a signal.
- **A single-commit pick or revert has no `sequencer/` at all** — only its
  `*_HEAD` file. After a conflicted *multi*-pick is committed by hand the
  `*_HEAD` file is gone and `sequencer/todo` remains (§4.8-6).
- **During a rebase `status.detached` is true**, so every gate written as "HEAD
  is detached" also fires mid-rebase; ask about `operation` first when the
  wording matters (the branch menu's merge help does). The chip reads
  `Detached at <sha> · rebasing`; reading the branch from
  `rebase-merge/head-name` would be the improvement, and is not built.

### 4.2 Preflight, continue, abort (OP-2 … OP-4)

- **OP-2 `rewrite_preflight(repo, replayed_from: Option<&str>)` — built
  (WS-C).** Mechanics are `TECHNICAL.md`, *History's multi-commit actions*.
  A refusal is **data** (`blocked`), in this order: the git floor, an operation
  in progress, a detached HEAD, an unborn one, tracked changes (named, up to
  ten; untracked files pass). **`Some(oldest)` is already built and tested for
  WS-E and WS-F** — it adds the merge-commit-in-range refusal and fills
  `rewrites_pushed`; cherry-pick passes `None`. Both clients call it **on the
  menu click, before any dialog opens**, and the action calls it again itself.
  What WS-E and WS-F inherit:
  - **Block, no stash** stands: the core has no stash, and one left behind
    after a conflicted rebase is a worse place to leave someone than "commit or
    discard first". Stash-and-restore belongs with `ROADMAP.md`'s *Stash
    management*.
  - **Read `status` with `-z`.** The line format quotes odd paths and writes a
    rename as `old -> new`; `-z` leaves paths raw and gives a rename's origin a
    field of its own, to skip. `run_git` only `trim_end`s, so ` M` survives.
  - **The upstream is `for-each-ref --format=%(upstream) refs/heads/<b>`** — an
    empty answer is "tracks nothing", never an error. A branch name cannot hold
    glob characters (`check-ref-format`), so the pattern is safe.
  - `rev-list -1 --merges HEAD --not <oldest>^@` exits 0 either way — read its
    stdout — and degrades to "no merge" when `<oldest>` is not an ancestor of
    HEAD, so the action itself must still check ancestry.
- **OP-3 `continue_operation(repo)` — built (WS-B).** Mechanics are
  `TECHNICAL.md`, *Operation in progress*. In order: refuse leftover markers
  by file name, stage **exactly the unmerged paths** (never `git add -u`,
  §4.8-12), for a rebase refuse unrelated unstaged edits, then `--continue` —
  or `--skip` for a pick or revert whose resolution left nothing to commit —
  under `GIT_EDITOR=:`. It returns `OperationOutcome { success, conflicts,
  error_message, skipped }`; a step that stops on the next conflict is
  `success: false` with the conflicts, not an `Err`. An `Err` always means git
  was never asked to continue **and nothing was staged** — both refusals run
  before the first write. `skipped` exists because git drops the commit
  silently; both clients turn it into a notice. What WS-C … WS-F inherit:
  - **The squash message survives because the amend is a todo line (§4.5)**,
    not because of anything continue does — continue always runs under
    `GIT_EDITOR=:` and has no message parameter.
  - **`--empty=keep` does not cover a pick emptied *by its conflict
    resolution***: `cherry-pick --continue` exits 1 there ("The previous
    cherry-pick is now empty"). Continue skips it, as `rebase --continue`
    silently does. §4.4's flag only covers a pick that is redundant from the
    start.
  - **`rebase --continue` refuses over unrelated unstaged tracked edits** with
    the misleading text "You must edit all merge conflicts and then mark them
    as resolved"; merge and cherry-pick do not care. Core pre-empts it with
    the real cause. OP-2 blocks a dirty tree at the start, so this only arises
    from edits made while the rebase is stopped.
  - **The marker check** is `git diff --check` *restricted to the unmerged
    paths* and read by line; bare, it also flags trailing whitespace anywhere.
    **`--check` prints paths raw where `ls-files` and `diff --name-only` quote
    them**, so its lines are matched against the paths in hand, never parsed.
    `git diff --name-only` also lists every unmerged path, once per conflicted
    stage. A lone `=======` is ignored (a Markdown setext heading), binary and
    modify/delete conflicts carry no markers at all, and whatever the worktree
    holds for those paths is what gets staged — the `U` badges are the only
    signal that such a file was never looked at.
  - **`--skip` is safe to automate**: it is `reset --merge`-like, not
    `--hard` — unrelated unstaged edits and untracked files survive — and it
    only ever fires when nothing is staged to lose. It works with and without
    a `sequencer/`.
  - **`git_add` is literal everywhere** (`--literal-pathspecs`): pathspec magic
    in a file name (`weird[1].txt`) would otherwise stage its glob matches too.
    Any new core code that passes user paths to git needs the same flag.
- **OP-4 `abort_operation(repo)` — built (WS-B).** It replaced `merge_abort`
  on both bridges *and* in core. **Abort rewinds exactly as far as git does**:
  a multi-commit pick rolls back to the pre-operation tip, *unless* HEAD was
  moved by hand in between — then git clears the sequence, keeps HEAD, and
  says so on stderr with exit 0 (`You seem to have moved HEAD. Not rewinding`,
  §4.8-13). Whatever git printed comes back as `Some(text)` and both clients
  show it in the notice banner; the core does not reset over commits the user
  made themselves. `--quit` is never used: it leaves the index and tree as
  they were.

### 4.3 Result and undo point (OP-5)

**Built (WS-C)**, in `history_rewrite.rs` and on both bridges: `RewriteResult
{ success, conflicts, error_message, selection, undo }` and `UndoPoint
{ branch, before_sha, after_sha, return_branch }`, one DTO for all three
actions. **The three outcomes are a contract every action keeps**
(`FRONTEND.md` §3.7): `Ok(success)` with `selection` newest first and `undo`
set; `Ok(!success)` **only** for a conflict, `conflicts` never empty, the
operation left open; `Err` with the repository back where it began, or a
second paragraph saying where the user was left. Once the commits have landed
nothing may turn the answer into an `Err` — a tip that cannot be read back
costs `selection` and `undo`, not the success.

**Decided in WS-C: continue hands back no `UndoPoint`.** `OperationOutcome`
(§4.2) stays as it is — an operation continued here may have been started in a
terminal — so an action that stops on a conflict and is then continued is not
undoable today. **WS-G's options**: put `before_sha` on the conflict result for
the client to keep (as it keeps `cherryPickReturn`), or read it at continue
time — `rebase-merge/orig-head` for a rebase, `sequencer/head` for a
multi-commit pick; a **single-commit pick has neither, and cherry-pick never
writes `ORIG_HEAD`**, so the first option is the only one that covers it.

`selection` is what both clients select, and scroll to, instead of jumping to
the tip (the reference's own TODO, `dispatcher.ts:3626-3629`). For cherry-pick
it is `rev-list <before>..HEAD`; after a continue that skipped a commit such a
range has one entry fewer than was asked for, so never match it to the
request by position.

### 4.4 Cherry-pick (CP)

**Built (WS-C)**: `cherry_pick_commits(repo, shas, target_branch)` — `git
switch --no-guess -- <target>`, then `git cherry-pick --empty=keep -m 1 <shas,
oldest first>` under `GIT_EDITOR=:`. Mechanics are `TECHNICAL.md`, *History's
multi-commit actions*. What the rewrite driver (§4.5) inherits, all run against
git 2.54:

- **Never trust `git switch` / `checkout`'s exit status.** It switches and
  *then* exits non-zero when a `post-checkout` hook fails — git-lfs installs
  one, and it fails wherever `git-lfs` is not on a GUI app's short `PATH`.
  `switch_to` asks `current_branch` afterwards; reuse it.
- **A stopped sequence is a conflict only if there are unmerged paths.** A
  signer that cannot run (`commit.gpgsign`, no `gpg` on `PATH`) and a failing
  `prepare-commit-msg` hook both stop with `CHERRY_PICK_HEAD` set and nothing
  to resolve, and `--continue` re-runs the same failure forever. **The rebase
  driver will meet the same two** (`rebase-merge/` present, no `U` files): treat
  them as failures and abort, never as "stopped on a conflict".
- **A clean pick runs only `prepare-commit-msg` and `post-commit`;
  `--continue` also runs `pre-commit` and `commit-msg`**, so a hook can refuse a
  continue it let through as a pick. `continue_operation` already reports that
  as git's text.
- **Non-conflict failures leave the sequencer open without `CHERRY_PICK_HEAD`**
  (an untracked file in the way), with the earlier picks committed; git then
  refuses to switch branches, so `--abort` comes first (`return_to`). Never
  `--quit`, which keeps the partial picks.
- **`switch --no-guess --`** is the only safe spelling: `checkout
  refs/heads/x` detaches, and a bare `switch x` creates `x` from a same-named
  remote branch. A target checked out in another worktree is exit 128 with
  HEAD unmoved.
- Picking a commit the target already contains makes an empty duplicate, by
  design (`--empty=keep`); a pick emptied by its *conflict resolution* is the
  different case continue skips (§4.2). `-m 1` is two arguments and a no-op on
  an ordinary commit (§4.8-5).
- **The branch a conflicted pick came from is client memory**
  (`cherryPickReturn`), set from the conflict result and dropped by one rule on
  the status read. It is what makes Abort end on the source branch; it is not
  persisted across a restart.

**SHAs cross the bridge newest-first, as the list shows them**, for all three
actions; the core reverses for cherry-pick and re-derives order from the range
walk for squash and reorder. Every sha is checked with `is_object_id` before
it becomes an argument — it is also what keeps `--abort` out of argv.

### 4.5 The rewrite driver, squash and reorder (SQ, RO)

One private driver runs a generated todo:

```
LEOGIT_TODO=<tempfile>  GIT_SEQUENCE_EDITOR='cp "$LEOGIT_TODO"'  GIT_EDITOR=:
git rebase -i --no-autostash --no-update-refs --empty=keep <base | --root>
```

- **The todo path travels in an environment variable**, so no path is ever
  spliced into a shell string (§2 flaw 4; verified with a path containing
  spaces and a quote, §4.8-1). git runs editors through `sh`, where `cp` is
  always present on macOS and Linux.
- `--empty=keep` because without it `rebase -i` **halts** on a commit that a
  reorder made empty (verified, §4.8-10) — a rewrite must neither stop for a
  reason the UI cannot explain nor lose a commit. `--no-autostash` and
  `--no-update-refs` pin the two user settings (`rebase.autoStash`,
  `rebase.updateRefs`) that would change what the todo means (verified with
  the setting switched on, §4.8-15). The todo names full SHAs and carries no comment lines, so
  `core.commentChar`, `rebase.abbreviateCommands` and
  `rebase.instructionFormat` cannot touch it. `commit.gpgsign` is left alone —
  a user who signs wants the replayed commits signed.
- `base` is the parent of the oldest affected commit, `--root` when that
  commit has none.

**The todo builder is the reference's algorithm** (`squash.ts:79-131`,
`reorder.ts:98-126`), ported once and shared: walk the range oldest → newest;
commits before the gathering point are picked in place; selected commits older
than it are buffered and flushed at it; unselected commits after it are
buffered and appended. **Decided: non-contiguous selections are allowed**, for
squash and reorder alike.

**SQ — `squash_commits(repo, shas, message)`.** **Decided (2026-09-19): the
target is the oldest selected commit, in both clients.** The reference uses
the right-clicked row, but `contextMenu(forSelectionType:)` hands the native
client the *set* and never the row; the oldest commit is how `fixup` is
already understood (later commits fold into the earlier one), its summary is
usually the real one, and the two clients stay identical in something that
rewrites history. Rejected: the right-clicked row on Tauri only — a permanent
divergence — and a row-level `.contextMenu` natively, which gives up the
re-select-on-right-click both lists rely on (`FRONTEND.md:530`). So the
gathering point is the oldest selected commit, every other selected commit is
newer than it, and the todo builder's "selected commits older than the
gathering point" branch is exercised by reorder only. The selected commits
become
`pick` + `fixup` lines, and **immediately after the last `fixup` — before the
buffered unselected commits are appended** — comes

```
exec git commit --amend --no-verify -q -F "$(git rev-parse --git-path leogit-squash-msg)"
```

The message is a file *inside the git dir*, written before the rebase and
removed on success and on abort. Because the amend is a line of the todo, it
runs whenever the rebase reaches it — **the message survives any number of
conflict rounds** (§2 flaw 1; verified through two, §4.8-3), and continue
never needs a special editor. `--no-verify` because replayed picks do not run
`pre-commit` either. The placement is the whole safety of it: at the end of
the todo the same line would amend the last replayed commit instead. The message is built with the existing
`format_commit_message` (`git.rs:3167`), so co-authors go through one path.

**RO — `reorder_commits(repo, shas, before_sha: Option<String>)`.**
`before_sha` is the commit the moved ones land just *under* in the list;
`None` moves them to the tip. The core refuses a no-op (the destination is
inside or adjacent to a contiguous selection) with `success: true` and an
unchanged tip rather than an error.

### 4.6 Force push recommended (FP) — built (WS-D)

**FP-1 — `SyncProposal::ForcePush`**, stateless, and **FP-2** — the force push
is a lease core pins and grants itself
(`--force-with-lease=refs/heads/<b>:<sha>`, `sync_ladder::force_push_lease`).
The contract is `FRONTEND.md` §6.2 and the mechanics are `TECHNICAL.md` (*Sync
proposal*). What the later workstreams inherit, and where this differs from the
plan's first draft:

- **Not git's `--force-if-includes`, which the draft named.** It is the same
  reflog test with a defect (git 2.54, `remote.c`, `is_reachable_in_reflog`):
  the walk over the local reflog is bounded by the newest entry of the
  **remote-tracking** ref's reflog, and that timestamp is read uninitialised
  when the ref has none — every `refs/remotes/origin/*` of a fresh clone until
  a fetch *moves* it. Measured: `init` + `push -u` + amend → accepted; `clone` +
  amend → refused `(remote ref updated since checkout)`; `clone` + no-op
  `fetch` + amend → refused; `clone` + commit + `push` + amend → accepted. The
  first build shipped the flag and passed every test, because the fixture was
  `init` + `push -u`; a verification agent found it. Core now runs the test
  itself (`reflog_holds`, shared with the probe) on the tip of
  `refs/remotes/<remote>/<b>` and pins the lease to that sha, so the check and
  the push are about the same commit however the automatic fetches move the
  ref in between. **A proposed Force Push is therefore one `push` accepts** —
  keep it so: anything that changes where a push lands
  (`ROADMAP.md`, *A push names the local branch…*) has to change
  `measured_push_target`, `tracking_ref` and the lease together.
- **Not cached.** The draft cached on `(head_sha, upstream_sha)`; the
  upstream's sha is not in `status --porcelain=2`, so the key costs a
  subprocess itself, and a reflog can change under unchanged tips
  (`reflog expire`), leaving a stale *Force Push* — the unsafe direction. The
  probe is three local subprocesses, paid only while `ahead > 0 && behind > 0`.
- **Proposed only when the push lands on the measured ref.** One
  `for-each-ref --format=%(upstream)%00%(upstream:remoteref)%00%(push:remotename)`
  must show the upstream to be `refs/remotes/<push remote>/<this branch's
  name>`. `%(push:remotename)` follows `pushRemote` → `remote.pushDefault` →
  the tracking remote whatever `push.default` says, which is
  `get_push_remote`'s order. **`%(push)` is the wrong field**: empty for a fork
  workflow under `push.default=simple`, and *equal to the upstream* under
  `push.default=upstream` with a fork — where core still pushes to the fork.
  Everything else stays Pull.
- **`behind > 0` alone is never asked.** After a `pull --rebase` the old
  upstream tip is in the reflog too, and the probe would call a branch that
  simply fast-forwards rewritten.
- **The Pull-state force push is a guarded item, not a free one.** On a branch
  diverged by somebody else's push the chevron's *Force Push (with Lease)…* is
  refused — by core, before git is spawned, once their commit has been fetched
  ("…holds commits this branch never contained…"); by the remote, `(stale
  info)`, before that. It is refused as well wherever the reflog cannot answer
  (`core.logAllRefUpdates=false`, a rewrite aged out of it). LeoGit therefore
  has no way to overwrite a commit the branch never contained; that is
  deliberate, and the terminal is the way.
- **"Never contained" is not "somebody else's".** A colleague's commit that was
  pulled and then dropped by a rebase is in the reflog, so the probe says
  *rewritten* and the force push removes it. That is what the user did, and git
  agrees; the confirmation says commits go "whoever wrote them". SQ-2 / RO-2's
  pushed-commit warning is the earlier place to say whose commits a rewrite
  touches, if that is ever wanted.
- **Core's Pull is `pull --ff`, which *merges* a diverged branch** — it does
  not refuse. On a rewritten branch that puts the old commits back beside their
  replacements, which is what made *Pull* the wrong face there and is worth
  remembering wherever a rewrite's next step is described (SQ-2, RO-2).
- **UN (undo) and the ladder.** Undoing a rewrite of pushed commits puts the
  tip back on the upstream: the next status read proposes Fetch again by
  itself. Undoing *after* the force push leaves the branch diverged with the
  force-pushed tip in its reflog, so the ladder proposes Force Push once more —
  correct, and UN-2's banner need say nothing about it.

### 4.7 The git floor

**Decided (2026-09-19): the system git, recent versions only.** LeoGit tracks
current git rather than carrying old spellings or second code paths; the floor
is simply the newest flag §4 uses, and it rises whenever a newer git has
something worth using.

| Flag | Since | Used by |
| --- | --- | --- |
| `cherry-pick --empty=keep` | **2.45** | CP |
| `rebase --no-update-refs` | 2.38 | SQ, RO |
| `rebase --empty=keep` (merge backend, which `-i` always uses) | 2.26 | SQ, RO |
| `cherry-pick -m 1` on non-merge commits | 2.21 | CP |

So **the floor is git 2.45** (April 2024). The development machines run 2.54
(Apple Git) and Arch's current git. **Built (WS-B):**
`core/src/git_version.rs` reads `git --version` once per process and
`require_floor()` answers with one sentence naming both versions — a single
check, no degraded mode, no flag fallbacks. `README.md` Requirements states
the floor. `rewrite_preflight` is its one caller, and it is on neither bridge.
The force push does not ask: `--force-with-lease=<ref>:<expect>` is older than
anything the app otherwise tolerates, and a push is not a History action.

**Platforms:** this plan targets **macOS and Linux**. Windows is not verified
and not a gate for any workstream; the one Windows-specific unknown — whether
Git for Windows' `sh` resolves the sequence editor's `cp` — is recorded in
§4.8 for whenever Windows gets first-class support.

### 4.8 What was verified

Run on 2026-09-19 against throwaway repositories, git 2.54.0, global and
system config disabled:

1. Todo injection through `cp "$LEOGIT_TODO"`, path with spaces and a quote —
   reorder `A B C D E` → `A B E C D`. ✅
2. Non-contiguous squash with `fixup` + `exec … --amend -F` — message, body
   and `Co-authored-by` trailer intact, both files in the one commit. ✅
3. The same through **two** conflict rounds under `GIT_EDITOR=:` — the final
   commit carries the typed message; `git diff --check` names the leftover
   markers on an unmerged path. ✅
4. `--root` when the squash includes the first commit. ✅
5. `cherry-pick --empty=keep -m 1` over an ordinary commit and a merge commit
   together; a redundant pick is kept. ✅
6. After a conflicted multi-pick is committed by hand, `CHERRY_PICK_HEAD` is
   gone and `sequencer/todo` remains; `--continue` finishes the sequence. ✅
7. The reflog probe says *rewritten* after a local squash of pushed commits
   and *foreign* once someone else pushes. ✅
8. `merge-base --is-ancestor` separates pushed from unpushed commits. ✅
9. `rev-list -1 --merges` finds a merge in a range; a dirty tree is refused
   with git's own two-line message. ✅
10. `rebase -i` halts on a commit that a reorder made empty; with
    `--empty=keep` it completes and keeps it. ✅

11. `rev-list --merges <root>^..HEAD` is fatal; `HEAD --not <root>^@` exits 0;
    the `--stdin` form of `rev-list` accepts `^<sha>` exclusions. ✅
12. `git add -u` during a paused rebase stages an unrelated edited file
    alongside the resolved one. ✅ (hence OP-3)
13. `cherry-pick --abort` rolls a multi-commit pick back to the old tip — but
    after a hand-made commit it only clears the sequence and keeps HEAD. ✅
14. The squash `exec` runs from the worktree root even when the rebase starts
    in a subdirectory, and `--git-path` resolves correctly in a linked
    worktree, matching `git_dir()` (`core/src/git.rs:797`). ✅

15. `--no-update-refs` overrides `rebase.updateRefs=true`: a branch pointing
    into the replayed range stays where it was. ✅

Added in WS-B (2026-09-20, same conditions), each behind a core test or a
scratch run. Items 18, 25, 26, the `--skip` half of 19 and the conflict-style
half of 22 were run by WS-B's verification agent and not repeated:

16. `git cherry-pick --abort` silently aborts a revert, and
    `cherry-pick --continue` can complete one — hence the `Revert` variant. ✅
17. A `--rebase-merges` rebase stopped on a `merge` todo line leaves
    `MERGE_HEAD` beside `rebase-merge/`. `CHERRY_PICK_HEAD` never appeared
    during any rebase stop. ✅
18. All four `--abort`s are silent with exit 0 on a full rewind; all four
    `--continue`s honour `GIT_EDITOR=:` over `core.editor`, and need no
    `GIT_SEQUENCE_EDITOR`. ✅
19. `cherry-pick --continue` on a pick emptied by its resolution exits 1, and
    `--empty=keep` does not change that; `--skip` finishes it and preserves
    unrelated unstaged edits and untracked files. `rebase --continue` drops
    such a commit silently; `merge --continue` makes the empty merge. ✅
20. `rebase --continue` refuses over unrelated unstaged tracked edits with the
    "edit all merge conflicts" text; merge and cherry-pick do not. ✅
21. `git add -- 'weird[1].txt'` also stages `weird1.txt`;
    `--literal-pathspecs` with `--pathspec-from-file=- --pathspec-file-nul`
    stages only the named file, and still recurses directories, stages
    deletions and embedded repos. ✅
22. `diff --check` prints raw paths, inspects added lines only, reports
    worktree line numbers, flags a bare `=======`, and still fires under
    `diff3` / `zdiff3` conflict styles. ✅
23. Staging a modify/delete resolution stages the deletion
    (`continue_stages_a_modify_delete_resolution`). ✅
24. `git branch -D` refuses the branch a stopped rebase is rewriting
    ("used by worktree"), although HEAD is detached and both clients list
    it as deletable — the refusal is data, so no client gate was added. ✅
25. `git revert -n` leaves `REVERT_HEAD` even when it applies cleanly
    (`cherry-pick -n` leaves nothing), so it reads as *reverting* and
    Continue commits what is staged — git's own `revert --continue`. ✅
26. A conflicted `merge --squash`, `cherry-pick -n`, `stash pop` and a
    `git am` leave unmerged files with **no** operation the probe can name,
    and every `--abort` refuses there. Filed in `ROADMAP.md` (*A way out of
    conflicts no operation owns*); relevant to cherry-pick only if it ever
    grows a no-commit mode. ✅

Added in WS-C (2026-09-20, same conditions). Items 27 – 29 and 33 are behind
core tests; 30 – 32 were run by WS-C's research and verification agents, and
27 and 28 were re-run by hand after the verifier reported them:

27. `git switch` with a failing `post-checkout` hook **switches, then exits
    1**. ✅ (`a_switch_is_judged_by_where_head_is_not_by_its_exit_status`)
28. A failing `prepare-commit-msg` hook, or `commit.gpgsign` with a signer that
    cannot run, stops a pick with exit 128, `CHERRY_PICK_HEAD` set and **no
    unmerged paths**; `--continue` fails identically each time, `--abort`
    exits 0. ✅
    (`a_pick_stopped_with_nothing_to_resolve_is_a_failure_not_a_conflict`)
29. An untracked file in the way of the *second* pick: exit 128, sequencer
    open, no `CHERRY_PICK_HEAD`, first pick committed; `switch` is refused
    ("cannot switch branch while cherry-picking") until `--abort`, which puts
    the target's tip back. ✅
30. The sequencer runs `prepare-commit-msg` and `post-commit` on a clean pick
    and **not** `pre-commit` or `commit-msg`; `--continue` runs all four. ✅
31. A single-sha pick has no `sequencer/`, and cherry-pick never writes
    `ORIG_HEAD`; `sequencer/head` holds the pre-operation tip of a multi-pick.
    After a `--skip`, `rev-list <before>..HEAD` has N − 1 entries. ✅
32. `switch --no-guess -- x` never creates `x` from `origin/x`; `checkout
    refs/heads/x` detaches; a target checked out in another worktree is exit
    128 with HEAD unmoved; preflight and pick both behave in a linked
    worktree. ✅
33. `status --porcelain=v1 -z` gives a rename as `R  new\0old\0`, paths raw. ✅
    (`a_dirty_tree_refusal_names_a_renamed_file_by_its_new_name`)

Added in WS-D (2026-09-20, same conditions). Items 38, 40 and 42 are behind
core tests, and so are the amend, rebase, fresh-clone, foreign-push,
expired-reflog and rename cases of 34; the rest of 34 and items 36, 37, 39 and
41 were run by WS-D's research agent; 35, 40 and 42 were run by hand:

34. The reflog probe against a bare origin and two clones: *rewritten* after
    an amend, a `rebase -i` squash, a feature branch rebased onto a newer
    main, a `reset --hard` past a pulled commit, a DWIM `git switch topic`, a
    fresh clone, a branch renamed after the rewrite, and inside a linked
    worktree; *foreign* after somebody else's push, and once the reflog has
    expired. ✅ (`push --force-if-includes` does **not** agree in the fresh
    clone — item 42.)
35. **`pull --ff` on a diverged branch merges** (`Merge made by the 'ort'
    strategy`): naming `--ff` counts as choosing how to reconcile, so git's
    "Need to specify how to reconcile divergent branches" never fires. After
    an amend of a pushed commit that puts the old commit back. ✅
36. `rev-list -g <ref>` is the only reflog reader whose stdout survives
    `log.showSignature` ("No signature" lands on **stdout** of `log -g` and
    `reflog show`) and `i18n.logOutputEncoding`. All three print nothing, exit
    0, for a missing reflog, and skip an entry whose object is gone. A 5 000
    entry reflog costs about 10 ms for both commands together. ✅
37. `rev-list -1 <upstream> --not --reflog` is wrong — `--reflog` walks the
    remote-tracking ref's own reflog, so it answers *rewritten* for a plain
    foreign push — and `merge-base --fork-point` reads the upstream's reflog,
    not the branch's. `--ignore-missing` on the `--stdin` form forgives a
    missing *positive* too, turning an error into an empty answer. ✅
38. A bare `--force-with-lease` after a fetch pushes over a foreign commit
    (the lease compares the fetched tip with itself); with
    `--force-if-includes` it is
    `! [rejected] main -> main (remote ref updated since checkout)`, exit 1,
    with a four-line `hint:` block. A stale lease — bare or pinned — is
    `(stale info)`, no hint. ✅
    (`force_push_refuses_a_commit_pushed_since_the_last_fetch`)
39. `--force-if-includes` is a no-op without `--force-with-lease`, a no-op
    beside `--force-with-lease=<ref>:<expect>`, live beside the bare and the
    `=<ref>` forms, overridden by `--force`, and never blocks a first publish
    with `--set-upstream`. **It reads the reflog of the local branch named
    after the *destination*:** `push … new:old` is refused however safe, until
    a local `old` whose reflog holds the tip exists. ✅
40. `for-each-ref`'s `%(push)` is empty when `push.default=simple` cannot name
    one destination (`pushRemote` / `remote.pushDefault` set), **equals the
    upstream under `push.default=upstream` with the same fork settings**, and
    `%(push:remoteref)` is empty under the default config even when `%(push)`
    is not. `%(push:remotename)` names the fork in all of them, and a stale
    `pushRemote` verbatim. `%00` separates fields safely. ✅
    (`sync_proposes_pull_when_the_push_goes_somewhere_else`)
41. `git push <remote> <local>` with `branch.<local>.merge` naming another
    branch creates `<local>` on the remote instead of pushing to the upstream,
    and `remote.<name>.push` remaps the bare name silently. Server-side
    refusals read `! [remote rejected] … (non-fast-forward)` under
    `receive.denyNonFastForwards` and `(hook declined)` from an `update`
    hook; no client flag beats either. ✅
42. **`push --force-with-lease --force-if-includes` refuses clone → amend →
    push** `(remote ref updated since checkout)`, and still does after a
    no-op fetch: `is_reachable_in_reflog` (`remote.c`, v2.54.0) declares
    `timestamp_t date;`, sets it only from the remote-tracking ref's reflog,
    and stops the local walk at the first entry older than it — before the
    `clone: from …` entry that *is* the remote tip. Accepted once the
    remote-tracking ref has a reflog entry (a push, or a fetch that moved it).
    `--force-with-lease=refs/heads/<b>:<sha>` is accepted in the same clone and
    refused `(stale info)` once the remote moves. ✅
    (`force_push_publishes_a_rewrite_made_in_a_fresh_clone`)

**Not verified:** any git between the 2.45 floor and 2.54; and anything on
Windows, in particular that Git for Windows' `sh` resolves `cp` for the
sequence editor (§4.7, *Platforms*).

## 5. Inventory

### 5.1 Selection (MS)

- **MS-1 … MS-4 — Built (WS-A).** The History selection is a set in both
  clients, from one set of rules per client shared with the file lists; the
  pane follows each client's existing rule; menu targets are read in list
  order. The contract is `FRONTEND.md` §6.4 and the code map is §3 above. What
  the later workstreams inherit:
  - **Setting the selection after a rewrite** (`RewriteResult.selection`) is
    built: `selectPickedCommits` in `MainLayout.svelte` (`selectKeys` +
    `selectCommits`) and the `.picked` case of `ContentView.finishCherryPick`.
    Both are generic — rename and reuse them. Set it *after* the log re-read
    has landed: `maintainsSelection` prunes ids the list does not hold, and the
    Tauri side needs the active `CommitInfo`. Both drop shas beyond the loaded
    window, and both then scroll the selection into view.
  - **The Tauri re-seat runs only while History is the visible tab.** Every
    action starts from a History menu, so that holds for them — but anything
    that reads `historySelection` from elsewhere has to prune it first.
  - **The pane's commit is always one of the selected ones, in both clients**,
    and a list with rows never has an empty selection. Natively that second
    half is a write-back in `maintainsSelection` (AppKit allows ⌘-click and a
    click below the rows to empty a `Set` selection), done from `onChange`
    rather than by refusing the write in the binding's setter, so that the
    table redraws. RO-1's "selection frozen" while insertion mode is armed
    will need its own guard there, since AppKit keeps answering clicks.
  - **Known AppKit hazard, not yet observed here:** `contextMenu(
    forSelectionType:)` has been reported to hand back a stale set after a
    *programmatic* selection change (Apple forum thread 710492). WS-C … WS-F
    set the selection programmatically, so `ListSelection.targets` — which
    drops ids the list no longer holds — stays between the closure and any
    action.
  - **The Tauri file list changed with it**, by decision during WS-A: sharing
    the gestures gave it ⌘/Ctrl-click, shift-arrow and ⌘A, which the native
    file list always had from AppKit, and a shift-click straight after
    arriving now extends from the file that opened itself.
  - **No frontend test runner exists.** `utils/listSelection.ts` was checked
    with a throwaway `node --experimental-strip-types` script. MS-6's merge
    predicate and RO-1's insertion arithmetic are the next pure helpers, and
    are the point at which adding one (vitest) is worth proposing.
- **MS-5 — The menus. Cherry-pick's items are built (WS-C); the rest arrive
  with their actions.** Multi-row: the three actions and nothing else — today
  just **Cherry-pick N Commits…**. Single row has **Cherry-pick Commit…** after
  *Check Out Commit…* and gains **Reorder Commit…** beside it in WS-F, since
  N = 1 is the same machinery and the reference has both. Title Case and a real
  `…` in both clients. No single-commit squash. **No item ships before its
  action** (the dead-surface rule), so WS-C added no disabled Squash or
  Reorder placeholders.
- **MS-6 — Menu-time gates, disable don't hide. The shared half is built
  (WS-C)**: every History action is disabled while `status.operation` is set,
  HEAD is detached, or a repository write is in flight
  (`historyActionsBlocked`, which is also the Tauri item's hover reason;
  `canStartHistoryAction`). Still to build, for squash and reorder only: they
  are also disabled when a selected commit sits at or below the newest merge commit in
  the list — every row from HEAD down to the oldest selected one is loaded,
  because the list is append-only from HEAD (`stores/repo.ts:49-73`), so
  `parents.count > 1` answers it without a call. The predicate is a small pure
  helper mirrored in each client (the `listNavigation.ts` /
  `ListNavigation.swift` precedent); the core preflight stays the authority.

### 5.2 In-progress operations (OP)

- **OP-6, OP-7 — Built (WS-B).** The branch chip carries the operation as a
  suffix in the conflict hue (`main · rebasing`); the composer shows a notice,
  locks its fields and offers **Continue Rebase / Cherry-pick / Revert** on
  the Commit button's ⌘↩; the branch menu's **Abort …** is worded for the
  operation. The contract is `FRONTEND.md` §6.14 and the code map is §3
  above. What the later workstreams inherit:
  - **Continue is enabled while files still read as conflicted** — a reversal
    of this plan's first draft, which disabled it. A resolved file stays
    unmerged in the index until it is staged, and staging is what Continue
    does, so a disabled button could only be enabled from a terminal. Core
    refuses by file name instead.
  - **A merge keeps the ordinary Commit button**: concluding one is a commit
    with a message the user writes. `continue_operation` still accepts a
    merge, for a caller that wants it.
  - **Continue must never route through the ordinary `commit()`**, which runs
    `git reset -- .` and re-stages the checked files: that would throw away
    the replayed commit's own cleanly-merged changes. (The same reset runs
    when a *merge* is concluded from the composer — pre-existing, and harmless
    only while every changed file stays checked.)
  - **Error classes** (`FRONTEND.md` §6.13): a refused or re-conflicted
    continue and a failed abort take the blocking modal / sheet; abort's
    "not rewinding" text and the `skipped` sentence take the dismissible
    notice. OP-8 should reuse these, not add a third surface.
  - **Amend mode ends by one rule in both clients**: a status whose `head_sha`
    is not the commit being amended (`refreshStatus` in `MainLayout.svelte`;
    `CommitStore.endAmendingUnlessHead(is:)` fed from `ContentView`). Every
    action in WS-C … WS-G moves HEAD, and none of them has to clear amend
    mode itself.
  - **One write gate per window (built, WS-C)** — §3 has the API. It covers
    commit, continue, every branch action, cherry-pick, checkout-commit,
    undo-commit and discard; **network transfers still have their own slot**,
    so a pull can start under a rewrite and the sync button stays live during
    a merge, cherry-pick or revert (`ROADMAP.md`, *Network transfers and
    repository writes do not exclude each other*). WS-D did not settle it: the
    rule changes what the composer does during a slow push, which is the
    owner's call, and the roadmap item now carries three candidates — the
    recommended one being that **Pull alone claims the write slot too**, as the
    only transfer that writes the working tree. A squash or reorder started
    under a pull is the case that matters to WS-E and WS-F; until the rule
    exists, git's own `index.lock` refusal is what the user would see, and it
    arrives as the action's `Err`.
  - **The status poll does not pause for a write**, so both clients now drop a
    status read that lands after a later-asked one (`FRONTEND.md` §6.1).
    Anything a new action keys on the status — as `cherryPickReturn` is —
    depends on that ordering; do not bypass `refreshStatus` / `RepoStore`'s
    three status writers.
  - **Cross-file comments name symbols, not line numbers.** WS-B shifted two
    dozen `CommitComposer.swift:<n>` references in the Tauri sources; they now
    read `CommitComposer.swift` or `BranchMenu.menuLabel`. Keep to that.
- **OP-8 — Built (WS-C).** A conflict closes the action's dialog, reloads
  status, history and branches, moves to the Changes tab — the conflicted files
  are already there with their `U` badge — and raises the §6.13 modal with
  git's text verbatim. Reload first, report second. A non-conflict failure
  stays in the native sheet (choosing another branch may fix it) and takes the
  modal in the Tauri client, whose popover has nowhere to hold it
  (`FRONTEND.md` §8). Resolving is still the user's editor or the embedded
  terminal (§10). **Not confirmed by eye yet:** natively the conflict modal is a
  second `.sheet` on `ContentView`, presented while the cherry-pick sheet is
  still dismissing. AppKit queues sheets, so it should appear; if the owner
  reports it missing, present it from the first sheet's `onDisappear`.

### 5.3 The three actions

- **CP-1, CP-2 — Built (WS-C).** The target picker is the branch dropdown's
  picking mode, armed from outside (Tauri), and `CherryPickSheet` (native);
  local branches, current one excluded. On success the user is on the target
  with the picked commits selected and in view; on a non-conflict failure they
  are back where they began; Abort of a conflicted pick returns to the source
  branch. The contract is `FRONTEND.md` §6.21.
- **SQ-1 — Message sheet**, built from the composer's summary, description and
  co-author fields — the *components*, not the live composer's state, which
  may hold a draft. Pre-filled in history order, oldest first: summary from
  the target, description from the target's body then each other commit's
  summary and body, co-authors as the de-duplicated union.
- **SQ-2 — The pushed-commits warning is a caption inside the sheet**, not a
  modal in front of it: "These commits are already on `origin/main`. After
  squashing, the next push is a force push." It is fed by
  `RewritePreflight.rewrites_pushed`, which core already computes and no
  client reads yet. The promise is kept by WS-D without further work: after
  the squash the sync button's face *is* Force Push.
- **RO-1 — Insertion mode. Decided: like the reference, plus a sober hint.**
  The menu item arms the list: selection frozen, context menu suppressed, a
  2 px accent insertion line between rows, ↑/↓ move it, ⏎ confirms, Esc or a
  click cancels, and the list scrolls to keep the line visible. The line
  cannot travel below the newest merge commit (MS-6). A small caption —
  `↑ ↓ choose a position · ⏎ move · esc cancel` — fades in when the mode arms
  and fades out after a few seconds or on the first arrow key; under Reduce
  Motion it appears and disappears without the fade.
- **RO-2 — A reorder that rewrites pushed commits asks first**, in a
  `ConfirmDialog` / sheet after ⏎, since reorder has no dialog of its own to
  carry a caption.
- **RO-3 — Native key handling is a spike gate.** `ChangedFileList.swift:118`
  already intercepts Space on a `List` with `.onKeyPress`; whether the arrows
  can be taken from `NSTableView` the same way is checked in a scratch project
  *before* WS-F is sized. The fallback is a focusable overlay that owns the
  keyboard while armed.

### 5.4 Undo (UN)

- **UN-1 — `undo_operation(repo, UndoPoint)`** puts `branch` back on
  `before_sha`, and refuses unless **the tip of `branch` is still
  `after_sha`** and no operation is open. The check is on the *branch*, not on
  HEAD: after a cherry-pick the user may already be back on the source branch,
  and the undo is still valid there. If `branch` is checked out it is a
  `reset --hard` and also needs a tree with no tracked changes, then a
  checkout of `return_branch`; if it is not, it is
  `git update-ref refs/heads/<branch> <before_sha> <after_sha>`, whose third
  argument makes the tip check atomic. Stricter than the reference, which
  would reset over a commit made since.
- **UN-2 — The notice banner learns to carry one action.** "Squashed 3
  commits." · **Undo**. It stays until dismissed, and **disappears by itself
  when the status poll shows `branch` checked out at a HEAD other than
  `after_sha`** — an Undo that can no longer work is not left on screen to
  fail. While another branch is checked out the poll cannot see `branch`'s
  tip, so the banner stays and UN-1 is the judge. The undo point lives in
  client memory; after a restart the reflog is the way back.

## 6. Workstreams

In user-flow order. Every one ships **both clients in the same change**, and
is tested by hand before the next starts.

1. **WS-A — A selection that is a set. Built 2026-09-19, confirmed and
   committed (`5ff2a4c`).** MS-1 … MS-4, no core change.
2. **WS-B — Operations in progress. Built 2026-09-20, confirmed and committed
   (`d0b68df`).** OP-1, OP-3, OP-4, OP-6, OP-7 and the git floor module.
3. **WS-C — Cherry-pick. Built 2026-09-20, confirmed and committed
   (`ff0e195`).** OP-2, OP-5, CP, MS-5's cherry-pick items, MS-6's shared gate,
   OP-8, the window-wide write gate, and status reads published in order.
4. **WS-D — Force push recommended. Built 2026-09-20**; the owner's visual
   check is pending. FP-1, FP-2, the ladder's own module, and one presentation
   table per client. Before squash on purpose: an amended pushed commit already
   produces this state, and squash lands into a sync button that knows what to
   say.
5. **WS-E — Squash. Next.** The rewrite driver, the todo builder, SQ.
6. **WS-F — Reorder.** RO, starting with the RO-3 spike.
7. **WS-G — Undo.** UN, for all three actions at once.

## 7. Verification gates

Per workstream: zero-warning `just mac-build`; `pnpm check`; `cargo test
--workspace` green; `cargo clippy --workspace --all-targets -- -W
clippy::pedantic` no worse than before; `pnpm tauri build` for the Tauri half;
a visual check by the user, **asked for and confirmed — no screenshots**.

Facts about running them on the owner's machine:

- **`just mac-build` links in full** — Xcode's Metal Toolchain, which
  SwiftTerm's shader needs and which is a separate download since Xcode 26
  (`xcodebuild -downloadComponent MetalToolchain`), is installed. It
  regenerates the UniFFI bindings first (`ffi/generated/` is gitignored), so
  the editor's SourceKit "cannot find type" errors on bridge types are noise
  until a build has run.
- **`pnpm build` is `tauri build`** (a full release bundle, several minutes);
  the frontend alone is `pnpm build:frontend`. `pnpm lint` cannot read
  `.svelte` files — prettier has no Svelte plugin configured — so formatting is
  only checked for `.ts`.
- **Clippy pedantic's baseline is about 150 warnings**, all in code older than
  this plan. "No worse" is judged by the count and by no warning naming a new
  file; the two commonest in new code are `needless_pass_by_value` (take
  `&str` in core, let the bridges own the `String`) and a missing `# Errors`.
- **Core tests run against the developer's global git config**: `git_cmd`
  reads `~/.gitconfig`, and `init_test_repo` only pins `user.*` and
  `commit.gpgsign` locally. `core.autocrlf`, a global `core.hooksPath` or
  `rebase.backend=apply` would change results — and a global `core.hooksPath`
  would silently disarm `failing_hook()`, letting WS-C's two hook tests pass
  without testing anything. Not fixed — the fix is
  `GIT_CONFIG_GLOBAL=/dev/null` inside `git_cmd` under `#[cfg(test)]`, which
  does not reach the bridge crate's tests.
- **`cargo test -p leogit-core <filter>` takes one filter**, a substring of the
  test path (`history_rewrite`), not a list.
- **A background build's exit status is the last command's**: a trailing
  `grep -c` that counts zero warnings exits 1. Read the log, not the status.
- **Run `pnpm tauri build` in the foreground.** Detached, its `bundle_dmg.sh`
  step fails (it scripts Finder) after the app itself has compiled and
  bundled, and leaves a scratch image mounted at `/Volumes/dmg.*` —
  `hdiutil detach` it before the retry. Nothing to do with the code.
- **`pnpm lint` does check `commands.ts`**, and prettier here takes no trailing
  comma after the last parameter of a multi-line arrow function.
- **A test that needs a remote** starts from `test_support::published_repo()`
  (bare `origin.git` plus `mine`, whose `main` is pushed and tracking) and
  `clone_of()` for a second contributor. **`mine` is `init` + `push -u`, not a
  clone, and the two differ** — its remote-tracking ref has a reflog, a
  clone's has none, which is what hid §4.8 item 42. Anything that reads a
  reflog or pushes gets a `clone_of()` test as well. The tests read the
  developer's global git config (`git_cmd` does): the probe no longer depends
  on `push.default`, but a global `remote.pushDefault` would still bend the
  force-push tests.
- **A verification agent's finding is reproduced by hand before it is fixed,
  and a research agent's git claim before it is built on.** WS-D had one of
  each wrong way round: two research agents misdescribed `pull --ff`, and the
  plan's own `--force-if-includes` was only caught by a verifier. `#[tokio::test]` is available in core
  for the async commands; an `EventSink` that drops everything is three lines
  (`NoProgress` in `git.rs`'s tests).

Core tests live with their module and build repositories with
`core/src/test_support.rs` (§3 lists it). Bridge tests copy
`cherry_pick_flow_preflights_copies_and_stops_on_a_conflict` (`ffi/src/lib.rs`).
WS-B's sixteen tests are in `operation.rs`, four more in `git_version.rs`,
WS-C's fourteen in `history_rewrite.rs`, and WS-D's in `sync_ladder.rs` (the
ladder and seven runs of the probe) and `git.rs` (seven `force_push_*`, each
asserting what the remote holds afterwards). Still
to write, at minimum, named as sentences:
`squash_gathers_a_non_contiguous_selection_at_the_target`,
`squash_keeps_the_message_across_a_conflict`,
`squash_reaching_the_first_commit_uses_root`,
`squash_stopped_with_nothing_to_resolve_is_a_failure_not_a_conflict`,
`reorder_moves_commits_to_the_tip_and_into_the_middle`,
`reorder_keeps_a_commit_that_became_empty`,
`todo_path_with_spaces_and_quotes_is_safe`,
`undo_refuses_once_the_branch_tip_has_moved`,
`undo_of_a_cherry_pick_works_from_the_source_branch`.
Two probe claims in `operation.rs` rest on a scratch run rather than a test and
are cheap to add: the `--rebase-merges` stop that leaves `MERGE_HEAD` beside
`rebase-merge/`, and a real `git am` (the existing test fabricates the
directory).

## 8. Documentation on completion

WS-B's and WS-C's shares are written. WS-C's: `FRONTEND.md` §1 (73 commands
per host), §3.7 (*history actions* rows and the three-outcome contract), §5
(*History actions* DTOs), §6.1 (status reads in order), §6.4, §6.14, the new
**§6.21** (the write slot and cherry-pick — **the next actions extend §6.21
rather than adding a rule each**) and two §8 rows; `DESIGN.md` flows 5 and 6;
`STYLE.md` (context menus, dialogs, branch picker); `TECHNICAL.md` (*History's
multi-commit actions*, both write gates, status ordering); `README.md`; and
`ROADMAP.md`'s entry, with *Cherry-pick / revert* reworded to *Revert, and
cherry-pick into the current branch* and left open. WS-D's: `FRONTEND.md` §5.1
(the `ForcePush` variant) and §6.2 (the rung, its doubts, the pinned-lease force
push); `DESIGN.md` flow 7 and the amend flow's last sentence; `STYLE.md` (the
glyph, and why the face is not red); `TECHNICAL.md` (*Sync proposal*, the
layout tree, test fixtures); `README.md`; and `ROADMAP.md`'s entry, which
closed *Force-push-recommended detection* and opened *A push names the local
branch…*. No command was added, so §1's count stands at 73. What is still owed:

- **`FRONTEND.md`** — §1 command counts again; §3.7 rows for `squash_commits`,
  `reorder_commits` and `undo_operation`; §6.21
  rules for the squash target, insertion mode and the undo banner's lifetime.
- **`DESIGN.md`** — one bullet per action that states the failure path as
  carefully as the happy one (cherry-pick's is the model).
- **`STYLE.md`** — the insertion line, the hint caption and the banner action
  get their metrics.
- **`TECHNICAL.md`** — the driver, the todo builder, the message file.
- **`ROADMAP.md`** — a dated entry for *Rebase (interactive UI)* (squash and
  reorder; edit, drop and drag stay open).
- **`README.md`** — *Browse history* gains squash and reorder.

## 9. Standing decision — where history rewriting runs

**Decided (2026-09-19): the system git, recent versions only (§4.7), until a
Rust drop-in replacement exists.** Every git call the core makes goes through
`git_cmd` (`core/src/git.rs:351`); neither `git2` nor `gix` is anywhere in
`Cargo.lock`. The alternatives, as they stood on the day of the decision:

| | Squash / reorder / cherry-pick | User's hooks, signing, LFS, config | Continue or abort from a terminal | Cost |
| --- | --- | --- | --- | --- |
| **System git, recent only — chosen** | Yes — §4 as written, verified | Full: it *is* their git | Native | One version check |
| Bundled git (what GitHub Desktop does, via `dugite-native`) | Yes, version pinned by us | Full | Native | A per-platform git build to ship, sign and keep updated; a larger bundle |
| `git2` 0.21 (libgit2 1.9) | Cherry-pick and a *pick-only* rebase exist; **squash and reorder must be hand-rolled** from `cherrypick_commit` plus commit creation — `git_rebase_init` takes no todo | Runs no hooks, does not sign by itself (`commit.gpgsign` becomes the app's job through `commit_signed`), does not run the LFS filter process | Only for its non-in-memory rebase; a hand-rolled squash leaves no state a terminal can continue | A C build (cmake) on every target; libgit2 is GPLv2 with a linking exception |
| `gix` 0.87 (pure Rust) | **No.** Its own `crate-status.md` lists rebase and cherry-pick as not yet built; GitButler's `but-rebase` hand-rolls both on gix's merge and commit primitives | Hooks, filters/LFS and signed-commit creation also unbuilt | None | Pure Rust, easy to cross-compile; the most code to own |

**What "drop-in" has to mean before this is reopened:** a pure-Rust library
that drives cherry-pick and a todo-style rebase, signs per the user's config,
runs hooks and the LFS filter process, and leaves sequencer state the embedded
terminal can continue or abort. `gix` is the candidate to watch; its
`crate-status.md` is the page that answers it. The seam is already in place:
§4 is two modules (`operation.rs`, `history_rewrite.rs`) behind seven bridge
calls, so the backend can change without the bridges or the clients noticing.

## 10. Non-goals

- **Drag-and-drop**, for reorder and squash-by-drop (`ROADMAP.md`, *Rebase
  (interactive UI)*) or cherry-pick onto the branch chip (*Drag a commit onto
  the branch dropdown*). RO-1 is built so a drop can feed the same
  `reorder_commits` call later; natively the macOS 26 drag-container family
  (`dragContainer(for:in:_:)`, `dragContainerSelection`) lifts a whole
  selection and type-checks against this project's target, and `onMove` is
  the wrong tool — single-row, and it fights paging.
- **A range diff for a multi-row selection.** The pane keeps each client's
  file-list rule (MS-3).
- **Cherry-picking onto a new branch, or from another branch's history**
  (`ROADMAP.md`, *Revert, and cherry-pick into the current branch*). History shows the current
  branch only; create the branch first.
- **Stash-and-continue on a dirty tree** — with the stash feature, and only
  with a restore.
- **A conflict-resolution surface** — still its own roadmap item. This plan
  makes a conflicted rebase legible and escapable, as merge is today.
- **Progress streaming.** Local operations do not touch `EventSink`
  (`core/src/events.rs:10-11`); the dialog holds itself under a busy label.
  Revisit only if a 500-commit replay proves slow in practice.
- **Revert, edit, drop, single-commit squash, AI-written squash messages, a
  "don't warn me again" setting.**
