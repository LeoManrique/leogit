# Plan — History multi-commit actions (cherry-pick, squash, reorder)

> Status: **complete. Every workstream is confirmed and committed: WS-A (a
> selection that is a set, `5ff2a4c`), WS-B (operations in progress,
> `d0b68df`), WS-C (cherry-pick, the preflight and the window-wide write gate,
> `ff0e195`), WS-D (force push recommended, `d979bf2`), WS-E (the rewrite
> driver and squash, `bc8f3db`), WS-F (reorder, `f5976a6`), WS-G (undo,
> `19d066b`) and WS-H (the owner's answers to what the plan had left open,
> §6.1, `497f4a3`).** Every decision in this document is the
> owner's and is marked **Decided**; nothing is left *Proposed*. §9 records the
> standing decision on where rewriting runs. The plan is kept as the record a
> later rewrite (edit, drop, a drag — `ROADMAP.md`, *Rebase (interactive UI)*)
> starts from: §3 is the code map as it stands *after* WS-H, §4.2 – §4.6 and
> §5.1 – §5.4 are what such a rewrite inherits, §4.8 is every git fact that was
> measured, and §7 is how the work was checked and what that taught.
> Produced from a three-way read of the native client, the Tauri client, and
> the GitHub Desktop source at
> `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop` — whose history and
> multi-commit code is upstream's in every file §2 names (the fork's changes
> to `app-store.ts` and `dispatcher.ts` sit above it, so **the few line numbers
> this plan still cites are this fork's**, not upstream's). The reference is used to judge *how*; the
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
operation** (built, WS-B), **and then the three actions on top** — cherry-pick
(WS-C), squash (WS-E) and reorder (WS-F) — **and the way back from each**
(undo, WS-G). WS-H closed what the plan had left open (§6.1). All of it is
built.

## 2. What the reference does

GitHub Desktop was read for *how*, and every action here is built, so only what
a later rewrite could still use is kept. Its code for this lives in
`app/src/lib/git/{cherry-pick,squash,reorder,rebase}.ts`,
`app/src/ui/history/commit-list.tsx` (the menus, the reorder hint) and
`app/src/lib/stores/app-store.ts` (the dirty-tree dialog, the undo banner).

- **Same shape**: a multi-row menu of exactly the three actions; cherry-pick
  onto another branch through a branch picker; squash and reorder as a generated
  todo fed to `git rebase -i` through a sequence editor, `--root` when the range
  reaches the first commit; reorder armed from the menu as an insertion mode
  (↑/↓, ⏎, Esc) that lands on the same path as its drag-and-drop; a merge commit
  in the replayed range refuses; an undo back to the pre-operation tip.
- **Different on purpose**: it stores the selection in click order and re-sorts
  where it matters, LeoGit hands every action its commits in list order; it
  offers *Stash Changes and Continue* on a dirty tree and never pops the stash,
  LeoGit blocks (§4.2); its undo is a bare `reset --hard` with a 15 s banner,
  LeoGit's is §5.4; it bundles its own git, **LeoGit runs the system git**
  (§4.7).
- **Four things it gets wrong, none copied**: a squash that conflicts before the
  squashed commit exists loses the typed message (its Continue runs under
  `GIT_EDITOR=':'`; here the message is a todo line, §4.5); its pushed-commits
  warning fires on somebody else's push (it counts `lastRetained..upstream`);
  force-push-recommended is an in-memory map a terminal rewrite or a restart
  never reaches (here it is the reflog, §4.6); and it splices paths into a shell
  string (`sequence.editor=cat "<path>" >`; here they travel in the
  environment).

## 3. What exists today

**Native History list** — `Screens/HistorySidebar.swift` is a stock
`List(commits, selection: $selection)` over a `Set<String>`
(`ContentView`'s `historySelection`), so shift-click, ⌘-click, shift-arrow and
⌘A are AppKit's, not ours; `selectedSha` beside it is the one commit the detail
pane shows, derived from the set. Its `.contextMenu(forSelectionType:)` puts
the ids in list order with `ListSelection.targets` and builds `rowMenu(for:)`
for one target and, for several, `cherryPickItem(for:)`, `squashItem(for:)` and
`reorderItem(for:)` (which `rowMenu` carries too), and `canStartHistoryAction`
beside it is the shared menu-time gate (no operation, not detached, no write in
flight); Squash and Reorder add `canReplay(_:)` — `HistoryRange.replaysMergeCommit`.
The list also holds reorder's insertion mode (`ReorderMode`, §5.3). A selection made in
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
`menuItems` is `[cherryPickItem, squashItem, reorderItem]` for several targets
and `singleCommitItems` (which holds `reorderItem` too) for one. An
item's `enabled` / `title` come from the `historyActionsBlocked` prop, the
shared menu-time gate, worded in `MainLayout.svelte`; squash and reorder add
`replayBlocked` — `replaysMergeCommit` (`utils/historyRange.ts`). The component
also holds reorder's insertion mode (§5.3). `ContextMenu.svelte` items are
`aria-disabled`, not `disabled`, so that `title` can show.

**History actions** (WS-C, WS-E, WS-F, WS-G, WS-H) — `core/src/history_rewrite/`: `mod.rs`
holds `rewrite_preflight(repo, replayed_from)` → `RewritePreflight { blocked,
rewrites_pushed }` (constructors `ready`, `refused`, `for_a_replay`),
`RewriteResult` with its three (`landed`, `unchanged`, `stopped`),
`UndoStart` (the way back as known before the run: `on_the_current_branch`,
`landed_on(after_sha)` → `UndoPoint`; a stop hands it back as
`RewriteResult.start`, §4.3), `UndoPoint`, and the two helpers more than one
action needs —
`open_operation_refusal` (the "a rebase is in progress" sentence) and
`switch_to`; `cherry_pick.rs` has `cherry_pick_commits` and the
private `return_to`; **`undo.rs` has `undo_operation(repo, &UndoPoint)` →
`UndoResult { undone, message }` (§5.4)**; **`replay.rs` is the rewrite driver**
(`Replay { repo_path, onto, todo, message: Option }.run()` → `Replayed::Done |
Conflict`, §4.5); **`lineage.rs` is `Lineage::of(repo, shas, landmark)`, which
places any shas on the branch (oldest, range, selected in history order)**;
`squash.rs` has `squash_commits(repo, shas, message)` and
`squash_draft(repo, shas)` → `SquashDraft { summary, description, co_authors }`;
`reorder.rs` has `reorder_commits(repo, shas, before_sha)` and
`reorder_preflight(…)`; `fixtures.rs` is the tests' `linear_repo()`,
`five_commits()`, `edits_of_one_line()`, `failing_hook()`,
`repo_with_a_submodule()`, `sha`, `branch`. All
seven commands are
on both bridges (`ffi/src/lib.rs`, *history actions*;
`src-tauri/src/shims/history_rewrite.rs`). Client side: `MainLayout.svelte`'s
*History actions* section — a `request…` / `run…` pair per action,
`SquashDialog.svelte`, **`finishHistoryAction({ repoPath, result, reload,
stoppedOnConflict, action, leavesOpen, asked })`, every action's ending**,
`finishContinue` (a Continue's ending, §5.4) and `runUndo` —
and natively `Stores/HistoryActionStore.swift`
(`HistoryActionOutcome` — every action's answer, whose `.landed` carries the
`UndoOffer` — `refusal`, `cherryPick`,
`prepareSquash` → `SquashReadiness`, `squash`, `prepareReorder` →
`ReorderReadiness`, `reorder`, the private `outcome(of:…)` all three end in,
`openAction` with `keepOpen(_:)`, `continueBegins()` and `wayBack(after:under:)`,
`undo` → `UndoOutcome`) with
`ContentView.requestCherryPick` / `requestSquash` / `requestReorder` /
**`finishHistoryAction`** (every action's ending) / `finishContinue` /
`undoHistoryAction` and
`Screens/CherryPickSheet.swift`, `Screens/SquashSheet.swift`,
`Screens/ReorderSheet.swift`. **A new action copies that shape: preflight on the
menu click, then the dialog, then one core call, then reload → select
`RewriteResult.selection` or go to Changes on a conflict.**

**The write gate** (WS-C) — one slot per window for every repository write:
`stores/repoWrite.ts` (`beginRepoWrite(kind)` / `endRepoWrite()`,
`isHeldByAnother`, `REPO_BUSY_MESSAGE` and its hover form `REPO_BUSY_REASON`;
add the new action's name to
`RepoWriteKind`, as `'squash'` was) and `Stores/RepositoryWriteGate.swift` (`claim()` →
`Claim?`, `release(_:)`, `busyMessage`, `busyReason`). **Pull holds it as well
as the network slot** (WS-H, §5.2) — the one transfer that does. **Every History action must claim
it**, report a refused claim as its own outcome (never success), and give any
dialog that can sit open under another write the `blocked` prop /
`WriteBlockedNote`. Stores keep *cannot start* (`isBlocked`, the gate) apart
from *I am working* (`isRunning`, their own).

**The operation in progress** (WS-B) — `core/src/operation.rs`:
`OperationInProgress { Merge, Rebase, CherryPick, Revert }`, the filesystem
probe `in_progress(git_dir)` that fills `RepoStatus.operation` on every status
read, `continue_operation` → `OperationOutcome { success, conflicts,
error_message, skipped, started_at }` and `abort_operation` → `Option<String>` (whatever git
printed). Both are on both bridges (`ffi/src/lib.rs`, *the operation in
progress*; `src-tauri/src/shims/operation.rs`). `core/src/git_version.rs`
holds `FLOOR`, the `git --version` parser and `require_floor()`, whose one
caller is `rewrite_preflight`. `core/src/test_support.rs` (`git`, `git_stdout`,
`git_stopping` for a command expected to stop, `init_test_repo`, `commit_file`,
`subjects`, and `conflicting_repo()`) is what every core test module builds its
repositories with; `history_rewrite/fixtures.rs` — `five_commits()`,
`edits_of_one_line()` (every commit rewrites one line, so any replay out of
order conflicts) — holds the fixtures to reuse for a rewrite. `git.rs` lends
`pub(crate)` `git_cmd`, `run_git`, `run_git_optional` (exit 1 is `None`),
`run_git_combined`, `run_git_combined_with_env`, `read_commits` (full
`CommitInfo`s for a list of ids, in the order given), `git_dir`, `current_branch`, `has_commits`,
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
`CommitComposer`'s `continuing:` / `onContinue:`), and answers its owner through
`onOperationContinued(outcome)` → `finishContinue` in both clients. Abort of a
cherry-pick begun in History also switches back to the branch it came from
(`openAction.start.return_branch`, client memory dropped by one rule on the
status read, §5.4). After a continue **both
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
`RootSheet` (the enum at the foot of `ContentView.swift`, with `.cherryPick`,
`.squash` and `.reorder` cases);
`ConfirmDialog.svelte` and the one-off dialogs on the Tauri side, all with the
`blocked` prop; the branch dropdown's *picking mode* (`STYLE.md`, *Branch
picker*), which `BranchDropdown.svelte` enters from outside when
`cherryPickCount` is set (`mode` is derived from it, so a second externally
armed mode is one more prop and one more `MenuMode`); and **the strip under the
header, one component per client** (`components/StatusStrip.svelte`,
`Design/StatusStrip.swift`: a tone — `warning` or `done` —, a message, an
optional detail, an optional action link and an optional ✕) on three separate
lines: the poll failure, the notice (`reportNotice` in `stores/repo.ts` and its
native counterpart) and the undo offer.

**The undo offer** (WS-G) — client memory, mirrored and pure:
`utils/undoOffer.ts` (`UndoOffer { repoPath, sentence, point, restores }`,
`UndoableAction`, `landedSentence(action, count)`, `undoStillStands`, and for an
action that stopped on a conflict `OpenAction { repoPath, operation, start,
action, asked }`, `openActionStillStands`, `undoPointAfterContinue` — tested in
`tests/undoOffer.test.ts`) and
`Services/UndoOffer.swift` (the same, less `repoPath`: the store is per
repository). One offer per window, set by `finishHistoryAction` — or by
`finishContinue` — **after** the
reload, because the rule that retires it reads the status. The store's status
says `headSha` on the Tauri side where core's `UndoPoint` says `after_sha` —
the helper takes `{ branch, detached, headSha }`, not core's `RepoStatus`.

## 4. Core design

The core lives beside `git.rs`, which is already 4 000+ lines, in modules
split by responsibility: `core/src/operation.rs` (detect, continue, abort),
`core/src/git_version.rs` (the floor), `core/src/sync_ladder.rs` (the sync
proposal and the reflog test), and `core/src/history_rewrite/` for everything
that *starts* an operation or takes one back — a module per action (§3). It
reuses the `pub(crate)` helpers §3 lists.

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
- **During a rebase `status.detached` is true and `status.branch` is empty**
  (`get_status` in `git.rs`), so every gate written as "HEAD
  is detached" also fires mid-rebase, **and any client rule written as "still on
  branch X" is false for the whole of a stopped rebase** — WS-H's first keep-rule
  for `openAction` was "same operation and same branch", and would have forgotten
  every squash and reorder on the first status read (§5.4). Ask about `operation` first when the
  wording matters (the branch menu's merge help does). The chip reads
  `Detached at <sha> · rebasing`; reading the branch from
  `rebase-merge/head-name` would be the improvement, and is not built.

### 4.2 Preflight, continue, abort (OP-2 … OP-4)

- **OP-2 `rewrite_preflight(repo, replayed_from: Option<&str>)` — built
  (WS-C).** Mechanics are `TECHNICAL.md`, *History's multi-commit actions*.
  A refusal is **data** (`blocked`), in this order: the git floor, an operation
  in progress, a detached HEAD, an unborn one, tracked changes (named, up to
  ten; untracked files pass). **`Some(oldest)`** adds the merge-commit-in-range
  and shallow-boundary refusals and fills `rewrites_pushed`; cherry-pick passes
  `None`, squash the oldest selected commit, and reorder has
  `reorder_preflight` (§4.5), the same checks from where *its* replay starts.
  Both clients call it **on the
  menu click, before any dialog opens**, and the action calls it again itself.
  What any later action inherits:
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
  under `GIT_EDITOR=:`; a rebase in that position drops the commit by itself on
  `--continue`, and only the report changes (§4.5). It returns `OperationOutcome { success, conflicts,
  error_message, skipped, started_at }`; `started_at` is git's own record of
  where the operation began, read **before** the step, since finishing deletes
  it (§4.3). A step that stops on the next conflict is
  `success: false` with the conflicts, not an `Err`. An `Err` always means git
  was never asked to continue **and nothing was staged** — both refusals run
  before the first write. `skipped` exists because git drops the commit
  silently; both clients turn it into a notice. What the actions rely on:
  - **The squash message survives because the amend is a todo line (§4.5)**,
    rescheduled by git if it fails, not because of anything continue does —
    continue always runs under `GIT_EDITOR=:` and has no message parameter. A
    continue that stops on a failed `exec` answers `success: false` with **no
    conflicts** and git's text; pressing Continue again re-runs the line.
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

**Built (WS-C, WS-H)**, in `history_rewrite/mod.rs` and on both bridges:
`RewriteResult { success, conflicts, error_message, selection, undo, start }`,
`UndoPoint { branch, before_sha, after_sha, return_branch }` and `UndoStart` —
the same less `after_sha` — one set of DTOs for all three
actions. **The three outcomes are a contract every action keeps**
(`FRONTEND.md` §3.7): `Ok(success)` with `selection` newest first and `undo`
set; `Ok(!success)` **only** for a conflict, `conflicts` never empty, the
operation left open, **and `start` set — on a stop and on nothing else**; `Err`
with the repository back where it began, or a
second paragraph saying where the user was left. Once the commits have landed
nothing may turn the answer into an `Err` — a tip that cannot be read back
costs `selection` and `undo`, not the success.

**Decided (WS-H, 2026-09-21): an action continued past a conflict offers its
Undo, by client memory.** Every action reads its `UndoStart` before it runs
anything; landing completes it (`landed_on`), a stop hands it to the client,
and the client completes it after a Continue that succeeds with the reloaded
status's `head_sha` (§5.4). **Continue itself still hands back no `UndoPoint`** —
an operation continued here may have been started in a terminal, and core
keeps nothing between two calls. What it does hand back is
`OperationOutcome.started_at`, where git's own state says the operation began:
the `orig-head` of `rebase-merge/` or `rebase-apply/`, `sequencer/head`, and —
with none of those directories, which is a merge and a pick or revert of **one**
sha (§4.8 items 31, 64) — `HEAD`, since such an operation has committed nothing
yet. It is the guard, not the memory: it says nothing about which *branch* the
user left or whether the app began the operation at all, and a client that finds
it different from the kept `before_sha` — or missing — offers nothing.
`undo_operation` needed no change: it judges any point by the branch's tip.

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
  (`openAction.start.return_branch`, §5.4), set from the conflict result and
  dropped by one rule on the status read. It is what makes Abort end on the
  source branch; it is not persisted across a restart. **After a Continue
  nothing switches back**: the user stays on the target, which is where the
  post-Continue Undo expects them.

**SHAs cross the bridge newest-first, as the list shows them**, for all three
actions; the core reverses for cherry-pick and re-derives order from the range
walk for squash and reorder. Every sha is checked with `is_object_id` before
it becomes an argument — it is also what keeps `--abort` out of argv.

### 4.5 The rewrite driver, squash and reorder — built (WS-E, WS-F)

**The driver, squash and reorder are built**; mechanics are `TECHNICAL.md`,
*History's multi-commit actions* (the command line, the todos, the message
file, `Move::plan`), and the contract is `FRONTEND.md` §3.7. What a later
rewrite (edit, drop, a drag) inherits, and where the build differs from this
plan's draft:

- **The driver is `replay.rs`: give it a todo, an `onto` and, optionally, a
  message.** `Replay { repo_path, onto: Option<&str>, todo, message:
  Option<&str> }.run()` answers `Done`, `Conflict(paths, text)` or `Err` with
  the rebase already aborted. `onto` is the parent of the oldest commit in the
  todo (`parent_of`), `None` → `--root`. With no message `LEOGIT_MESSAGE` is
  empty and the sequence editor copies the todo alone.
- **A message travels inside the rebase**, at `rebase-merge/leogit-message`,
  so git owns its lifetime — every conflict round, finish and abort, from the
  app or a terminal. Nothing in `continue_operation` or `abort_operation` knows
  which action opened the rebase, and nothing should.
- **`--reschedule-failed-exec` is not optional**: a failed `exec` is crossed off
  the todo *as it fails*, so the next `--continue` finishes the rebase without
  it. Any `exec` line a later todo adds gets the same protection for free. The
  other pinned flags are `TECHNICAL.md`'s.
- **A stop is a conflict only with unmerged paths**, as for cherry-pick. Every
  other stop — signer, `prepare-commit-msg` (which runs on every pick, fixup
  *and* the amend despite `--no-verify`), an untracked file in the way, a failed
  `exec`, a fold that would come to nothing — is aborted by the driver. A
  refusal before the rebase opened (`pre-rebase` hook, a broken sequence
  editor) leaves no `rebase-merge/`, so the driver checks before aborting.
- **`Lineage::of(repo, shas, landmark)` (`lineage.rs`) does not trust the order
  shas arrive in.** Oldest = `merge-base --octopus <shas> <landmark>`; range =
  `rev-list --reverse HEAD --not <oldest>^@`; refused unless every sha *and the
  landmark* are in the range and the range starts at the oldest. The landmark
  is a commit the range must reach without being selected — reorder's
  destination. **It answers a `Result` in a `Result`**: the inner `Err` is
  `merge_refusal`, asked of the merge-base *before* the walk — a walk across a
  merge is not one line, and without that order a merge commit is reported as
  "not all on the current branch". The opposite mistake is closed just before
  it: the base must be one of the named commits and an ancestor of `HEAD`, or
  any merge in the branch's past gets blamed for a commit that is somewhere
  else. An id that names no commit fails `merge-base` and gets the same
  sentence.
- **Each action writes its own todo; there is no general todo builder.**
  Squash's is "target, the other selected as `fixup`, the amend, then everything
  unselected"; reorder's (`Move::plan`) emits every unselected commit in range
  order and the block just before the destination's line, or at the end for the
  tip. They share the driver and `Lineage`, which is where the sharing pays.
- **A selection is read back by counting from HEAD** — squash's is
  `HEAD~<unselected commits in the range>`, reorder's `rev-list -n <above +
  moved> HEAD` from `above` on. `--empty=keep` keeps every replayed commit,
  which is what makes counting safe; a commit dropped by a conflict resolved to
  nothing would shift it — after a *continue*, which hands back no selection
  anyway.
- **Reorder judges a no-op by building the new order and comparing**, and then
  runs no git at all: even a rebase that ends where it began writes `ORIG_HEAD`
  and two HEAD reflog entries (measured), and a move that changes nothing should
  leave no trace. The answer is `RewriteResult::unchanged` — success, no `undo`,
  so there is no Undo to offer for it, and an earlier action's offer stays where
  it is.
- **Reorder trims the leading commits that keep their place out of its todo**,
  so the replay starts at the first *displaced* commit. That is what
  `reorder_preflight` judges — which is why reorder has a preflight command of
  its own: `rewrite_preflight(replayed_from)` needs a start the client cannot
  know without the destination.
- **A rebase silently drops a commit whose conflict is resolved to nothing**, on
  `--continue`, whatever `--empty` says (measured; pinned by
  `a_moved_commit_resolved_to_nothing_is_dropped_and_continue_says_so`).
  `continue_operation` now answers `skipped: true` there, and both clients say
  so in the notice banner, as for a skipped pick. **The sign is git's own state:
  `rebase-merge/stopped-sha` exists and `rebase-merge/amend` does not**
  (`rebase_stopped_on_a_pick`). The first draft used "this continue staged a
  conflict" and was wrong both ways — a resolution staged from the terminal
  leaves nothing unmerged and the commit still goes; a conflicted `fixup`
  resolved to HEAD staged a conflict and lost nothing. Measured stops: pick
  conflict Y/n, `edit` Y/Y, `fixup` conflict Y/Y, `break` n/n, failed `exec`
  n/n (`stopped-sha` / `amend`).
- **`core.commentChar` set to a command's first letter (`p`) turns every `pick`
  into a comment**; git answers "error: nothing to do" before touching anything
  (reproduced by hand). Left alone: `-c core.commentChar=#` for the run would
  desynchronise the rounds Continue runs. git checks nothing about a todo it is
  handed beyond its syntax — that every commit of the range appears once is
  entirely `Move::plan`'s to get right.
- **`--no-update-refs` leaves any other branch that pointed into the range on
  the old commits.** Deliberate, as for squash. And making a commit that
  *modifies* a file the new root usually conflicts, since the file does not
  exist under it yet — correct, and continued or aborted like any conflict.
- **300 replayed commits take about 1.5 – 1.8 s** here; no progress streaming
  was added (§10).
- **A shallow clone's boundary commit reads as a root** — `rev-list --parents`,
  `<sha>^@` and `rev-parse <sha>^` all hide its parent — and `rebase --root`
  from it *succeeds*, severing the branch from the history below. WS-E's
  verifier found it; `replay_refusal` (the preflight's replay half) now compares
  the walk with the commit object (`cat-file commit`) and refuses. Reorder asks
  the same function of its `from`; **anything else that derives "this is
  the root" from a parent list needs the same check** (`parent_of` /
  `records_a_parent` in `mod.rs`).
- **The preflight is two functions**, `branch_ready_for_an_action` (the shared
  refusals, answered before an action reads anything else off the repository —
  otherwise a detached HEAD surfaces as "not on the current branch") and
  `replay_refusal(oldest)`; `rewrite_preflight` composes them for the clients.
- **A terminal `git rebase --skip` on a conflicted `fixup` drops that commit's
  changes and the amend still stamps the typed message.** The app never runs
  `rebase --skip` (for a rebase `resolved_to_nothing` changes the report, not
  the step), so it is reachable from a terminal alone — git's own semantics,
  not defended against.
- **While a rewrite is stopped, HEAD's message may be git's working text** (`#
  This is a combination of 2 commits…`). Nothing in either client shows HEAD's
  message during a rebase today; keep it so.

**Decided (2026-09-19): the squash target is the oldest selected commit, in
both clients** — `contextMenu(forSelectionType:)` hands the native client the
*set* and never the row, the oldest commit is how `fixup` is already understood,
and the two clients stay identical in something that rewrites history.
**Decided: non-contiguous selections are allowed**, for squash and reorder
alike.

**RO — `reorder_commits(repo, shas, before_sha: Option<&str>)`.** `before_sha`
is the commit the moved ones land just *under* in a newest-first list; `None`
moves them to the tip; it may itself be one of the moved commits ("take the
moved rows out, put them back where the line is"). A destination that is not on
the branch is an `Err` from `Lineage::of`, like a selected commit that is not.

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
  correct, and the undo strip says nothing about it. Both are tests
  (`undo_of_a_rewrite_of_pushed_commits_is_back_in_sync_with_the_upstream`,
  `undo_after_the_force_push_proposes_the_force_push_again`).

### 4.7 The git floor

**Decided (2026-09-19): the system git, recent versions only.** LeoGit tracks
current git rather than carrying old spellings or second code paths; the floor
is simply the newest flag §4 uses, and it rises whenever a newer git has
something worth using.

| Flag | Since | Used by |
| --- | --- | --- |
| `cherry-pick --empty=keep` | **2.45** | CP |
| `rebase --no-rebase-merges` (as the override of `rebase.rebaseMerges`) | 2.41 | SQ, RO |
| `rebase --no-update-refs` | 2.38 | SQ, RO |
| `rebase --empty=keep` (merge backend, which `-i` always uses) | 2.26 | SQ, RO |
| `cherry-pick -m 1` on non-merge commits | 2.21 | CP |
| `rebase --reschedule-failed-exec` | 2.21 | SQ, RO |

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

Added in WS-E (2026-09-20, same conditions). Items 43 – 47 and 51 are behind core
tests; 48 – 50 were run by WS-E's research agent, and 47 and 51 were found by
agents and re-run by hand before they were built on:

43. A message file copied to `rebase-merge/leogit-message` by the sequence
    editor (`$(dirname "$1")`) is used by the `exec` amend, survives two
    conflict rounds, and is gone after finish and after `--abort`; `--git-path
    rebase-merge/…` resolves in a linked worktree and from a subdirectory, and
    git hands the editor an absolute todo path in all three. ✅
44. The squash todo under `rebase.updateRefs`, `autoSquash`, `autoStash`,
    `rebaseMerges`, `abbreviateCommands`, `missingCommitsCheck=error`,
    `core.commentChar=;`, `sequence.editor=false` and `core.editor=false` all
    set: same result, and a branch pointing into the range does not move.
    `GIT_SEQUENCE_EDITOR` outranks `sequence.editor`. ✅
45. A conflict **on the `fixup`** (before the amend), then one on a replayed
    pick: both continue under `GIT_EDITOR=:` with no editor, the fold still
    folds, and the final message is the typed one. Mid-chain HEAD's message is
    `# This is a combination of 2 commits.…`. ✅
46. A failing `prepare-commit-msg` hook stops the rebase with no unmerged paths
    (it runs on every pick, fixup and the amend, `--no-verify` or not);
    `pre-commit` and `commit-msg` never run; `post-commit` / `post-rewrite`
    failures are ignored; a `pre-rebase` refusal and a failing sequence editor
    leave **no** `rebase-merge/`. ✅
47. **A failed `exec` is already in `done`: `--continue` skips it and finishes
    "successfully"** — without `--reschedule-failed-exec`, which keeps it in the
    todo and is stored with the rebase. ✅
    (`a_failed_amend_is_run_again_by_the_next_continue`)
48. `--amend -F` keeps the oldest commit's author and author date; with a
    working signer every rewritten commit is signed, the amended one included;
    a signer that cannot run stops at the first commit the rebase has to
    *create* — a fast-forwarded first `pick` creates none. ✅
49. A fold whose result is empty fails at the `fixup` ("would make it empty"),
    with a staged change and no unmerged paths; `--continue` only fails again at
    the amend. A replayed `pick` that became empty is kept silently by
    `--empty=keep`. ✅ (`a_squash_that_would_come_to_nothing_is_refused_and_undone`)
50. `commit.cleanup=strip` deletes `#` lines from a `commit -F` message;
    `--cleanup=whitespace` overrides it. `%b` includes the trailer block.
    `log.showSignature` writes into `git log`'s stdout without
    `--no-show-signature`, and `i18n.logOutputEncoding` re-encodes it without
    `--encoding=UTF-8`. 300 replayed commits take about 2 s. ✅
51. **In a `--depth` clone the boundary commit's parent is hidden** from
    `rev-list --parents`, `^@` and `rev-parse <sha>^`, while `cat-file commit`
    shows it; `rebase --root` from it exits 0 and produces a parentless
    branch. ✅
    (`squash_refuses_to_replay_from_the_commit_a_shallow_clone_ends_on`)
52. A todo of `pick` lines in a new order, replayed with the driver's flags,
    moves commits to the tip, into the middle and under the first commit (a new
    root, `--root`); a commit the new order made empty is kept by
    `--empty=keep`. ✅ (`reorder_moves_*`, `reorder_under_the_first_commit_*`,
    `reorder_keeps_a_commit_that_became_empty`)
53. **A rebase that ends where it began still writes `ORIG_HEAD` and two HEAD
    reflog entries**, so reorder's no-op runs no git. ✅
    (`reorder_of_a_no_op_destination_changes_nothing`)
54. **`rebase --continue` drops a commit whose conflict was resolved to
    nothing**, without a word and whatever `--empty` says; the rebase then goes
    on — also when the resolution was staged by hand. `rebase-merge/stopped-sha`
    without `rebase-merge/amend` is the stop where that can happen. ✅
    (`a_moved_commit_resolved_to_nothing_is_dropped_and_continue_says_so`,
    `a_resolution_staged_from_a_terminal_still_reports_the_dropped_commit`,
    `a_fold_resolved_to_nothing_is_not_reported_as_a_skipped_commit`)
55. `core.commentChar=p` makes every `pick` line a comment: "error: nothing to
    do", exit 1, no `rebase-merge/`, nothing moved. ✅ (by hand)
56. A walk across a merge (`rev-list HEAD --not <oldest>^@`) is not one line, so
    "every sha is in the range and it starts at the oldest" fails before any
    merge check placed after it. ✅
    (`reorder_refuses_a_merge_between_the_commits_and_the_destination`)
57. **`reset --keep <sha>` is the only way back that harms nothing.** Over the
    same before/after pair: `reset --hard` deletes an untracked file that sits
    where the old commit has one; `reset --merge` throws staged changes away
    without a word; `switch -C` runs `post-checkout` and exits 1 on the hook's
    failure *after* it has moved everything. `--keep` refuses all of them and
    moves nothing. ✅ (`undo_leaves_an_untracked_file_in_its_way_alone`,
    `undo_refuses_over_tracked_changes_and_works_once_they_are_gone`)
58. **Under `GIT_OPTIONAL_LOCKS=0` — which core sets — `status` never refreshes
    the index**, so a file whose stat data went stale (rewritten with the same
    content, a new inode) makes `reset --keep` refuse with `Entry 'x' not
    uptodate`. `git update-index -q --refresh` first cures it. It bites **only
    where the two commits differ in that file**: a squash and a reorder keep the
    tree, so only a cherry-pick's undo can meet it, and a test built on a squash
    passes with the refresh removed. ✅
    (`undo_works_over_an_index_whose_stat_data_is_stale`, seen to fail without
    the refresh)
59. **A bare `update-ref refs/heads/<b>` moves a branch that is checked out** —
    here or in another worktree — and leaves that worktree's index describing
    the old commit. **`for-each-ref --format='%(worktreepath)'` and `worktree
    list --porcelain` both miss a worktree that is detached in the middle of a
    rebase or a bisect of the branch**; `git branch -f` does not: exit 128,
    `used by worktree at …`. ✅
    (`undo_refuses_a_branch_checked_out_in_another_worktree`,
    `undo_refuses_a_branch_another_worktree_is_rebasing`)
60. `rev-parse --verify --quiet <sha>^{commit}` answers "is this still a
    commit here" with exit 1 and no output; `cat-file -e` prints `fatal: Not a
    valid object name` and exits 128. ✅
    (`undo_refuses_what_is_not_an_id_and_expires_on_what_is_gone`)
61. **Every way back overwrites an *ignored* file that sits where the old
    commit has a tracked one** — `reset --keep` included; git treats ignored
    files as expendable. Not defended (WS-H item 7). ✅ (by hand)
62. Undoing a rewrite of pushed commits reads 0 ahead / 0 behind at once; undone
    *after* the force push it reads diverged and the ladder proposes Force Push
    again. ✅ (the two tests §4.6 names)
63. **`reset --keep` writes the index and the files first and the ref last.** A
    stale `refs/heads/<b>.lock`, or a `reference-transaction` hook that aborts,
    makes it exit 1 with the branch unmoved and the old commit's files in
    place, staged. A refusal made up front leaves the index equal to `HEAD`
    (`diff-index --quiet --cached HEAD` exits 0), which tells the two apart.
    `read-tree -m -u <before> <after>` puts index and files back exactly and
    touches no untracked file; **`reset --keep <after>` does not** — `HEAD` is
    on `after` already, so the files stay. Any future command built on `reset`
    or `checkout` inherits this order of work. ✅
    (`undo_puts_the_files_back_when_the_branch_cannot_be_moved`, seen to fail
    without the recovery)

Added in WS-H (2026-09-21, same conditions). Items 64 and 67 – 69 are behind core
tests, and so is 66 by the fixture's existence; 65 was measured by WS-H's
research agent, and its fetch and its pull-under-a-lock re-run by hand (the lost
commit is a race and was not):

64. **Where an operation began, by what git keeps while it is open**: a rebase's
    `rebase-merge/orig-head` is the pre-rebase tip through every round; a pick of
    several shas keeps the pre-pick tip in `sequencer/head` through every round;
    a pick of **one** sha has `CHERRY_PICK_HEAD` and nothing else — and has
    committed nothing, so `HEAD` is still the pre-pick tip (`ORIG_HEAD` is not:
    a pick never writes it, and a stale one names another commit). All of it is
    gone the moment the operation finishes, so it is read before `--continue`.
    ✅ (`continue_reports_where_a_rebase_began`,
    `continue_reports_where_a_sequence_of_picks_began_in_every_round`,
    `continue_reports_head_as_where_a_single_pick_began`)
65. **A pull and a write do collide; a fetch and a write do not.** `pull` under a
    held `index.lock` fetches and then fails its merge; a commit that is slow
    under a pull loses `HEAD`'s lock and is lost without a word; `git fetch`
    beside an `index.lock`, and beside a stopped rebase, succeeds — it moves
    remote-tracking refs only. ✅
66. `git submodule add` from a local path is refused in a test (`transport 'file'
    not allowed`, git's default since 2.38.1). **A submodule fixture is an
    embedded repository committed as a gitlink plus a hand-written
    `.gitmodules`** (`fixtures::repo_with_a_submodule`); for
    `submodule.recurse` to act, `submodule.<name>.url` and `.active` must be
    registered in the superproject's config as well. ✅
67. **A reset or checkout moves a submodule's gitlink and not its files**: the
    status reads ` M sub`, `diff-index --raw HEAD` shows `160000 160000` with two
    different ids, and `tracked_changes` — so every preflight, and the next undo
    — refuses over it. `--ignore-submodules=dirty` keeps that row and drops a
    submodule that is only dirty inside. ✅
    (`undo_of_a_submodule_bump_names_the_submodule_git_left_behind`,
    `a_submodule_left_behind_is_named_although_it_is_dirty_inside`)
68. **"The two commits differ in a gitlink" is not "a submodule was left
    behind"**: an undo that removes the submodule leaves `?? sub/`; one that
    restores it leaves a clean empty directory; a submodule never checked out
    reads clean; under `submodule.recurse=true` the reset updates it; and a
    picked bump undone from the target reads clean once the source branch is
    back. Hence a probe of the working tree after the undo, not a diff of the
    point. ✅ (`undo_that_takes_a_submodule_away_says_nothing_about_it`,
    `undo_says_nothing_when_git_updates_the_submodule_itself`,
    `undo_of_a_picked_bump_says_nothing_once_the_source_branch_is_back`)
69. `diff-index --raw -z` gives `:<modes and ids> <status>\0<path>\0`, the path
    raw — a space and a non-ASCII letter survive. ✅
    (`a_submodule_is_named_as_it_is_whatever_its_path`)

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
    built and generic: `selectResultingCommits` in `MainLayout.svelte`
    (`selectKeys` + `selectCommits`) and the `.landed` case of
    `ContentView.finishHistoryAction`. Set it *after* the log re-read has
    landed: `maintainsSelection` prunes ids the list does not hold, and the
    Tauri side needs the active `CommitInfo`. Both drop shas beyond the loaded
    window, and both then scroll the selection into view — natively the list's
    `.onChange(of: selectedSha)`, and in `CommitList.svelte` an effect on
    `activeSha` alone (`revealVirtualRow`, `utils/virtualList.ts`), declared
    after the go-to-top effect so that it wins when a re-read and a selection
    land in one flush. **WS-C documented that scroll for the Tauri client and
    had not built it; WS-E's verifier found it**, because a squashed commit is
    rarely the tip.
  - **The Tauri re-seat runs only while History is the visible tab.** Every
    action starts from a History menu, so that holds for them — but anything
    that reads `historySelection` from elsewhere has to prune it first.
  - **The pane's commit is always one of the selected ones, in both clients**,
    and a list with rows never has an empty selection. Natively that second
    half is a write-back in `maintainsSelection` (AppKit allows ⌘-click and a
    click below the rows to empty a `Set` selection), done from `onChange`
    rather than by refusing the write in the binding's setter, so that the
    table redraws. RO-1's "selection frozen" is `.selectionDisabled` while
    the mode is armed (§5.3, RO-3).
  - **Known AppKit hazard, not yet observed here:** `contextMenu(
    forSelectionType:)` has been reported to hand back a stale set after a
    *programmatic* selection change (Apple forum thread 710492). Every action
    sets the selection programmatically, so `ListSelection.targets` — which
    drops ids the list no longer holds — stays between the closure and any
    action.
  - **The Tauri file list changed with it**, by decision during WS-A: sharing
    the gestures gave it ⌘/Ctrl-click, shift-arrow and ⌘A, which the native
    file list always had from AppKit, and a shift-click straight after
    arriving now extends from the file that opened itself.
  - **The frontend's test runner is Node's own** (WS-F): `pnpm test` is
    `node --test 'tests/*.test.ts'`, Node stripping the types, no dependency
    added. It holds RO-1's placement tests; `utils/listSelection.ts` and MS-6's
    `historyRange.ts` are pure, import only types, and can join them as they
    are. A helper that imports a value through `$lib` cannot be loaded this way.
- **MS-5 — The menus. Built (WS-C, WS-E, WS-F).** Multi-row: the three actions
  and nothing else — **Cherry-pick N Commits…**, **Squash N Commits…**,
  **Reorder N Commits…**. Single row has **Cherry-pick Commit…** and **Reorder
  Commit…** after *Check Out Commit…*, since N = 1 is the same machinery and the
  reference has both. Title Case and a real `…` in both clients. No
  single-commit squash. **No item ships before its action** (the dead-surface
  rule).
- **MS-6 — Menu-time gates, disable don't hide. Built (WS-C, WS-E, WS-F).** Every
  History action is disabled while `status.operation` is set, HEAD is detached,
  or a repository write is in flight (`historyActionsBlocked`, which is also the
  Tauri item's hover reason; `canStartHistoryAction`). Squash and reorder are
  also disabled when a merge commit sits anywhere from HEAD
  down to the oldest selected commit: `replaysMergeCommit`, one pure helper per
  client (`utils/historyRange.ts`, `Services/HistoryRange.swift`) over the
  loaded rows' `parents`, which works because the list is append-only from HEAD
  (`stores/repo.ts`). **The oldest selected commit being a merge itself blocks
  too**, matching core's `rev-list --merges HEAD --not <oldest>^@`. For reorder
  the stretch runs down to the *destination* when that is older than the
  selection, which the placement model covers from the other side: no slot
  exists under the newest merge commit (§5.3). The core preflight stays the
  authority.

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
    action in WS-C … WS-G moves HEAD, the undo included, and none of them has
    to clear amend mode itself.
  - **One write gate per window (built, WS-C)** — §3 has the API. It covers
    commit, continue, every branch action, every History action and its undo,
    checkout-commit, undo-commit, discard — **and Pull, which holds it as well
    as the network slot (Decided and built, WS-H)**: write slot first, network
    slot second, released in reverse (`handlePull` in `Header.svelte`;
    `SyncStore.pull`, built with the gate). Push, Force Push, Fetch, Publish and
    the automatic fetches stay outside it, and the automatic fetches were
    **deliberately not given a "no write in flight" rule**: a fetch beside a
    write was measured safe (§4.8 item 65), so the rule would be state with no
    evidence behind it. What is still open is the sync button staying live
    during a merge, cherry-pick or revert (`ROADMAP.md`, *The sync button stays
    live while…*) — a ladder question, not a slot one.
    - **A control that cannot start and must say why is never `disabled`** — no
      pointer events, no tooltip. Tauri's sync face is `aria-disabled` with a
      guard in the handler. **The native split button is a `Menu(primaryAction:)`,
      which disables only whole**, menu included, so it stays enabled, is
      dimmed to the app's 0.55, `pull()` refuses, and `.help` swaps to the reason
      (`FRONTEND.md` §8). The first build left it looking ready — "never a
      button left enabled that does nothing" is the slot's own contract, and the
      clients verifier held the code to it.
    - **"Blocked" must not include one's own hold.** The pull sets the write
      slot itself, so the first build dimmed the button for the whole of its own
      pull: `isHeldByAnother($activeRepoWrite, 'pull')`, and natively
      `writeGate.isHeld && syncStore.activeOperation == nil`.
    - Tauri's slot has **no ownership token** (`endRepoWrite()` frees whoever
      holds it), where the native `Claim` is generation-stamped. Harmless while
      every holder frees in its own `finally`; a release from anywhere else
      would need the token first.
  - **The status poll does not pause for a write**, so both clients now drop a
    status read that lands after a later-asked one (`FRONTEND.md` §6.1).
    Anything a new action keys on the status — as `openAction` is —
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
  terminal (§10). Natively the conflict modal is a second `.sheet` on
  `ContentView`, presented while the cherry-pick sheet is still dismissing;
  the owner has seen it and it looks fine (WS-H item 6). If it ever fails to
  appear, present it from the first sheet's `onDisappear`.

### 5.3 The three actions

- **CP-1, CP-2 — Built (WS-C).** The target picker is the branch dropdown's
  picking mode, armed from outside (Tauri), and `CherryPickSheet` (native);
  local branches, current one excluded. On success the user is on the target
  with the picked commits selected and in view; on a non-conflict failure they
  are back where they began; Abort of a conflicted pick returns to the source
  branch. The contract is `FRONTEND.md` §6.21.
- **SQ-1, SQ-2 — Built (WS-E).** `SquashDialog.svelte` and
  `Screens/SquashSheet.swift`: the composer's two fields as *components*, seeded
  from core's `squash_draft` (one implementation, so both clients open with the
  same words; the reference's shape — the oldest summary, then every message
  oldest first). The contract is `FRONTEND.md` §6.21 and the metrics are
  `STYLE.md`, *Modals*. What a later action inherits:
  - **Neither client has a co-author field** — co-authors are invisible state
    in the composer too — so the dialog *names* the draft's co-authors under the
    fields and passes them through `format_commit_message` untouched. A
    co-author editor would be new surface for the composer first.
  - **The pushed-commits warning is a caption in the dialog**, fed by
    `RewritePreflight.rewrites_pushed` and naming `RepoStatus.upstream`; WS-D
    keeps its promise with no further code. RO-2 has no dialog to put it in,
    hence its own confirmation.
  - **A failure stays in the dialog with the message intact**; a conflict closes
    it (the message is already inside the rebase). `⌘/Ctrl+↩` submits — plain
    Return belongs to the description, and natively a focused `TextEditor`
    swallows it before a `.defaultAction` button could see it.
  - **Every action ends on one path in each client**:
    `ContentView.finishHistoryAction` over `HistoryActionOutcome` natively,
    `finishHistoryAction({ repoPath, result, reload, stoppedOnConflict, action,
    leavesOpen, asked })` in `MainLayout.svelte`. The undo offer is made there
    and in `finishContinue` (§5.4) and nowhere else, so a new action gets its
    Undo — the one after a conflict included — by adding its case to
    `UndoableAction` / `UndoOffer.Action`, naming the operation a stop leaves
    open, and passing the commits it was asked for.
    `DescriptionEditor` and `RefusalText` (`Design/`) are shared views.
- **RO-1 — Insertion mode. Built (WS-F)**, decided as "like the reference, plus
  a sober hint". The contract is `FRONTEND.md` §6.21 (*Reorder*), the metrics
  `STYLE.md` (*Commit list*), the mechanics `TECHNICAL.md`. What differs from
  the reference, and what a drag would inherit:
  - **The placement model is pure and mirrored** — `utils/reorderPlacement.ts`
    (tested: `pnpm test`) and `Services/ReorderPlacement.swift`. A *slot* `i` is
    the gap above row `i`; its destination for core is the sha of the row above
    it, `null` for slot 0. A drop target is a slot.
  - **↑/↓ skip the slots that would change nothing, except home** (above the
    topmost moved row, where the line starts), and ⏎ at home just ends the
    mode. The reference steps one row at a time and ignores ⏎ on a no-op; the
    two research agents disagreed on which to build, and this was chosen so
    that every step is a different move. **Decided (WS-H item 1): keep.** The
    two clients must match: it is one helper.
  - **The slot under the last loaded row is real** even with more history to
    page in (core takes any commit of the branch), and paging only adds slots,
    so the mode survives a page load. It ends on a list reset, a moved commit
    gone, a slot no longer allowed, or another action starting.
  - **No slot under the newest merge commit** (MS-6); with a merge at or above a
    moved commit the item is off, and with nowhere to go it says so.
- **RO-2 — A reorder that rewrites pushed commits asks first. Built (WS-F)**:
  `reorder_preflight` on ⏎, then a `ConfirmDialog` / `ReorderSheet` only when
  `rewrites_pushed`. A failure after the confirmation is a modal in Tauri and
  stays inside the sheet natively (`FRONTEND.md` §8).
- **RO-3 — Native key handling: the spike passed (WS-F), measured on the
  owner's OS (Darwin 27).** `.onKeyPress(.upArrow / .downArrow / .return / .escape)` on a `List`
  runs before `NSTableView` and `.handled` keeps the selection still;
  `.selectionDisabled(true)` freezes clicks and arrows without dimming the
  highlight. **Apple documents no precedence between `onKeyPress` and `List` —
  re-run the spike on an OS bump.** What did *not* work: `.onMoveCommand` never
  fires on a `List`; a clear overlay with a tap gesture kills scroll-wheel
  scrolling (hence `Design/MouseDownMonitor.swift`, an `NSEvent` local monitor
  that only observes); a row overlay is clipped 4pt outside the row's content
  box, which is the whole of the insertion line's room.

### 5.4 Undo (UN)

**Built (WS-G, WS-H).** The contract is `FRONTEND.md` §3.7 and §6.21 (*Undo*), the
mechanics `TECHNICAL.md` (*Undo*), the strip's metrics `STYLE.md`. What a later
action inherits, and where the build left the draft:

- **UN-1 — `undo_operation(repo, &UndoPoint)` → `UndoResult { undone,
  message }`, three outcomes the clients tell apart.** `undone` — the branch is
  back on `before_sha`, and `message` is what did not follow it, as paragraphs:
  a cherry-pick's source branch that could not be checked out again, then the
  submodules left behind (below). `!undone` — **expired, for good**: the
  branch is gone, its tip is not `after_sha`, or `before_sha` is no longer a
  commit here; the client drops the offer. `Err` — a refusal that may not hold
  next time (an operation open, tracked changes, an untracked file in the way,
  the branch held by another worktree) or a git failure; **the offer stays**.
  Nothing has moved unless `undone`. The check is on the *branch*, not on HEAD:
  after a cherry-pick the user may be back on the source branch, or detached,
  and the undo is still valid there. Stricter than the reference, whose undo is
  a bare `reset --hard` with no tip check and console-only failures.
  - **Checked out here**: tracked changes refuse, then `update-index -q
    --refresh`, then `reset --keep <before>` (§4.8 items 57, 58), then
    `switch_to(return_branch)`.
  - **Not checked out here**: `git branch -f -- <branch> <before>`, *not*
    `update-ref` (§4.8 item 59). The price is `update-ref`'s compare-and-swap:
    between the tip check and the move there is a window a terminal could land
    a commit in. Taken deliberately — the other side of the trade is a corrupted
    worktree.
  - **A reset that fails half way is put back** (`after_a_failed_reset`, §4.8
    item 63), so "nothing has moved" holds for an `Err` too.
  - **An undo names the submodules it left behind and updates none (Decided and
    built, WS-H item 8).** `submodules_left_behind` is a probe of the working
    tree **after the reset and after the switch back** — `diff-index --raw -z
    --no-renames --ignore-submodules=dirty HEAD`, keeping the paths whose two
    modes are both `160000` — and `submodules_not_updated` words it. The plan's
    own suggestion, `diff --raw <after> <before>`, would have lied four ways
    (§4.8 item 68); ask what is true *where the command came to rest*, not what
    the two commits say. `git submodule update` is not run: it may reach the
    network, and when the app updates submodules is one decision for every
    checkout, pull and switch (`ROADMAP.md`, *Full submodule support*). The
    `branch -f` path touches no tree and gives no note.
  - **Known and not defended (Decided, WS-H item 7: leave it)**: ignored files
    in the way (§4.8 item 61) — git's own stance, and what `.gitignore` means.
  - The `--keep` over `--merge` choice is **not pinned by a test** — the
    tracked-changes refusal above it masks the difference (a verifier's
    surviving mutant). Whoever loosens that refusal must pin it first.
- **UN-2 — The Undo is its own line of the strip, not the notice's.** The
  notice is a warning that a later notice replaces; an offer that a "skipped a
  commit" note could wipe out would be a lottery. "Squashed 3 commits into
  one." · **Undo** · ✕, in the `done` tone. Its lifetime, one rule per client
  (`undoStillStands` / `UndoOffer.stillStands(under:)`):
  - **Made** by `finishHistoryAction`, after the reload, only when the result
    carries an `undo` — or by `finishContinue` (UN-3) — a no-op reorder leaves
    the previous offer standing, and a new landing replaces it.
  - **Retired** by the ✕, a repository switch, the undo itself (undone or
    expired), and **a status that shows `branch` checked out, not detached, at
    a HEAD other than `after_sha`**. On another branch or detached the status
    cannot see `branch`'s tip, so the offer stays and UN-1 is the judge.
  - **Kept** on `Err` and on a busy write slot — both go to the blocking modal,
    and the user can try again.
  - **Inert**, with the reason on hover, while an operation is open or the write
    slot is held; a detached HEAD does not block it.
  - After an undo both clients reload status, log and branches, then select
    `restores` — the commits the action was asked for — whichever tab is up.
  - The point lives in client memory; after a restart the reflog is the way
    back, and the expired message says so. It never times out (the reference's
    banner leaves after 15 s) and asks for no confirmation — **Decided (WS-H
    item 4): keep as built.**
- **UN-3 — Undo after a conflict and a Continue. Decided and built (WS-H item
  3), by client memory.** The contract is `FRONTEND.md` §6.21 (*Undo after a
  conflict*); core's half is §4.3. One `OpenAction` per window — the operation
  the stop left open, core's `UndoStart`, which action, the commits asked for —
  set by `finishHistoryAction` after the reload, and two pure, mirrored rules
  beside `undoStillStands`:
  - **`openActionStillStands` forgets it** on a status whose `operation` is not
    the one left open; **only a cherry-pick is also held to its branch**, because
    a stopped rebase reports no branch at all (§4.1). It is also the one memory
    of a stopped action: the abort's return branch is `start.return_branch`.
  - **`undoPointAfterContinue` vouches for the point or offers nothing**:
    detached, another branch, no HEAD, a `started_at` that is not `before_sha`
    (null included), or `head_sha == before_sha` all mean no offer — logged
    (`[undo] no offer after the continue`), since from outside it looks like
    nothing happened. The first build let a single-sha pick through on a null
    `started_at`, git keeping no record for one; core's verifier showed the
    hole (abort in a terminal, commit, pick again, Continue here → an Undo that
    takes the hand-made commit too), and core now answers `HEAD` there (§4.3),
    so one rule covers every action.
  - **Take what you need off the open action before asking core, not before the
    reload.** The status poll does not wait for a write, so a read showing the
    operation over can land between git finishing and the Continue's answer
    arriving, and forget `openAction` first — rarely, and without a trace. The
    composer therefore calls
    `onOperationContinuing` before core (`beginContinue`;
    `HistoryActionStore.continueBegins()`), and `finishContinue` takes what was
    remembered on every path, reloads status, log and branches (natively the
    branch list on this path is new), and only then asks the rule. The abort
    already read first.
  - **Both clients set it after the reload** — natively the outcome carries it
    (`.stoppedOnConflict(_, open:)` → `keepOpen`). Setting it inside the store as
    the action returned happened to be safe (`RepoStore` drops out-of-order
    reads, and `.onChange` does not fire on nil → nil), which is not the same
    as the rule the contract states.
  - The sentence counts **the commits asked for**, also when a round dropped one
    (§4.8 item 54), and `restores` is those commits: the undo brings them all
    back.
  - **Not offered, knowingly**: a Continue run in a terminal (the client never
    sees `started_at`; the status rule forgets the open action); an app restarted
    mid-conflict. **Known and not defended**: a commit made by hand on the target
    while a pick is stopped sits between `before_sha` and the new tip, so the
    undo takes it too — it stays in the reflog (`ROADMAP.md`, *An undo after a
    hand-made commit…*).

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
4. **WS-D — Force push recommended. Built 2026-09-20, confirmed and committed
   (`d979bf2`).** FP-1, FP-2, the ladder's own module, and one presentation
   table per client. Before squash on purpose: an amended pushed commit already
   produces this state, and squash lands into a sync button that knows what to
   say.
5. **WS-E — Squash. Built 2026-09-20, confirmed and committed (`bc8f3db`).**
   The rewrite driver, squash's todo, SQ, MS-6's merge gate, and
   `history_rewrite` split into a module per action.
6. **WS-F — Reorder. Built 2026-09-20, confirmed and committed (`f5976a6`).**
   RO, `reorder_preflight`, `Lineage` in its own module with a landmark, the
   driver's optional message, a rebase's dropped commit reported as `skipped`,
   one ending for every action in `MainLayout.svelte`, and the frontend's test
   runner.
7. **WS-G — Undo. Built 2026-09-20, confirmed and committed (`19d066b`).** UN
   for all three actions at once, `undo.rs`, `UndoResult`, one `StatusStrip`
   component per client in place of the inline banner markup and the native
   `ErrorBanner`, the mirrored `UndoOffer` model, `switch_to` and
   `open_operation_refusal` shared in `mod.rs`, and `finishHistoryAction` taking
   one object.
8. **WS-H — The open decisions. Done, `497f4a3`.** §6.1. Three of the eight answers were code:
   Pull holds the write slot (clients only), Undo after a conflict and a
   Continue (`UndoStart`, `RewriteResult.start`, `OperationOutcome.started_at`,
   the mirrored `OpenAction` model in place of `cherryPickReturn`), and the
   undo's submodule note (`undo.rs`, `fixtures::repo_with_a_submodule`).

### 6.1 WS-H — what the owner decided

Eight things the plan had chosen on its own or left open, put to the owner with
options and a recommendation each, and answered on 2026-09-21. All **Decided**;
where the answer was code, the section named holds what was built.

1. **Reorder's ↑/↓ skip the slots that change nothing — keep** (§5.3, RO-1). The
   reference steps one row at a time; the change, if the jump ever reads wrong
   in use, is one helper per client and its tests.
2. **Only Pull claims the write slot as well as its own — built** (§5.2). Not
   one slot for everything, which would leave the composer inert for the whole
   of a slow push.
3. **Undo after a conflict and a Continue, by client memory — built** (§4.3,
   §5.4 UN-3). Not from git's state at continue time, which a single-sha pick
   does not have and which would also offer an Undo for a rebase begun in a
   terminal; that state is used as a guard only.
4. **Where Undo lives, how long, whether it asks — keep as built** (§5.4 UN-2):
   its own line of the strip, no timeout, no confirmation, no ⌘Z (which belongs
   to the text fields).
5. **What ends reorder's mode, native mouse-down and Tauri `click` — leave it**,
   a recorded difference (`FRONTEND.md` §8).
6. **The native conflict modal as a second sheet — seen by the owner, fine**
   (§5.2, OP-8).
7. **Ignored files in the way of an undo — leave it** (§4.8 item 61). It takes a
   picked commit that deleted a file and an ignored file made at that path
   since; a refusal would be `ls-files -i -o --exclude-standard` against
   `diff --name-only <after> <before>`.
8. **A submodule pointer moved by an undo — say so, run nothing — built** (§5.4
   UN-1). Submodule handling for the whole app is its own roadmap item.

## 7. Verification gates

Per workstream: zero-warning `just mac-build`; `pnpm check`, `pnpm lint` and
`pnpm test`; `cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -W
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
- **Core's tests are isolated from the developer's git configuration** (WS-E):
  `test_support::isolate_from_user_config` — `GIT_CONFIG_GLOBAL` and
  `GIT_CONFIG_SYSTEM` at `/dev/null`, `LC_ALL=C` — is applied by the
  `test_support` helpers and, under `#[cfg(test)]`, by `git_cmd` itself. A test
  that arranges its repository with a raw `Command` is still exposed, and so are
  **the bridge crate's tests**, which run core as a dependency (no `cfg(test)`
  there): `seeded_repo` sets `user.*` locally and nothing else.
- **`cargo test -p leogit-core <filter>` takes one filter**, a substring of the
  test path (`history_rewrite`), not a list.
- **A background build's exit status is the last command's**: a trailing
  `grep -c` that counts zero warnings exits 1. Read the log, not the status.
- **Run `pnpm tauri build` in the foreground.** Detached, its `bundle_dmg.sh`
  step fails (it scripts Finder) after the app itself has compiled and
  bundled, and leaves a scratch image mounted at `/Volumes/dmg.*` —
  `hdiutil detach` it before the retry. Nothing to do with the code. From an
  agent's shell it also fails in the foreground more often than not (WS-F:
  three runs of five), leaving `target/release/bundle/macos/rw.*.dmg` behind and
  nothing mounted. **`pnpm tauri build --bundles app` is the gate on the
  code**; the DMG is the owner's to build from a terminal of their own.
- **`pnpm lint` does check `commands.ts`**, and prettier here takes no trailing
  comma after the last parameter of a multi-line arrow function.
- **A test that needs a remote** starts from `test_support::published_repo()`
  (bare `origin.git` plus `mine`, whose `main` is pushed and tracking) and
  `clone_of()` for a second contributor. **`mine` is `init` + `push -u`, not a
  clone, and the two differ** — its remote-tracking ref has a reflog, a
  clone's has none, which is what hid §4.8 item 42. Anything that reads a
  reflog or pushes gets a `clone_of()` test as well — and anything that reads
  *parents* gets a `clone --depth` one (§4.8 item 51).
- **A verification agent's finding is reproduced by hand before it is fixed,
  and a research agent's git claim before it is built on.** WS-D had one of
  each wrong way round: two research agents misdescribed `pull --ff`, and the
  plan's own `--force-if-includes` was only caught by a verifier. WS-E's
  research agent was asked to *run* every failure path rather than describe
  it, which is what surfaced the skipped `exec` (item 47); its verifier found
  the shallow boundary (item 51) by attacking where `--root` comes from —
  thirteen passing tests had not.
- **Two research agents can disagree about the reference, and then the code
  decides nothing** (WS-F: whether ↑/↓ skip no-op slots). Read the reference
  yourself, pick one, write it in one mirrored helper, and mark it *Proposed*.
- **A native behaviour question is answered by a scratch Swift package, not by
  documentation** (WS-F's RO-3 spike). Synthetic *keys* posted with
  `CGEvent.postToPid` reach a SwiftUI `List`; synthetic *mouse* events do not,
  so clicks were checked by hand. Apple's docs are fetchable as JSON at
  `https://developer.apple.com/tutorials/data/documentation/swiftui/<path>.json`
  when the HTML page is an empty shell.
- **WS-F's core verifier could not break `Move::plan`** — it wrote its own model
  of the list semantics and ran all 627 selection × destination shapes of a
  five- and a six-commit branch against the real function — and broke the
  heuristic beside it instead (§4.5, `skipped`). A flag derived from "what this
  call happened to do" is a guess; ask git's state files. It also found four
  tests weaker than their names (a commit that *was* empty standing in for one
  that *became* empty; an `if` around a conflict round). Name a test after what
  it sets up, and pin the number of rounds.
- **Svelte 5: never read a `$derived` of X after writing X in the same
  function.** WS-F's `confirmReorder` cleared the mode and then asked a set
  derived from it whether the move was a no-op; the set was already empty, the
  guard was dead, and ⏎ at home ran a reorder. `svelte-check` and the helper's
  tests were green — a verifier caught it by compiling the shape. Take what you
  need off the state first.
- **Native: a watcher attached to a view that can unmount stops watching.**
  `HistorySidebar`'s list leaves the hierarchy whenever `commits` empties (a
  repository switch publishes `[]` first), while `@State` on the sidebar
  survives; the reorder mode's `.onChange`s and click monitor therefore sit on
  `body`'s `Group`. `.onChange` without `initial: true` does not fire on
  remount.
- **A generated UniFFI function is a global**, so a `GitBridge` method of the
  same name shadows it inside the type and recurses (`preflightReorder`, not
  `reorderPreflight`).
- **Remove the fix and watch the test fail before trusting it** (WS-G). The
  first stale-index test was built on a squash and passed with `update-index
  --refresh` deleted: a squash keeps the tree, and `reset --keep` only looks at
  entries that differ (§4.8 item 58). Rebuilt on a cherry-pick, it fails without
  the line.
- **The plan's own draft can be the thing the research overturns** (WS-G): UN-1
  said `reset --hard` and an atomic `update-ref`, and both were wrong for this
  app (§4.8 items 57, 59). The research agent was asked to *run* each candidate
  against untracked files, staged changes, hooks and worktrees; every row of its
  table was then re-run by hand before the code changed.
- **WS-G's core verifier broke the contract, not the happy path**: it attacked
  "nothing has moved on `Err`" by making the *last* step of a git command fail
  (a stale ref lock) and found the tree moved under an unmoved branch (§4.8 item
  63). **Its proposed recovery was wrong** — `reset --keep <after>` leaves the
  files deleted — which only the hand reproduction showed. Ask of every git
  command an action runs: in what order does it write, and what is left if the
  last write fails? It also ran 13 mutants against the suite; the two survivors
  are recorded in §5.4.
- **A verifier's suspected race can be unreachable**, and reproducing cuts both
  ways (WS-G's clients verifier: a native offer landing on another repository).
  The reset that clears the offer and the guard that admits it read the same
  `store.repoPath`, so there is no window. What it did find was the app's own
  earlier lesson forgotten: a `.disabled` SwiftUI control shows no tooltip
  (`RepoPickerList` says so in a comment) — grep for how the app already solved
  a thing before solving it again.
- **The Tauri store's status is camelCase (`headSha`), core's DTOs are
  snake_case (`after_sha`)**, and `svelte-check` is what says so. A pure helper
  that compares the two takes the three fields it needs, not `RepoStatus`.
- **A Swift enum case and a static function may not share a name**
  (`.landed` the outcome, `landing(_:after:of:)` the helper that builds it).
- **A research agent's *client* claim is checked against core, as its git claims
  are against git** (WS-H). Its keep-rule for the open action — "same operation
  and same branch" — read well and was wrong for two actions of three: core's
  status has no branch during a rebase (§4.1). Five minutes in `get_status`
  found it; both type checkers and the helper's own tests would have passed.
- **A test of a rule about "what git left behind" needs the cases where git left
  nothing, and each flag needs a test that fails without it** (WS-H's submodule
  note). The four silent cases of §4.8 item 68 were tests from the start; core's
  verifier still removed `--ignore-submodules=dirty`, the second mode check and
  `-z` one at a time and all 30 tests passed. The fixture that "covered" a dirty
  submodule moved its pointer *as well*; the odd path held nothing git quotes
  under `core.quotepath=false`, which `git_cmd` always sets (a `"` does); and no
  undo can produce a one-sided gitlink at all, so the parse is now a function of
  its own (`gitlinks_on_both_sides`) tested on hand-made listings. The same
  habit on the client side: every negative rule of `undoPointAfterContinue` was
  pinned on the squash fixture alone, and two mutants that spared cherry-pick
  survived (a third, dropping the `detached` test, is equivalent: core empties
  `branch` whenever HEAD is detached).
- **A verifier's repro can be its own artefact** (WS-H, core). "An undo that
  restores a submodule whose directory was deleted leaves ` D sub` unsaid" was
  built with `git rm --cached sub; rm .gitmodules; git add -A` — and `add -A`
  put `sub/` straight back as a gitlink, so the "drop" commit never dropped it
  and ` D sub` was there *before* the reset, where `tracked_changes` refuses the
  undo. Re-run by hand with `git ls-tree` on each commit, `reset --keep`
  recreates the empty directory and reads clean. Check what a repro's commits
  actually contain before believing what follows them.
- **An agent that mutates code needs a target directory of its own.** Core's
  verifier ran mutants on a copy of the crate with `CARGO_TARGET_DIR` on the
  repository's `target/`; cargo gave the copy the real crate's unit hash, the
  next real `cargo test` failed eleven tests, and the cure was `cargo clean -p
  leogit-core`. Tell a mutation agent to build somewhere else.
- **A conflict fixture may conflict more often than its name says**:
  `conflicting_repo()` stops a two-commit pick onto `side` **twice** (first on
  `base`, then on `main edits shared`). A round-trip test that continues once
  and asserts success fails there; pin the number of rounds.
- **`#[tokio::test]` is available in core** for the async commands; an
  `EventSink` that drops everything is three lines (`NoProgress` in `git.rs`'s
  tests).

Core tests live with their module and build repositories with
`core/src/test_support.rs` and `history_rewrite/fixtures.rs` (§3 lists both);
`TECHNICAL.md` names each module's tests where it describes the module
(`operation.rs`, `git_version.rs`, `sync_ladder.rs`, the `force_push_*` in
`git.rs`, and `history_rewrite/` per action). Bridge tests copy
`cherry_pick_flow_preflights_copies_and_stops_on_a_conflict` (`ffi/src/lib.rs`),
one flow per action. The frontend's pure models are in `apps/tauri-app/tests/`
(`reorderPlacement`, `undoOffer`). Two probe claims in `operation.rs` rest on a
scratch run rather than a test and are cheap to add: the `--rebase-merges` stop
that leaves `MERGE_HEAD` beside `rebase-merge/`, and a real `git am` (the
existing test fabricates the directory).

## 8. Documentation on completion

Every workstream through WS-H has written its share, and nothing is owed. Where
the feature lives in the documents, for whoever changes it next:

- **`FRONTEND.md`** — §1 and §3 (the command count, 78 per host); §3.7 (*history
  actions*: a row per command, the three-outcome contract, `UndoResult`'s
  three); §5 (the DTOs); §6.1 (status reads in order); §6.2 (the Force Push
  rung); §6.13 (the strip's three lines and the blocking modal); **§6.21 — the
  write slot and every History action, the undo included: a new action extends
  §6.21 rather than adding a rule of its own**; §8 (where the two clients
  differ).
- **`DESIGN.md`** — flow 5 (a bullet per action, and the way back), flows 6 and
  7, the menus and the shortcut table.
- **`STYLE.md`** — context menus, *Modals*, *Branch picker*, *Commit list* (the
  insertion line), and the strip under the header (tones, the action link).
- **`TECHNICAL.md`** — the layout tree, *History's multi-commit actions* (the
  driver, `Lineage`, each action, *Undo*, the clients' one ending and the undo
  offer), *Sync proposal*, both write gates, test fixtures, `pnpm test`.
- **`README.md`** — *Browse history*.
- **`ROADMAP.md`** — one checked entry per workstream at the top of *Recently
  completed*; never patch an older one. A WS-H item that gets built closes or
  rewords its open item there.

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
§4 is two modules (`operation.rs`, `history_rewrite/`) behind nine bridge
calls today, so the backend can change without the bridges or the clients noticing.

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
