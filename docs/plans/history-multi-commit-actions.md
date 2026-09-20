# Plan — History multi-commit actions (cherry-pick, squash, reorder)

> Status: **WS-A (a selection that is a set) is built, confirmed and committed
> (`5ff2a4c`). WS-B (operations in progress) is built in both clients,
> 2026-09-20, and awaits the owner's visual check; WS-C (cherry-pick) is
> next.** The owner's decisions are marked **Decided**; the ones this plan made
> on its own are marked **Proposed** and are open to change until their
> workstream starts. No question is open; §9 records the standing decision on
> where rewriting runs. §3 describes the code as it stands *after* WS-B, and
> §5.1 and §5.2 record what the next workstreams inherit.
> Produced from a three-way read of the native client, the Tauri client, and
> the GitHub Desktop source at
> `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop` — whose history and
> multi-commit code is upstream's in every region §2 cites (the fork's changes
> to `app-store.ts` and `dispatcher.ts` sit above them, so **line numbers are
> this fork's**, not upstream's). The reference is used to judge *how*; the
> feature itself is LeoGit's own, promised by `ROADMAP.md`'s open items
> *Rebase (interactive UI)* and *Cherry-pick / revert* (roadmap items are named
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
the ids in list order with `ListSelection.targets` and builds `rowMenu` only
for a single target — **the `targets.count == 1` branch is where WS-C adds the
multi-commit menu.** The rules are `Services/ListSelection.swift`, generic
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
`contextTargets`, `activeKey`, the key and click helpers) shared with
`FileList.svelte`. `openContextMenu` returns early when `contextTargets` yields
more than one sha — **that early return is where WS-C adds the multi-commit
menu**; the targets are already in list order, newest first.

**The operation in progress** (WS-B) — `core/src/operation.rs`:
`OperationInProgress { Merge, Rebase, CherryPick, Revert }`, the filesystem
probe `in_progress(git_dir)` that fills `RepoStatus.operation` on every status
read, `continue_operation` → `OperationOutcome { success, conflicts,
error_message }` and `abort_operation` → `Option<String>` (whatever git
printed). Both are on both bridges (`ffi/src/lib.rs`, *the operation in
progress*; `src-tauri/src/shims/operation.rs`). `core/src/git_version.rs`
holds `FLOOR`, the `git --version` parser and `require_floor()` — **which
nothing calls yet; OP-2 is its first caller.** `core/src/test_support.rs`
(`git`, `git_stdout`, `init_test_repo`) is what every core test module builds
its repositories with, and `operation.rs`'s `conflicting_repo()` is the fixture
to copy for a conflict. `git.rs` lends `pub(crate)` `git_cmd`, `run_git`,
`run_git_combined`, `run_git_combined_with_env` (the extra-environment
sibling the rewrite driver needs), `git_dir`, `ls_files_unmerged` and
`git_add`.

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
Continue lives in the composer under the composer's own lock, never the
branch-operation lock (`CommitMessage.svelte` `handleContinue`;
`CommitStore.continueOperation` → `ChangesSidebar.continueOperation` →
`CommitComposer`'s `continuing:` / `onContinue:`). After a continue **both
clients re-read first and report second**, because a continue that stops again
has still moved the repository.

**Sync** — `SyncProposal` (`core/src/git.rs`) has no force-push rung: a
diverged branch proposes *Pull* and force-push-with-lease sits under the
chevron (`SyncControls.swift`, `Header.svelte`). `ROADMAP.md`'s
*Force-push-recommended detection* item already files the promoted state as "a
new `SyncProposal` variant plus each client's word for it".

**Surfaces** — one sheet slot per window on the native side, driven by
`RootSheet` (the enum at the foot of `ContentView.swift`); `ConfirmDialog.svelte` and the
one-off dialogs on the Tauri side; the branch dropdown's *picking mode*
(`STYLE.md:251`), which exists precisely so "an action that needs a branch
borrows the list instead of opening a second one"; and a text-only notice
banner (`stores/repo.ts:247-254` and its native counterpart).

## 4. Core design

The core lives beside `git.rs`, which is already 4 000+ lines, in modules
split by responsibility: `core/src/operation.rs` (built — detect, continue,
abort), `core/src/git_version.rs` (built — the floor), and a new
`core/src/history_rewrite.rs` for everything that *starts* an operation — the
preflight, cherry-pick, the rewrite driver, squash, reorder and undo. It reuses
the `pub(crate)` helpers §3 lists.

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

- **OP-2 `rewrite_preflight(repo, oldest_affected)`** returns
  `{ blocked: Option<String>, rewrites_pushed: bool }`. Blocks on: detached
  HEAD or unborn branch, an operation already in progress, tracked changes
  (staged or not — untracked files pass, and git's own refusal is data if one
  would be overwritten), a merge commit among the replayed commits, and a git
  below the floor (§4.7). The merge probe is
  `git rev-list -1 --merges HEAD --not <oldest_affected>^@` — the obvious
  `<oldest>^..HEAD` is a fatal error when the oldest commit is the root
  (verified, §4.8-11).
  **Proposed: block, no stash.** The reference stashes on request and never
  restores it, which is the failure this avoids: the core has no stash, and a
  stash left behind after a conflicted rebase is a worse place to leave
  someone than "commit or discard first". Stash-and-restore belongs with the
  stash feature (`ROADMAP.md`, *Stash management*).
  `rewrites_pushed` is `git merge-base --is-ancestor <oldest_affected>
  <upstream>`, and `false` when the branch tracks nothing: the replayed range
  is linear (no merges), so the oldest commit being on the upstream is exactly
  "at least one replayed commit is pushed". This does not fire on someone
  else's push (§2 flaw 2; verified, §4.8-8).
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

One DTO for all three actions, shaped like `MergeResult`. Continue already
returns its own `OperationOutcome` (§4.2), which has no `selection` or `undo`:
an operation continued here may have been started in a terminal, where there
is no selection to restore. **WS-C decides whether a continue of an operation
LeoGit started should hand back an `UndoPoint`** — `rebase-merge/orig-head`
and `sequencer/head` hold the pre-operation tip while the operation is open,
so continue could read it before the final step; the alternative is the
client keeping the undo point it got when the action stopped on the conflict.

```rust
pub struct RewriteResult {
    pub success: bool,
    pub conflicts: Vec<String>,        // ls_files_unmerged, as git wrote them
    pub error_message: Option<String>, // git's own text, verbatim
    pub selection: Vec<String>,        // new SHAs of the commits acted on
    pub undo: Option<UndoPoint>,       // only on success
}
pub struct UndoPoint {
    pub branch: String,
    pub before_sha: String,
    pub after_sha: String,
    pub return_branch: Option<String>, // cherry-pick: the branch we came from
}
```

`selection` lets both clients re-select what the user was working on instead
of jumping to the tip (the reference's own TODO, `dispatcher.ts:3626-3629`).
It is cheap: the todo order *is* the resulting order, so the new SHAs are read
by position from one `rev-list`.

### 4.4 Cherry-pick (CP)

`cherry_pick_commits(repo, shas, target_branch)`. Checkout and pick are one
core call so the ordering — and the checkout back when the pick fails for a
reason that is not a conflict — live in one place. The command:

```
git cherry-pick --empty=keep -m 1 <shas, oldest first>
```

`--empty=keep` keeps a pick that turns out redundant instead of stopping the
sequence on it (a pick emptied by its *conflict resolution* is a different
case, which continue skips — §4.2), and `-m 1`, passed as two arguments, lets a selection mix
ordinary and merge commits (both verified, §4.8-5). On conflict the user stays
on the target branch with
`operation = CherryPick`; abort restores the target and the client switches
back to the source branch it remembered.

**SHAs cross the bridge newest-first, as the list shows them**, for all three
actions; the core reverses for cherry-pick and re-derives order from the range
walk for squash and reorder.

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

### 4.6 Force push recommended (FP)

**FP-1 — `SyncProposal::ForcePush`**, stateless. When the branch has diverged
(ahead > 0 *and* behind > 0), the core asks whether the upstream tip is
reachable from any entry of the local branch's reflog:

```
git rev-list -1 --stdin      # stdin: <upstream>, then ^<sha> per reflog entry
```

Empty output means *the upstream is a commit this branch used to contain* —
the divergence is our own rewrite, and the ladder proposes **Force Push**
(with lease, as today). Anything else stays **Pull**. This is the same test
`git push --force-if-includes` applies, it needs no remembered state, it
survives a restart, and it is right for an amend and for a rebase done in the
terminal (§2 flaw 3; verified in both directions, §4.8-7). It costs one or two
subprocesses **only while diverged**, cached on `(head_sha, upstream_sha)`.
The reflog goes in on stdin, never argv, so its length is not a limit.
**Every doubt reads as Pull**: no reflog at all, a non-zero exit from a pruned
object, or the pre-rewrite tip having aged out of the reflog
(`gc.reflogExpireUnreachable`, 30 days by default) all leave the ladder where
it is today, with force push still under the chevron (verified, §4.8-7).
This closes `ROADMAP.md`'s *Force-push-recommended detection* item.

**FP-2** — the force push itself gains `--force-if-includes` beside
`--force-with-lease`, so a fetch that landed between the proposal and the
click cannot make the lease pass over a foreign commit.

### 4.7 The git floor

**Decided (2026-09-19): the system git, recent versions only.** LeoGit tracks
current git rather than carrying old spellings or second code paths; the floor
is simply the newest flag §4 uses, and it rises whenever a newer git has
something worth using.

| Flag | Since | Used by |
| --- | --- | --- |
| `cherry-pick --empty=keep` | **2.45** | CP |
| `rebase --no-update-refs` | 2.38 | SQ, RO |
| `push --force-if-includes` | 2.30 | FP-2 |
| `rebase --empty=keep` (merge backend, which `-i` always uses) | 2.26 | SQ, RO |
| `cherry-pick -m 1` on non-merge commits | 2.21 | CP |

So **the floor is git 2.45** (April 2024). The development machines run 2.54
(Apple Git) and Arch's current git. **Built (WS-B):**
`core/src/git_version.rs` reads `git --version` once per process and
`require_floor()` answers with one sentence naming both versions — a single
check, no degraded mode, no flag fallbacks. `README.md` Requirements states
the floor. **Nothing calls `require_floor()` yet**: nothing WS-B runs needs
more than git 2.25, so the preflight (OP-2, WS-C) is its first caller and it
is on neither bridge.

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
   and *foreign* once someone else pushes; `--force-if-includes` refuses the
   foreign case. ✅
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
    conflicts no operation owns*); relevant to WS-C only if cherry-pick ever
    grows a no-commit mode. ✅

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
  - **A multi-row selection raises no menu yet.** §3 names the one branch in
    each client where WS-C adds it. On the Tauri side the early return still
    calls `preventDefault`, so the WebView's own menu never shows.
  - **Setting the selection after a rewrite** (`RewriteResult.selection`, CP-2)
    is one call in each client. Tauri: `selectCommits(selection, active)` in
    `MainLayout.svelte`, with a `ListSelection` of the new shas — anchor on
    one of them, since the module keeps the anchor inside the key set. The
    module has no constructor for an arbitrary key set yet (`selectOnly` is
    private); WS-C adds one there rather than building the object by hand.
    Native:
    assign `historySelection` in `ContentView`; `maintainsSelection` derives
    `selectedSha`. Set it *after* the log re-read has landed. Natively the
    modifier prunes on every selection change, so shas assigned before the
    list holds them are dropped at once and the selection falls back to the
    newest commit; in the Tauri client `selectCommits` takes the active
    `CommitInfo`, which also only exists once the log has it.
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
- **MS-5 — The menus.** Multi-row: the three actions and nothing else. Single
  row gains **Reorder Commit…** and **Cherry-pick Commit…** in the
  repository-changing group (`STYLE.md:194-199`), since N = 1 is the same
  machinery and the reference has both. Title Case and a real `…` in
  both clients. No single-commit squash.
- **MS-6 — Menu-time gates, disable don't hide.** All three are disabled while
  `status.operation` is set or HEAD is detached. Squash and reorder are also
  disabled when a selected commit sits at or below the newest merge commit in
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
  - **Two locks, not one.** The composer's lock (Commit, Continue) and the
    branch-operation lock (switch, merge, Abort) are separate in both clients,
    so an Abort can be confirmed while a long Continue is still replaying.
    The History actions will be a third holder; `ROADMAP.md` files the shared
    gate (*One "a repository write is in flight" gate*), and **WS-C is the
    natural place to build it** — MS-6 disables the menu on
    `status.operation`, which says nothing about a write that is mid-flight.
    The same item covers the sync button staying live during a merge,
    cherry-pick or revert.
  - **Cross-file comments name symbols, not line numbers.** WS-B shifted two
    dozen `CommitComposer.swift:<n>` references in the Tauri sources; they now
    read `CommitComposer.swift` or `BranchMenu.menuLabel`. Keep to that.
- **OP-8 — A conflict is reported where the action was started**, git's text
  verbatim, and the dialog closes onto the Changes tab: the conflicted files
  are already there with their `U` badge. Resolving is still the user's editor
  or the embedded terminal (§10).

### 5.3 The three actions

- **CP-1 — Target picker.** Tauri borrows the branch dropdown's picking mode;
  native uses a `RootSheet` case with the same filterable list. Local
  branches, current branch excluded.
- **CP-2 — On success the user is on the target branch** with the picked
  commits selected. On a non-conflict failure they are back where they began.
- **SQ-1 — Message sheet**, built from the composer's summary, description and
  co-author fields — the *components*, not the live composer's state, which
  may hold a draft. Pre-filled in history order, oldest first: summary from
  the target, description from the target's body then each other commit's
  summary and body, co-authors as the de-duplicated union.
- **SQ-2 — The pushed-commits warning is a caption inside the sheet**, not a
  modal in front of it: "These commits are already on `origin/main`. After
  squashing, the next push is a force push."
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
   committed (`5ff2a4c`).** MS-1 … MS-4, no core change. Until WS-C lands,
   right-clicking a multi-row selection shows no menu.
2. **WS-B — Operations in progress. Built 2026-09-20**; the owner's visual
   check is pending. OP-1, OP-3, OP-4, OP-6, OP-7 and the git floor module.
   Tested with a rebase, cherry-pick, revert or merge started in the embedded
   terminal.
3. **WS-C — Cherry-pick. Next.** OP-2 (the first caller of `require_floor()`),
   OP-5, CP, MS-5's menus with the first item live, OP-8, and the shared
   write gate §5.2 describes.
4. **WS-D — Force push recommended.** FP-1, FP-2. Before squash on purpose: an
   amended pushed commit already produces this state, so it is testable today,
   and squash then lands into a sync button that already knows what to say.
5. **WS-E — Squash.** The rewrite driver, the todo builder, SQ.
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
  `rebase.backend=apply` would change results. Not fixed — the fix is
  `GIT_CONFIG_GLOBAL=/dev/null` inside `git_cmd` under `#[cfg(test)]`, which
  does not reach the bridge crate's tests.

Core tests live with their module and build repositories with
`core/src/test_support.rs`; `operation.rs`'s `conflicting_repo()`,
`commit_file` and `git_stopping` are the conflict fixtures to lift into it when
`history_rewrite.rs` needs them. Bridge tests copy
`operation_flow_names_refuses_and_continues_a_cherry_pick`
(`ffi/src/lib.rs`). WS-B's sixteen tests are in `operation.rs` and four more
in `git_version.rs` (`preflight_refuses_a_git_below_the_floor` parses version
strings, including Apple's `2.54.0 (Apple Git-157)` form). Still to write, at
minimum, named as sentences:
`cherry_pick_copies_commits_oldest_first_and_keeps_empty_ones`,
`cherry_pick_conflict_is_data_and_abort_restores_the_target`,
`squash_gathers_a_non_contiguous_selection_at_the_target`,
`squash_keeps_the_message_across_a_conflict`,
`squash_reaching_the_first_commit_uses_root`,
`reorder_moves_commits_to_the_tip_and_into_the_middle`,
`reorder_keeps_a_commit_that_became_empty`,
`rewrite_refuses_a_merge_commit_in_the_range_and_a_dirty_tree`,
`preflight_reaches_the_root_commit_without_failing`,
`rewrites_pushed_is_false_without_an_upstream`,
`todo_path_with_spaces_and_quotes_is_safe`,
`rewrites_pushed_ignores_a_foreign_push`,
`sync_proposes_force_push_after_a_rewrite_and_pull_after_a_foreign_push`,
`sync_proposes_pull_when_the_reflog_cannot_answer`,
`undo_refuses_once_the_branch_tip_has_moved`,
`undo_of_a_cherry_pick_works_from_the_source_branch`.
Two probe claims in `operation.rs` rest on a scratch run rather than a test and
are cheap to add: the `--rebase-merges` stop that leaves `MERGE_HEAD` beside
`rebase-merge/`, and a real `git am` (the existing test fabricates the
directory).

## 8. Documentation on completion

WS-B's share is written: `FRONTEND.md` §3.7 (*the operation in progress*,
with `continue_operation` and `abort_operation`), §5, §6.14 and §8;
`DESIGN.md`, `STYLE.md` and `TECHNICAL.md` (*Operation in progress*);
`README.md`'s floor; and `ROADMAP.md`'s entry. What is still owed:

- **`FRONTEND.md`** — §1 command counts; §3 a new *Git — history rewrite*
  table (`cherry_pick_commits`, `squash_commits`, `reorder_commits`,
  `rewrite_preflight`, `undo_operation`); §5 `RewriteResult`, `UndoPoint`,
  `SyncProposal::ForcePush`; §6 rules for the newest-first SHA order, the squash target, insertion mode and the undo
  banner's lifetime, and the sentence saying a multi-row History selection
  raises no menu rewritten; a §8 row for the native/Tauri picker surfaces.
- **`DESIGN.md`** — the History bullet that says its menu "will host future
  actions" rewritten to the present, and the one saying a multi-row selection
  raises no menu along with it; one bullet per action that states the failure
  path as carefully as the happy one.
- **`STYLE.md`** — the insertion line, the hint caption and the banner action
  get their metrics.
- **`TECHNICAL.md`** — the `history_rewrite` module: the preflight, the
  driver, the todo builder, the message file, the reflog test.
- **`ROADMAP.md`** — dated entries for *Force-push-recommended detection* and
  *Rebase (interactive UI)* (squash and reorder; edit, drop and drag stay
  open). *Cherry-pick / revert* is **reworded, not ticked**: what ships is
  cherry-pick *onto another branch*; "into current branch" needs another
  branch's history on screen, and revert is untouched — both stay open.
- **`README.md`** — one bullet under *Browse history*.

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
  (the "into current branch" of `ROADMAP.md`'s *Cherry-pick / revert*). History shows the current
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
