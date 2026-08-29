# Plan — Cross-client feature parity (SwiftUI ⇄ Tauri)

> Status: **in progress — WS-A…WS-E shipped 2026-08-27, WS-F through WS-J and WS-L…WS-N 2026-08-28, WS-O and WS-P 2026-08-29. WS-Q is next. One named half is deliberately outstanding: the Tauri client's multi-line diff copy (DF-6), whose design is written out in that item. WS-K is unblocked but needs a Linux machine (§6's sequencing note), so it is taken whenever that machine is available. WS-T is the deliberate last one.**
> The remaining work was re-cut into seventeen smaller workstreams (C…S) on
> 2026-08-27 after WS-B proved too large to review as one piece — see §6.
> Accepted 2026-08-27 with every open decision resolved. Per-workstream state
> is in §6; what a shipped workstream found that the next one needs lives in
> that workstream's §6 entry, not here.
> Produced by a three-way
> audit of the SwiftUI client, the Tauri client, and the GitHub Desktop source
> at `/Users/leo/Dev/LeoManrique/Desktop/lms-github-desktop` (reference only —
> used to judge approaches to features LeoGit already has, never as a source of
> new features). Eleven feature areas were audited in parallel, every claim
> cited to code, and the load-bearing ones re-verified by hand. Companion
> contract: [`FRONTEND.md`](../../FRONTEND.md), corrected as part of this
> audit — see §2.

## 1. Motivation

The two clients are supposed to be feature-equal on one shared `leogit-core`.
They are not, and the gap runs in **both directions**:

- The Tauri client was originally ported from GitHub Desktop by a weak model
  and grew by accretion; several of its features are wired but unreachable
  (the merge UI), gated on the wrong thing (amend/undo by row index), or
  silently broken (a remote-less repo poisons the connectivity breaker
  app-wide). The previous plan
  ([`native-refresh-and-diff-continuity.md`](native-refresh-and-diff-continuity.md))
  pulled the *native* client ahead on refresh machinery, diff continuity, and
  config handling — none of that has flowed back.
- The native client, built flow-by-flow, still lacks whole contract surfaces
  the Tauri client ships: the `leogit <dir>` CLI launch, the update checker,
  launch-time repo discovery, multi-select in the file list, and a long tail
  of keyboard and presentation details the docs claim are shared.

Parity looked done because every Tauri flow *is* ported at core-command
granularity. At behavior granularity it isn't, in both directions. This plan's
goals, in the user's priority order:

1. **Feature parity between the two clients** — every difference either closed,
   or recorded in FRONTEND.md §8 as a deliberate platform divergence. No new
   features; GitHub Desktop informs *how*, never *what*.
2. **Current features working well** — the audit found real defects (some
   destructive) in shipping behavior; they come first.
3. **End-user convenience** — where the clients differ, the more seamless /
   responsive / honest behavior wins, whichever client has it.
4. **Efficiency** — low or reasonable CPU, network, and memory. The audit
   quantified several standing wastes (§3.3); the fixes ride the same
   workstreams.
5. **Shared core over duplication** — where the same logic is hand-written
   twice (and in several cases has already drifted), hoist it into
   `leogit-core` unless the performance cost is real (§5).

## 2. Method and rules of engagement

- Eleven areas audited: repo management; changes/commit; diff viewer; history;
  branches/merge; sync/publish; background machinery + update checker;
  clone/gh; settings/config + AI; terminal; app shell. The findings are
  condensed into §4 with citations kept for every claim a decision rests on;
  the destructive/high-severity claims and the doc contradictions were
  re-verified by hand against the code before this document was written.
- Every difference is classified: **defect** (wrong today), **missing-in-X**
  (parity gap), **divergence** (both defensible — gets a verdict), or
  **efficiency** (same behavior, different cost).
- GitHub Desktop is cited only where it settles a "which approach is better"
  question about a feature LeoGit already has.
- Findings carry stable IDs (`BR-1`, `DF-13`, …) so workstreams and decisions
  can reference them without repetition.
- **The living docs were wrong and have been fixed.** ROADMAP, FRONTEND,
  DESIGN, STYLE, TECHNICAL and README each claimed behavior the code doesn't
  have: a contract mandating what neither client does, three FRONTEND §8 rows
  describing features that don't exist, open ROADMAP items written against
  unreachable controls, four false claims in one DESIGN paragraph. Those
  corrections are already applied, so every document describes today's
  behavior and this plan describes only the work. Two classes could not be
  reached that way and are carried in WS-S: stale comments in Swift and
  TypeScript source, and a few doc claims outside the audit's checklist.

## 3. Defect register — fix before parity work

These are behaviors that are wrong *today*, ranked by severity. All verified
in code. "Make the current features work well" starts here (workstream A).

### 3.1 Defects — fixed

Nineteen of the twenty are closed: twelve in WS-A, D-5 / D-7 / D-17's
structural half with their hoists in WS-B, D-6 in WS-C, D-20 in WS-E, D-4 in
WS-I, and D-14 with D-15 in WS-P. Kept as a register (IDs are referenced from
§4) and trimmed to what each fix *is*, since the code now carries the reasoning.

| ID | Client | Fixed | Left over |
|---|---|---|---|
| D-1 | Tauri | **Destructive.** Amend/Undo/Checkout gated on the row's index into the *loaded window*, so past a slide Undo reset the real HEAD and seeded the composer from another commit. Now gated on `status.head_sha`, per FRONTEND §6.10. | — |
| D-2 | Tauri | A remote-less repo's doomed `git fetch origin` opened the breaker against every other repo. `fetchActiveRemote` now gates on `status.hasRemote`, like the tier path already did — and on the new `statusLoaded`, since `hasRemote` defaults to false and an unqualified read would decide "no remote" about a repo nobody has looked at yet. Natively, `silentFetch` returns `Bool?` so a slot conflict or a local `git remote` failure stops being reported as a network failure. | — (WS-B: `get_remote` answers `Option`, so the guard is live rather than dead). |
| D-3 | Tauri | Silent poll failures vanished forever. Three consecutive **background** failures now raise a non-blocking banner off `repoState.pollError` — native's shape and threshold, and its ownership: `refreshStatus` grew a `background` opt separate from `silent`, because four of the seven silent callers are user actions whose own `index.lock` races would otherwise accuse a healthy repo. Reset per repository in both clients. | — (BG-4's equality gate, the other half of that item, closed in WS-J; a skipped tick still retires the banner). |
| D-4 | Tauri | **Terminal listener-registration race.** Output and exit listeners were registered two async IPC round trips *after* `start_terminal` returned, while the reader thread was already emitting, and Tauri drops an event with no listener. The session's stream is now a `Channel` built with its handler attached and passed *into* `start_terminal`, mirroring the bridge — the id is minted client-side before any IPC, so the gap cannot exist. | — (the close message can now overtake `start_terminal`'s return, which the panel handles by refusing to adopt the pid afterwards). |
| D-8 | Native | ⌘W with a text field still focused dropped the typed value. `flushPendingSave` now also writes an edit that never scheduled a save, guarded by a diff against `lastPersisted` — which holds the *normalized* form of the fields, not the raw file, or a config written by the other client would be rewritten on an open-and-close that changed nothing. A completed debounce also clears `pendingSave` now (generation-guarded), which it never did. | — |
| D-10 | Tauri | A commit could land mid-Generate and have the late result overwrite the cleared composer. `canSubmit` gained `!isGenerating`, and the lockout runs off `isCommitInProgress` — `isCommitting` is still false while the embedded-repo confirmation waits, and its Confirm calls `performCommit` past `canSubmit` entirely, so the composer stayed live behind the dialog. | — |
| D-11 | Tauri | The HEAD-move reset was read as a backward slide and scrolled to the bottom of the fresh page, paging again. `log.resetSeq` marked the replacement. ✅ *WS-E* finished it: HI-2's append model deleted the slide, so `resetSeq` now means only "go to row 0" and the `skip > 0` hole it kept (a new commit while parked at offset 0 bumped nothing) has no case left to miss. | — |
| D-12 | Tauri | An empty parse fell through to "Select a file to view its diff" with a file selected. Both diff panes now have an explicit "No Textual Changes" state, blank while the fetch is in flight. The test is `hasRenderableDiff`, not `!== null`: `parse_diff` returns null only for empty input, while a mode change or pure rename parse into a header with zero hunks — a blank pane, the same dead end one layer along. | — (WS-B: H-9 supplies the reason, and a failed load is an `Err` rather than an empty parse; WS-E: DF-10's presentation half). |
| D-13 | Tauri | The header hand-rolled a status write that skipped `is_merging`, the `userDeselected` reconciliation, and the badge feed. It now takes the one refresh path as a prop: **one status writer in the client**. Checkout and undo also reload branches. | — (WS-F: post-op became status **+ log**, extracted as `reloadAfterHeadMove`). |
| D-14 | Native | **Stale-diff scroll: no reset on file switch.** The pane had no scroll control at all, so switching files landed the reader at the previous file's offset, often past the end of the new one. ✅ *WS-P*: `DiffStore` publishes `rendered` — the identity of the diff *on screen*, its path plus the commit where there is one — and a bound `ScrollPosition` answers a change to it with `scrollTo(edge: .top)`. Every re-read of the same diff keeps the offset, which is §6.3's contract; the Tauri reset key gained the commit in the same pass, since one path in two commits is two different diffs. | — |
| D-15 | Native | **Copying from a diff yielded garbage.** `.textSelection(.enabled)` sat on the whole row stack, so a drag beginning in the gutter pulled line numbers and `+`/`−` glyphs into the copy, and the tab expansion the pane needs (SwiftUI `Text` honours no tab stops) put spaces where the file has tabs. ✅ *WS-P*, and it is what finally consumed **H-16**: the gutter is a line handle (click, ⇧-click, a context menu, ⌘C, Escape) and the copy is `copy_diff_text` over the run's flat range, so it can contain neither the chrome nor a rendered tab. The gutter and glyph opt out with `.textSelection(.disabled)`, so a drag begun on them puts nothing on the clipboard. `onCopyCommand` returns nil when no run exists, so a within-line character selection answers ⌘C exactly as before — and *within-line* is all a native drag can ever be (**D-22**, deferred). | The Tauri half of the *multi-line* copy: its drag-select is already the file's lines for a unified diff and still interleaves a side-by-side one (DF-6). |
| D-16 | Tauri | `Ctrl+P` reached the shell *and* pushed; ⌃` could not leave a focused terminal; Escape closed overlays instead of reaching `vim`. One rule now (FRONTEND §6.11): `attachCustomKeyEventHandler` releases only the toggle, and the window handlers test the event's origin. | — (WS-I narrowed the toggle to ⌃`; TE-1's remaining half is the app's *other* chords, deferred to ROADMAP). |
| D-17 | Tauri | `tab_size: 999` and emptied fields persisted (and the emptied ones failed the save with a raw serde error). WS-B replaced the form's own clamp with `Config::normalized()`, which every writer passes through — including ones that never see this form — and whose bounds the controls now read (`config_bounds`) instead of restating. | — |
| D-18 | Native | The warm-up fetch ran offline, and against remote-less repos, discarding its outcome. Now gated on the breaker *and* `status.hasRemote`, and reports to the breaker (RM-10). Waits on the new `RepoStore.awaitLoadSettled()` so the gate reads a real status. | — |
| D-19 | Tauri | The `\ No newline at end of file` marker rendered its backslash twice. `linePrefix` no longer adds one — core keeps it in `content`. | — (WS-E took DF-8's minus sign; the two remaining alignments are native's, WS-P). |
| D-20 | Both | **Slow-load threshold destroyed the state it claimed to keep.** ✅ *WS-E*: crossing it now dims the pane and overlays a spinner in both clients instead of replacing its contents — Tauri through a shared `SeamlessDiffPane` wrapper, native through `.opacity` + `.overlay` on `content` rather than a branch beside it. The native comment claiming scroll survived was false for exactly this reason: a branch gives SwiftUI a different view to build, so the `ScrollView` was destroyed and rebuilt at the top and the store's equality skip was preserving something nothing could see. | — |
| D-5 | Tauri | **Config lost-update on a shared file.** A save posted the whole config as it looked when the dialog *opened*, so a native-side `tab_size` change was silently reverted. `patch_config` (H-10) is now the only writer: a surface names the fields it owns and cannot touch the rest, and core reads-edits-writes under a lock the file never had. | — |
| D-6 | Tauri | **Config never re-read while running** — a native-side save reached a running Tauri window never, so theme, diff settings, auto-fetch and provider stayed at their launch values for the lifetime of the app. WS-C: `resyncOnActive` calls `refreshConfig` first, before the refreshes that consume it. D-5 stopped this client from *clobbering* the shared file; this is the other half — reading it. | — (WS-H: the read now re-arms the fetch timer too, so a cross-client `auto_fetch` change takes effect on that activation). |
| D-7 | Both / core | **Empty-string AI config poisoned Generate in both clients.** `Some("")` is not `None`, so `--model ""` and a hostless Ollama URL sailed past every `unwrap_or`. `Config::normalized()` (H-10) treats blank-after-trim as absent on every read and every write, so an already-poisoned file heals on first load whichever client opens it. | — |

### 3.2 Defects still open

One of the original twenty, plus two this plan's own work uncovered (D-21, D-22).

| ID | Client | Defect | Severity |
|---|---|---|---|
| D-21 | Both | **A directory-backed entry has no content stamp, so its open diff can go stale.** `FileEntry.stat_stamp` is `symlink_metadata` mtime + size (`core/src/git.rs:708-724`), which for a *directory* describes its top-level entry list and not what is inside. Two entries are directories: a **submodule whose recorded pointer moved** (`SC..`, which `is_dirty_submodule` lets through, so a diff *is* read and renders `-Subproject commit A` / `+Subproject commit B`) and an untracked directory or embedded repo. Move that submodule to another commit in a terminal without changing its top-level entries and nothing in the diff's key moves — the pane keeps showing the old target indefinitely, and the poll can't catch it either, since `parse_ordinary_entry` discards porcelain v2's `hH`/`hI` gitlink shas so `RepoStatus` compares equal too. The fix is to keep those two fields and let a submodule entry's stamp be them: they are exactly what the diff renders, they cost nothing (they are already on the line being parsed), and `stat_stamp` is opaque by contract. Narrow — before WS-P narrowed the key, only the native client's unconditional refocus reload caught it, and the Tauri client never did. | Low |
| D-22 | Native | **A drag through the diff cannot select more than one line.** `.textSelection` makes a `Text` selectable; it does not join separate `Text` views into one selection domain, and the pane draws one `Text` per line, so a drag is confined to the line it began in no matter where the grant is attached. **Deferred by decision — see §10.** Multi-line copying is served by the gutter run instead (click, ⇧-click, *Select All Lines*, ⌘C), which is strictly more faithful than a drag would be, so what is actually missing is only the *gesture*. | Low |
| D-9 | Native | **Collapsing the terminal reflows the emulator to one row.** The zero-height frame is full-width, so SwiftTerm's degenerate-size bail (width *and* height zero) doesn't fire; the buffer reflows to `MINIMUM_ROWS = 1` and each collapse/expand cycle sends a spurious `SIGWINCH` (TerminalDock's `.frame(height: 0)` + SwiftTerm `AppleTerminalView.swift:353-356`). | Medium |

### 3.3 Standing efficiency wastes (quantified)

The user's second priority. Each is attached to a workstream; none requires a
behavior change the user would notice — except battery.

| ID | Where | Waste | Scale |
|---|---|---|---|
| E-1 | ✅ *WS-B.* `merging` rides on `RepoStatus`, answered by a filesystem read of `<repo>/.git` rather than a subprocess (H-1). The waste it named was real; its explanation was not — `get_status` never resolved the git dir. | was ~1 800 spawns/hour **per client** |
| E-2 | ✅ *WS-J.* `pollHeadSha` is gone and `refreshStatus` returns the status it read, so HEAD is compared against `head_sha` from the reply already in hand. The command went with it — its last two callers were this and the post-op re-seed — which is FRONTEND §6.1's rule finally reaching the code. | was ~30 spawns/min |
| E-3 | ✅ *WS-J* with BG-1. The hidden window drops to the 30 s rung, auto-fetch stretches ×3, and the tier scheduler parks outright until the window is active again — deadlines keep passing while parked, so waking up is one sequential catch-up rather than a lost cycle. | was ~60× hidden-state cost |
| E-4 | ✅ *WS-J.* One `pacedLoop` over a three-deadline array, native's exact shape: it sleeps to the nearest deadline and runs the due tiers sequentially, so "sequential" is now true across tiers as well as within them. | was 4 concurrent fetches worst case |
| E-5 | ✅ *WS-C* (RM-4's MRU sort deleted the `get_last_commit_timestamp` call with the store that made it) and *WS-J*: `ensureRepoIdentifiers` queues its paths and drains them four at a time, so opening the switcher on a machine with fifty repositories no longer spawns fifty `git remote get-url` processes in one turn. | was 2N at once, now ≤4 |
| E-6 | ✅ *WS-J* with BG-4. An idle repository publishes nothing. The same pass fixed the native mirror of it: `RepoDirectoryStore.noteActiveStatus` wrote an identical `RepoSync` into an observed dictionary on every tick, invalidating every switcher row. | was continuous idle re-render, both clients |
| E-7 | ✅ *WS-N.* `RepoStore.refreshWorkingTree()` re-reads the status only, for the actions that cannot move `HEAD`. Not `refreshQuietly` either: this *is* the user's action completing, so a failed read is theirs to see rather than one tick of a streak, and the epoch is bumped unconditionally since the file a discard rewrote may be the one on screen. The 30-file case also stopped being 30 calls — CH-1's bulk discard is one. | was one `git log`@500 per row action |
| E-8 | ✅ *WS-B.* `DiffOptions` makes the render artifacts opt-in, so the native path no longer builds HTML for the bridge to drop (H-8). `DiffLine.text` became `Option` in the same pass, dropping a duplicate of every line's content from both wires. *WS-O* finished the pairing half in the other direction: it is asked for by whichever host is about to render the split layout and by neither in the unified one, where the Tauri client had been building and serializing it unconditionally. | was ~40 k allocations per 20 k-line diff load |
| E-9 | ✅ *WS-P.* `DiffView`'s `LoadKey` carries the open file's own `stat_stamp` and `xy` plus the status's `head_sha`, so an unrelated edit anywhere in the tree re-reads nothing — the Tauri client's DF-1 shape, which also closed a hole in *both*: the working-tree diff is `HEAD` against disk, so a `--mixed` reset changes it while leaving the bytes and the status letters untouched. `workingTreeEpoch` and the refocus `forceDiffReload` went with it; a file edited while the app was away comes back with a moved stamp on its own. Phase two picked up the 80 ms debounce beside it. | was up to ~140 ms background CPU per unrelated edit |
| E-10 | ✅ *WS-N.* The fit is held in `@State` and re-derived from one `.onChange` over its three inputs (path, measured width, faces), so a hover, a selection change or a checkbox click no longer re-measures every visible row. TECHNICAL.md now describes what the code does. | was ~350 text measurements per interaction |
| E-11 | Tauri | Diff viewer mounts every row (no virtualization) and phase 2 re-parses N `innerHTML`s in one tick. **Terminal half closed**: the size guard landed in core (H-15), output coalesces under back-pressure instead of crossing once per 4 KiB read (H-14), and each delivery is now one channel send rather than a window broadcast (WS-I). Virtualization is the ROADMAP item DF-4 defers. | terminal half closed; virtualization deferred to ROADMAP |
| E-12 | ✅ *WS-I.* `start` / `resize` / `close` are `#[tauri::command(async)]`, off the main thread. `write` deliberately stayed sync — a sync command runs inline in IPC arrival order, and that order *is* the keystroke-ordering guarantee. | was a ~250 ms hitch on every teardown |

## 4. The parity inventory

Per area: what differs, who is right, and why. Verdicts follow the priority
order in §1. Items already listed in §3 are referenced, not repeated.
"→ WS-x" names the workstream that closes each item. Where an item was a real
fork in the road, a bolded **Decided** clause carries the ruling and the
reasoning behind it; that clause is what binds the implementation.
Cross-cutting rulings that no single item owns are in §7. Behaviors the audit
checked and found equivalent are not listed — each area also produced a
parity-confirmed list (breaker numbers,
tier cadences, progress aggregation, commit semantics, seamless-diff rules,
terminal lifecycle, and so on), so an item's absence here means it was checked
and matched, not that it was skipped.

### 4.1 Repo management (RM)

- **RM-1 · No-repo state.** ✅ *WS-L.* Native Welcome is the switcher's own list
  under the app's name — one view (`RepoPickerList`) in both places, so the pair
  cannot drift a third time. Launch resolves as Tauri's does: the recorded repo,
  else a sole discovered one opened by itself, else the picker; the auto-open is
  confined to launch, so a later scan-path edit cannot pull the user out of the
  picker they are standing in. The restore runs *before* the walk — it validates
  its own path and does not need the list, so the common launch no longer queues
  behind a filesystem crawl. `Open Repository…`, ⌘O and the `.fileImporter`
  behind them went with the same change (RM-2). Native's no-back-to-welcome
  model is unchanged.
- **RM-2 · Open a repo outside the scan paths.** **Decided: neither client gets
  a per-folder open action**, GitHub Desktop's File ▸ Add Local Repository ⌘O
  notwithstanding. A repo list is *what the scan paths cover*, so a local
  repository missing from it means the paths are wrong; sending the user to
  Settings fixes the cause and holds next launch, where a one-off open patches
  one symptom and invites the list to disagree with its own configuration. The
  empty state's "Choose folders to search" CTA is the sanctioned route, and a
  repo genuinely outside every scan path still arrives by clone or
  `leogit <dir>` and then keeps its row via RM-3's MRU union. ✅ *WS-C* for the
  switchers, ✅ *WS-L* for Welcome's last entry point — load-bearing only until
  the list arrived, and gone in the change that brought it.
- **RM-3 · Row-list membership.** Native unions discovery with the
  existence-checked MRU, so an Open-Other repo keeps its row across launches;
  Tauri's list is discovery-only — clones, CLI opens, and Open-Other rows all
  vanish on restart, and `last_opened_repo` restore is conditioned on
  discovery re-finding it. **Native right**; Tauri is throwing away state it
  already persists. ✅ *WS-B*: the union rule is core's (`known_repos`) and both
  clients call it, so a clone, a CLI open or an Open-Other repo keeps its row
  across restarts and a path that no longer exists loses one — which also stops
  native tiering dead MRU entries and burning a time-boxed fetch on each per
  tier interval (the reverse defect). ✅ *WS-C* for the rest: the
  `last_opened_repo` restore now tests that the path is still a repository
  instead of that this launch's walk re-found it, and lists it itself when the
  union didn't — a repo outside the scan paths that aged out of the MRU's cap
  used to drop the user into the picker with the repo they were just in
  missing from it.
- **RM-4 · Switcher sort order.** Decided: **MRU-of-use on both** — zero
  subprocesses, a list that doesn't move while you're aiming at it, and the
  signal a switcher is actually for (last-commit-time answers "where did a
  commit land most recently", which can be someone else's work you just
  fetched). ✅ *WS-C* for the Tauri half: the dropdown ranks active → MRU index
  → name-ordered tail, native's rank function in TS, and `repoActivity` +
  `get_last_commit_timestamp` are deleted with their last consumer — which is
  also half of E-5. ✅ *WS-L* for the persisted clock↔A-Z toggle, which native
  had been ignoring — a Tauri-set "alphabetical" silently did nothing there.
  Hydrated once per launch in both clients, because the toggle's write is
  asynchronous and a later read could put the old value back over a choice the
  user had just made.
- **RM-5 · Row labels + search input set.** Tauri labels rows with the GitHub
  `owner/name` (colliding basenames get a muted `owner/` prefix — GitHub
  Desktop's rule) and searches over those names; native shows bare basenames
  and searches only them — so the "tier-for-tier identical" search contract
  (FRONTEND §6.9) has silently drifted on its *input set*, and Tauri's own startup
  picker disagrees with its own dropdown. **Tauri's labels right; the search
  rule belongs in core**. ✅ *WS-B*: `match_repo` and the batch `filter_repos`
  both hosts call replaced the two drifted files, which are deleted. Tauri
  gained the `$HOME` path-root and native gained case- and
  separator-normalized prefix matching (its raw `hasPrefix` silently made the
  whole absolute path searchable whenever a scan folder's case differed).
  ✅ *WS-L* for the labels: `RepoIdentifierStore` is the native
  `repoIdentifiers.ts`, and the four label rules (row label, owner-qualified
  label, the searchable pair, the collision set) live in one place per client —
  which also closed the Tauri startup picker's own drift, since it was rendering
  raw paths and searching only basenames beside a dropdown that showed
  `owner/name`. The bound on Tauri's fan-out shipped with E-5 in WS-J.

  **The plan's "visible rows only" was not taken, and should not be.** Labels
  are *searchable*, so a query has to reach a repository the user has never
  scrolled to; fetching per visible row would have made the filter silently
  depend on how far the list had been scrolled. E-5's worker pool is the bound
  the phrase was reaching for, and it already exists — so the list asks for
  every row and pays four subprocesses at a time.
- **RM-6 · Switcher keyboard cursor.** ✅ *WS-L.* ↑/↓ over the rows with
  scroll-into-view, Return on the cursor's row, and the cursor snapping back to
  the top match on every keystroke — Tauri's behaviour, with
  `ListNavigation.nextIndex` as `nextActiveIndex` written for Swift so an arrow
  key cannot answer differently. **Not** `List(selection:)`, which this entry
  had proposed: a list moves a cursor only while *it* is first responder, and
  these lists keep focus in the filter field — taking it away would end the
  typing that produced the rows. The cursor is read from the field instead, via
  `onKeyPress`.
- **RM-7 · Empty/loading states.** Native's switcher distinguishes
  looking/none-found(+searched folders)/no-matches; Tauri's dropdown said
  "No repositories" for everything, with the rich state only in the startup
  picker. ✅ *WS-C* for Tauri: one shared `RepoListEmptyState` answers all three
  in both lists, and the "Choose folders to search" CTA is on **both** dead
  ends, not only the empty one — "none matched" is what you see when the repo
  you want lives somewhere discovery was never pointed at. (The "looking" state
  is unreachable in the dropdown by construction: the open repo is always
  listed there. It is live in the picker, the phase that can have none.)
  ✅ *WS-L* for native: `RepoListEmptyState` answers the same three, with the
  same strings and the same CTA in both dead ends.
- **RM-8 · Switching mid-transfer.** **Native right** given the single global
  slot (GitHub Desktop allows it, but scopes state per repo — out of reach
  without per-repo op state). ✅ *WS-C* for Tauri, ✅ *WS-F* for the other half
  (⌘R is held back during a transfer too — SH-3), and ✅ *WS-L* for **where** the
  hold lives: the repository *rows*, in both clients, not the control that opens
  the list. Switching away would leave the old repo's transfer running while the
  new repo's header read "Pushing…" with no progress, and gate the new repo's
  polling for invisible reasons — none of which browsing or cloning does. See
  CL-1.
- **RM-9 · Discovery freshness.** **Native right.** ✅ *WS-C* for Tauri:
  `services/repoDiscovery.ts` re-walks on dropdown open and on Settings close
  in *both* phases (button, Escape and ⌘, all route through one handler), with
  a single in-flight pass shared rather than duplicated and the open repo
  re-added if a walk racing the fire-and-forget MRU write would drop its row.
  The main view used to need a restart for a scan-path edit or a terminal
  clone. ✅ *WS-L* for the native refinement: the walk and the badge sweep are
  two independent `.task`s, and the sweep is keyed on the row list so it re-runs
  when the walk publishes new rows — running it *behind* the walk delayed every
  badge by the slower half and then swept only what the list happened to hold
  when the popover opened.
- **RM-10 · On-switch breaker feed.** ✅ *WS-A.* The native warm-up fetch now
  reports its outcome to the breaker like every other real attempt, in the
  extracted `ContentView.warmUpFetch` alongside D-18's gating.
- **RM-11 · Sweep re-check granularity.** Tauri re-checks the network slot
  between every repo of a sweep and bails mid-list; native's visible sweep
  checks once at entry (its tier runner *does* re-check — internal
  inconsistency). **Tauri right**; move the native guard inside the loop. → WS-S

### 4.2 Background machinery, connectivity, update checker (BG)

- **BG-1 · Cadence policy.** ✅ *WS-J.* `services/backgroundPolicy.ts` is
  native's `BackgroundSchedulingPolicy`, table for table: a three-state ladder
  (`active` / `inactive` / `hidden`, from `document.hidden` + `hasFocus()`),
  2/10/30 s status poll, auto-fetch ×3 while hidden, and only the multi-repo
  sweeps pausing. The 0–30 s once-per-session skew landed on **both** clients,
  on the automatic fetch only — skewing the status poll would delay the first
  local read after launch by up to half a minute, visibly, for no contention
  worth avoiding. The one thing the web client cannot pin is the hidden rung
  (no App Nap analogue; a WebView may throttle a backgrounded document), which
  can only make hidden work *cheaper* and is recorded as the residual §8 row.
- **BG-2 · Live re-arm of `auto_fetch` / `fetch_interval_ms`.** ✅ *WS-H* for
  the re-arm, ✅ *WS-J* for the shape: `services/pacedLoop.ts` replaced both
  `setInterval`s. Native's idle 30 s re-check has no equivalent and needs none
  — a parked loop (`dueAt` of `Infinity`) is re-armed by the config effect
  directly, and a native-side edit arrives on the next wake-up's config read.
- **BG-3 · Connectivity signal.** Native `NWPathMonitor` is authoritative;
  Tauri's `navigator.onLine` is hard-wired `true` on WebKitGTK, silently
  disabling the offline gate, the recovery kick, and the update-check retry on
  Linux (the breaker's lapsing backoff is the de-facto recovery — up to 5 min,
  not never). **Decided: build the core observer in this plan** (H-17):
  a `core::net` watcher emitting online/offline over the event seam, Linux
  backend first (netlink route watch — the broken platform, and a Linux test
  machine is available), then macOS and Windows; the Tauri client adopts it
  there, and the native client retires `NetworkPathObserver` once the macOS
  backend proves equivalent. Until it lands, `navigator.onLine` stays
  authoritative-negative only. **Decided by the user: built beside its
  adopter** — a workstream earlier would have meant an unverifiable, uncalled
  backend, which is the dead-wiring shape this plan otherwise deletes. It is
  now a workstream of its own (see §6's WS-K entry) because three OS backends
  on a Linux machine is not a rider on anything. → WS-K
- **BG-4 · Poll equality + failure surfacing.** ✅ The failure half landed in
  WS-A (D-3), the equality half in *WS-J*: `refreshStatus` compares
  `JSON.stringify(status)` against the previous reply and returns without
  touching the store when they match. Whole-value rather than a named-field
  fingerprint, for the reason native's `Equatable` compare is whole-value — a
  hand-picked list is one a later field silently falls out of. Only *silent*
  refreshes take it (an explicit one also clears the error modal on success),
  and a standing `pollError` still retires on a skipped tick.
- **BG-5 · Update checker.** ✅ *WS-M.* `check_for_update` and `open_url`
  are exported, `UpdateInfo` is mirrored, and one `UpdateStore` runs the check
  from the root view — so it covers the picker phase, as Tauri's does. Gated on
  `isOnline` **alone**: the breaker guards git remotes, and one open for those
  reasons would silently suppress the check for the whole session. The outcome
  does not feed it either. `NetworkPathObserver` grew keyed recovery
  subscribers, so the retry sits beside the repository catch-up rather than
  needing a second monitor. One `UpdateChip` serves the toolbar and the
  picker.
- **BG-6 · Typing guard.** ✅ *WS-J.* `utils/focus.ts` holds one predicate over
  a node, asked about `e.target` by the key handler and about
  `document.activeElement` by the fetch tick. The latch is gone, and with it
  the fault where killing a focused terminal left auto-fetch dead for the
  session. Terminal focus still suppresses the fetch — xterm's hidden textarea
  matches — but as a decision now rather than a side effect: it is what native
  does deliberately, and a shell is exactly where the file list gets changed
  from.
- **BG-7 · Un-occlude resync.** Tauri resyncs on visibility *and* focus;
  native only on app activation (documented — up to one 30 s beat after
  un-occluding without activating). Cheap to close: fire the existing resync
  from the policy's occlusion edge. → WS-S
- **BG-8 · Auto-fetch budget (shared).** Both clients run auto-fetch under the
  *user* network budget (15/30/600 s), not the 12 s background budget the tier
  fetches use. ✅ *WS-B.* `fetch` takes a `background` flag and both clients'
  automatic paths pass it, so an unreachable remote gives up in 12 s instead of
  holding the single network slot for ten minutes with every other repo's
  refresh queued behind it. A fetch the user asked for keeps the generous
  budget a large transfer needs.

### 4.3 Sync & publish (SY)

- **SY-1 · Control shape.** ✅ *WS-F.* Tauri's Pull + Push-split + Refresh
  collapsed into native's one adaptive ladder button, and the ladder itself
  moved to core (H-3). The three-control layout was not merely different — it
  was what made SY-3's rejected push, SY-9's echoing chevron and SY-2's missing
  fetch reachable at all. Tauri keeps its on-button counts; native's
  platform-forced `↑N ↓N` text stays, now a FRONTEND §8 row.
- **SY-2 · Manual Fetch.** ✅ *WS-F.* Tauri had no route to a fetch at all —
  `gitApi.fetch`'s only caller was the automatic loop, so checking the remote
  while in sync meant a working-tree-mutating pull. It is now the proposal in
  the in-sync state and a menu item in every split state, on its own
  `'fetch'` network-op slot. The planned three-line interim was subsumed rather
  than shipped: SY-1 landed in the same change, so there was no window in which
  a stopgap would have been visible.
- **SY-3 · A push git will reject.** ✅ *WS-F*, structurally: pull outranks push
  in the ladder, so the diverged state proposes Pull and the rejected push has
  no way to be offered. No interim disable was needed.
- **SY-4 · ⌘P semantics.** ✅ *WS-F.* Runs the proposed action in both clients.
  Pull had no keyboard route in Tauri at all before this.
- **SY-5 · Inferred counts.** ✅ *WS-F.* Tauri stopped suppressing them in the
  publish-branch state — core computes them against
  `refs/remotes/<remote>/<branch>` precisely so they can be shown.
- **SY-6 · Publish dialog failure mode.** ✅ *WS-F.* gh's error is inline with
  every field intact, the progress bar sweeps rather than sitting empty, and
  the org `owner/name` hint is there. The modal it used to stack over itself
  cost two dismissals to fix a name collision, with the doomed name still in
  the field behind it.
- **SY-7 · Force-push confirm.** ✅ *WS-F* for the Tauri half: the target is
  named from `status.upstream` (right even when the upstream branch name
  differs), which also retired a `git remote` per repo open spent purely on
  dialog text. Its error moved inline for SY-6's reason, which the plan had
  asked for only on publish — the same dialog-stacking, in the same file.
  Native still owes Tauri's dialog lifetime. → WS-Q
- **SY-8 · Post-op refresh.** ✅ *WS-F.* Post-transfer is status **+ log** in
  both clients, extracted beside the commit path that already did it, with the
  `lastHeadSha` re-seed so the next poll doesn't refetch the log again.
- **SY-9 · Chevron contents.** ✅ *WS-F.* A chevron appears only where it offers
  something the face doesn't — the publish-branch, pull and push states — and
  carries Fetch plus, while diverged, force push.
- **SY-10 · Transfer error surface.** Tauri renders git's multi-line rejection
  in a selectable `<pre>`; native's `.alert` collapses whitespace and can't be
  copied (D-15's sibling). Route native sync failures to the selectable
  banner, or make the alert text monospaced + selectable. → WS-Q
- **SY-11 · Progress presentation.** ✅ *WS-F* for Tauri's indeterminate case:
  fetch and publish report no percentages and a push reports none until git's
  first tick, so the in-button fill sweeps instead of sitting at zero under a
  spinner. Each client keeps its own shape (in-button fill vs full-width
  strip), now a FRONTEND §8 row.

### 4.4 Branches & merge (BR)

- **BR-1 · Merge UI was dead code in Tauri.** ✅ *WS-G.* Nothing ever set
  `showMerge`, `mergeTarget` was never written and `countCommitsToMerge` had zero
  callers: a complete flow, wired end to end, that no user could start. Rather
  than wiring the old overlay to a new button, the dropdown became the branch
  **menu** — the source pick, the commit-count preview, Merge / Squash & Merge,
  conflicts as data and Abort, in the shape §8 records. Both native refinements
  shipped with it: the merge submenu hides itself mid-merge (git would refuse
  anyway), and a zero count reads *already up to date* with both buttons
  disabled in **both** clients, where the native sheet used to print "Brings in
  0 commits." beside a live Merge button.
- **BR-2 · Abort merge had no Tauri UI.** ✅ *WS-G*, in the same pass and for the
  reason WS-F named: it is not a second item but the other half of one surface.
  A merge begun in the embedded terminal now has an in-app exit, offered only
  while `RepoStatus.merging` — the one action with no meaning outside that state.
- **BR-3 · Branch-list freshness.** ✅ *WS-G.* The Tauri list is re-read on every
  menu open, which is the moment of intent and one `for-each-ref`. It used to
  reload at five call sites, none of them this one, so a branch created in the
  embedded terminal could be invisible for the whole session — the poll only
  notices the ones that move HEAD.
- **BR-4 · Busy state.** ✅ *WS-G* for the Tauri half: one `branchOp` serializes
  every branch operation and locks the menu's controls, so a double-click can no
  longer issue two checkouts to contend on `index.lock`. Built deliberately
  *unlike* native's `run`, whose `nil` return makes "dropped because busy"
  indistinguishable from success — a refused start here returns without
  dismissing the surface that asked. Native's nil-return fix is still open.
  → WS-Q
- **BR-5 · Same-branch re-select.** ✅ *WS-G.* Guarded in the handler, so the
  keyboard route is covered too, not only the click.
- **BR-6 · Create-branch failure.** ✅ *WS-G.* The form keeps the typed name and
  states git's refusal under the field — FRONTEND §6.13's dialog refinement,
  which WS-F had turned from a preference into a written rule.
- **BR-7 · Delete confirmation.** ✅ *WS-G.* Native's "Unmerged commits are
  lost." is now the shared wording, and the hover-only ✕ — invisible to keyboard
  users, and the one destructive action sitting on a row's most casual gesture —
  became a row context menu with the destructive item first behind a divider,
  which is where rename lands later. GitHub Desktop's "also delete on the
  remote" checkbox stays deferred with BR-10 to a ROADMAP item that builds it on
  both clients at once (core has `delete_remote_branch`; a combined
  `delete_branch(…, include_remote)` keeps the ordering semantics in one place).
- **BR-8 · Detached/merging markers.** Native rides the branch chip's label;
  Tauri shows an icon swap + two yellow badges. Both platform-appropriate —
  **document as a FRONTEND §8 row** — but native's `· merging` suffix is easy to miss
  and truncates first; give it the same color treatment as the conflicted
  badge. → §9, WS-Q
- **BR-9 · `is_merging` fold-in** — ✅ *WS-B.* `RepoStatus.merging` is filled
  from a filesystem read of the git dir, so it costs the poll nothing and no
  refresh path can forget to ask (D-13's bug class). The standalone command
  was deleted rather than kept for compatibility: with both clients reading
  the status field it had no callers, and an unused second route back to the
  same answer is how they diverge again.
- **BR-10 · Rename / remote-delete plumbing.** Tauri carries registered,
  wrapped, never-called commands; the FFI's dead-surface rule (don't export
  what nothing calls) is the better policy — the dead wiring is exactly how
  BR-1 rotted unnoticed. **Decided: defer the feature, delete the wiring** —
  rename is a genuine gap in both clients (today you rename in the terminal),
  but building it is new feature work, which this plan excludes; leaving dead
  wrappers standing is how BR-1 stayed broken. Delete the Tauri wrappers now
  (with `hasMergeConflicts`, the three other unconsumed derived stores, and
  `get_repo_name`, which WS-H left unconsumed on purpose — the window title
  takes the client's own `basename()`, like every other repo label here);
  ROADMAP carries rename + delete-on-remote as one build-on-both-clients
  feature, homed in BR-7's branch-row context menu and backed by the combined
  core `delete_branch(…, include_remote)`. Branch management still reaches a
  reasonable bar inside this plan — BR-1…BR-8 and BR-11 are unaffected by the
  deferral. → WS-S
- **BR-11 · Large branch lists.** ✅ *WS-G.* The Tauri popover gained the filter
  input STYLE.md had already specified, the repo pickers' ↑/↓/Enter cursor
  (`listNavigation`, reused rather than re-written) and a keyed `#each`. Native
  keeps AppKit's scrolling and type-select. DESIGN's claim that the two lists
  match is now true, so WS-S no longer owes that correction.

### 4.5 Changes tab & commit flow (CH)

- **CH-1 · Multi-select + bulk actions.** ✅ *WS-N.* `List(selection: Set<String>)`,
  so shift-click and shift-arrow are AppKit's rather than hand-tracked anchors, and
  `contextMenu(forSelectionType:)` already hands the menu the whole selection when
  the right-click starts inside one — which is the Tauri re-select rule without the
  bookkeeping. Multi-row collapses to "Discard N Selected Changes…", one bulk call.
  The old §8 row is gone; the new one records what still differs (which row an
  extension leaves open — a `Set` cannot say which one was clicked).
- **CH-2 · Space / keyboard toggle.** ✅ *WS-N.* `.onKeyPress(.space)` on the
  *list*, not a row — the selection is what it acts on, and a focused row checkbox
  still gets the key first because AppKit gives a focused control priority. Same
  target state as the master checkbox in both clients: any excluded → include all.
- **CH-3 · Select-all header.** ✅ *WS-D* (label) + *WS-N* (tri-state).
  `Toggle(sources:isOn:)` is the only route to a mixed checkbox in SwiftUI; it takes
  a `Binding<[RowInclusion]>` over the committable rows.
- **CH-4 · Status badge style.** The conflicted letter differs between the
  clients (`U` in Tauri, `!` natively): **verdict `U`** — git's own porcelain
  letter, which is the vocabulary native's own comment claims to follow. Hoist
  `letter()`/`label()` onto core's `FileStatus` so the glyphs can't drift
  again. ✅ *WS-B* for the glyph and the token: `file_status_styles()` is the
  one table both clients read (once, at startup — colour stays per-platform),
  the conflicted letter settled on `U`, and conflicted gained its own purple
  status token in both palettes rather than reusing red, because red is
  already Deleted and a glance down the list has to separate "you deleted
  this" from "git couldn't merge this": opposite actions, and one of them
  blocks the commit. The token's second consumer is BR-8's easily-missed
  `· merging` suffix. **Decided: the native 18×18 tinted plate behind the
  letter stays** and becomes the shared look — STYLE.md's old no-plate rule was
  a leftover of the earlier TUI-flavored direction, macOS style is the design
  authority today. ✅ *WS-D*: the Tauri badge is that plate — 18×18, 4px radius,
  the status colour at 15% behind its own letter — set from one `--badge-tint`
  per row so the letter and the wash can't name different statuses, and applied
  to the embedded / dirty-submodule glyphs too so the column doesn't read ragged.
- **CH-5 · Rename display.** ✅ *WS-N* for both file lists (they share
  `ChangedFileList`): `orig_path → path`, the from-side fully muted, both sides
  greedy so they split the row evenly and truncate filename-first independently.
  ✅ *WS-P* gave the native **diff header** the same treatment (DF-8), sourced
  from the parsed diff rather than from the entry — see that item for why the
  two sources differ.
- **CH-6 · Embedded/submodule row treatment.** ✅ *WS-N.* The ↪ replaces the
  status letter (blue for an embedded repo, muted for a dirty submodule) rather
  than sitting beside it, because the letter is the part that would be wrong. The
  two explanatory sentences live once, on `FileEntry.repositoryEntryHint`, read by
  the badge and by the disabled checkbox.
- **CH-7 · Exclusion-set semantics.** ✅ *WS-J*, on both clients, through
  `core::exclusions::reconcile_exclusions` (H-20). Native semantics plus a
  grace window; the native comment claiming the two clients already matched is
  gone. Three refinements the entry hadn't anticipated. The window is
  **wall-clock, not a tick count**, because BG-1's ladder makes a tick worth
  anything from 2 s to 30 s. It **also counts consecutive misses**, because
  wall-clock alone fails at the other end of that same ladder: at the 30 s rung
  one read is charged the entire window, so a purely time-based rule prunes on
  the *first* look — the look that can land mid-rewrite — and the window would
  have bought nothing exactly where unattended rewrites are likeliest. And the
  IPC cost the decision worried about is **skipped entirely while the set is
  empty**, which is the app's usual state, so the poll pays only once the user
  has unchecked something.
- **CH-8 · Discard confirmation copy.** Native names the actual per-file
  outcome (restored from HEAD vs moved to Trash) — what FRONTEND §6.10 asks for; Tauri
  states both rules generically and dismisses on backdrop click (STYLE.md
  violation). But native *guesses* the outcome from status while core decides
  it authoritatively via `ls-tree`. ✅ *WS-B*: `classify_discard` returns the
  same plan the discard itself runs on, and both dialogs render it — so the
  three cases the guess got wrong (a staged re-add of a path that exists in
  HEAD, a rename whose original is *not* in HEAD, and every file under an
  unborn HEAD) now read truthfully instead of promising something the action
  then doesn't do. ✅ *WS-N* for the rest: the native confirmation became a
  **sheet**, since a system confirmation dismisses on the click and can therefore
  neither say *Discarding…* nor hold the refusal §6.13 keeps inside the dialog
  that raised it. Its outcome sentences are Tauri's count-based forms, which work
  at any N, with the question line naming the single file.
- **CH-9 · Embedded-repo confirm.** Tauri's copy is better (names the outer
  repo, states the clone consequence, "Commit as link" verb); native's system
  `confirmationDialog` is the right container. Merge: native container +
  Tauri text. ✅ *WS-D* for the Tauri half: one `canCancel` gate now answers
  the backdrop, Escape and the Cancel button, where only Escape had checked —
  the tell that a per-dismissal list had already drifted once. ✅ *WS-N* for the
  merge: Tauri's title, body and **Commit as link** verb inside native's
  confirmation. D-10's native half rode along — the composer locks while the
  dialog waits, since Confirm commits the files the dialog was *opened* with.
- **CH-10 · Composer details.** Port to native: the 72-char summary counter
  (STYLE.md; skip Tauri's silent 200-char hard cap — it truncates pasted
  and AI-generated summaries), the included-row weight cue, tooltip only when
  truncated, keyboard resize on the handle (ROADMAP's composer-resize item), an
  in-flight "Committing…" label, and coalescing the per-drag-frame
  `UserDefaults` writes to drag-end. ✅ *WS-D* for the Tauri half: ⌘↩/⌘G moved
  to `MainLayout`'s window handler above its `inField` bail (the fields' own
  listeners made them reachable only once you were already typing), inert on
  History and under any dialog; and the height clamp landed, measured off a
  wrapper around both tab panes because the Changes pane reports zero while
  History shows. D-10's lockout landed in WS-A, and ST-7's revert turned out to
  have landed in WS-B. ✅ *WS-N* for the native half: all six, plus the deliberate
  omission of the 200-char cap. The counter sits **beside** the field rather than
  inside it — `NSTextField` scrolls its own text to the trailing edge as the caret
  moves, so an overlay would land on the characters it is counting. The weight cue
  is measured as well as drawn (`PathText.nameWeight`), or a fit taken in the
  lighter face would overflow the row it had just promised to fit.
- **CH-11 · Row-action errors** (with **SH-5**). Decided: **split by class in
  both** — an operation the user is waiting on takes the modal, with a retry
  where the same attempt can just be made again; a failure that was never their
  task states itself in the strip. ✅ *WS-D* for Tauri: `reportActionError` /
  `reportNotice` in the repo store are that choice, so a call site picks a
  function instead of copying the `repoState.update` shape from its neighbour —
  which is how every failure in the client, down to "couldn't reveal the file",
  came to seize the window. The strip gained a second, dismissible variant (the
  poll's own has no ✕ because its own recovery retires it; nothing can retire
  this one), and `ErrorModal` was finally passed the `onRetry` it has always
  accepted. The rule is now FRONTEND §6.13. ✅ *WS-N* for the native changes tab:
  one `ActionFailure` + `.actionFailureAlert`, adopted by `SyncControls` and
  `BranchMenu` too, so this client also has one such function rather than a copied
  `.alert` per site. Discard and ignore take the window with a retry (they fail on
  a lock race far more often than on anything the user must change first), reveal
  and open-with take the strip, and the strip gained its ✕ — split on
  `RepoStore.canDismissError`, since the poll's own banner is retired by its own
  recovery. Checkout and undo are still native's drift. → WS-Q
- **CH-12 · Copy File Path.** ✅ *WS-D.* Tauri crossed to the backend and back to
  concatenate two strings it already held. `utils/path.ts` gained `absolutePath`
  — the one place the filesystem path and git's always-`/` path meet — which
  reads the separator off the repo root rather than assuming one, so a Windows
  paste still gets backslashes.
- **CH-13 · First-file auto-select.** ✅ *WS-D.* Tauri landed on an empty pane
  beside a list of files while its own commit-detail pane had always
  auto-selected. It now re-seats on native's two conditions and no others
  (nothing open, or what was open left the tree), keyed on a `$derived` of the
  *path list* so a 2 s tick that only changes content can't move the selection.
  Pairs with DF-1 so the auto-selected diff stays fresh.

### 4.6 Diff viewer (DF)

- **DF-1 · Open-diff freshness (Tauri).** ✅ *WS-J* for the Tauri half, and
  per-file as the entry intended: a `$derived` key of the active file's
  `path`, `xy` and `stat_stamp`, read by an `untrack`ed effect that reloads
  only when the *same* path's key moved — a different path means the selection
  changed and its own load is already in flight. The §8 staleness row retired
  with it. ✅ *WS-P* for the native gate (E-9), copying that shape into
  `DiffView`'s `LoadKey` — and adding `head_sha` to **both** clients' keys,
  which neither had: the working-tree diff is `HEAD` against disk, so a
  `--mixed` reset changes it while leaving the bytes and the status letters
  exactly as they were.
- **DF-2 · Side-by-side.** ✅ *WS-O, both clients.* The native split layout is an
  arrangement of one row model rather than a second renderer — `DiffStore`
  holds the flat line list either way and a pair carries indices into it, so
  both columns read the same lines, each side indexes its own tokens, and one
  `DiffLineCell` draws a cell for both layouts. Core's `build_sbs_pairs` stays
  the only pairing, crossing the bridge as a `u32`-indexed record and only
  while the split layout is on screen — now true of the Tauri client too,
  which had been building and serializing pairs for every read. The toggle is
  a two-segment control in the diff header on both clients (GitHub Desktop's
  placement), still persisted in the shared `side_by_side_diff`; the Settings
  checkbox is gone. FRONTEND §8's row retired with it.
- **DF-3 · Structured wire.** FRONTEND §7's open decision (where the HTML
  collapse lives) should close **toward the structured token wire for both**
  — that is GitHub Desktop's own shape (worker tokens, view-side collapse),
  it halves Tauri's per-diff IPC, and it unblocks Tauri virtualization.
  ✅ *WS-B* for the core half: `DiffOptions` makes HTML and side-by-side pairs
  opt-in (E-8), `DiffLine.text` is an `Option` filled only for the Hunk and
  NoNewline rows that read it, and `get_parsed_diff` /
  `get_parsed_commit_diff` fuse the read and the parse. FRONTEND §7's open
  decision — whether the *phase-2* wire becomes structured tokens for both —
  is untouched and still open.
- **DF-4 · Virtualization + size guards.** Tauri mounts every row.
  GitHub Desktop solves the exact variable-height
  problem Tauri gave up on (measured-height cache). Neither client has any
  large-diff guard; GH Desktop has three (70 MB buffer / ~4.4 MB "reasonable"
  / 5 000 chars-per-line → a "Show Diff anyway" state). **Decided: guard only
  in this plan** — the two fixes sit at very different sizes, and only the
  guard is cheap. ✅ *WS-B*: `DiffSizeGuard` withholds a patch over 4 MiB, or
  one with a line over 5 000 bytes, and both viewers render the measurements
  with a "Show diff anyway" button that re-asks with the guard lifted (per
  request, not sticky — moving to another file gets the guard back), so a
  pathological diff is survivable and explained instead of a hang. The
  long-line limit earns its place separately: a minified bundle is slow at a
  total size the byte bar waves through. Virtualization only smooths the
  merely-large diff, so it stays a ROADMAP item recording GitHub Desktop's
  measured-height approach, why it is a web-only concern (SwiftUI's
  `List`/`LazyVStack` already virtualize — no native work exists there), and
  that it is re-judged after DF-3's structured wire changes how rows are
  built, then taken only if large diffs still feel heavy.
- **DF-5 · Dead per-line-selection scaffolding (Tauri).** **Decided: the
  scaffolding stays** — per-line staging is the unfinished GitHub Desktop
  feature, and ROADMAP commits to finishing it here and porting it natively
  (core's `build_patch` is complete and tested). ✅ *WS-E* neutralized its cost:
  the hunk header renders as a control only under `showSelection`, so today's
  band is plain selectable text instead of one focusable no-op button per hunk
  on an unvirtualized list. Two branches rather than conditional attributes,
  because a click handler without a role is what the a11y lint is for.
  → ROADMAP (finish + port)
- **DF-6 · Model-based copy** (D-15). Adopt GitHub Desktop's approach in both
  clients: rebuild clipboard text from the line model (immune to gutters,
  prefixes, wrapping, side-by-side interleaving, and native's tab expansion,
  since `line.content` keeps real tabs). ✅ *WS-E* for the Tauri interim —
  `user-select: none` on the prefix, joining the gutter that already had it, so
  an ordinary drag-select yields the file's own lines. Native has no interim, and needs none: no
  arrangement of `.textSelection` gives it a drag that spans lines at all
  (**D-22**), so the model-based run *is* the native answer rather than a
  refinement of one. The core helper that keeps the
  two byte-identical landed in WS-B (`copy_text`, exported as `copy_diff_text`)
  and ✅ *WS-P* gave it its consumer, natively. The rule both clients
  implement: **within a line, the characters the reader selected; across lines,
  those lines from the model** — a multi-line copy is a copy of *lines*, and
  the parsed diff is the only faithful source for one. Which gesture picks the
  lines is per-platform (FRONTEND §8), because SwiftUI hands back neither the
  extent of a `Text` selection nor a hook on the copy: natively the **gutter**
  is the line handle (click, ⇧-click, a context menu, ⌘C, Escape) while the
  content keeps its character selection, and the pane claims the Copy command
  only while a run exists. **The half left is Tauri's**, where the browser's
  own selection already spans rows: hang a `copy` listener off `.diff-body`,
  map the anchor and focus nodes to their rows' flat indices through a
  `data-line-index` attribute, and when they differ call `copy_diff_text` for
  that range instead of letting the browser answer. Two things to know going
  in: a `copy` handler must set `clipboardData` **synchronously** and the shim
  is `#[tauri::command(async)]`, so either `preventDefault` and write through
  the clipboard plugin after the await (the OSC 52 path already does exactly
  that, and a ⌘C is the user gesture WebKit wants), or keep a synchronous
  command for it; and a selection *inside* one line must be left to the
  browser, or copying two words would yield the whole line. → Tauri half open
- **DF-7 · Empty-parse reason** (D-12) — ✅ *WS-B.* `EmptyDiffReason` names the
  three situations one caption used to cover, and both clients render each
  honestly. The whitespace case needed the fused call to exist at all: when the
  filtered diff has nothing to render, core re-reads the unfiltered one to tell
  "unchanged" from "re-indented, and the setting is hiding it". A *load
  failed* variant turned out to be unnecessary — the fusion makes a failure an
  `Err` rather than an empty parse (see DF-10).
- **DF-8 · Header details.** ✅ *WS-E* for the minus sign: Tauri renders `−`
  (U+2212) in the file header's `−N`, the commit card's totals and the removed-line
  prefix, so both clients now use one glyph and STYLE.md carries the header
  rule it only had for rows. ✅ *WS-P*, and it corrected the Tauri half as well:
  a rename reads `old → new` under the file's name, sourced from
  `FileEntry.orig_path` and **not** from the parsed diff, because the diff
  cannot answer it — both reads pathspec-limit to the file's current path, so
  git never pairs it with the deleted counterpart and reports a rename as a
  plain add. Sourcing it from the diff also rendered `/dev/null → <file>` for
  every added file, which the Tauri header had been doing; core now answers an
  absent side as absence. Each count is drawn only when non-zero, so a binary
  file says nothing rather than `+0 −0`, and the counts are gated on the
  payload being a diff **of the file the header is naming**, which a seamless
  reload makes a real distinction. D-19's doubled `NoNewline` row was the
  fourth alignment; fixed in WS-A.
- **DF-9 · Slow-load presentation** (D-20). ✅ *WS-E, both clients.* Neither
  unmounts the old diff any more: it dims and takes a spinner overlay. Tauri
  through a `SeamlessDiffPane` wrapper shared by the two panes that had the rule
  written twice; native by making the threshold a modifier on `content` rather
  than a branch beside it — a branch was what destroyed the `ScrollView`'s
  identity, which is why the store's equality skip preserved nothing visible.
  FRONTEND §6.3 now also carries the scroll contract (*same diff → keep scroll;
  different diff → reset*), keyed on the rendered diff's own identity. ✅ *WS-P*
  took the half that was left, D-14.
- **DF-10 · Failure surface.** ✅ *WS-E.* Tauri clears the stale payload and
  states the failure inline in the pane, matching native. **The retry WS-D put
  on these two loads was dropped deliberately**, not lost: native has none, a
  button on one client only is the parity gap WS-D warned about, and the gesture
  survives anyway — the payload is cleared, so the loader's "already open"
  short-circuit no longer fires and clicking the row is the retry. FRONTEND
  §6.3 now says why this is outside §6.13's two classes: nothing is blocked, and
  it is the user's own task.
- **DF-11 · Dirty-submodule pane.** ✅ *WS-E, both clients.* Decided before the
  read in each: Tauri stopped fetching a diff it then discarded, and native —
  which had no branch at all and rendered git's raw `Subproject commit …-dirty`
  line, against STYLE.md's explicit rule — gained the pane and the skipped
  subprocess together.
- **DF-12 · Phase-2 debounce.** ✅ *WS-P.* `DiffStore.highlightDebounce` (80 ms,
  beside `slowLoadThreshold`) sits between the plain render and the tokenize, so
  arrowing down a file list tokenizes only the file the reader stopped on. A
  cancelled sleep resumes rather than throwing, so the guard after it is what
  stops the work: `generation` catches the reader moving on, `Task.isCancelled`
  catches the pane leaving the hierarchy — a tab change moves no generation.
- **DF-13 · Wrap break policy.** ✅ *WS-O — checked, and the risk is not real.*
  SwiftUI `Text` uses `byWordWrapping`, which Apple documents as wrapping "at
  word boundaries, unless the word doesn't fit on a single line" — so a
  minified or base64 run is broken by the platform rather than overflowed, the
  same outcome Tauri buys with `overflow-wrap: anywhere`. No code change, and
  deliberately none: the two candidate fixes were both worse. `.byCharWrapping`
  is not reachable from SwiftUI `Text`, and zero-width break insertion in the
  tab-expansion pass would put U+200B into every copy taken from the pane,
  which is exactly what DF-6 exists to prevent. Left as a visual check in the
  workstream's own testing list.

### 4.7 History (HI)

- **HI-1 · HEAD gating** — ✅ *WS-A.* D-1: both clients now gate the rewriting
  actions on `status.head_sha`, and the Tauri context menu no longer carries a
  row index at all.
- **HI-2 · Log windowing.** ✅ *WS-E.* **The append model is now both clients'**:
  the log is append-only and rooted at HEAD, `commits[0]` is HEAD by
  construction, paging drops nothing from the front, and a HEAD move re-reads
  from offset 0 at the depth the user has paged (capped at 500, oldest rows
  dropped) and scrolls to row 0. Tauri's bidirectional sliding window and its
  `windowStartOffset` scroll compensation are deleted with it — the window
  worked, and every defect around it existed because row 0 stopped being HEAD
  when it advanced (D-1's bug class, D-11's replacement-vs-slide signal, and the
  `skip > 0` gate that made a commit at offset 0 bump nothing). Re-reading from
  the top makes all three unrepresentable rather than fixed. `resetSeq` survives
  as "go to row 0" alone. **One deliberate reading of this entry's text**: it
  said "prepend", which neither client had and which only differs from a capped
  re-read past 500 loaded rows — the re-read also refreshes tag decorations on
  the visible rows, and §6.8's own invariant is *refetch*, so the simpler shape
  won. FRONTEND's §8 paging row is retired and §6.8 now carries the shared
  model; only page size stays per-platform (50 / 100).
- **HI-3 · Selection behavior.** ✅ *WS-E.* Tauri auto-selects the newest commit
  and re-seats on native's exact two conditions — nothing selected, or the
  selected sha is no longer in the list, which is what an amend or an undo does
  to it; it used to land on an empty pane and then keep rendering a
  rewritten-away commit's detail. Built as the `$derived` key + `untrack`ed
  `$effect` WS-D established, gated on History being the visible tab so the pane
  behind Changes doesn't spend a `git log` on a selection nobody sees.
  Right-click now selects the row it opens on, as this client's own `FileList`
  already did.
- **HI-4 · Loading/empty gating.** ✅ *WS-E* for Tauri: the detail pane tells
  "no history" from "nothing selected" rather than inviting the user to select a
  commit from a list that has none. Native still flashes "No commits" before the
  first `get_log` lands. → WS-Q
- **HI-5 · Relative dates.** Tauri re-ticks; native is a snapshot (its comment
  says so), and FRONTEND §8 now carries the difference. Add a visibility-gated
  10 s tick natively (reuse `BackgroundSchedulingPolicy` — don't invent a
  second gate) and retire that row; pin the
  tier vocabulary in FRONTEND §6.11 (the clients currently render "yesterday" vs
  "1 day ago"). Align the detail card's date format (Tauri's shows raw
  `toLocaleString()` seconds; both should use the abbreviated form — and
  both show the **author** date while DESIGN.md says committer; fix the doc).
  → WS-Q, §9
- **HI-6 · Commit-list keyboard.** ✅ *WS-E.* `FileList`'s `focusRowAt` ported
  verbatim — arrows, Home/End (⌘↑/⌘↓ on macOS), scroll-into-view, and the
  `tick()` ordering a virtualized list needs to focus a row that isn't mounted
  yet. It was the one list in the app where an arrow key did nothing.
- **HI-7 · Detail loads.** ✅ *WS-B* for the fusion: `get_commit_detail` is one
  `git log -1 -z --raw --numstat`, halving subprocesses per selection in both
  clients and removing the error-policy split — the files and the totals now
  come from one read, so neither can describe a different commit than the
  other. ✅ *WS-E* for Tauri's re-select, which used to blank the pane and
  refetch what it was already showing. Native's half is structural rather than
  guarded (`List(selection:)` writes the same value and `.task(id:)` doesn't
  re-fire), so it needs nothing there. Still open, both native: key the detail
  task on `(repoPath, sha)` not sha alone (`repoPath` is published before the
  new log lands, so the window is real; the `LoadKey` pattern is one file over),
  and clear `commits` on repo switch. → WS-Q
- **HI-8 · Paging.** ✅ *WS-E* for the three Tauri halves: a failed page is a
  `reportNotice` in the banner rather than a modal mid-scroll (the history on
  screen is still correct and scrolling re-asks); paging owns `log.isPaging`
  instead of the repo-wide `isLoading` that was disabling Commit on the other
  tab — which left `isLoading` with no writer at all, so it and its term in
  `canCommit` are deleted; and the detail card's trailer list is gone, since
  `%b` already ends in them. Still open, both native: zero prefetch margin
  (trigger at N−5) and E-7's full reload on row actions; tag chips, where the
  accent capsule diverges from STYLE's neutral treatment and from its own
  unpushed plate two lines away. → WS-Q
- **HI-9 · Checkout busy state.** Tauri holds the dialog with "Checking
  out…" and suppressed Escape; native dismisses instantly with no feedback
  and nothing preventing a second checkout. **Tauri right.** → WS-Q
- **HI-10 · Undo details.** ✅ *WS-E* for the ellipsis: "Undo last commit" now
  says what it does, and **"Checkout commit…" gained the one it had earned** —
  the rule is a promise about asking first, and applying it to only the item
  that broke it would have left the same menu inconsistent in the other
  direction. Still open, both native: copy Tauri's `lastHeadSha` re-seed after
  undo so the next poll doesn't redundantly refetch, and stop reloading branches
  after an undo (a `--mixed` reset cannot change the branch list). → WS-Q

### 4.8 Clone & gh (CL)

- **CL-1 · Reachability.** ✅ *WS-C* for Tauri: `Clone Repository…` is in the
  footer of the startup picker as well as the switcher, so the first-run user
  most likely to want it can finally reach it. ✅ *WS-L* for the transfer gate,
  **on both clients**: it disabled the control that opens the list, which took
  Clone away with it, and cloning contends with nothing since clone deliberately
  claims no slot in either client. The hold moved onto the rows, which is what
  RM-8 actually decided — the list opens, Clone works, only the switch is held.
  The blocked rows are dimmed and inert rather than `disabled`, because a
  disabled control takes no pointer events and the tooltip saying *why* would
  never appear.
- **CL-2 · List caching.** GitHub Desktop's shape — cache **plus** an
  always-visible refresh button — adopted on both. ✅ *WS-C* for Tauri: the
  once-per-run cache keeps its per-open speed and gains a Refresh button beside
  the filter, so a repo created since launch is no longer unreachable until
  restart; the filter already stayed live during loads. ✅ *WS-L* for native:
  `CloneStore` moved to `ContentView`, so it outlives the sheet and the list is
  the same once-per-run cache, with the same Refresh beside the filter.
  `hasLoadedList` is set only on success, so a failed load retries on the next
  open rather than reopening onto a stale error with no list behind it.
- **CL-3 · Keyboard.** ✅ *WS-C* for Tauri: Return clones from anywhere in the
  dialog when Clone is enabled — the `defaultAction` the native sheet has and
  this one, having no `<form>`, never did. In the gh list, Return on a row the
  cursor hasn't picked yet selects it and the *next* Return clones: one press
  would clone before the derived destination path had ever been on screen.
  ✅ *WS-L* for native, reaching the same rule by a different route: the field a
  tab lands on takes the caret, and ↑/↓ from the filter move the *selection*
  rather than a separate cursor — so the *Clones into…* preview follows the
  arrow keys and one Return is enough, because the path has already been seen.
  The rule both satisfy is "no clone starts before the user has seen where it
  lands"; the keystroke counts differ, and §9 records that. Flipping Tauri to
  match is the two-line change WS-C's entry already flagged.
- **CL-4 · URL/name derivation has drifted** (the "ported verbatim" pair):
  `.git`-on-shorthand handled differently, whitespace trimmed only natively
  (an untrimmed Tauri path creates literal `" …"` directories and persists the
  poisoned `last_clone_dir`), empty-destination preview shows `/repo` in
  Tauri. Both share two latent bugs (`owner/repo/` and scheme-less
  `github.com/…` enable Clone then fail). ✅ *WS-B*:
  `derive_clone_target(raw_url, parent)` and `clone_target_path(parent, name)`
  live in `core::repos`, matrix-tested, and both dialogs gate Clone on a real
  parse — so the preview and the button can no longer disagree about whether
  the app is about to succeed. Both latent bugs are covered, and the
  destination join keeps a bare root (`/`) that the old trailing-slash strip
  turned into a relative path.
- **CL-5 · Mid-clone state.** ✅ *WS-C.* Tauri left tabs/filter/rows/URL/
  destination editable, so clicking another repo mid-clone rewrote the
  "Clones into…" preview to a path nothing was being written to. One
  `<fieldset disabled>` now covers every control that chooses *what* to clone —
  a group rather than a per-control list a later control can forget to join —
  dimmed so the freeze is visible, with progress and errors moved outside it so
  the bar reporting the clone isn't dimmed by the clone.
- **CL-6 · Progress.** Native always shows motion (indeterminate for gh
  clones); Tauri shows nothing for a gh clone but the button label.
  ✅ *WS-B*: `gh repo clone … -- --progress` forwards to `git clone`, so
  `gh_clone` reuses the streaming seam a URL clone already had and both routes
  report real numbers. Nothing was wrong with the plumbing — nobody had passed
  the flag through.
- **CL-7 · Small deltas.** ✅ *WS-C* for Tauri: empty-state discrimination (you
  have none vs none matched), the description as a row tooltip, the *tab kept /
  inputs cleared* split across opens (it used to persist everything, including
  a stale selection with Clone already lit), and a name tiebreak on the
  recency sort. Its per-tab error state needed nothing — `selectTab` already
  clears the clone error and a gh-list failure is its own state inside the
  list. ✅ *WS-L* for all five native halves: the empty-state split, the per-tab
  error clear, the *tab kept / inputs cleared* reset (`reopen()`, which also
  keeps the list and the sort mode — the cache is the point), `NameCollation`
  for a case- and diacritic-blind order with an explicit tiebreak after it, and
  filter-then-sort recomputed on its three inputs rather than per body pass.
- **CL-8 · `check_auth`.** Tauri spawns `gh auth status` on every launch to
  write a field with **zero readers** (the PR feature that consumed it was
  retired); the FFI deliberately doesn't export it and gh's own error text
  ("Run `gh auth login`") is the better UX. Delete the call + wrapper; drop
  the command from FRONTEND §3's tables (and its registered count) or record
  the exemption. → WS-S

### 4.9 Settings, config, AI (ST)

- **ST-1 · The field matrix.** All 15 `Config` fields audited. ✅ *WS-B* for the
  dead fields: the AI timeout now travels on `AiProviderConfig` and bounds both
  providers' requests — a control that persisted a value nobody read was worse
  than no control, because the user believed the timeout was set — and
  `ai_api_key`, mapped but read by neither provider, is gone. ✅ *WS-C* for the
  cross-client read (D-6). ✅ *WS-H* for live-apply: every field either
  re-renders from the `config` store or, for `auto_fetch` /
  `fetch_interval_ms`, re-arms the timer from an effect watching those two —
  the one setting in the window that used to be ignored until a restart, and
  the reason native's "no restart needed" footer could not be ported honestly
  before. This is BG-2's *first* half, taken here because ST-3 could not ship a
  true footer without it; ✅ *WS-J* replaced the flat intervals underneath with
  the self-scheduling chain, and the effect now reschedules a loop rather than
  restarting a timer. **Still open, native:** no control for the two AI
  timeouts, so they are Tauri-set and natively honoured (now a FRONTEND §8
  row). → WS-R
- **ST-2 · Save semantics** — ✅ *WS-B.* `patch_config` is the only writer, and
  it reads-edits-normalizes-writes under a lock the shared file never had, so
  a surface can only change the fields it names. `Config::normalized()` runs on
  both the read and the write, which is what heals an already-poisoned file on
  first load. Both clients' whole-object writes are deleted; so is
  `save_config`'s export.
- **ST-3 · Surface model.** ✅ *WS-H* for the Tauri half. Each control patches
  its own field through `patch_config` as it changes — discrete controls on the
  click, text and numeric fields on `change`, which is already "blur or Return"
  — the whole-object save is deleted, and the footer is a single **Close**.
  What Save bought was a way to be wrong: it posted every field the form held,
  so a `tab_size` written by the other client while the overlay stood open was
  reverted by an unrelated toggle, and Cancel promised to undo edits that had
  already reached the file. The half-typed-value risk it claimed to guard is
  covered by clamp-on-write plus commit-on-change, and native's per-section
  footers are ported verbatim in native's own section order. Two rules the
  build added, both consequences of having no Save: a **failed** write puts its
  control back (a control showing a rejected value with nothing pending is
  claiming a setting that isn't on disk), and a clamp that lands on the value
  already displayed re-seeds the form anyway — otherwise 999 typed into a field
  already at its maximum stays on screen, which is the one case the
  "corrections are visible" promise exists for. **Still open, native:** its
  patch names every field the window holds rather than the one that changed
  (D-5 at form scale — see §6's WS-H entry), and it should adopt Tauri's
  load-failure handling rather than rendering editable defaults that aren't the
  user's settings. D-8's lost text edit is already fixed. → WS-R
- **ST-4 · Units and bounds.** ✅ *WS-B* for the bounds: `config_bounds()` is
  the one declaration, read by both forms and enforced by the one writer, so a
  control can no longer offer a value the writer then clamps away (native's
  load-clamp floored to 1 s while its own control started at 5). ✅ *WS-H* for
  the units: the Tauri interval field reads in seconds like native's, with its
  `min`/`max` divided from the same bounds, so milliseconds are now the config
  file's business alone.
- **ST-5 · `ai_provider` ownership.** Native has **two independent owners**
  (composer's CommitStore and SettingsStore) that never observe each other —
  with both windows open the pickers can disagree, and a Settings save of any
  unrelated field silently reverts a composer-side provider change. Tauri's
  single `$config` store is the shape to copy.
  Route both native surfaces through `AppConfigStore` (which exists precisely
  to be the single owner — and grow it the `scanPaths` accessor three other
  call sites currently bypass it for). → WS-R
- **ST-6 · AI mapping duplication** — ✅ *WS-B.* `ai::provider_config` and
  `ai::load_ai_config` live in core; the bridge is the delegation it always
  claimed to be, the TS copy is deleted, and both clients call
  `load_ai_config` per generate. The three behavioural differences between the
  old copies go with it — including the one that mattered: the model and the
  server URL now always belong to the provider actually about to run, rather
  than being spliced from a picker value over a separately-loaded config.
- **ST-7 · Provider save failure.** ✅ *WS-B*, ahead of the workstream it was
  scheduled to — `patch_config`'s adoption rewrote `setProvider` and brought the
  revert with it. WS-D found it already done and narrowed it: the rollback puts
  back the one field rather than the whole config snapshot, which would have
  reverted everything the store learned meanwhile — D-5's lost update, one layer
  up. ✅ *WS-H* for the Settings overlay: instant-apply made the revert a rule
  rather than one picker's special case — any control whose write is refused is
  re-seeded from the config on disk, the provider picker included. **Still open,
  native:** a failed save there leaves the control showing the value that didn't
  land, and clears only on the next successful write. → WS-R
- **ST-8 · One model field, two providers.** Set `sonnet`, switch to Ollama,
  Generate fails — a shared design flaw; GitHub Desktop stores per-provider
  models. **Decided: split per provider, restructured cleanly** —
  backwards compatibility is explicitly waived (sole user; the config file
  can be regenerated), so the config gains a real per-provider shape (a
  Claude section with model + timeout, an Ollama section with model + server
  URL) instead of a migration shim; `ai_api_key` goes away in the same pass;
  unknown old keys keep parsing (`config_ignores_retired_keys`). ✅ *WS-B*, UI
  included: both Settings surfaces show the selected provider's own fields, so
  switching provider in the composer never requires a Settings trip. Field
  order in `Config` is now load-bearing — a TOML table swallows every key after
  it, so nothing scalar may be declared below those two (pinned by a
  round-trip test).
- **ST-9 · `check_provider_status`.** **Decided: use it rather than delete
  it** — a dead command path is exactly how BR-1 rotted. ✅ *WS-D*, both clients:
  the probe (`ProviderStatus { ready, reason, fix_command }`) plus
  `provider_status_from_failure`, the status strip, and the offer to run the fix
  command in each client's own terminal. Details in FRONTEND §6.7 and TECHNICAL.
  It took **four** corrections across three visual checks, and none of them was a
  bug in the code — each was a wrong model of the problem. Read them before
  building any other readiness gate:
  - **`claude --version` answers the wrong question.** It proves the binary
    exists, not that it will answer. Hence the second question, `claude auth
    status`, read from its JSON payload rather than its exit code.
  - **A probe cannot see an expired session at all.** Signing out *deletes* the
    credentials; an expiry leaves them on disk, so `auth status` still reports a
    signed-in CLI and only a real request discovers the refresh failed — with a
    different message (`Not logged in · Please run /login` vs `Failed to
    authenticate: OAuth session expired…`). **Testing with `logout` proves
    nothing about the expired case.** That was the trap, and the user caught it.
  - **Chained `{:else if}` made the remedy unreachable.** It was written after
    the error block, so on a failed generate — the one case it existed for — it
    was structurally impossible to render. A conditional whose sibling is *also*
    true in the case that matters is not a chain.
  - **A fix shipped to one client is not shipped.** The whole first pass was
    Tauri-only while the parity plan's own subject is the two clients agreeing.
    Port in the same change, or the gap is what the user finds.
- **ST-10 · Scan-path editor.** **Decided: locked by default on both clients** —
  read-only field, **Edit** button beside it, which becomes **Done**; Done
  parses, applies through `patch_config`, and locks again. Nothing touches the
  config until Done, so leaving mid-edit discards the draft; no confirmation
  anywhere. ✅ *WS-H* for the Tauri half, which also moved parsing to Done (the
  old parse-on-input desynced the textarea from the model on every keystroke)
  and made the applied edit re-walk discovery itself. **Still open, native:**
  the plain field, plus `.monospaced()` on it. → WS-R

### 4.10 Terminal (TE)

- **TE-1 · Key routing** — ✅ *WS-A* for the routing half (D-16: the shell owns
  every key but the panel's toggle, FRONTEND §6.11), ✅ *WS-I* for the toggle's
  modifier: ⌃` only, in both the window handler and `Terminal.svelte`'s custom
  key handler, which are one rule written twice — and above `MainLayout`'s
  `inField` bail, which had been swallowing it in the commit composer since the
  chord existed (native binds a key equivalent, so it never had the fault). What
  is left is not the terminal's: WS-A's modifier-blind rule leaves ⌘,/⌘B/⌘L/⌘R/⌘P inert with the
  terminal focused, which is right for `Ctrl` (the shell really does want
  `Ctrl+R`) and wrong for `Cmd` (no shell consumes it, and macOS reserves ⌘, for
  Preferences). **Decided by the user: the modifier follows the platform** — ⌘ on
  macOS, Ctrl on Windows and Linux — which resolves the capture as a side effect
  rather than as a second rule. **Not scheduled**: correct on the shipping
  platforms, imperfect only on a macOS Tauri build, so it reopens if noticed.
  ROADMAP carries the decision with the affected-chord table.
- **TE-2 · Transport** — ✅ *WS-B* for the core half (bounded reader→emitter
  channel, back-pressure-driven coalescing) and ✅ *WS-I* for the host half: the
  session's stream is a frontend-created `Channel` passed into `start_terminal`,
  which closes D-4 structurally and mirrors the bridge's seam;
  `start`/`resize`/`close` became `(async)` and `write` deliberately did not.
  **The coalescing is still the only batching** — WS-I added none, per WS-B's
  note.
- **TE-3 · Collapse/resize** — D-9 (native emulator reflow; fix by pinning
  the inner frame) and the missing native 80 ms resize debounce (a divider
  drag is one SIGWINCH per column crossed today — put the coalescing in
  `TerminalController.resize`, not the delegate, to keep the one-shot
  initial-size push). ✅ *WS-B* for the core half: `resize_terminal` ignores a
  `< 2×2` grid itself, so no host can announce a collapsed panel to the PTY —
  which leaves D-9 as purely the native inner-frame pin. → WS-R
- **TE-4 · Scrollback.** 500 (native) vs 1000 (Tauri) — both library
  defaults, neither chosen. Set 1000 explicitly on both (`git log --stat`
  exceeds 500; VS Code ships 1000). → WS-R
- **TE-5 · Links + OSC 52.** ✅ *WS-I* for the Tauri half, and the rule is
  FRONTEND §6.17. Modifier-click on both (⌘ on macOS, Ctrl elsewhere), with the
  convention taught on hover rather than the modifier dropped — which turned out
  to be mandatory, not merely nice: xterm's link addon cannot make its own
  underline conditional, so a gated link with no affordance simply looks broken.
  OSC 52 is honoured write-only in both clients; the read form is swallowed, not
  declined. **Native half left, and it leaves Tauri ahead** — per WS-E's rule,
  stated rather than left to be discovered: SwiftTerm already has ⌘-click and
  OSC 52, so the *behaviour* matches and only the hover affordance is missing
  there. It was not taken here because SwiftTerm's hover surface needs an API
  check first (a tracking-area overlay if it exposes none), which is native UI
  work in a Tauri workstream. → WS-R
- **TE-6 · Refocus.** ✅ *WS-I.* The Tauri panel takes the caret back when the
  window returns, reading `document.activeElement` at that moment rather than
  latching a `focusin` flag; AppKit already did it natively.
- **TE-7 · Small parity.** ✅ *WS-I* for three of four: the header strip reads
  `Terminal` before a session names itself, `280` means the *emulator* in both
  clients (the header sits above it — rows are what the number is really
  setting), and the shell preference is already fresh per session in Tauri via
  the config store's activation re-read, so no second read was added. ✅ *WS-M* for
  the fourth: ⌃` moved to View ▸ Show/Hide Terminal, which turned out to be a
  correctness fix rather than a filing one — a key equivalent on a *button* is
  matched through the responder chain, so SwiftTerm could swallow it in the one
  place the chord is most wanted.

### 4.11 App shell (SH)

- **SH-1 · CLI launch, single-instance, init prompt.** ✅ *WS-M* for the app's
  half. `CFBundleDocumentTypes` declares `public.folder`, so `open -a LeoGit
  <dir>` activates the running instance and delivers the folder to
  `application(_:open:)` — cold and warm through one callback, with Finder's
  *Open With* and a drop on the Dock icon free. `resolve_launch_target` and
  `init_repo` are exported; the target outranks `last_opened_repo`; the
  *Create a repository here?* prompt lives on the root view, so it works over
  the picker and over an open repository. Two exports the entry asked for were
  deliberately **not** taken: `is_git_repo`, which has no native consumer
  (`resolve_repo_root` already validates the restore) and would be dead surface
  the FFI's own rule forbids, and the `set`/`take_pending_launch_target` pair —
  see WS-M's §6 entry.
  **`install.sh` is unfinished and needs a decision** (§6, WS-M's findings):
  the entry's premise was wrong twice over — the shell function points at a
  *path*, not a bundle id, and its `open -na … --args` form reaches argv rather
  than the document callback, so the plan's own suggested command would work
  only on a cold start. The deeper problem is that `install.sh` installs the
  Tauri release into that path and the native app has no release artifact, so
  there is nothing for a branch to key on. → the open question in WS-M's entry
- **SH-2 · Menu bar as the discovery surface.** ✅ *WS-H* for the Tauri half:
  ⌘1/⌘2 select the two tabs absolutely, beside the ⌘L toggle rather than
  replacing it — the toggle is the one people have, and an absolute binding is
  what makes the pair worth learning. ✅ *WS-M* for native: File ▸ Clone
  Repository… (⇧⌘O — **not** File ▸ Open, which RM-2 rules out), View ▸ ⌘1/⌘2,
  View ▸ Show/Hide Terminal, View ▸ Refresh, and a Branch menu rendering the
  toolbar control's own items from one shared definition. The pattern is the
  one the entry named: a state-dependent item is a focused scene value
  published from the *window content*, since one set inside `.toolbar` never
  reaches the scene.
  **⌘B was not bound, and should not be**: AppKit matches a key equivalent by
  walking *into* submenus, so no chord opens a menu, and putting ⌘B on one of
  the Branch menu's items would give the same chord different meanings in the
  two clients. Recorded as a FRONTEND §8 row; the menu itself is the
  discoverability the entry was after. Tauri on macOS should eventually get a
  real `tauri::menu`; out of this plan's scope beyond recording it.
- **SH-3 · ⌘R.** ✅ *WS-F.* Tauri's is now native's: status + log + branches,
  guarded on `activeNetworkOp`, and reachable from a focused field — which it
  had to become, since SY-1 removed the Refresh button and left ⌘R as the only
  route to a forced reload.
- **SH-4 · Escape.** ✅ *WS-G* for the half that turned out to be silently
  broken: every Tauri confirmation handled Escape on its own overlay element,
  and not one of them took focus when it mounted, so **no confirmation in the
  client could be dismissed by keyboard**. ✅ *WS-H* for the fold: the two
  hand-written, already-drifted lists of overlay flags are gone, and each
  surface registers its own dismissal on one LIFO
  (`actions/overlayStack.ts`) for as long as it is mounted — registration order
  *is* stacking order, so there is nothing to keep in step and nothing to
  forget. The `stopPropagation()` workarounds WS-G added went with it, and the
  context menu, which had neither a workaround nor a place in either list,
  stopped closing itself *and* the popover underneath. Native's per-surface
  AppKit handling is fine and was not touched. The same registration replaced
  `modalOpen` — see §6's WS-H entry for what that list had been missing.
- **SH-5 · Error model.** Split by class in both — the ruling and the Tauri half
  are in CH-11. Native's remaining half: its background banner still has no
  dismiss ✕, and the classes it puts in the banner that the rule puts in the
  modal. → WS-N, WS-Q
- **SH-6 · Window.** ✅ *WS-H.* `tauri-plugin-window-state` saves the frame on
  exit and restores it at launch, so `tauri.conf.json`'s 1280×800 is the
  first-run default rather than every launch; the title is the open repo's
  folder name, native's own value, falling back to the product on the picker.
  Min-size disagreement (720×460 vs 900×600) was left as it is.
- **SH-7 · Tab behavior.** ✅ *WS-H* for the Tauri half: `resetRepoState` keeps
  `activeTab`. Which tab is showing is a view preference, not repository state,
  and resetting it was an accident of reusing `defaultState` for the reset.
  Native loses the commit-list scroll position on tab round trips (Tauri keeps
  both panes mounted — its trade); close it with a `ScrollViewReader` restore
  to the hoisted selection instead of keeping subtrees alive. → WS-Q
- **SH-8 · Pre-main phases.** ✅ *WS-L.* A failed walk sets
  `RepoDirectoryStore.discoveryError`, which both native lists render as one
  inline row with a Retry above whatever the last successful pass found —
  deliberately not a phase swap: those rows still open, and an error screen in
  their place would take the repositories away along with the bad news. Native's
  deliberate silence about a missing restored repo stays.

## 5. Core-hoist catalogue

Everything above that moves into `leogit-core`, collected. Rule of thumb
applied: hoist when the logic is pure, duplicated (or about to be), and
IPC-cost-free; keep per-platform when it's presentation or host-lifecycle.
None of these sacrifice measurable performance; several *save* subprocesses.

Eighteen shipped in WS-B, H-3 in WS-F and H-20 in WS-J. The one left is H-17:
three OS backends that cannot be verified from macOS (→ WS-K, a workstream of
its own).

**H-20 shipped as a pure function the client calls only when it has something
to ask about**, which is the answer to the IPC objection that deferred it. The
reconciliation needs the excluded set, the present paths and the time elapsed;
the first is almost always empty, so the client short-circuits and the crossing
never happens. That is a third shape worth remembering beside H-3's: a hoist
whose cost is real but *conditional* can be paid only in the condition, rather
than folded into a call that always happens.

**H-3 shipped as a field, not a command, and that is the rule above doing its
job.** A `sync_proposal` command would have been IPC-cost-free for native (an
in-process UniFFI call) and expensive for Tauri: a crossing per poll tick,
carrying the whole file list up, to run six comparisons. Carried on
`RepoStatus` instead it costs both hosts nothing, cannot be forgotten by a
refresh path, and has exactly one route — the same three reasons H-1 folded
`merging` in, and the same reason BR-9 deleted the standalone `is_merging`
rather than keeping it beside the field.

| # | Hoist | Replaces | Feeds |
|---|---|---|---|
| H-1 | ✅ `RepoStatus.merging: bool` filled by `get_status` | one subprocess per tick per client (E-1) + the forgot-isMerging bug class | shipped |
| H-2 | ✅ `get_remote` returns no-remote honestly (`Option`); `DEFAULT_PUBLISH_REMOTE` carries the assumption at the one call site that creates a remote | D-2's dead guard, D-18's doomed fetches | shipped |
| H-3 | ✅ `sync_proposal(&RepoStatus) -> SyncProposal`, filled into `RepoStatus.proposal` by `get_status` (titles/icons stay per-platform) | native `SyncControls`'s own ladder + Tauri's three loose booleans; makes ROADMAP's force-push-recommended a one-place change | shipped |
| H-4 | ✅ `derive_clone_target(url, parent)` + `clone_target_path`, matrix-tested | the drifted TS/Swift pair + two shared latent bugs (CL-4) | shipped |
| H-5 | ✅ `match_repo` + the batch `filter_repos` both hosts call | two implementations already drifted on input set (RM-5); gives Tauri the home-dir root free | shipped |
| H-6 | ✅ `known_repos(scan_paths, depth)` (discovery ∪ existence-checked MRU) | Tauri's forgotten rows + native's dead-MRU tiering (RM-3) | shipped |
| H-7 | ✅ `get_commit_detail(repo, sha)` (one `git log -1 -z --raw --numstat`) | two subprocesses per commit selection per client + the error-policy split (HI-7) | shipped |
| H-8 | ✅ `get_parsed_diff` / `get_parsed_commit_diff` (fuse read+parse); `DiffOptions` splits parse from render; `DiffLine.text: Option` | E-8, half of Tauri's per-diff IPC, the double traversal in both (DF-3) | shipped |
| H-9 | ✅ `EmptyDiffReason` (`NoChanges` / `WhitespaceOnly` / `NoTextualChanges`) | D-12 + native's generic copy (DF-7) | shipped |
| H-10 | ✅ `patch_config` + a config write lock + `Config::normalized()` + `config_bounds()`; blank-means-absent everywhere | D-5, D-7, D-17 structurally | shipped |
| H-11 | ✅ `ai::provider_config` / `load_ai_config` in core; per-provider config sections; `timeout_secs` wired into both providers; `ai_api_key` dropped | the duplicated mapping + the untestable pin (ST-6) + the shared-model footgun (ST-8) | shipped |
| H-12 | ✅ `classify_discard(repo, files) -> DiscardPlan` from core's own `ls-tree` logic | native's guessed dialog copy; upgrades Tauri's generic copy free (CH-8) | shipped |
| H-13 | ✅ `FileStatus::letter()` / `label()` + the `file_status_styles()` table both hosts read once | three-way glyph drift (CH-4) | shipped |
| H-14 | ✅ Terminal reader→emitter split with a bounded channel (flow control) and back-pressure-driven coalescing; `resize_terminal` ignores `< 2×2` | E-11/E-12 transport waste, native's unbounded relay, the FitAddon-internals dependency (TE-2/TE-3) | shipped |
| H-15 | ✅ `DiffSizeGuard` (4 MiB total / 5 000 bytes per line) with a `show_anyway` escape | the missing size guard in both (DF-4) | shipped |
| H-16 | ✅ `copy_text(file_diff, start, end)`, consumed natively in WS-P | byte-identical clipboard in both (DF-6) | shipped; Tauri's caller outstanding |
| H-17 | `core::net` connectivity observer emitting online/offline over the event seam — Linux netlink backend first, then macOS/Windows | Tauri's hard-wired `navigator.onLine` on WebKitGTK; eventually native's separate `NetworkPathObserver` (BG-3) | WS-K |
| H-18 | ✅ `gh_clone` through the `git clone` streaming seam (`gh repo clone … -- --progress`) | the progress-less gh clone in both clients (CL-6) | shipped |
| H-19 | ✅ `fetch(.., background)` picks the 8/8/12 s budget for automatic fetches | an automatic fetch holding the single slot on the 15/30/600 s user budget (BG-8) | shipped |
| H-20 | ✅ `exclusions::reconcile_exclusions(&[Exclusion], &[String], elapsed_ms)` + `EXCLUSION_GRACE_MS` / `EXCLUSION_GRACE_READS`, 10 tests; the window is wall-clock *and* a consecutive-miss count, because the cadence ladder breaks either term used alone | the two hand-written, already-drifted exclusion rules (CH-7) | shipped |

Deliberately **not** hoisted: sort collation (locale into a chrono-free core —
no), relative-date formatting (platform), scheduling policy (host lifecycle),
tab-expansion (single consumer), disambiguation labels (small, but H-6-adjacent
if it ever grows), keyboard-cursor index arithmetic (four lines, and a crossing
per arrow key would be absurd — the two copies are named the same and carry the
same doc, which is the cheaper guard here), and the per-status *colour* (H-13
hoists the glyph and the name; the tint resolves against each host's own
palette).

## 6. Workstreams

Per CLAUDE.md: one at a time, user-flow order inside each, visually verified
before the next starts. Every workstream ends with the §8 gates. Sizes are
relative (S/M/L).

**Sizing rule, learned from WS-B.** A workstream is *one client, one or two
adjacent feature areas, roughly a dozen item-halves* — small enough that one
agent holds the whole surface in context and the user can walk its visual check
in a single sitting. WS-B broke that: it was a core-wide layer touching every
area of both clients at once, and while it landed, it was too large to review
as one piece. WS-A's shape (thirteen items, one theme) is the target instead.
A heavy single item counts for several — BR-1's merge port, DF-2's split view,
TE-2's transport rewrite, SH-1's launch path and H-17's three OS backends each
fill most of a workstream alone, so each gets one.

Order is user flow *within* each client — how you get into a repo, then what
you do there, then the machinery underneath. The Tauri block runs first for the
reason it always did: that client is further from the shared bar and its work
is mostly adoption of already-proven native behavior.

1. ✅ **WS-A — Defect burn-down. Shipped 2026-08-27.** Thirteen items: D-1,
   D-2's client gate, D-3, D-8, D-10, D-11, D-12, D-13, D-16, D-17, D-18, D-19
   and RM-10, each summarized in §3.1. D-5 and D-7 went to ground with their
   hoists in WS-B; D-6 shipped early in WS-C; D-4, D-9, D-14, D-15 and D-20
   stayed with the areas they belong to (D-20 closed in WS-E, D-4 in WS-I).

   **Still live.** *A gate is only as good as what it can see* — the workstream
   added `log.resetSeq`, `repoState.statusLoaded`, `RepoStore.awaitLoadSettled()`
   and `SyncStore.silentFetch`'s `Bool?` return for that one reason.
   `apps/tauri-app/src/lib/utils/keyboard.ts` holds D-16's terminal-origin rule;
   anything binding a window-level chord goes through it.
2. ✅ **WS-B — Core convergence layer. Shipped 2026-08-27.** Eighteen of the
   twenty hoists (per-hoist state in §5), with tests, regenerated bindings, and
   adoption in **both** clients — each replaced its duplicates rather than
   sitting beside them. D-2's dead guard, D-5 and D-7 went to ground with their
   hoists; D-17's clamp became structural and D-12 gained its reason. Four host
   surfaces were deleted rather than kept for compatibility (`is_merging`, the
   four raw diff readers, `get_commit_files`/`get_commit_stats`,
   `discover_repos`), along with the drifted `repoSearch.ts` /
   `RepoSearch.swift` pair.

   **Still live.**
   - **`ConfigPatch` fields default to "leave it alone"** (`#[uniffi(default =
     None)]`), so a one-field write is one line, and clearing an optional field
     is patching it to `""` — the blank-means-absent rule doing double duty.
     It is what made ST-3's instant-apply form a change per control rather than
     a redesign, and what WS-R's native half still has to adopt.
   - **`config.toml` field order is load-bearing.** `toml` serializes in
     declaration order and a table swallows every key after it, so nothing
     scalar may be declared below `[claude]` and `[ollama]`. A round-trip test
     pins it.
   - **H-14's coalescing is back-pressure-driven, not a fixed window**, and is
     therefore self-tuning: nothing is held when the host keeps up. WS-I's
     `Channel` rewrite inherited it and added no second layer, which still
     stands as the rule; the native relay's own coalescing is redundant but
     harmless (WS-R may simplify it).
   - **`resize_terminal` ignores a `< 2×2` grid itself**, so D-9 (WS-R) is purely
     the native inner-frame pin.
   - **H-16 had no consumer for six workstreams.** `copy_text` /
     `copy_diff_text` were this plan's one deliberate dead surface; WS-P wired
     the native side to them (DF-6), and the Tauri `copy` listener is what
     finishes the job. A hoist landed ahead of its caller is survivable exactly
     as long as the workstream that owes the caller is named.
3. ✅ **WS-C — Tauri repo switcher & clone. Shipped 2026-08-27.** RM-3's
   remainder, RM-4, RM-7(T), RM-8, RM-9, CL-1/2/3/5/7(T) and **D-6**; RM-2 was
   decided against instead, in *both* clients. Clone reached the startup picker,
   `last_opened_repo` restores on the path still being a repository, discovery
   re-walks on switcher-open and Settings-close, both lists share one empty
   state, the switcher sorts active → MRU → name and locks mid-transfer, config
   is re-read on activation, and the clone dialog gained Return-to-clone, a
   Refresh button, a mid-clone freeze and a *none / no matches* split.

   **Still live.**
   - **A repo list is exactly what the scan paths cover** (RM-2). A workstream
     that finds a repository unreachable reaches for discovery or the scan-path
     setting, never a second way in. There is no longer an exception in either
     client. `resolve_repo_root` is FFI-only; the Tauri launch restore uses
     `is_git_repo`, the cheap existence check it wants.
   - **Discovery re-walks where the setting changes, not where the dialog
     closes.** WS-H removed both hosts' `closeSettings` handlers and hung the
     walk off the `scan_paths` / `scan_depth` patch instead, which is why the
     rule survived a dialog rewrite: a hook on dismissal is a hook someone can
     route around.
   - **`RepoListEmptyState` is the shared empty state for both repo lists, in
     both clients** — same three answers, same strings. Change them in the two
     components, never in a copy.
   - **The `<fieldset disabled>` freeze is the pattern for uncancellable
     dialogs** (STYLE.md). Reset `min-inline-size: 0` or it refuses to shrink and
     widens the dialog; keep progress and errors outside it.
   - **`$effect` tracks every reactive read, including ones inside a branch.**
     Read the trigger, `untrack` the rest — WS-D generalized this into the
     derived-key form in its entry below.
   - **One deliberate divergence from this plan's text.** CL-3's
     Enter-on-row-clones is **two presses** in the clone dialog (the first
     commits the cursor's row as the selection, the second clones), because the
     destination path is derived from the selection and one press would clone
     before the user had seen where it lands. Flipping to one press is a two-line
     change in `handleListKeyDown` if the user prefers it.
4. ✅ **WS-D — Tauri changes tab & composer. Shipped 2026-08-27.** CH-13,
   CH-3(T), CH-4(T), CH-9(T), CH-10(T), CH-11 + SH-5(T), CH-12 and ST-9's
   Generate gate **on both clients**; ST-7 turned out to have shipped in WS-B.
   The changes list opens its first file and heads its rows with "N of M files
   included", the status letter sits on its 18×18 tinted plate, ⌘↩/⌘G are
   window-wide, and the composer can no longer clip its own Commit button. The
   largest piece is the error split: **`reportActionError` / `reportNotice` in
   the repo store**, with `ErrorModal` finally receiving the `onRetry` it always
   accepted. ST-9's four corrections are recorded in its §4.9 entry.

   **Still live.**
   - **Ship both clients in the same change.** ST-9's first pass was Tauri-only,
     and the user's next message was that nothing had appeared natively. A fix
     that lands in one client is a new parity item, not a finished one.
   - **Never clear state on the way *into* an async refresh.** Write the answer
     when it arrives and hold the old one until then, tagged with what it
     describes, so staleness is a comparison rather than a clearing step someone
     forgets. Anything that refetches on focus has this shape.
   - **One strip per surface, not one per source.** A new failure joins a
     surface's existing slot rather than stacking beside it.
   - **Check the code before implementing an inventory item.** WS-B's hoists
     reached further into the clients than §5 records, and items still marked
     open in the areas it touched are often already half-done.
   - **STYLE.md keeps turning out to be ahead of the code.** Three workstreams
     have now found their target already written there. Read STYLE for the
     surface you are about to change before designing it.
   - **Failure classification is two functions, not a shape you re-copy.** A new
     call site picks `reportActionError` or `reportNotice`; a third path is a sign
     the classification is being re-litigated at the site. (WS-F added the
     refinement: a failure raised *inside a dialog* stays there — FRONTEND §6.13.)
   - **"Is something on top?" was a hand-kept list of overlay flags, and a new
     overlay joined it or joined nothing.** WS-H replaced it with the overlay
     stack's own depth after finding four dialogs had never joined.
   - **A window-level chord that acts on a component reaches it by `bind:this`
     plus an exported function**, not by lifting the component's state — one
     gate, two entry points.
   - **Auto-select is a `$derived` key read by an `untrack`ed `$effect`.** The
     effect must not read the polled store directly or it re-runs every tick.
     Anything reacting to a polled store should be built this way.
   - **Measure a pane through a wrapper, not the pane** — a `display: none` tab
     pane reports zero height.
   - **Native's error split still disagrees with the rule this plan wrote.**
     Discard, checkout and undo failures go to native's banner; §6.13 puts them
     in the modal. WS-N and WS-Q close it, and native's banner needs its dismiss
     ✕ in the same pass.
5. ✅ **WS-E — Tauri history & diff panes. Shipped 2026-08-27.** HI-2, HI-3,
   HI-4(T), HI-6, HI-7(T), HI-8(T), HI-10(T), DF-5's interim, DF-6(T), DF-8(T),
   DF-10 — plus **DF-9 and DF-11 on both clients**, and D-20 closed with them.
   The log became an append-only list rooted at HEAD, deleting the sliding
   window and the three defects that only existed because its top could drift;
   the list gained arrow keys, selection-on-right-click and native's
   auto-select/re-seat rule; paging left the repo-wide loading flag and its
   failure left the modal; and both diff panes stopped blanking themselves on a
   slow load and started stating a failed read inline.

   **Still live.**
   - **A plan entry is a decision, not a specification, and the code gets a
     vote.** Read what the clients actually do before building what an entry
     describes — three of this workstream's items were already half-true.
   - **Deleting the mechanism beats fixing it.** Three separate items were all
     the sliding window's shadow. When several items in one area keep pointing at
     one mechanism, price replacing it before patching each.
   - **A flag with no writer left is a flag to delete.** Worth a sweep after any
     item that re-homes state.
   - **Check which direction a "one client half" would leave the clients pointing
     before deferring the other one.** Shipping DF-9/DF-11 Tauri-only would have
     put Tauri *ahead*, which is the same parity gap reversed.
   - **The retry a previous workstream added is part of the surface you are
     replacing.** State whether the gesture survives, either way.
   - **Applying a rule to only the item that broke it leaves the surface
     inconsistent in the other direction.**
   - **`.opacity` + `.overlay` is a modifier; a `ProgressView` branch is a
     different view.** Any native "keep it on screen while it reloads" is a
     modifier on the same view, never a sibling branch.
   - **Two branches beat conditional a11y attributes.** `{#if}` around the whole
     block is what keeps `pnpm check` at zero.
   - **Re-verify a count you are about to cite rather than carrying it** —
     FRONTEND was contradicting itself on its own command and DTO counts.
6. ✅ **WS-F — The sync ladder, in core and on both clients. Shipped
   2026-08-28.** **H-3** plus SY-1 through SY-9 and SY-11's indeterminate case,
   with SH-3. The ladder is now `core::git::sync_proposal`, a total function of
   `RepoStatus` carried on `RepoStatus.proposal`; native deleted its own copy and
   Tauri's Pull + Push-split + Refresh collapsed into one adaptive button, with
   both counts on it, Fetch and force-push under a chevron that only appears
   where it adds something, ⌘P running whatever is proposed, and ⌘R promoted to
   the full reload. Post-transfer became status **+** log in both. Per-item state
   is in §4.3.

   **Still live.**
   - **§5's "IPC-cost-free" test can decide a hoist's *shape*, not just whether
     to hoist** — the reasoning is under §5's H-3 note. When a hoist's two hosts
     pay differently, look for the shape that makes it free for both before
     gating the derivation behind a key.
   - **Fill a derived status field at one exit, not at each return.**
     `get_status` is a wrapper around a private `read_status` with three returns;
     filling `proposal` in the wrapper is what makes "an early return forgot"
     unrepresentable. Any future field of this kind uses the same shape.
   - **Collapsing controls is what removes their defects.** SY-2, SY-3 and SY-9
     were three filed items and one cause: three controls answering "what now?"
     independently. WS-G found the same shape a second time in BR-1/BR-2.
   - **The interim a plan entry asks for is only worth shipping if it will be
     visible.** Say so in the entry rather than leaving it looking skipped.
   - **A rule applied to one dialog belongs to every dialog in the file.**
   - **Native's `RepoStatus` is a `#[uniffi::remote(Record)]` mirror**, so a new
     core field is a compile error in `ffi/src/lib.rs` until it is restated
     there. A new core *enum* needs a `#[uniffi::remote(Enum)]` block plus a
     `pub use` from `leogit_core`, and UniFFI lowercases the first letter of each
     variant (`PublishRepository` → `.publishRepository`).
   - **`activeNetworkOp` has a fourth kind, `'fetch'`**, claimed only by the
     *user's* fetch. The automatic ones deliberately claim no slot; WS-J's
     restructure kept that line exactly where it was, and the policy module now
     reads the store so every loop asks the same question.
   - **Tauri's ⌘R is above `MainLayout`'s `inField` bail**, with the composer
     chords — and so is ⌃` since WS-I. The ordering in that handler is
     load-bearing: terminal origin first, then the chords that must work inside a
     field, then the bail, then everything else. **The bail is opt-in, not a
     default**: a chord placed below it is being declared to be something a
     person might plausibly be typing, and most are not.
7. ✅ **WS-G — Tauri branches & merge. Shipped 2026-08-28.** BR-1 through BR-7
   and BR-11, plus SH-4's silently-broken half; the two BR-1 refinements landed
   **natively** in the same change. Per-item state is in §4.4. The dropdown
   became the branch *menu* — filter, keyboard cursor, row context menu, and a
   footer carrying New branch… / Merge into "…" / Abort merge… / Delete branch…
   — with `MergeOverlay.svelte` replaced by `MergeBranchDialog.svelte` and the
   delete and abort confirmations sharing a new `ConfirmDialog.svelte`.

   **What later workstreams should know.**
   - **WS-F's lesson held on the first attempt to reuse it.** BR-1 (a complete
     merge flow nothing could reach) and BR-2 (an abort with no UI at all) were
     filed as two items and are one surface, exactly as WS-F predicted. Building
     the reachable control first and hanging both on it meant BR-3, BR-4, BR-5,
     BR-7 and BR-11 landed as properties of that one surface rather than as five
     separate patches. **This is now twice.** When an area's items keep naming
     the same control, the control is the item.
   - **An action that needs an argument can borrow the list it is standing on.**
     Merge and Delete each need a branch; rather than a second list per action,
     the popover narrows its own rows under a header stating the question, with a
     back arrow out. That is what a native submenu *is*, and it meant the filter,
     the cursor and the row rendering were written once. Any future
     branch-argument action (rename) joins the same mechanism.
   - **A confirmation that cannot hear Escape is not a confirmation.** Every
     Tauri dialog handled Escape on its own overlay element and none took focus
     on mount, so the key was raised on whatever launched the dialog: **not one
     confirmation in the client had ever been keyboard-dismissable.** Five got
     `use:autofocus`; WS-H then moved the key itself off the elements entirely,
     which is what a dialog listening for its own dismissal was always going to
     cost. **A keyboard route nobody has exercised is not a keyboard route** —
     look for the ones that exist only on paper.
   - **"Busy" must not be able to mean "done".** Native's `BranchStore.run`
     returns `nil` both for success and for "dropped because busy", and its
     callers read that as success — the bug WS-Q still owes. The Tauri guard was
     built the other way round: a refused start returns without dismissing the
     surface that asked, so nothing closes as though the work had run. Any future
     serializer should make the refusal a distinguishable outcome rather than an
     absent error.
   - **Split a dialog's failures by whether the dialog is the fix.** A rejected
     branch *name* stays under the field (§6.13's refinement); a refused *merge*
     takes the modal (§6.13's first class), because it has already changed the
     repository and pressing the same button cannot resolve a conflict. The test
     is not "was it raised in a dialog" but "is the dialog where the correction
     happens".
   - **The docs' API inventory had drifted three workstreams deep.**
     TECHNICAL's `gitApi` table still listed `getDiff`,
     `getDiffWhitespaceIgnored`, `getCommitDiff`, `getCommitFiles`, `isMerging`,
     `discoverRepos`, `getLastCommitTimestamp` and `saveConfig` — every one
     deleted in WS-B or WS-C — and had no `reposApi` row at all. It is
     regenerated from `commands.ts` now. **A table of names is the doc form most
     likely to go stale silently**; re-derive it rather than reading it.
   - **`count_commits_to_merge`'s parameter is named `targetBranch` and means
     the *source*.** It counts what its argument holds that HEAD does not. Both
     clients pass the branch being merged in. Left as-is rather than renamed
     mid-workstream — a core rename would touch the shim, the FFI, the Swift call
     site and FRONTEND §3.7 — but it is a live trap for the next reader.
8. **WS-H — Tauri settings & window chrome (S/M).** ✅ *Shipped 2026-08-28.*
   Settings became instant-apply: each control patches its own field through
   `patch_config`, the whole-object save is gone and the footer is a single
   Close (ST-3). With it: the seconds-not-milliseconds interval field (ST-4),
   the scan-path Edit ▸ Done lock that re-walks discovery where the edit was
   made (ST-10), ST-7's revert-on-refused-write, and the auto-fetch re-arm
   (ST-1 / BG-2's first half). Around it: ⌘1/⌘2 (SH-2), the overlay-stack fold
   (SH-4), window frame restoration and a repo-named title (SH-6), and the
   visible tab surviving a repo switch (SH-7).

   **Still live.**
   - **The native Settings window has D-5's lost update at form scale, and it is
     WS-R's first item.** `SettingsStore.currentPatch`
     (`SettingsStore.swift:196-215`) names the *same twelve fields on every
     save*, filled from what `load()` read when the window opened — so a
     `tab_size` the Tauri client writes while that window stands open is
     reverted by the next unrelated toggle. `patch_config` cannot protect
     against this; a field-wise writer only helps a caller that names fields
     field-wise. The Tauri form is the reference.
   - **A Svelte `value={expr}` will not repaint a control whose expression did
     not change**, even when the DOM has diverged from it — 999 typed into a
     field already at its maximum comes back as the maximum, so nothing in the
     model moved and the control keeps showing 999. Hence the `{#key formSeq}`
     bumped on a refused write and on one whose result equalled what was there.
     Any surface promising "the backend's correction is what you see" needs the
     same escape hatch.
   - **A hand-kept list of surfaces is a list someone forgets to join.**
     `modalOpen` had never been told about four dialogs, so ⌘↩ fired a commit
     through the embedded-repo confirmation asking about that commit. It is the
     overlay stack's depth now, which cannot be forgotten because it is not a
     list. Excluding a future surface from that count is done explicitly — not
     registering also takes away its Escape.
   - **Both hosts' `closeSettings` are gone**; the discovery walk hangs off the
     `scan_paths` / `scan_depth` patch, which is what let WS-L give native
     Welcome a live list without inventing a dismissal hook — the setting
     announces itself, and native carries the announcement across scenes as
     `leogitScanPathsChanged`.
   - **`git::get_repo_name` is a registered Tauri command with no caller.** The
     window title uses the client's own `basename()` (CH-12's rule: don't cross
     the IPC boundary for path arithmetic already in hand). One for WS-S's
     dead-wrapper sweep, alongside `rename_branch` and `delete_remote_branch`.
   - **STYLE's *Forms (Settings)* section describes toggle switches and
     segmented controls that only the native form has.** The Tauri form still
     draws a checkbox and a `<select>`, deliberately: that is a visual migration
     under ROADMAP's Primer→Apple item, not parity work. The section says which
     bullets are the target and which are today.
9. **WS-I — Tauri terminal transport (M).** ✅ *Shipped 2026-08-28.* TE-2 and
   **D-4** landed as one change, because the race was a property of the
   transport: the session's stream is a `Channel` the frontend builds with its
   handler attached and passes into `start_terminal`, so the listener exists
   before core can hold it. `start`/`resize`/`close` became `(async)` (E-12);
   `write` stayed sync. With it: TE-1's ⌃-only narrowing in both key handlers,
   TE-5's Tauri half (modifier-click, hover affordance, write-only OSC 52),
   TE-6's caret restore, and three of TE-7's four.

   **Still live.**
   - **A `Channel`'s id is minted in its JS constructor, before any IPC**, so a
     handler attached there cannot miss anything — but messages delivered before
     one is attached hit the constructor's no-op default and are **gone, not
     queued**. The safe form is `new Channel(handler)`; `ch.onmessage = …` after
     the await is the same bug `listen()` had. Any future streaming command
     should take its channel as an argument for this reason.
   - **A per-session message can overtake the command that created the
     session.** `closed` and `start_terminal`'s return travel independently, so
     a shell dying on its own startup file reports the death before the caller
     has a handle. Anything that returns a handle *and* streams on it needs the
     same guard.
   - **Tauri's channel forks by payload size**: JSON under 8 KiB is eval'd
     directly, anything larger is stashed and fetched by a second async invoke.
     Core's coalescing can produce 256 KiB in one delivery, so a flood takes the
     fetch path — still the right trade. **Do not retune `MAX_DELIVERY_BYTES`
     for this**: it is core's, shared with a host that has no such threshold.
   - **A page reload leaks the session.** `Channel::send` returns `Ok` into a
     reloaded document's empty callback registry, so the old PTY and its two
     threads survive with nobody listening. Nothing on the Rust side learns that
     a channel's other end is gone. Relevant to `just dev`, where a hot reload
     accumulates shells.
   - **`attachCustomKeyEventHandler` runs for `keyup` and `keypress` too**, and
     xterm tests the return value with `!1===`, so `undefined` counts as
     "process it". Match on `e.type` and return an explicit boolean.
   - **xterm's link addon cannot gate its own decorations.** `LinkComputer`
     pushes links with no `decorations`, which means all of them, so a
     modifier-gated link still underlines on a bare hover. The hover affordance
     is load-bearing rather than a nicety — WS-R's native half has to match the
     string *Follow link (⌘ + click)*. Making the underline conditional needs a
     hand-written `ILinkProvider`.
   - **`navigator.clipboard` is not usable for anything a click didn't ask
     for** — WebKit gates a programmatic write on recent user activation, which
     an OSC 52 write does not have. Hence `tauri-plugin-clipboard-manager`,
     granted `allow-write-text` and deliberately not the read permission.
   - **The host crate has tests now**, pinning the channel's JSON. Anything else
     whose shape is read by hand-written TypeScript and produced by a serde
     attribute belongs there too.
   - **The bail is the exception, not the default.** `MainLayout`'s "a field has
     focus, leave it alone" bail had swallowed ⌃` since the chord existed —
     found by the user against the native client, where a SwiftUI
     `keyboardShortcut` is a key equivalent and the question never came up.
     Placing a chord below the bail asserts that a person might be typing it,
     which is false for every modified chord the app binds. **TE-1's deferred
     platform split (ROADMAP) should re-read the whole handler with that in
     mind** rather than preserving today's placements.
   - `utils/platform.ts` holds the one `isMac()` / `isWindows()` test; the
     second copy of it was about to be written in `Terminal.svelte`.
   - The window handlers pass `Escape` to the overlay stack, but
     **`isFromTerminal(e)` returns before that**, which §6.11 requires. The
     shell owning `Escape` and a dialog owning it never collide because a dialog
     takes focus when it mounts; TE-6's caret restore fires only on window
     activation and only when the terminal already holds `activeElement`, so it
     cannot pull focus out of a dialog that opened while the app was away.
10. **WS-J — Tauri background cadence (M).** ✅ *Shipped 2026-08-28.* BG-1,
    BG-2's remaining shape, BG-4/E-6, BG-6, DF-1's Tauri half, E-2, E-3, E-4,
    E-5's remainder — and **H-20 / CH-7 on both clients**. Per-item state is in
    §4.2, §4.5, §4.6 and the §3.2 register. Three new modules carry it:
    `services/backgroundPolicy.ts` (the activity ladder, native's table ported),
    `services/pacedLoop.ts` (the self-scheduling chain all three loops share)
    and `core/src/exclusions.rs` (the opt-out grace window both clients call).
    `utils/focus.ts` holds the one text-entry predicate. `get_head_sha` was
    deleted end to end — core, shim, registration, wrapper and FRONTEND row —
    since E-2 took its last caller. Gates: `pnpm check` 0/0 over 152 files,
    prettier clean, `pnpm tauri build` bundled, zero-warning `just mac-build`,
    **178 core** + 24 bridge + 2 host tests, clippy-pedantic 165 core with
    `leogit` / `leogit-ffi` at zero.

    Findings for whoever takes WS-K and beyond:
    - **`setInterval` is the wrong primitive for anything whose rate can
      change**, and every background loop in this client had one. Its period is
      fixed when it is armed, so a cadence that depends on state means tearing
      the timer down and rebuilding it, and a run that outlives its period
      stacks a second on top of the first. Deciding the next delay *after* each
      run settles gives non-overlap, immediate re-cadencing and a representable
      "parked" state for free — the in-flight guard, the re-arm dance and the
      `intervalMs <= 0` early return all disappeared into it. Native reached the
      same shape from the other direction (`while !Task.isCancelled` + `sleep`);
      this is that loop written for a host without structured concurrency.
    - **Independent timers are a fan-out waiting for their common multiple.**
      The three tier `setInterval`s were 2/5/10 min, so every ten minutes all
      three fired in the same turn and the "sequential" each tier promised was
      true only within itself. One loop over a deadline array — native's shape,
      copied — makes the promise true globally. **The general form: when N
      timers guard one shared resource, they are one timer.**
    - **A parked loop needs something to un-park it, and only some conditions
      raise events.** The tier loop parks on the *activity* half of
      `canRunRepoSweeps()` and not on the network-op half, because an activity
      change fires a listener and a transfer ending fires nothing. Parking on
      the full predicate would have left the scheduler dead until the next time
      the user alt-tabbed. Anything that gates a loop on a composite predicate
      has to ask which of its terms are observable.
    - **The equality gate is whole-value on purpose.** `JSON.stringify(status)`
      rather than a named-field fingerprint, matching native's `Equatable`
      compare: a hand-picked list is one that a field added later silently falls
      out of, and the failure is invisible — a new field simply stops moving the
      UI. It costs one serialization of an object that was parsed a microsecond
      earlier, which is nothing against the subprocesses that produced it.
    - **Anything that must advance every tick has to live outside that gate.**
      The exclusion clock is the example: a path is pruned for having been
      *absent* long enough, which an unchanged file list keeps being true of, so
      gating the aging on "something changed" would freeze it exactly when it
      matters. The gate ended up depending on the reconcile's answer rather than
      the reverse.
    - **A cadence ladder breaks a tick count *and* a bare wall-clock window, in
      opposite directions, and CH-7 needed both terms.** It was specified as "N
      consecutive absent ticks (≈30 s at the visible cadence)"; with BG-1 in the
      same workstream a tick is 2 s or 30 s, so N ticks would have meant
      anything up to seven minutes — hence wall-clock. But the first attempt was
      wall-clock *alone*, at 30 s, which the review pass caught: the hidden rung
      is also 30 s, so one tick is charged the entire window and the opt-out was
      pruned on the **first** observed absence — the one read that can land
      mid-rewrite, and the exact failure the window exists to prevent, now
      concentrated at the rung where unattended rewrites are likeliest. The rule
      is `absent_ms >= 30 s && absent_reads >= 2`. **The general lesson: a
      duration threshold is only meaningful when it is comfortably larger than
      the sampling interval, and when the interval is variable it is safer to
      say what you mean — "not on one observation" — than to encode it as a
      time.** Check any other threshold in this codebase against its sampler:
      the diff debounce and the slow-load threshold are fine (they sample
      continuously), but a future one might not be.
    - **A hoist's IPC cost can be conditional rather than structural.** §5's
      rule disqualified H-20 for costing a crossing per tick; the crossing only
      exists when the user has excluded something, which is rare, so the client
      short-circuits an empty set and the objection evaporates. Worth trying
      before distorting a call that always happens (H-3's shape) or giving up on
      the hoist.
    - **`document.hidden` is not occlusion.** The native policy reads AppKit's
      occlusion state; the WebView sets `hidden` for a minimized or hidden
      window and *may* set it for a fully covered one. Where it doesn't, the
      window reports `inactive` and polls at 10 s — slower than frontmost,
      faster than the hidden rung, never wrong in a way the user can see. The
      residual difference is a §8 row, not a bug.
    - **The hidden rung is a floor, not a guarantee.** A WebView may throttle
      timers in a backgrounded document, so 30 s is "30 s or slower" — which
      only makes hidden work cheaper, and the wake-up resync is what actually
      guarantees a current screen. Pinning it exactly would mean a Rust-side
      ticker emitting an event nothing else needs; **WS-K is the workstream that
      would already be adding a `CoreEvent`**, so if that turns out to be wanted,
      that is where it is cheapest.
    - **The wake-up condition is a *rise* in the ladder**, not a single event.
      `focus` and `visibilitychange` both fire on one activation and a
      hidden→visible→focused wake is two steps; ranking the three states and
      resyncing when the rank goes up dedupes that structurally, with the
      `resyncing` flag left in place for the overlapping case. It also kept
      BG-7's Tauri behaviour (resync on the occlusion edge as well as focus),
      which native still lacks.
    - **A throttle whose reason has gone should go with it.** The tier
      scheduler's 30 s refocus throttle existed because focus and
      `visibilitychange` double-fired; with deadlines, a second kick simply
      finds nothing due. It was kept anyway — a rapid alt-tab would otherwise
      re-fetch the top five repositories on every pass — but the *reason* is now
      rate-limiting, not deduplication, and the comment says so.
    - **`document.activeElement` does not clear when the window loses focus** —
      that is what `document.hasFocus()` is for. A typing guard built on the
      element alone answers "still typing" for the whole time the user is away
      in another app, which would have held back every automatic fetch the
      ladder is stretching rather than pausing. The native predicate gets this
      free (`NSApp.keyWindow` is nil while inactive, so its first responder is),
      which is why "port the native rule" was not the same as "port the native
      *expression* of the rule". Anything that ports a first-responder question
      to the DOM needs the `hasFocus()` half.
    - **A tier abandoned mid-fan-out loses its whole cycle, and a blur is now
      enough to abandon one.** `dueAt[index]` is advanced *before* the tier runs
      (so a slow tier doesn't drift the next one), and `syncTier` returns on
      losing `canRunRepoSweeps()` — which since BG-1 includes "the window is
      active". Alt-tabbing halfway through tier 2 therefore leaves its remaining
      repos on stale badges for the full 10 minutes; `kickTopTier` only pulls
      tier 0 forward. **This is native's behaviour, ported faithfully**
      (`RepoDirectoryStore.runScheduler` has the same order), and the data is
      the most deferrable in the app, so it was left alone rather than made to
      differ. If it ever matters, the fix is to restore an abandoned tier's
      deadline to "due now" instead of advancing it — at the cost of re-fetching
      the repos it had already reached, on both clients.
    - **Native carries two efficiency defects of the same family as E-6**, both
      out of scope here: `RepoStore`'s `workingTreeEpoch` bumps on any status
      change, so an unrelated edit re-parses the open diff (E-9, WS-P), and
      `sweepVisible` checks `canRunRepoSweeps` once at entry rather than inside
      its loop (RM-11, WS-S). The third — `noteActiveStatus` rewriting an
      identical `RepoSync` every tick — was fixed here because it *is* E-6,
      one store along.
11. **WS-K — Connectivity observer, both clients (M).** **H-17** and BG-3,
    alone because they are a mini-project on a different machine: three OS
    backends, a new `CoreEvent` variant and a new dependency, none of it
    verifiable from macOS. Linux netlink first — it is the broken platform
    (`navigator.onLine` is hard-wired `true` on WebKitGTK, silently disabling
    the offline gate, the recovery kick and the update-check retry) and the
    user's Linux machine is where the only consumer is written. Then macOS, and
    only once it proves equivalent does native retire `NetworkPathObserver`.
    Until then `navigator.onLine` stays authoritative-negative only.

    From WS-J: **the Tauri adopter is two call sites, not a search.**
    `connectivity.ts` still owns the breaker and `shouldAttemptBackground()`;
    what an online/offline `CoreEvent` replaces is `isBrowserOffline()` inside
    it, plus the `window.addEventListener('online', …)` in `initConnectivity`
    and in `updateChecker`. The recovery kick already routes through
    `repoSyncScheduler.kickTopTier()`, which is deadline-based now, so a
    recovery that arrives while the window is parked simply brings the top
    tier's deadline forward instead of firing a fan-out into a hidden window.
    If the observer's transport ends up wanting a per-subscription channel
    rather than a broadcast, WS-I's entry above is the shape.
12. **WS-L — Native welcome, switcher & clone (M).** ✅ *Shipped 2026-08-28.*
    RM-1 (Welcome is the switcher's own list, with launch resolution and the
    sole-repo auto-open — and `Open Repository…` / ⌘O deleted with the
    `.fileImporter` behind them, RM-2's last entry point), RM-4's native half
    (the persisted clock ⇄ A-Z toggle), RM-5's native half (remote-derived row
    labels, both of them searchable), RM-6 (the ↑/↓ cursor), RM-7's native half
    (the three empty states and the *Choose folders to search* action), RM-9's
    native refinement (walk and badge sweep run concurrently), SH-8 (the
    scan-failure row), and the native halves of CL-1, CL-2, CL-3 and CL-7. Three
    items landed on **both** clients: CL-1's transfer gate moved from the control
    that opens the list onto the rows; RM-5's label rules were hoisted beside the
    identifier cache in each client, which closed the Tauri startup picker's own
    drift (raw paths, basename-only search) beside a dropdown showing
    `owner/name`; and RM-4's toggle became a shared component both Tauri pickers
    use, so a sort chosen in either list is honoured in both. Per-item state is
    in §4.1, §4.8 and §4.11. New native files: `Screens/RepoPickerList.swift`,
    `Design/RepoListEmptyState.swift`, `Stores/RepoIdentifierStore.swift`,
    `Stores/SortMode.swift`, `Services/ListNavigation.swift`,
    `Services/NameCollation.swift`; the FFI gained `repo_identifier` and a
    `RepoIdentifier` mirror (62 → 63 exports). Gates: `pnpm check` 0/0 over 153
    files, prettier clean, `pnpm tauri build` bundled, zero-warning
    `just mac-build`, 178 core + 24 bridge + 2 host tests, clippy-pedantic 165
    core with `leogit` / `leogit-ffi` at zero.

    Findings still live for the native block:
    - **A boolean meaning "in progress" cannot also answer "has this ever
      run".** The picker's empty state read `isRefreshing`, which is false
      before the first walk starts as well as after the last one ends, so every
      launch was greeted with "No repositories found — choose folders to
      search" on a machine whose repositories the app was about to list. A
      separate `hasSearched`, set on every exit from the walk including the
      failing ones, is the fix. Any surface that renders before its data source
      has looked inherits this.
    - **A call to action has to have somewhere to come back to.** *Choose
      folders to search* opens Settings, and nothing native re-walked when
      `scan_paths` changed. `leogitScanPathsChanged` is posted from the patch
      and answered by the **root** view — the picker is the surface offering the
      advice and the one with no switcher to re-open. WS-R rewrites that
      Settings store field-wise and must keep the hook on the patch.
    - **A SwiftUI `body` is not a memoization boundary.** The picker ranked,
      disambiguated and crossed into core *per body pass* while the identifier
      store published once per repository — fifty repositories, fifty rankings
      and fifty FFI crossings for one list appearing. The rows are `@State`,
      rebuilt from an `Equatable` `Inputs` struct naming exactly what they are a
      function of. `CloneStore.visibleRepos` is the same pattern. **Anything a
      native view derives from a streaming store wants this shape** — including
      WS-N's file list.
    - **A `disabled` control cannot explain itself.** Neither `.disabled(…)` nor
      the DOM attribute delivers pointer events, so the tooltip saying why is
      never seen — which is the entire reason the row is on screen rather than
      hidden. Both clients dim and refuse instead. This applies to any future
      "you can't do this right now" affordance.
    - **Re-check after every long `await` in a launch path.** A `.task` being
      cancelled does not stop its continuation, and `store.open` has no
      same-path guard, so a rule that fires after a filesystem crawl must re-ask
      what it started from.
    - **Still pointing the wrong way, for WS-S.** A *re-walk* that fails is an
      inline row with a Retry natively and a `console.error` in Tauri, whose
      pickers then show "No repositories found" for a walk that never ran.
      Tauri's launch-time `phase: 'error'` covers only the first walk. Small,
      and the native shape is the one to port.
13. **WS-M — Native launch, menus & updater (M).** ✅ **Shipped 2026-08-28.**
    SH-1's app half: `CFBundleDocumentTypes: public.folder` makes LaunchServices
    deliver `open -a LeoGit <dir>` to `application(_:open:)` cold and warm alike,
    so single-instance, Finder *Open With* and drag-onto-Dock all come from the
    platform; the target outranks `last_opened_repo`, and a folder that isn't a
    repository raises *Create a repository here?*. SH-2's native half: the menu
    bar (File ▸ Clone ⇧⌘O, View ▸ ⌘1/⌘2, View ▸ Show/Hide Terminal — TE-7's ⌃` —
    View ▸ Refresh, and a Branch menu sharing one `BranchMenuContent` with the
    toolbar control). BG-5: `UpdateStore` + `UpdateChip`, once per session, gated
    on `isOnline` alone. Per-item state is in §4.2, §4.10 and §4.11. The FFI gained
    `resolve_launch_target`, `init_repo`, `open_url` and `check_for_update` plus
    `LaunchTarget` / `UpdateInfo` mirrors (63 → 67 exports). Three deliberate
    deviations from this document's text, each recorded where it belongs:
    `is_git_repo` and the pending-target setters stayed unexported (dead surface,
    and native needs an observable slot fed by two sources), and there is no ⌘B
    (FRONTEND §8).

    **Still live.**
    - **`.onChange` cannot see a value set before the modifier existed**, and on
      a cold start that is the normal case: AppKit delivers
      `application(_:open:)` between will- and did-finish-launching, ahead of
      every SwiftUI task. So the launch path claims the target *itself* and the
      handler covers only later ones. It also reads a **kept** copy of the
      target rather than the claimable one, because the two consumers run in the
      same turn and their order is not fixed. **Any native state fed by an AppKit
      callback wants both halves of this.**
    - **A cancelled `Task` finishes after its replacement is stored**, so a loop
      that clears its own handle on exit drops the reference to the task that is
      still running — and the next start, seeing `nil`, adds a third. The handle
      is cleared only where it is replaced. `UpdateStore` is the instance; any
      retry loop restarted by an event has the same shape.
    - **A menu-bar menu has no "about to open" hook**, and `.onAppear` inside
      `CommandMenu` content is unreliable at best and a reload loop at worst. The
      branch list reloads on app *activation* instead — better for the case BR-3
      named, since a branch created in a terminal is followed by returning to the
      app. **Any menu-bar surface mirroring an in-window one inherits this.**
    - **The same chord declared in two rendered copies of one view is
      registered twice**, and SwiftUI resolves the duplicate arbitrarily. A
      shared menu-content view takes a flag saying which copy owns the key
      equivalents; only the menu bar does, since it is also always present.
    - **A window hosts one sheet at a time, so the root view has one slot.** Two
      `.sheet` modifiers on the same view is not two slots: a request arriving
      while the other is up has nowhere to go, and the binding it set can be left
      standing so that `.sheet(item:)` never presents that item again. The root
      carries a single `RootSheet` enum, where assigning *replaces*. WS-N added
      its discard sheet to that slot rather than a second one.
    - **A modifier on a view whose body is a `Group` reaches every child.**
      `BranchMenuContent`'s body is one, so an `.onAppear` hung on the whole
      thing would have fired the branch reload once per section; it sits on a
      single child instead. `.disabled` propagating the same way is what makes
      the busy state work, so the rule cuts both ways.
    - **`RepoStore.open` still has no reentrancy guard.** Two overlapping opens
      interleave `repoPath` with `loadRepoData`, so a fast double switch can
      leave one repository's path beside another's history. `switchRepo`'s
      same-path guard does not help, since `repoPath` is published after an
      `await`. Small; WS-Q or WS-S is the place — `awaitLoadSettled` already has
      the depth-count machinery a guard would build on.
14. **WS-N — Native file list & composer (M).** ✅ **Shipped 2026-08-28.**
    CH-1, CH-2, CH-3's native half, CH-5, CH-6, CH-8's, CH-9's, CH-10's and
    CH-11's, plus **E-7** and **E-10** — per-item state in §4.5 and §3.3. The
    native changed-file list became `List(selection: Set<String>)` with a Space
    inclusion toggle, a tri-state select-all (`Toggle(sources:isOn:)`), `old →
    new` renames, the ↪ badge, a discard **sheet** carrying its own in-flight
    state and refusal, one bulk discard call and a status-only refresh. New
    files: `Design/ActionFailureAlert.swift`, `Screens/DiscardSheet.swift`,
    `Services/FileListSelection.swift`. No FFI change.

    **Two follow-ups for WS-S, both one line, both closing a gap this opened in
    the *other* direction:** Tauri's `DiscardConfirm` still closes into the
    modal where native now keeps the refusal in the dialog (§6.13's
    refinement), and its discard / ignore `reportActionError` call sites pass
    no `retry` though the parameter has been there since WS-D.

    Findings still live for the native block:
    - **`head_sha` is an edge, and more than one thing is triggered off it.** A
      cheaper refresh that writes the status without doing the history and
      branch reloads *consumes* that edge: both consumers compare against the
      value it just advanced, see no move, and stay stale indefinitely.
      `refreshWorkingTree` hands over to the full reload on a moved `HEAD` and
      returns that it did so the caller can reload branches. **Any future
      "cheaper path" past a shared refresh has to account for every edge the
      full one was feeding.**
    - **`isLoading` was doing double duty**: the progress bar *and* the status
      poll's mutual exclusion, so a read that skipped `beginLoad` to avoid the
      bar also stopped blocking the poll. `beginLoad(showsProgress:)` and
      `isBusy` separate them; anything that wants a quiet read wants the lock
      too.
    - **A macOS menu item is clickable while a sheet is up.** Every sheet
      request checks the slot is free and **drops** rather than replaces — the
      sheet standing there was opened deliberately and may be mid-write.
    - **A failure raised in a subtree is presented at the root**, beside the
      sheet slot, which is also where a repo switch can retire a retry closure
      before its captured `repoPath` names the wrong repository.
    - **A conditional modifier rebuilds everything inside it**, so `PathText`'s
      tooltip is applied *innermost*. The same identity rule is why WS-O put
      its two arrangements inside one `ScrollView` rather than beside it.
    - **A `Set` selection cannot say which row a gesture landed on**, which is
      why "which file does the detail pane show" is a rule of its own
      (`Services/FileListSelection.swift`): one row selected is the choice,
      several leaves the pane where it was, and a fallback picks the first in
      **list** order — never `Set.first`, a hash order that lands differently
      each launch. **Any native surface deriving a single thing from a
      multi-selection wants this.**
    - **Both file lists are one view, so a change to it lands in History too.**
      `ChangedFileList` is shared, which made renames and the ↪ badge free on
      the commit-detail list — and made `CommitDetailStore` need a selection
      `Set` it has no other use for. `DiffView` is shared the same way, which
      is how WS-O's split layout reached both tabs at once.
    - **Measure every face you draw.** `PathText` binary-searches a character
      budget against rendered width, and the included-row weight cue changes
      the filename's face, so the search measures the two halves separately or
      overflows the row it just promised to fit.
    - **Hold a derived layout answer in state; do not re-derive it in `body`.**
      E-10 was a correct algorithm run on every repaint. One `.onChange` over
      an `Equatable` struct of *all* the inputs, so adding an input cannot
      silently stop invalidating.
    - **`.borderedProminent` + `.tint(.red)` is the destructive button**, and a
      sheet that runs one gives the default key to nothing — Return must not
      discard. `.cancelAction` on Cancel is what Escape gets.
    - **One `ActionFailure` modifier is the client's only `.alert("Error")`.**
      WS-Q's checkout and undo failures should move onto it rather than growing
      a fourth copy; it already carries the retry closure §6.13 asks for.
      `UpdateChip` keeps its own alert on purpose — its title names the thing
      that failed ("Could not open the release page"), which is better than
      "Error".
15. **WS-O — Native split diff (M).** ✅ **Shipped 2026-08-29.** DF-2 on both
    clients, plus DF-13 (checked, no change needed) — per-item state in §4.6.
    The native split layout is an arrangement of one row model rather than a
    second renderer: `DiffStore` holds the flat line list either way and, when
    the layout asked for it, `pairs` — core's `SbsPair`s carrying *indices* into
    those rows. Both arrangements are branches inside one `ScrollView`, and the
    branch reads the loaded pairing rather than the setting, so the pane changes
    on the frame its data arrives instead of blanking for the length of a
    re-read. The toggle moved out of Settings into the diff header in both
    clients, still persisted in the shared `side_by_side_diff`. New native file:
    `Design/DiffLineRow.swift`. Also fixed here, in the **Tauri** client,
    because this work depended on it: the *hide whitespace* reload called
    `loadDiffForFile` without `force`, so the loader's "already open"
    short-circuit returned before reading and the setting only took effect on
    the next file clicked — and it never touched the History pane at all.

    Findings still live for the workstreams after it:
    - **Compare at the granularity of what you would rebuild, not at the
      granularity of the reply.** A layout change returns an *identical*
      `file_diff` with a pairing that appeared or vanished, so comparing the
      payload whole would drop the rows and the syntax colour every time the
      reader switched arrangement — hence `modelMoved` / `pairingMoved`. Any
      second artifact on a payload wants the same treatment.
    - **`usize` does not cross the bridge.** Core's `SbsPair` indexes with it,
      so the mirror is purpose-built at `u32` and saturates rather than
      wrapping. Any future core type with a `usize` field needs the same.
    - **Anything that adds a re-read to a surface has to check what the old
      read was silently clearing.** The size guard's *Show diff anyway* was a
      per-call flag, so the moment the layout control made the diff re-read from
      inside its own header, revealing a large diff and then switching
      arrangement re-armed the guard and removed the control that had got the
      reader past it. Both clients now remember *which diff* was revealed.
    - **A config field can belong to a surface other than Settings.** The layout
      is patched from the diff header, which is why the Settings patch must keep
      *not* naming it — naming it would revert whatever the header last wrote
      while that window stood open. **WS-R rewrites that patch field-wise and
      must not "complete" it by adding `sideBySideDiff`.**
    - **A control the user clicks must write synchronously, and a shared file
      needs a queue rather than only a lock.** A SwiftUI control re-reads its
      `Binding` in the same layout pass and a `Task` does not start there, so a
      deferred write leaves the pressed segment snapping back for a frame.
      Core's `patch_config` lock keeps the file coherent but decides *nothing*
      about order: two writes in flight are two patches whose winner is the
      scheduler's. Both clients chain their writes in the store, and a store
      that reads the same file needs the other half — `AppConfigStore.reload()`
      drops a read a write overtook. **All three matter directly to WS-R.**
    - **DF-13 is closed by the platform, and both candidate fixes were worse.**
      SwiftUI `Text` uses `byWordWrapping`, documented as breaking a word that
      cannot fit a line on its own. `.byCharWrapping` is not reachable from
      `Text`, and zero-width break insertion in the tab-expansion pass would put
      U+200B into every copy taken from the pane — which is precisely what
      **DF-6** exists to prevent, and a live constraint on how DF-6 is closed.
16. **WS-P — Native diff polish (S/M).** ✅ **Shipped 2026-08-29.** D-14, D-15,
    DF-6's native half, DF-8's two native halves and DF-12 + E-9; per-item state
    in §3 and §4.6. One half is deliberately left and named: the Tauri client's
    *multi-line* copy, whose design is written out in DF-6.

    Four defects, three of which came from the pane deriving something from the
    wrong source. **D-14, the reader's place in the file:** `DiffStore` publishes
    `rendered`, the `DiffIdentity` (source + path) of the payload *on screen*,
    and `DiffView` binds a `ScrollPosition` that answers a change to it with
    `scrollTo(edge: .top)`. Every re-read of the same diff keeps the offset.
    **DF-8, what the header says:** a rename reads `old → new` under the file's
    name, and each of `+N` / `−N` is drawn only when non-zero. **DF-12 + E-9,
    what makes it re-read:** `LoadKey` carries the open file's own `stat_stamp`
    and `xy` plus the status's `head_sha`, and phase two waits out
    `highlightDebounce` (80 ms). `RepoStore.workingTreeEpoch` and the refocus
    `forceDiffReload` were deleted with the last thing that read them.

    Two one-line fixes went into the **Tauri** client to keep the contracts
    honest in both: its scroll-reset key gained the commit (one path in two
    commits is two different diffs, and stepping through History with a file
    selected kept the previous commit's offset), and its freshness key gained
    `head_sha`.

    **DF-6 / D-15, what a copy is a copy of.** `copy_diff_text` had sat unused
    in core, the bridge and the Tauri shims since WS-B. It has a consumer: the
    native gutter is a **line handle** — click a number, ⇧-click to extend,
    right-click for *Copy N Lines* / *Select All Lines*, ⌘C, Escape — and the
    clipboard text is `copy_diff_text` over the run's flat range, so it carries
    none of what the pane drew around the code and keeps the file's real tabs.
    Dragging the content still selects characters and copies exactly that:
    `onCopyCommand` returns **nil** when no run exists, so the two selections
    never contend for ⌘C. That character selection is confined to one line and
    always was — **D-22**, deferred by decision after two failed attempts to
    treat it as a bug; §10 carries the analysis and the ways out.

    Gates: zero-warning `just mac-build`, `pnpm check` 0/0 over 153 files,
    prettier clean, `pnpm tauri build` bundled, **181** core (three new: the
    absent-side parse, the synthesised header, the dropped marker) + 24 bridge
    + 2 host tests, `cargo fmt --check` clean, clippy-pedantic 161/163 core —
    two below the standing baseline, from extracting `format_patch_path` — with
    `leogit` and `leogit-ffi` at zero. The FFI surface is unchanged at 67
    exports: the copy consumes a function that was already there.

    Findings for WS-Q and the workstreams after it:
    - **A store that a view reads should publish what it *is showing*, not only
      what it holds.** `payload` describes a diff without naming it, and under a
      seamless reload the diff it describes is the *previously* open file's for
      the length of the load. One published identity answered three separate
      questions — when to reset scroll, whether the `+N −N` totals belong beside
      the name in the header, and whether the rename arrow does. Any pane that
      keeps stale content on purpose needs the same distinction, and WS-Q's
      commit detail is the next one that will.
    - **Publish that identity outside the equality skip.** Two *different* files
      can parse to an equal payload — an identical one-line change in two files
      — and a skip that also skipped the identity would leave the header
      captioning the wrong file and the reader scrolled into the middle of a
      diff they just opened.
    - **A key that gates a read must name everything the answer is a function
      of.** Narrowing native's whole-status signal to the open file was the
      point of E-9, but `stat_stamp` and `xy` alone would have *lost* coverage
      the coarse signal had: the working-tree diff is `HEAD` against disk, so a
      `--mixed` reset changes it while leaving the bytes and the status letters
      untouched. Both clients were missing `head_sha`; both have it now.
    - **Prefer the platform's own scroll state over an imperative reader.**
      `ScrollPosition` (macOS 15+) with `scrollTo(edge: .top)` needs no
      `scrollTargetLayout` and no row to exist yet, and Apple documents an edge
      as *stable across content-size changes* — so the jump survives the rows
      landing after it, and clears itself the moment the user scrolls. A
      `ScrollViewReader` keyed on a row id would have had to wait for that row.
    - **Watch the scroll from outside the branch that owns the scroller.** A
      binary file, an empty state or a failure takes the `ScrollView` out of the
      hierarchy, and a modifier that is not there cannot notice the diff that
      replaces it.
    - **A cancelled `Task.sleep` resumes; it does not throw out of the
      function.** With `try?` in front of it, the guard *after* the sleep is the
      only thing that stops the work — and `generation` alone is not enough,
      because a pane leaving the hierarchy (a tab change) cancels the task
      without starting a new load. `Task.isCancelled` is the other half.
    - **A trailing newline is a terminator, not a line.** `raw.split('\n')`
      yields a final `""`, which the hunk body read as an empty context row —
      a blank numbered line at the foot of every diff in both clients, carried
      into `html`, `sbs_pairs` and anything copied. Only the *last* one goes:
      a genuinely blank context line also arrives as `""` when a tool has
      stripped the trailing space git writes.
    - **`FileDiff` cannot answer "renamed from what?", and looked as if it
      could.** The parser fills `old_path`/`new_path` from git's `--- a/` and
      `+++ b/` lines and ignores `rename from`/`rename to` entirely — but the
      deeper problem is upstream of the parser: `diff_args_for_file` and
      `get_commit_diff` both pathspec-limit to the file's *current* path, so
      git's rename detection never sees the counterpart and emits a plain add.
      A pair that differs therefore means an add or a delete, never a rename.
      `FileEntry.orig_path` is the answer, in both panes, and core drops it for
      a copy — which has a source but took nothing from it. **A field being
      present is not the same as it being answerable; check what the command
      was actually asked before sourcing a fact from its output.**
    - **`FileEntry.stat_stamp` is `None` for commit files by design** (immutable
      history), so the freshness gate is a working-tree concern only and the
      History pane needs no equivalent.

    **Three defects the copy work exposed in core, all fixed here.**
    `strip_path_prefix` left `/dev/null` intact, so `old_path` was `/dev/null`
    for every added file and `new_path` was for every deleted one — a header
    comparing the two sides to spot a rename read every add as
    `/dev/null → <file>`, which the **Tauri** client had been rendering for as
    long as that header has existed. It is answered as absence now, the way
    `parse_binary_marker` already answered it, and `build_patch` writes it back
    as `/dev/null` so a synthesised header still applies. `copy_text` emitted
    `\ No newline at end of file` as if it were a line. And the parser
    materialised the patch's own trailing newline into an **empty context row**
    at the foot of every diff — a blank numbered line in both clients, and one
    line too many in anything copied. Three tests pin the three (181 core now).

    **The rename arrow comes from the entry, not the diff, in both clients.**
    Both reads pathspec-limit to the file's current path (`git diff HEAD --
    <path>`), so git never sees the deleted counterpart and reports a rename as
    a plain add: `FileDiff`'s two paths cannot answer "renamed from what?" at
    all. `FileEntry.orig_path` can, in the working tree and in commit detail
    alike, and it is what the file lists already read. The Tauri viewer takes it
    as a prop now. **This reverses what §4.6's DF-8 line used to prescribe** —
    "source it from the parsed diff" was written before anyone checked whether
    the parsed diff *had* the answer.

    Findings on the copy half specifically:
    - **Two selections can coexist if they live on different surfaces.** The
      gutter addresses lines, the content addresses text, and neither gesture
      can reach the other's target — which is what let the character selection
      survive a change that was originally framed as replacing it. GitHub's own
      diff splits it the same way. The corollary is the ⌘C rule: the pane must
      be able to *decline* the Copy command, or whichever selection is wired
      wins even when the reader meant the other.
    - **`onCopyCommand(perform:)` takes an optional closure, and the optional is
      the whole design.** Passing `nil` opts the view out and the command
      continues down the responder chain to the text selection. Anything that
      wants to conditionally claim a system command should look for this shape
      before hand-rolling a precedence rule.
    - **A responder-chain command needs a responder.** The pane is
      `.focusable()` with the focus effect disabled, and the gutter's own
      actions take focus, or Copy is never offered to it at all.
    - **Own a selection where you own what it indexes.** `DiffLineSelection`
      lives in `DiffStore` beside `rows`, so it is cleared in the one place the
      row model is rebuilt and cannot address lines of a diff replaced under it
      — and deliberately *not* on a layout change, which moves no line, so the
      run survives it exactly as the scroll position does.
    - **A selection cue has to cover the rows that cannot be clicked.** The
      `@@` band and the no-newline marker have no line number, but core copies
      a hunk header as its own text, so a run that spans them carries them —
      and they have to carry the wash too, or the highlight has a hole in the
      middle of what was copied.
    - **`.textSelection` does not give a diff a text selection** (**D-22**,
      deferred — §10 carries the analysis). It marks a `Text` selectable; it
      does not make a stack of them behave as one document. A row-per-`Text`
      pane therefore has no multi-line drag available to it at any price, which
      is *why* the gutter run exists rather than merely a reason it is nice.
      The chrome opts out with `.textSelection(.disabled)`, which costs nothing
      now that there is no cross-row run for it to interrupt, and the gutter's
      gestures sit on a `Color.clear` pad overlaid on the number rather than on
      the number itself, keeping the interactive layer and the drawn text
      independent.

17. **WS-Q — Native history & sync polish (M).** All small, one screen each.
    HI-4's native half (gate "No commits yet" on
    the first load), HI-5 (a visibility-gated 10 s tick reusing
    `BackgroundSchedulingPolicy` — don't invent a second gate — plus the shared
    tier vocabulary and the detail card's date format), HI-7's native halves
    (key the detail task on `(repoPath, sha)`, clear `commits` on repo switch),
    HI-8's native bits (prefetch at N−5, the tag-chip treatment STYLE.md
    specifies), HI-9 (checkout busy state), HI-10's native half (a `--mixed`
    reset can't change the branch list), SY-7's native half (Tauri's dialog
    lifetime), SY-10 (selectable, monospaced transfer errors), BR-4's
    nil-return fix (callers read "dropped because busy" as success), BR-8 (give
    the `· merging` suffix WS-B's purple token), SH-5 and SH-7's native halves.
18. **WS-R — Native settings & terminal (S/M).** **Lead with the field-wise
    patch, which WS-H found and did not fix:** `SettingsStore.currentPatch`
    (`SettingsStore.swift:196-215`) names the same twelve fields on every save,
    filled from the load that ran when the window opened, so a field the Tauri
    client writes while that window stands open is reverted by the next
    unrelated toggle — D-5 at form scale, in the client that was supposed to be
    the reference. Each control should patch what it owns, as the Tauri form now
    does; the 300 ms debounce can stay, keyed per field. With it: a refused save
    should re-seed its control from disk rather than leaving the value that
    didn't land (ST-7), the two AI timeouts want controls (ST-1 — they are
    honoured natively and settable only from the other client, a FRONTEND §8 row
    until they land), ST-5 (route both provider owners through `AppConfigStore`
    — with both windows open the pickers can disagree today; grow it the
    `scanPaths` accessor three call sites bypass),
    ST-3's native half (don't render editable defaults that aren't the user's
    settings — the form currently renders struct defaults behind
    `"Could not read the configuration file."`, editable and silently inert),
    ST-9's native export, ST-10's native half (Edit ▸ Done +
    `.monospaced()`), TE-3 + **D-9** (pin the inner frame so a collapsed panel
    stops reflowing the emulator to one row — WS-B already made the PTY side
    safe, so this is purely the frame — plus the missing 80 ms resize debounce,
    placed in `TerminalController.resize` and *not* the delegate, to keep the
    one-shot initial-size push), TE-4 (scrollback 1000 explicitly, both
    clients), TE-5's native half (the hover affordance alone — ⌘-click and OSC 52
    are already right there; SwiftTerm's hover surface needs an API check, and
    the string to match is the Tauri client's *Follow link (⌘ + click)*).
19. **WS-S — Sweep & contract cleanup (S/M).** What genuinely had no home, plus
    the work that can only land once everything else has:
    - **Leftover efficiency**: RM-11 (move the native sweep's slot re-check
      inside the loop — its own tier runner already does), BG-7 (fire the
      existing resync from the policy's occlusion edge).
    - **Dead-surface deletions** — CL-8's `check_auth` (spawns `gh auth status`
      every launch to write a field with zero readers), BR-10's rename and
      remote-delete wrappers (the feature is deferred to ROADMAP, the wiring
      goes now), the `has_staged_changes` wrapper, the unconsumed derived
      stores. DF-5's scaffolding stays and ST-9's probe is wired by then, so
      neither is deleted. Each deletion also drops a row from FRONTEND §3's
      command tables.
    - **FRONTEND.md** — retire each §8 row the parity work closed, and add the
      divergences this plan keeps that are not yet filed: error surface, loading
      presentation, detached/merging markers, settings surface. (Counts
      placement and progress surface went in with WS-F, branch-menu shape with
      WS-G — each workstream files its own.) The terminal link convention
      becomes a shared §6 rule (TE-5), not a §8 row.
    - **Stale source comments**, all verified: `CommitStore.swift:20-23`,
      `SyncControls.swift:9-11,131`, `TerminalStore.swift:32`,
      `CloneSheet.swift:8-10`, `ContentView.swift:154-159`,
      `TerminalSessionView.swift:124-126`,
      `BackgroundSchedulingPolicy.swift:7`, `repoSyncScheduler.ts:66-70`.
    - **Doc claims outside the audit's checklist**, fixed as their area lands:
      DESIGN's committer-vs-author date for commit rows (HI-5); DESIGN's
      header-cluster list (ahead/behind are badges *on* the Pull/Push buttons).
      (TECHNICAL's width-keyed `PathText` claim became true in WS-N.)
    - **Two one-line Tauri catch-ups WS-N opened in the other direction**:
      `DiscardConfirm` should keep its own refusal rather than closing into the
      modal (§6.13's refinement, which native now follows), and the discard /
      ignore `reportActionError` call sites should pass the `retry` the function
      has accepted since WS-D.

20. **WS-T — `leogit` reaches the native app (S).** ✅ **Decided, deliberately
    last.** `install.sh` writes a `leogit [dir]` shell function pointing at
    `/Applications/leogit.app` — the **Tauri** bundle, which is the only one with
    a release artifact. It stays that way until parity is done, because the two
    candidate shortcuts are both wrong: the function names a *path*, not a bundle
    id, so on a case-insensitive volume a native `LeoGit.app` cannot coexist there
    anyway; and its `open -na … --args "$dir"` form sends the folder through
    **argv**, which reaches a native app only on a cold start, while the working
    form (`open -a … "$dir"`) is one the Tauri app would ignore, having no
    document types. Falling back to `open -b <native bundle id>` would silently
    point `leogit` at whatever build LaunchServices last registered, a stale
    DerivedData one included. So: **the native app gets a release artifact and a
    real installer branch, once every other workstream has landed** — release
    engineering, taken as the last piece rather than smuggled into a parity
    workstream. Until then the native app is reached from Finder, `just mac-run`,
    or `open -a <path> <dir>`, and every living doc says so plainly.

Suggested order: **A → B → … → S**, then **T**, as lettered. Each workstream
maintains its own doc rows as it lands (per CLAUDE.md); WS-S carries only what
needs the whole plan finished, and WS-T needs WS-S.

One sequencing note that is not free to reorder: **WS-K needs a Linux machine
rather than a predecessor** — WS-J, the workstream it waited on, has shipped, so
schedule it when that machine is available and run the native block in the
meantime rather than blocking behind it. WS-L was taken on that basis. (The
other such entry was H-3, the last core hoist and the one Tauri-block item that
also touched native code; it shipped with WS-F.)

## 7. Standing decisions

Rulings that cut across areas and therefore belong to no single item. Every
other decision lives inline with the item it governs, marked **Decided** in §4.

- Parity target per §1: whichever client has the better behavior wins;
  GH Desktop breaks ties. Both-wrong cases converge on GH Desktop's shape
  where cited (slow-load dimming, model-based copy, clone-list refresh,
  merge zero-count, per-diff SBS toggle placement).
- The dead-surface rule (don't ship wrappers/exports nothing calls) is now
  FRONTEND §1's policy — each host exposes what it consumes and records its
  exemptions, replacing the old "all 69 wrappers on both sides" mandate that
  neither host obeyed. The native FFI already works this way; Tauri's dead
  wiring is how BR-1 rotted.
- History converges on the bounded-append model (HI-2), not Tauri's sliding
  window.
- The exclusion-set rule is native semantics + grace pruning (CH-7).
- `wrap_long_lines`-style key retirements continue via
  `config_ignores_retired_keys` for the fields ST-8's restructure drops
  (`ai_api_key`, the shared `ai_model`).
- Backwards compatibility of `config.toml` is explicitly waived: the app has a
  single user who can regenerate the file, so config changes restructure
  cleanly instead of carrying migration shims.

## 8. Verification gates

Per workstream, matching the previous plan's bar:

- Zero-warning `xcodebuild` via `just mac-build`; `pnpm check` (svelte-check)
  0/0 over 153 files; `cargo test --workspace` green (**178 core + 24 bridge + 2 host** — up
  from the 120 + 24 this plan started at; the last ten are WS-J's `exclusions`
  module, and the host's two are WS-I's, pinning the terminal channel's JSON
  because the TypeScript that reads it is hand-written);
  `cargo clippy --workspace --all-targets -- -W clippy::pedantic` at
  **165** or better, never worse (the plan opened at 184; WS-B took it to 170,
  WS-C to 166, WS-F to 165), with `leogit` and `leogit-ffi` at zero. A Tauri
  workstream also runs `pnpm tauri build` clean, so the bundle is proven rather
  than only typechecked — and it is the only gate that would catch a missing
  capability or an unresolvable plugin dependency, neither of which
  `svelte-check` or `cargo check` can see.
- Visual checks per workstream (ask for confirmation, no screenshots), on
  **both** clients whenever a change touches shared core or a ported
  behavior — the checklist comes from the workstream's inventory items.
  WS-A's destructive items (D-1) additionally get a scripted repro against a
  throwaway repo before/after.
- DF-13's wrap check is a named visual item (a minified file in both
  clients) before WS-O closes.

## 9. Documentation updates on completion

The audit's factual corrections are already in the living docs (§2). What each
document still needs is the update that rides its workstream — per CLAUDE.md,
written as each chunk lands, no duplication between documents:

- **FRONTEND.md** — the contract carries the most: every §8 row a parity item
  closes is deleted rather than annotated, and each divergence this plan keeps
  gets a row (WS-S). FRONTEND §3's command tables lose every deleted wrapper;
  FRONTEND §5.2 tracks the diff wire as DF-3 changes it, and FRONTEND §7's
  open decision closes with it. §6.1 and §6.3 gained WS-P's two shared rules —
  the per-file diff-freshness key (`stat_stamp` + `xy` + `head_sha`) and the
  scroll contract restated around the rendered diff's *identity* rather than
  its paths alone — and §7 gained the 80 ms phase-two debounce and the copy
  rule (within a line the characters, across lines the model's lines), with the
  per-platform gesture recorded as an §8 row.
- **TECHNICAL.md** — new mechanics paragraphs only for genuinely new machinery
  (the core hoists, the Tauri channel transport, the native launch path, WS-N's
  `Set` selection + `FileListSelection` + held `PathText` fit, WS-O's one-row-
  model split layout and the `AppConfigStore` writer, WS-P's `LoadKey` and
  `rendered`-identity rules), plus the claims WS-S lists as their areas land.
- **DESIGN.md** — flow 1 is shared end to end since WS-M, `leogit <dir>` and
  the *Create a repository here?* prompt included, and flow 11 (the update
  chip) is now both clients'. The per-flow client hedges retire as parity
  closes them. One difference flow 10 now records rather than hides: Return in
  the clone list is two presses in Tauri (its cursor and its selection are
  separate, so the first press makes the destination visible) and one natively
  (arrowing *is* selecting, so the preview already follows the cursor). Both
  satisfy the rule — no clone starts before the user has seen where it lands —
  and flipping Tauri to one press is the two-line change WS-C flagged. Flow 3's
  selection paragraph is shared since WS-N, with the two Tauri-only gestures
  (the checkbox anchor, and an extension activating the shift-clicked row)
  called out rather than described as everyone's. Flow 3's diff paragraph is
  shared since WS-O — one arrangement control, in the diff's own header, on
  both clients — and WS-P added the three rules that were only ever written for
  one: where a diff switch leaves the reader's scroll, what the header is
  allowed to say about the file it is naming, and what a copy taken from the
  pane actually contains.
- **STYLE.md** — the status-letter row settled on `U` + the purple token with
  H-13 (done); WS-C added the *Repo pickers* section (the two lists are one
  component family, with the shared footer and empty state) and the
  `<fieldset disabled>` rule for uncancellable dialogs; WS-F collapsed the
  header-strip bullet to one description now that both clients carry the same
  control, split the count placement into its own bullet, and added the
  sweeping-fill rule for a transfer with no percentage to report; WS-G rewrote
  *Branch picker* around the action footer and the borrow-the-list picking mode,
  dropped its ahead/behind indicator (no data backs it — `BranchInfo` carries no
  counts, and computing them would be one subprocess per row), and added the
  focus-on-mount and shared-`ConfirmDialog` rules under *Modals / dialogs*;
  WS-H rewrote *Forms (Settings)* around instant-apply (no Save button, the
  error line above the sections, the section footer, the Edit ▸ Done lock for a
  destructive text field, units the user thinks in) and marked which of its two
  control-shape bullets are the target rather than today, and replaced the
  focus-on-mount rationale — `Escape` no longer needs focus, so autofocus is
  there for `Tab`; WS-N rewrote the *File list* selection, inclusion-cue and
  select-all bullets around the set selection and the tri-state checkbox, and
  noted that the composer's character counter sits inside the field only where
  the platform's field can reserve room for it; WS-O gave the *Diff viewer*
  section the header arrangement control and rewrote the side-by-side bullet
  around per-cell tints, the filler wash and the spanning hunk header, pointing
  at *Forms*' segmented-control treatment rather than restating it; WS-P added
  the file header's rename rule beside the totals rule that was already there,
  the gutter-is-the-line-handle rule, and rewrote the selected-line bullet
  around the wash a run draws (the staging cue it used to describe has to be a
  control when it lands, not a second wash).
- **ROADMAP.md** — items close as their workstreams land; the deferrals this
  plan makes (per-line staging, diff virtualization, branch rename +
  delete-on-remote) are already filed there. WS-L closed the two WS-B and the
  pre-plan work had left open (GitHub identifiers and a keyboard cursor in the
  native switcher).
- **README.md** — the branch bullet now describes one menu on both clients, the
  merge scoping gone with WS-G.

## 10. Findings log (out of scope here, worth keeping)

**D-22 — a native diff cannot be drag-selected across lines. Deferred 2026-08-29
by decision, after two failed attempts.** Worth writing down in full, because
the failure mode is expensive: nothing about it shows up at compile time, in
clippy, or in any gate this plan runs, so it costs a build-and-look each time.

*What is actually true.* SwiftUI's `.textSelection(_:)` sets a **selectability
trait** on the text it is applied to. It does not compose separate `Text` views
into a single selection domain, and a selection begun in one `Text` cannot
extend into the next — regardless of whether the grant sits on each `Text`, on
their `HStack`, on the `LazyVStack`, or on the `ScrollView`. The diff pane draws
one `Text` per line (it has to: each line carries its own tint, its own gutter
columns, its own intra-line backplate, and the list is lazy over tens of
thousands of rows), so the reader's drag is confined to one line. This is a
property of the framework, not of anything this pane does wrong.

*What was tried, and why each looked plausible.*

1. Scoping the grant to the content `Text` alone, so a drag could not begin on
   the chrome. Reverted on review as "this will break the cross-row drag" —
   a correct-sounding objection to a drag that did not exist.
2. Keeping the grant on the row stack and opting the numbers and glyph out with
   `.textSelection(.disabled)`, plus a `simultaneousGesture` on the content to
   drop the line run on a click. Shipped, and the reader reported the selection
   confined to one line. Diagnosed as the `.disabled` neighbours cutting a
   selection "run" into per-row islands — a mechanism that does not exist.
3. Removing both, restoring the rows to bare `Text`s. Rebuilt, and the
   selection was **still** confined to one line — which is the observation that
   settles it. Configuration was never the variable.

The chrome exclusion from (2) is back in place, since with no cross-line
selection to interrupt it is free and it does keep a drag begun on a line number
off the clipboard.

*The three ways out, none of them small.*

- **`NSTextView`-backed pane.** A real text view over the whole diff: selection
  across lines, Find, native Copy and Services, accessibility, and the system's
  own text behaviours for free. Per-line tints and the gutter become drawing
  concerns (a background layout callback and a ruler view) rather than view
  composition, and the lazy row list is replaced by the text system's own
  layout. The most capability, the most work, and it would take
  `DiffLineCell`'s single-definition-of-a-row property with it — which is what
  currently keeps the two arrangements from ever describing a line differently.
- **Own the selection outright.** Drop `.textSelection` from the diff, put a
  `DragGesture` on the rows in a named coordinate space, and resolve a
  y-position to a flat row index against a map each visible row publishes via
  `onGeometryChange`. Dragging anywhere then extends the existing
  `DiffLineSelection`, so copying stays `copy_diff_text` and stays exact, and
  the gutter's click / ⇧-click / menu keep working unchanged. Costs character
  selection *within* a line, and needs care where a drag leaves the viewport
  (only laid-out rows are in the map, so it clamps). Moderate work, and the
  `Color.clear` pad the gutter's gestures already live on is where it would
  attach. **Recommended if this is picked up**: it is the smallest change that
  makes the natural gesture do the right thing, and the copy is model-based
  either way.
- **Leave it.** The gutter run already copies any number of lines, exactly, in
  both arrangements. What is missing is one gesture, and its absence is
  discoverable (the numbers carry a tooltip). This is the current decision.

*The rule to keep either way:* a multi-line copy must be rebuilt from the parsed
diff, never harvested from what was drawn. Every option above preserves that;
it is the reason `copy_diff_text` exists.

- GH Desktop's `BackgroundFetchMinimumInterval` idea (don't re-fetch a repo
  fetched < 30 min ago): neither client tracks last-fetched-per-repo;
  switching between two repos re-fetches both every time. Candidate for the
  tier scheduler after parity.
- GH Desktop's mergeability preview (`git merge-tree` conflict count before
  merging) — a genuine convenience neither client has; pairs with a future
  `merge_preview` core function (BR-3's cousin).
- A Tauri macOS `tauri::menu` (SH-2) — the platform-respect follow-up once
  the shortcut surface stabilizes.
- The Tauri list's *shift-click on a checkbox to range-toggle* (its second
  anchor) has no native counterpart, and WS-N did not build one: Space over a
  swept selection reaches the same result in two presses, and a second anchor
  is the part of that design most likely to surprise. Revisit only if the
  keyboard route turns out not to cover it.
