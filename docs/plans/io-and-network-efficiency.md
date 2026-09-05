# Plan — I/O and network efficiency

> Status: **implemented — Steps 0–5 and 7 done; Step 6 deferred (§7 Q6);
> F3 and F12 deferred for lack of a measurement.** Each completed
> step is trimmed to a summary in §6 with the numbers the harness measured;
> the next step is named there. Implementation rule, applied to every item:
> the current experience is the baseline, and an item that would trade any of
> it for throughput is deferred and marked, not built.
>
> Produced from measurement on this
> machine plus six parallel source audits (Rust core, network layer, Tauri
> frontend, SwiftUI frontend, then a startup/IPC/persistence sweep and an
> adversarial fact-check). Every claim below was re-verified first-hand against
> source or by experiment; §3 lists the hypotheses that measurement *killed*,
> which is the more useful half of the document.
>
> The fact-check pass corrected this document in eleven places — a false "already
> correct" entry in §2, a redundant-fetch scenario that the code disproves in a
> comment, two wrong line citations, dependency versions the repo does not build,
> and five inflated numbers. Each correction is marked inline where it lands
> rather than listed here, so a reader of any one finding sees it. **Every
> quantitative error found ran in the same direction — making the finding look
> bigger** — which is the bias to watch for in the next pass.
>
> §7's five decisions were taken on 2026-09-05 under a single rule — the
> current experience is the baseline and nothing here may change it. That
> withdrew one finding (F9), kept F17 off ROADMAP, and fixed the wire shape for
> the largest item (F23).
>
> Prompted by a sibling project's `SCANNER_PERFORMANCE.md`. That audit's method
> is borrowed wholesale — measure the real tree first, denominate every finding
> in a unit, and record the corrections — but none of its *findings* transfer:
> that project scans many repos with libgit2, this one drives one open repo
> through the `git` binary.

## 1. The one number that matters

LeoGit has no libgit2 dependency. Every git question is a subprocess. So the
first thing to measure is what a subprocess costs, and on macOS the answer
swamps everything else.

Measured on this machine, `/usr/bin/git` 2.50.1 (Apple Git-155), 20 iterations
each, warm page cache:

| command | per call | real work |
|---|---|---|
| `/usr/bin/true` (fork/exec floor) | 2.2 ms | — |
| `git --version` | **8.3 ms** | none |
| `git config --get remote.origin.url` | 8.4 ms | reads 357 bytes |
| `git remote` | 9.6 ms | reads the same 357 bytes |
| `git status --porcelain=2 -z -uall` (11.7k files, 243 MB) | 16 ms | ~8 ms |
| `git log -n 50 --pretty=…` | 15.8 ms | ~7 ms |

**~8.4 ms of every git invocation is process startup.** A second, independent
measurement pass agreed to within 0.2 ms. The consequence sets the whole agenda:

> Making an individual git command cheaper is worth almost nothing. Removing a
> git command is worth ~8.4 ms. **Spawn count is the only lever.**

### The steady-state budget

Frontmost window, one idle repo open, 19 repos in the recents list, default
config (2 s poll, 30 s auto-fetch):

| source | spawns/min | fork+exec cost |
|---|---|---|
| status poll (30 ticks × 2) | 60 | 504 ms |
| auto-fetch (2 ticks × 4) | 8 | 67 ms |
| tier sweeps | 12 | 101 ms |
| **total** | **80** | **~672 ms/min** |

The auto-fetch row is 4 spawns per tick, not 2: `performAutoFetch` is
`fetchActiveRemote()` *then* `refreshStatus()`, and `fetchActiveRemote` spawns
`get_remote` **and** `git fetch`, so the trailing status read is a second
`get_status` on top of the 30 poll ticks.

About 1% of one core, permanently, on process creation alone — and **38 of
those 80 spawns are `git remote`**, a question whose answer lives in a 357-byte
file that changes maybe twice in a repository's life. That is finding F1.

(Costed at the measured 8.4 ms throughout. An earlier draft of this table used
9 ms and 76/36; both are corrected here.)

**After Step 4**, the same minute is: status poll 30 × 1, auto-fetch 2 × 2
(`git fetch` + the trailing status), tier sweeps 6 — **~40 spawns/min, ~336
ms/min**, half the original, with every `git remote` gone.

## 2. What is already correct

Recorded so it is not re-investigated. This subsystem is in good shape; the
findings in §4 are a short list against a long one.

**Scheduling.** `pacedLoop` guarantees non-overlapping runs, immediate cadence
changes, and parking via `dueAt = Infinity` ([pacedLoop.ts](../../apps/tauri-app/src/lib/services/pacedLoop.ts)).
`backgroundPolicy` is the single predicate every loop names rather than
composing its own boolean, ported number-for-number to
`BackgroundSchedulingPolicy` natively. The 2/10/30 s ladder that slows but never
stops for the active repo, while only the multi-repo fan-out pauses outright, is
the right asymmetry and is argued for in-comment.

**The equality gate.** `refreshStatus` compares `JSON.stringify(status)` whole-value
and returns without touching the store when it matches
([MainLayout.svelte:559](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L559)); `DiffStore` and
`RepoDirectoryStore` do the same natively. This is what keeps a 2 s poll from
being felt anywhere in either view tree, and it is load-bearing for everything else.

**Spawn frugality already applied.** `git_dir` resolves `.git` from the
filesystem rather than `rev-parse`, so `merging` rides the poll for free
([git.rs:659](../../core/src/git.rs#L659)). `get_commit_detail` fuses `--raw` and
`--numstat` into one `git log`. `head_paths` batches every path into one
`ls-tree`. The unpushed `rev-list` is skipped when there is nothing to compute.
`repo_sync_status` uses `-unormal` and breaks at the first change record.
`discover_repos` spawns nothing at all and stops descending at repo boundaries.

**The active repo is excluded from every sweep tier** in both clients, its badge
synthesised from the status already in hand. This is the single best cost
decision in the design and it removes a whole class of duplicate work.

**Bounded output.** `get_log` always passes `--max-count`. The diff size guard
is shared core-side (4 MiB / 5000-byte line) with a "show anyway" escape.
Highlighting caps at 20 000 lines and 1024-byte lines.

**Network correctness.** Two-layer time-boxing with separate background (8/8/12 s)
and UI (15/30/600 s) budgets. Both pipes *are* drained on helper threads
([process.rs:188](../../core/src/process.rs#L188)), so the classic `try_wait`
pipe-buffer deadlock is prevented in the ordinary case — but see F13, and note
that **it is not covered by a regression test**. `process::tests` holds exactly
three: a quick command, a CR/LF split over 5 bytes, and a `sleep 30` that writes
nothing. Nothing in the repo drives a child chatty enough to fill a 64 KiB pipe.
TECHNICAL.md:1242 names `run_timed_kills_a_hung_child_promptly` as the test for
this; it is not, and an earlier draft of this section repeated that claim rather
than checking it. A circuit breaker with exponential backoff that correctly *downgrades
to a local recompute* rather than skipping, so the dirty dot keeps working
offline. Failures are never cached as answers. `has_remote` gating stops a
remote-less repo from poisoning the breaker app-wide. `silentFetch`'s
three-valued return distinguishes "no attempt" from "attempt failed".

**Frontend rendering.** Both sidebar lists are virtualized in Tauri; every long
list is `List`/`LazyVStack` natively. Diff highlighting is debounced 80 ms
behind a request token checked on both sides of the await. In-flight
coalescing exists in every store that fans out, and `RepoIdentifierStore` is
bounded to 4 concurrent lookups.

## 3. Hypotheses that measurement killed

Recorded because each is plausible, each would have produced work, and each is
wrong. This is the section to read before proposing anything in this area again.

- **"`--untracked-files=all` on the 2 s poll is expensive."** No. Warm and
  alternating, `-uall` costs 1–3 ms more than `-unormal` on an 11.7k-file repo.
  The poll already uses the cheap mode where it can afford to.
- **"`GIT_OPTIONAL_LOCKS=0` blocks index writeback, so every poll re-hashes."**
  No. Built a 400 MB / 200-file repo, invalidated every mtime, polled five times
  with and without the flag: 15.4 ms, flat, no divergence. The flag costs nothing
  measurable and its `index.lock` protection is worth keeping.
- **"Large repos are slow to poll."** No. My first reading of that repo was
  394 ms and it was **cold page cache**; warm it is 16 ms, and the 690 ms
  first-touch is a once-per-boot cost no design change can avoid. My initial
  `-uall` vs `-unormal` comparison was confounded the same way — `-uall` ran
  first, cold. *Any measurement in this area that does not warm up first is wrong.*
- **"A filesystem watcher would beat the 2 s poll."** Not on performance grounds,
  and this is the useful conclusion. A poll tick is ~24 ms of which ~17 ms is
  process startup; a watcher removes the *scheduling*, not the cost, and buys a
  cross-platform FSEvents/inotify/ReadDirectoryChangesW dependency, a
  rename-storm debounce, and a fallback for network mounts. **Deliberately not
  proposed.** If the poll ever needs to get cheaper, F1 takes ~40% off it for
  two days' work and no new failure modes.
- **"`reqwest::Client` should be pooled."** Technically true — all three sites
  build per request — but #1 fires once per launch and the other two are usually
  loopback Ollama with no TLS. Near-zero. The real connection-reuse win is SSH
  multiplexing (F3), because every GitHub round trip goes through `git`/`gh`.

## 4. Findings

Ranked by measured cost × confidence. Every one is invisible to the user except
where noted — nothing here trades UX for throughput.

### Tier 1 — efficiency, no behaviour change

**F1. `git remote` runs on every status poll, every badge sweep, and every
auto-fetch.** [git.rs:690](../../core/src/git.rs#L690), called unconditionally
from `read_status` ([git.rs:956](../../core/src/git.rs#L956), *before* the
`bytes.is_empty()` early return, so no input shape skips it) and
`repo_sync_status` ([git.rs:2475](../../core/src/git.rs#L2475)). **A third site
is `git::get_remote`** ([git.rs:2739](../../core/src/git.rs#L2739)), its own
command, called on every auto-fetch tick from `MainLayout.svelte:1106` and
natively by `SyncStore.silentFetch`. A fix that touches only the first two
leaves the auto-fetch path spawning `git remote` twice a minute forever.

The poll's two spawns are `git status` and `git remote`, and the second answers
only `has_remote` plus the first remote's name. Measured, it is **38–44% of the
tick's git cost** depending on repo size — 9.6 / (9.6 + 16) on the 243 MB repo
in §1, 9.8 / (9.8 + 12.7) on an ordinary one, where `git status` is itself
mostly startup and the ratio is worst. That is a `.git/config` read wearing a
9.6 ms process. In steady state it is **38 spawns/min, ~319 ms/min**, per open
window — doubled when both clients run.

The sweep cost is larger than the tier ceiling suggests: `syncVisibleRepos` is
called with the **whole discovered list**, not the 19 tiered repos. Discovery on
this machine returns 76, so a full visible sweep is ~76 × 9.6 ≈ **730 ms** of
`git remote` alone.

The file already solves this exact shape one function earlier: `git_dir` reads
`.git` from the filesystem, and its comment says *"That path costs no
subprocess, which is what lets `get_status` carry `merging` for free on a 2 s
poll."* The same move has been made three times before in this codebase
(TECHNICAL.md:1421-1428, ROADMAP.md:46 and :50 — one of them specifically
retiring a `git remote`), so this is institutional practice, not a novelty.

*Fix:* parse `[remote "…"]` sections from `.git/config` directly, falling back to
`git remote` when the file is unreadable or uses `include`/`includeIf`. Cheapest
correct version caches per repo path keyed on the config file's mtime+len.
*Risk:* low, but with one hole worth naming up front: **for a linked worktree
`git_dir()` returns `.git/worktrees/<name>`, whose `config` does not hold
remotes** — they live in the common dir. Without a `commondir` read, every
worktree silently takes the fallback and the win there is zero. `include` and
`includeIf` directives are the other reason the fallback has to exist.
*UX:* none. Removes ~40% of the poll's git cost.

*Doc collision to resolve with the fix:* TECHNICAL.md:1913 calls this "the cheap
signal (`get_status` computes it from the same `git remote` call it needs for the
ahead/behind fallback)". Both statements are true — the fallback *uses* the call
conditionally, but the call itself is unconditional — and the line must be
rewritten alongside the fix or a reader will take it as a refutation.

**F2. No per-repo fetch cooldown anywhere.** Verified by grep across the core and
both clients: no `lastFetchedAt`, no cooldown, in any network path. The only
timestamps are pass-level throttles (`REFOCUS_THROTTLE_MS`,
`LIST_SWEEP_THROTTLE_MS`), not per-repo.

Redundant round trips this permits:
1. Tier 0 fetches the top four at *t*; at *t+31 s* the user alt-tabs and
   `kickTopTier` refetches all four. The tier says two minutes; refocus delivers 30 s.
2. Switching A → B → A refetches A seconds later.

*Not* a case, though an earlier draft claimed it was: the repo-switch path does
**not** double-fetch. `pacedLoop.start()` sets `lastRunAt = Date.now()`
([pacedLoop.ts:103](../../apps/tauri-app/src/lib/services/pacedLoop.ts#L103)) and
`MainLayout.svelte:1679` says why in as many words — *"Re-start rather than
reschedule: the new repository's cadence is measured from the fetch that just
opened it."* The clock is reset deliberately.

*Fix:* one `Map<path, lastFetchedAt>` consulted in `syncRepo`
([repoSync.ts:66](../../apps/tauri-app/src/lib/stores/repoSync.ts#L66)) and `sync`
([RepoDirectoryStore.swift:343](../../apps/swift-ui-app/Sources/LeoGit/Stores/RepoDirectoryStore.swift#L343)),
with a window shorter than the shortest tier — 60–90 s. Only *successful*
fetches are stamped, so an unreachable remote still retries immediately.
**Those two entry points do not cover the active repo**, whose fetches go
through `fetchActiveRemote` (Tauri) and `silentFetch` (Swift); the cooldown has
to be consulted there too, which is the same plumbing F15 needs.
*Risk:* low. The one thing to get right is that a **user-initiated** fetch must
never consult the cooldown.
*UX:* none, by construction — the window is shorter than the cadence that would
otherwise refresh the badge. Note this does not undo the refocus-fetch behaviour
ROADMAP.md:127 records as a deliberate win; it deduplicates it.
*Prior art in-repo:* QUICK-WINS.md:90-91 and ROADMAP.md:187 already sketch the
same `lastFetched` timestamp — the latter as a user-facing "last fetched…"
caption, which this would supply for free.

**F3. No SSH connection multiplexing.**
[git.rs:528](../../core/src/git.rs#L528) builds `GIT_SSH_COMMAND` as
`ssh -o ConnectTimeout={n} -o BatchMode=yes` and nothing else. Grep for
`ControlMaster`/`ControlPersist`: zero hits.

Every background fetch therefore pays a full TCP handshake + SSH key exchange +
public-key auth. A 19-repo sweep is 19 complete handshakes, every 2/5/10 minutes.
Adding `-o ControlMaster=auto -o ControlPath=<socket> -o ControlPersist=60`
collapses a whole sweep onto one connection per host.

*Risk:* moderate, and the highest of the Tier-1 items. `ControlPath` must live
in a directory whose full path stays under the ~104-byte `sun_path` limit
(hash the host, don't template `%h/%p/%r` under a long prefix), stale sockets
need handling, and this must not apply to the interactive terminal panel.
Windows OpenSSH does not support `ControlMaster` at all, so it must be
platform-gated.
*UX:* faster background badges; nothing visible.
*Size of the win is unmeasured* — it depends entirely on how many of the user's
remotes are SSH rather than HTTPS. **Measure before building** (§6).

**Deferred by the implementation pass (2026-09-05): unmeasurable here.** Every
one of the 71 remotes under `~/Dev` on this machine is HTTPS, so the harness
has nothing SSH to time and the win cannot be sized. Left as written for a
machine that has SSH remotes; the research also notes that OpenSSH's
ControlPersist master `setsid`s and, only under `ssh -v`, keeps our stderr
pipe — so if this is ever built, F13's bounded join is what makes it safe.

**F4. The Tauri `repo_sync_status` shim pins a tokio core worker.**
[shims/git.rs:123](../../apps/tauri-app/src-tauri/src/shims/git.rs#L123) is
`#[tauri::command(async)]` on a *sync* fn whose body can run a 12-second-budget
`git fetch`.

I verified the mechanism against the dependency source rather than trusting the
repo's own comment. Against **the versions `Cargo.lock` actually pins** — `tauri`
2.11.5 and `tauri-macros` 2.6.3 — `wrapper.rs:389` emits
`resolver.respond_async_serialized(async move { let result = $path(args); … })`
→ `respond_async_serialized_inner` (`tauri-2.11.5/src/ipc/mod.rs:371`) calls
`async_runtime::spawn` → `tokio::spawn` (`async_runtime.rs:279`). That is the
**multi-threaded core pool**, not `spawn_blocking`. The sync body runs inline
inside the spawned future, so it holds a core worker for its whole duration.
(Tauri labels this case `"sync_threadpool"` in its tracing span at
`wrapper.rs:264`, which is misleading — it is the core pool.)

*Correction to my own method:* my first pass read `tauri` 2.11.2 and
`tauri-macros` 2.6.2, stale copies that happen to sit in `CARGO_HOME` beside the
pinned ones. The emission is identical in both, so the conclusion survives — but
"verified against the dependency source" was, for one draft, verified against
the wrong source.

The repo's own rule at [process.rs:116](../../core/src/process.rs#L116) states
exactly this, and the SwiftUI FFI already obeys it —
[ffi/src/lib.rs:1737](../../apps/swift-ui-app/ffi/src/lib.rs#L1737) wraps the same
function in `spawn_blocking`. Only the Tauri host diverges. `gh_repo_list` has
the same shape with a 20 s budget.

*Fix:* `async fn` + `process::run_blocking`, matching the Swift host — applied
to every `(async)` sync command that can block ≥100 ms, which turned out to be
seven, not one (see Step 1). The ffi comments claiming *"The Tauri host makes
the same hop implicitly via `#[tauri::command(async)]`"* were false by this
finding and are rewritten.
*Risk:* none. Reference implementation is already in the tree.
*Doc:* ROADMAP.md:156's sweep of sync commands should record that `(async)` is
**not** the finish line for a sync fn with a 12 s budget.
*UX:* on a 2-core machine this is half the command bus held for up to 12 s while
the status poll and diff loads queue behind it.

**F5. `sort_file_entries` allocates two `String`s per comparison.**
[git.rs:1750](../../core/src/git.rs#L1750) — `a.path.to_lowercase().cmp(&b.path.to_lowercase())`
inside a sort comparator, so it runs O(n log n) times per status read. On a repo
with a large untracked directory not yet in `.gitignore` (a fresh `node_modules`),
50k entries is ~780k comparisons — **~1.56M String allocations every 2 seconds**
— alongside 50k `symlink_metadata` calls from `stat_stamp`, whose comment at
[git.rs:1176](../../core/src/git.rs#L1176) assumes "the list is short", which is
exactly the assumption this breaks.

*Fix:* `sort_by_cached_key` — one lowercase per entry instead of one per
comparison. A file-count ceiling on the `-uall` result was considered and
rejected (§7 Q1) because it changes what the Changes tab shows.
*Risk:* none. *UX:* none. Removes a genuine cliff.

**F6. `scan_for_repos` stats every child.**
[git.rs:3059](../../core/src/git.rs#L3059) uses `fs::metadata` on every non-hidden
entry, files included, where `entry.file_type()` answers from the `dirent`
`read_dir` already returned with no syscall on macOS and Linux.

Measured by simulating the walk on this machine's real config (`~/Dev`, two
sumup subdirs, `~/Documents`, depth 3), and reproduced independently:

| depth | entry `metadata` | `.git` probes | total | after the fix |
|---|---|---|---|---|
| 3 (default) | 370 | 160 | **530** | ~161 |
| 10 (max) | 37 894 | 3 008 | **40 902** | ~3 009 |

Depth 10 costs 77× the syscalls of depth 3 and, on this machine, finds **zero**
extra repos — the setting is a foot-gun with no cushion. The fix removes the
entry stats but not the `.git` probes, so depth 3 goes 530 → ~161 (3.3×), not
the 6.3× an earlier draft claimed by forgetting the probes.

*Fix:* `entry.file_type()`, with `fs::metadata` retained only for the symlink
case that needs following.
*Risk:* low; changes symlink handling subtly, so it needs the existing discovery
tests plus a symlinked-project-folder case.
*UX:* none.

**F7. `run_diff` re-runs `has_commits` on every call, and `get_selected_diff`
calls it in a loop.** The `has_commits` call is inside `run_diff`
([git.rs:1220](../../core/src/git.rs#L1220)); the loop is in `get_selected_diff`
([git.rs:1336](../../core/src/git.rs#L1336)). The AI "Generate commit message"
path over 30 changed files is 60 spawns ≈ 0.5 s of pure fork/exec before any
diffing happens.
*Fix:* hoist it out of `run_diff` — 2N becomes N+1. Not a two-line change, since
it means altering `run_diff`'s signature, which `get_diff` also calls.
*Risk:* low. *UX:* Generate gets faster.

**F8. `syncByPath` is a two-valued cache.**
[RepoDirectoryStore.swift:349](../../apps/swift-ui-app/Sources/LeoGit/Stores/RepoDirectoryStore.swift#L349)
writes nothing on failure, and the sweep's miss test is a bare
`syncByPath[path] == nil` ([:269](../../apps/swift-ui-app/Sources/LeoGit/Stores/RepoDirectoryStore.swift#L269)).
So a listed repo whose `repo_sync_status` *errors* — a directory that stopped
being a repo, a permissions change, a stale MRU entry — is re-probed on every
sweep forever, unthrottled, inside a sequential loop that every other badge
waits behind.

The sibling store one file over is the reference implementation for exactly this:
`RepoIdentifierStore` is explicitly three-valued and uses `updateValue` so a
stored `nil` is not mistaken for "not looked up".
*Fix:* store a failure marker; re-probe on a longer cadence. *Risk:* low. *UX:* none.

**F9. Repo discovery is unthrottled on picker open — withdrawn (§7 Q2).** Tauri
re-walks on every dropdown open ([MainLayout.svelte:1730](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L1730));
natively on every popover open ([RepoSwitcher.swift:93](../../apps/swift-ui-app/Sources/LeoGit/Screens/RepoSwitcher.swift#L93)).
Both have an in-flight guard but no time throttle — while the badge sweep beside
them is throttled to 30 s. Five opens in ten seconds is five full walks (~530
stats each, plus a `repos-state.json` read).

The 30 s throttle this finding first proposed is rejected. Both pickers show
the cached list the instant they open and let the walk publish into it, so the
walk never delays the picker, and the freshness it buys is the documented
reason it runs on open at all. Kept here so the next reader does not
re-propose it; the per-walk cost is F6's problem.

**F10. Duplicate `git log` on repo switch (Tauri).**
[MainLayout.svelte:2124](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L2124):

```js
$effect(() => {
  if ($repoState.activeTab === 'history' && !$repoState.log.loaded) {
    loadInitialLog()
  }
})
```

Traced in source: `resetRepoState()` clears `log.loaded` while deliberately
preserving `activeTab`; the `await patchReposState` on the next line yields, the
effect flushes and fires `loadInitialLog()`; then line 1677 calls it again inside
`Promise.all`. `loadInitialLog` has no in-flight guard and `loaded` only flips on
*resolve*, so each intervening `repoState` publish can re-fire it, whenever
History is the visible tab.

**The count is a source-derived upper bound, not a measurement.** Svelte 5
batches effect flushes within a microtask, so a re-fire needs a distinct flush
cycle; whether the real number is 2 or 4 wants a runtime trace. At least 2 is
certain, since both call sites are unconditional.

This is also the one effect in the file that departs from the file's own stated
rule — its three siblings all use a derived key read under `untrack`, with the
reason written at [:2163](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L2163):
*"reading `repoState` inside the branch would re-run this on every status tick."*
*Fix:* give it the derived-key shape its siblings use, plus an in-flight guard on
`loadInitialLog`. *Risk:* low. *UX:* also removes duplicated `log.resetSeq`
increments, each of which scrolls the list to row 0.

**F11. `unpushedShas` rebuilds a `Set` per row per redraw.**
[HistorySidebar.swift:50](../../apps/swift-ui-app/Sources/LeoGit/Screens/HistorySidebar.swift#L50)
is a computed property read inside the `List` row builder at `:99`. With ~30
mounted rows and a branch 300 commits ahead, ~9 000 string hashes per repaint,
and the list repaints every 10 s from the relative-date tick.
*Fix:* hoist to a `let` above the `List`. One line. *Risk:* none.

**F12. `PathText` forces ~8 synchronous layouts per row, per resize frame.**
[PathText.svelte:77](../../apps/tauri-app/src/lib/components/PathText.svelte#L77):
`measureParts` writes `textContent` on two spans then reads both rects — a
write→read thrash — inside a binary search over path length. Only the *first*
read of each pair forces a layout (no intervening write), so one fit is
~log₂(len)+1 ≈ 8 forced reflows, not the ~14 an earlier draft claimed by
counting rect reads. `PathText` is instantiated per row, so ~36 rows ≈ ~290
forced layouts per drag frame.

`Terminal.svelte` debounces its `ResizeObserver` by 80 ms for exactly this class
of reason; `PathText` has no equivalent.
*Fix:* **debounce `PathText`'s observer.** Note that rAF-coalescing the
`sidebarWidth` write — the other obvious idea — probably buys nothing:
`Terminal.svelte:51`, the very precedent cited here, says *"A panel drag fires
ResizeObserver every frame"*, and RO delivery is already frame-coalesced by
spec, so batching the write does not reduce how often `fit()` runs.
*Risk:* low. **Measure first** — this wants a devtools trace during a drag with
~40 changed files before anyone spends effort on it.

**Deferred by the implementation pass (2026-09-05): not measured.** The
harness measures the core, not the WebView, and a debounce on a resize
observer is a change the user could feel (a path that re-fits 80 ms after the
drag stops) — exactly the kind of trade this plan does not make on argument
alone. Needs the trace first.

### Tier 1b — startup, IPC and persistence

A second sweep covered the areas the first four audits did not reach: the
terminal/PTY subsystem, config persistence, the highlight pipeline, the IPC and
FFI boundaries, and startup. Two of the findings are larger than anything above.

**F22. `fix_path_env` costs ~430 ms of every launch, blocking, before anything
else exists.** [process.rs:81](../../core/src/process.rs#L81) spawns
`$SHELL -ilc 'echo -n "$PATH"'` and waits. Called first thing in both hosts —
`main.rs:13` before `tauri::Builder`, and `LeoGitApp.swift:21` in `App.init`
before the scene body.

Measured here, 5 runs each:

| flags | cost |
|---|---|
| `-ilc` (current) | **397–497 ms**, ~434 ms typical |
| `-lc` | ~53 ms |

The `-i` costs ~380 ms of every cold start. On this machine it buys nothing —
the *set* of unique PATH entries is identical between the two, the extra 14
being duplicates — but dropping it is a real compatibility gamble, because
nvm/pyenv/rbenv commonly append to `PATH` from `.zshrc` rather than `.zprofile`.

The obvious async fix is forbidden by the function's own soundness contract: it
calls `set_var`, which is only sound before any other thread exists
([process.rs:71](../../core/src/process.rs#L71)).
*Fix:* cache the probed `PATH` keyed on the rc files' mtimes, use the cached
value at startup, and re-probe in the background for the *next* launch. Keeps
`-i`, keeps the contract, removes it from the critical path.
*Risk:* moderate — needs a correct staleness key and a first-run path.
*UX:* ~0.4 s off every cold start. **The largest single item on the startup path.**

**F23. The `FileDiff` round-tripped into `highlight_diff` is ~90% dead weight,
and its uphill leg is parsed on the macOS UI thread.**

`highlight_from_blobs` ([highlight.rs:159](../../core/src/highlight.rs#L159)) is
the normal path — both clients always supply `source`. Verified by reading it:
it touches only `old_path`, `new_path`, and per line `line_type` /
`old_line_no` / `new_line_no`, then **re-reads the content from git**
via `read_blob`. It never reads `line.content`, `line.text`, `file_header` or
`intra_line_diff`. Only the fallback `highlight_from_diff_lines` uses `content`.

Measured on a real 3190-line diff from this repo: the `FileDiff` crosses as
531 KiB of JSON, of which the fields `highlight` actually reads are 55 KiB —
**476 KiB of dead weight per crossing**, ~2.3 MiB total for one file selection
across all four legs.

The uphill leg is the expensive one and it is not off-main. In the vendored
sources, `tauri-2.11.5/src/ipc/protocol.rs:527` does
`serde_json::from_slice::<serde_json::Value>(&body)` inside
`parse_invoke_request`, reached from the `Method::POST` branch at
`protocol.rs:62`, which wry calls synchronously from
`webView:startURLSchemeTask:` (`wry-0.55.1/.../url_scheme_handler.rs:319`) — a
main-thread WebKit callback. So a throwaway `Value` tree is built on the UI
thread, then materialised a third time on the worker. Order 3–8 ms per file
selection, linear in diff size, and the size guard permits ~100k lines behind
"Show diff anyway".

The amplifier: the diff reload key includes `stat_stamp`
([MainLayout.svelte:1300](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L1300)),
so **every save of the open file re-runs the whole pipeline** — an editor
autosaving drives this every few seconds.

The project already knows about the round trip — TECHNICAL.md notes `FileDiff`
"deliberately stays lean … because the frontend round-trips it back into
`highlight_diff`". The optimisation stopped one step short: it kept derived
artifacts *off* `FileDiff` but never asked whether `highlight_diff` needs
`FileDiff` at all.
*Fix as first written:* pass a lean descriptor (paths + the line-number/type
triples) rather than the whole `FileDiff`; the fallback answers `NeedContent`
and the client re-sends the full `FileDiff` (§7 Q5).

**Correction found while mapping the pipeline for implementation — the
premise is wrong for Tauri.** `highlight_diff` there does not return tokens; it
returns HTML, built by `render::highlighted_html`
([render.rs:27](../../core/src/render.rs#L27)), whose per-row renderer reads
`line.content` and `line.intra_line_diff`. The "dead weight" is dead for
*tokenizing* and load-bearing for *rendering*, and this finding never looked
at `render.rs`. A lean request cannot serve Tauri's call as it stands. Nor can
the backend rebuild `content` from the blobs it reads: for the working tree
those blobs are read *after* the diff was taken, and an autosave in between —
the very amplifier named above — would render text that disagrees with the
rows on screen. Only the tokenizing half of the claim survives, and that is
the Swift client, whose crossing is ~188 KiB of memcpy on a concurrent
executor — the part this finding itself rated "much cheaper".

Two smaller facts from the same pass: the 55 KiB figure assumed a compact
encoding — with default serde field names the lean rows are ~187 KiB, a 65%
cut, and only a tuple/parallel-array shape reaches ~44 KiB; and UniFFI 0.32
generates the `NeedContent` enum as a plain Swift `enum` with associated
values (this repo's `BlobSource` already ships that way).

**Deferred — §7 Q6.** The options are a decision about where rendering
happens, not an efficiency detail, and the harness cannot measure a WebView.

**F24. Config and state writes are non-atomic, unsynced, and `config.toml`
cannot self-heal.** [config.rs:458](../../core/src/config.rs#L458) and
[:540](../../core/src/config.rs#L540) both use `fs::write` — `File::create`
(truncate) + `write_all`. No temp-file + rename, no `sync_all`. The only
`tempfile`/`fs::rename` in the repo are under `#[cfg(test)]`.

The sharp edge is an asymmetry: `update_state` explicitly self-heals a corrupt
state file ([config.rs:554](../../core/src/config.rs#L554), with a comment
saying why), but `load_config` hard-errors on a parse failure
([config.rs:448](../../core/src/config.rs#L448)) — and `patch_config` calls
`load_config()` first. So a torn `config.toml` cannot be repaired from Settings;
the user must delete the file by hand.

Compounding it, `CONFIG_LOCK` is a process-local `static Mutex<()>` whose own
doc comment says *"Two clients share this file and each runs its commands
concurrently"* — precisely the case a process-local mutex cannot address.
TECHNICAL.md calls these "the backend's atomic writers", which overstates it.
Files are small (449 B / 2763 B measured) so the window is ~100 µs, but it is
real, and F25 widens it.
*Fix:* temp-file + `sync_all` + `fs::rename`; give `load_config` the same
self-heal `update_state` has.
*Risk:* low. *UX:* none, until the day it saves a config.

**F25. Two full state-file read-modify-writes per repo switch.**
`patch_state` then `record_recent_repo` — Tauri
[MainLayout.svelte:1670](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L1670)
and `:1674`, natively `RepoDirectoryStore.swift:225` and `:229`. Each is a full
2763-byte read + parse + serialize + truncate-write. With the picker's own
`known_repos` read, one dropdown switch is 3 reads + 2 truncating writes of one
file. `update_state` already takes a closure, so one combined call halves both
the writes and F24's windows. *Risk:* low. *UX:* none.

**F26. `write_terminal` can block the Tauri main thread on the session mutex.**
[shims/terminal.rs:36](../../apps/tauri-app/src-tauri/src/shims/terminal.rs#L36)
is deliberately sync — keystroke ordering, and the reasoning is sound — and its
comment says the write is "microseconds". But it must first take
`session.lock()`, and `close_terminal` holds that same per-session mutex across
portable-pty's SIGHUP → grace → SIGKILL escalation (~250 ms). Core's comment
acknowledges the mutex is held but not that a main-thread `write_terminal`
blocks on it. Narrow — typing during a panel close — but real.

**F27. `CommitInfo.body_without_coauthors` duplicates `body`.**
[git.rs:153](../../core/src/git.rs#L153). On a repo with no co-author trailers
it is byte-for-byte identical to `body`, worth ~33% of the log payload; measured
on this repo (an outlier — 61% of commits carry a trailer) the two fields
together are 66% of a 286 KiB log payload.

The fix already exists one type over: `DiffLine.text` is `Option<String>`,
`None` when it would duplicate `content`, with a comment
([diff.rs:17](../../core/src/diff.rs#L17)) explaining it "duplicated `content`
byte for byte, once per line of every diff, in both clients' memory and across
both wires". `CommitInfo` never got the same treatment.
*Fix:* make it `Option<String>`, `None` when equal to `body`. *Risk:* low.

**F28. `has_commits` is a spawn on every history page and every file
selection.** Found by the Step 0 harness: `get_log` is 2 spawns, not 1, and
`get_diff` is 2, because both first run `git rev-parse --verify --quiet HEAD`
([git.rs:644](../../core/src/git.rs#L644)) to decide between `HEAD` and the
empty tree. That is ~8 ms of a 16.6 ms file click and of a 19.9 ms history
page, for a question the filesystem answers: `HEAD` names a ref, and the ref
exists as a loose file under `refs/heads/` or as a line in `packed-refs`, or
`HEAD` is a detached sha. Same move as `git_dir()` one function over.
*Fix:* resolve from the filesystem (through the common dir, so worktrees work),
falling back to the spawn whenever the repo looks unusual — a reftable ref
store (`refs/heads` is not a directory, or a `reftable/` dir exists), a
symbolic ref chain, an unreadable `HEAD`. *Risk:* low with the fallback.
*UX:* none; file selection and history load lose one spawn each.

**F29. `run_timed` quantises every network op to 50 ms.** Found by the
harness: it waits with `try_wait` + `sleep(50 ms)`, so a fetch that finishes in
120 ms is observed at 150 ms, and a user-initiated pull or push carries up to
50 ms of pure sleep. *Fix:* rides on F13, whose reader/waiter redesign replaces
the poll with a blocking `wait()` on a helper thread signalled over a channel,
so the timeout is a `recv_timeout` and completion is observed immediately.

**F30. `discover_repos` costs 0 spawns and 0.9 ms** for 70 repos at depth 3.
Not a finding to fix — recorded because it retires the last argument for the
withdrawn F9 throttle: there is no process cost in that walk at all.

**The terminal/PTY subsystem is clean** and should not be re-investigated.
Output is batched three ways — a self-tuning coalescer in core (lone chunk
immediate, burst gathered to 8 ms / 256 KiB), a second locked-buffer batching
layer in the Swift relay, and a bounded `sync_channel` that back-pressures the
shell rather than dropping output. Swift routes writes through a serial
`ioQueue`, not the main actor. Terminal output over the Tauri `Channel` was
measured at 1.18× JSON inflation for colorized `git log` and 1.46× for a dense
TUI frame — not worth a binary transport.

**Config's read side is clean.** No `load_config()` on any timer, frame, row or
keystroke; both clients own one in-memory store with exactly three reload sites
(launch, wake, Settings).

**Tauri's response side is clean** — `serde_json::to_string` runs on a tokio
worker via `respond_async_serialized`, off-main the whole way. **UniFFI lifting
is off the MainActor** — `@concurrent` on 59 bridge functions is the required
opt-out under `SWIFT_DEFAULT_ACTOR_ISOLATION: nonisolated`, and the decode
happens inside the concurrent body, so what reaches the main actor is a finished
value.

### Tier 2 — robustness, found on the way

Not efficiency, but all four are in the network path and two are hangs.

**F13. The timeout is not a backstop — the reader-thread join is unbounded.**
[process.rs:252](../../core/src/process.rs#L252):

```rust
if status.is_none() {
    // Killing the child closes its pipe write ends, so the reader threads'
    // `read_to_end` returns and the joins below don't hang.
    let _ = child.kill();
    let _ = child.wait();
}
let stdout = out_reader.join().unwrap_or_default();
```

**The comment's premise is false, and I proved it.** `prepare_child` sets no
process group (grep for `pre_exec`/`setsid`/`process_group`: zero hits), and
`git fetch` does not talk to the network itself — it spawns `ssh` or
`git-remote-https`, and those grandchildren inherit our pipe write ends.
`child.kill()` kills `git` only.

Experiment: a parent that spawns a lingering grandchild sharing stderr, then
`kill()` + `wait()` on the parent. `read_to_end` **did not return** — still
blocked after 3 s, and would stay blocked for the grandchild's lifetime.

**It is specifically `err_reader.join()` at
[process.rs:258](../../core/src/process.rs#L258) that hangs**, not the stdout
join above it: `ssh`'s *stdout* is a pipe to `git`, so that write end closes when
git dies, but its *stderr is inherited from git* — our pipe. That also matches
the experiment, which blocked on stderr.

So the hard timeout that exists specifically as the backstop *"in case a
transport ignores the knobs and wedges anyway"* does not bound anything in
exactly that case. It burns a thread — or, via F4, a **core worker** — and for a
user-initiated pull/push/clone the `invoke` never resolves, `endNetworkOp()`
never runs, and the spinner is permanent.
*Fix:* `setsid` in a `pre_exec` and `killpg` the group; or bound the joins.
*Reachability:* narrow but real — HTTPS mostly self-heals on EOF; SSH is the
exposure, which is F14.

**This contradicts two live claims in the canonical docs**, and fixing it means
fixing them: TECHNICAL.md:1242 says `run_timed` "kills the child if it outlives
`timeout`" and names `run_timed_kills_a_hung_child_promptly` as the proof
(that test's child is `sleep 30`, which has no grandchildren and so cannot catch
this); ROADMAP.md:123 says it "kills **any subprocess** that outlives its
budget". Both are false as written.

**F14. SSH has no post-connect timeout.** Same line as F3.
`ConnectTimeout` bounds the TCP connect and banner exchange only; once the
session is up, ssh has no keepalive. A remote that accepts then goes silent
(captive portal, VPN drop, a firewall dropping an established flow) leaves `ssh`
alive indefinitely — which is precisely the orphan that wedges F13. HTTPS
already has the equivalent via `http.lowSpeedLimit`/`lowSpeedTime` on the very
same command; SSH just never got it.
*Fix:* `-o ServerAliveInterval=5 -o ServerAliveCountMax=3`. One string. Closes
the common path into F13 and is worth doing **before** F13's larger change.

**F15. Neither client claims the network slot for a silent fetch.**
[MainLayout.svelte:1117](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L1117)
calls `gitApi.fetch` directly, bypassing `syncRepo`'s in-flight set;
[SyncStore.swift:91](../../apps/swift-ui-app/Sources/LeoGit/Stores/SyncStore.swift#L91)
*reads* `activeOperation` but never sets it. Four triggers can overlap natively —
`warmUpFetch` (`ContentView.swift:671`), the fetch loop tick (`:719`), the wake-up
resync (`:766`) and the connectivity-recovery kick (`:797`); `resyncOnWake` guards
against *itself* via `isResyncing`, but nothing stops it overlapping a tick. So two
`git fetch` can run in one `.git`. The loser hits a ref lock, returns failure,
and is charged to the connectivity breaker as a *network* failure. Two of those
open the breaker and suppress background fetching app-wide for 30 s+.

That is the same breaker-poisoning class that the `has_remote` gate and
`silentFetch`'s three-valued return were both written to prevent, arriving by a
third door. Severity is one notch below the audit's framing — one collision
produces one failure, and the threshold is two — but it is self-inflicted.
*Fix:* route silent fetches through the same in-flight guard.

**F16. `run_claude` is unbounded.**
[ai.rs:208](../../core/src/ai.rs#L208) — `.output().await.ok()` with no timeout
and no `kill_on_drop`, so `claude --version` and `claude auth status` can hang
forever. `generate_claude` does it correctly 140 lines later. Low severity (an
async future, not a pinned thread) but trivially fixed.

**F17. No cancel for user-initiated transfers.** Once a pull/push/clone starts,
the only exits are completion or the 600 s cap. A user on bad wifi has a dead
action cluster for up to ten minutes. This is a **feature**, not a fix — it needs
a UI affordance in both clients — so it is not proposed here (§7 Q3).

It is also **not a discovery**: DESIGN.md:180, TECHNICAL.md:1046, ROADMAP.md:78
and STYLE.md:242 all record "there is no cancel" as settled design, the last of
them building a UI convention on it. Any ROADMAP entry would have to supersede
four existing statements, not fill a blank. §7 Q3 leaves it alone unless a user
actually asks.

### Tier 3 — correctness, found on the way

Not efficiency at all. Recorded because the audits surfaced them and they are
more serious than most of Tier 1.

**F18. `RepoStore.open()` publishes the previous repo's `status` under the new
`repoPath`.** [RepoStore.swift:196](../../apps/swift-ui-app/Sources/LeoGit/Stores/RepoStore.swift#L196)
sets `repoPath`, `repoName`, and clears `commits` with eight lines explaining
*why* history must be dropped with its path — then never applies that same
argument to `status`. For the width of one `git status` + `git log`(100)
awaited together (easily 100–500 ms), the Changes tab's file list and count, the
ahead/behind text, the Branch menu's current branch, and `SyncControls`'
proposal are all the old repository's. The proposal is live, so ⌘P in that
window acts on the previous repo's proposal against the new repo's path. History
is fully protected by `historyLoaded`; Changes has no equivalent gate.

**F19. `BranchStore.load` has no generation or path guard.**
[BranchStore.swift:31](../../apps/swift-ui-app/Sources/LeoGit/Stores/BranchStore.swift#L31)
— `if let fresh = try? await GitBridge.branches(in: repoPath) { branches = fresh }`.
The store does not record which repository the list describes, and `list_branches`
is a blocking `@concurrent` call that cancelling the owning task cannot
interrupt. So repo A's branches can land after repo B has opened. The toolbar
menu reloads on open, which narrows it; the menu-bar Branch menu has no such
hook (`BranchMenu.swift:63` passes an onOpen closure, `AppMenus.swift:127` does
not). Picking a row then runs `git checkout <A-branch>` against repo B.

The stores that *do* carry a generation counter — `RepoStore`, `DiffStore`,
`CommitDetailStore`, `SyncStore`, `AppConfigStore` — are the precedent to copy.
(An earlier draft claimed *every* other store has one; that is false —
`CloneStore`, `LaunchStore`, `RepoDirectoryStore`, `RepoIdentifierStore`,
`SettingsStore` and `UpdateStore` do not. The finding does not need the flourish.)

**F20. `default_scan_paths()` collides with itself on a case-insensitive
volume.** [config.rs:211](../../core/src/config.rs#L211) returns
`["~/Dev", "~/dev", "~/code", "~/Code", "~/Projects", "~/src"]`. Verified on this
machine: both `/Users/leo/Dev` and `/Users/leo/dev` resolve, and `realpath` does
**not** normalise case — `/Users/leo/dev/LeoManrique` stays lowercase.
`discover_repos` dedupes discovered *repos* through a `HashSet<String>` of
canonical paths ([git.rs:3100](../../core/src/git.rs#L3100)), but never the
*roots*, so the two spellings produce two different strings for the same
directory. A default **macOS** install with a `~/Dev` folder therefore walks the
tree twice and lists **every repo twice**, under two differently-cased paths,
which then flows into the picker, the MRU and the badge sweep. `~/code` vs
`~/Code` is the same collision.

**On Windows only the double walk survives**, not the duplicate rows:
`std::fs::canonicalize` there goes through `GetFinalPathNameByHandleW`, which
returns the on-disk canonical casing, so both roots produce the identical string
and `seen` collapses them. An earlier draft claimed the duplicate listing on
both platforms.

This machine's config sets `scan_paths` explicitly, so it does not bite here —
it bites default installs.
*Fix:* dedupe roots after canonicalisation, case-insensitively on
case-insensitive volumes.

**F21b. Three stale claims in the living docs, found on the way.** Recorded
here rather than fixed, because the docs are the user's to correct and one of
them is a ROADMAP item whose scope this audit changes.

- **[ROADMAP.md:156](../../ROADMAP.md#L156) is stale.** The unchecked item says
  `write_terminal` is "one main-thread hop per keystroke" and `close_terminal`
  "blocks the UI thread for portable-pty's SIGHUP → grace → SIGKILL escalation
  (~250 ms)". Verified: `close_terminal` and `resize_terminal` **are** `(async)`
  today, each with a comment naming that exact escalation as the reason. The
  later checked entry at ROADMAP.md:40 fixed it. `write_terminal` is still sync,
  but now deliberately so with a written rationale (keystroke ordering), not as
  an oversight. The item's remaining scope is F26 and nothing else.
- **[TECHNICAL.md:24](../../TECHNICAL.md#L24) states the wrong config path** —
  `~/.config/leogit/`. Verified on this machine: that directory does not exist;
  the real one is `~/Library/Application Support/leogit/`. TECHNICAL.md:1726
  states the platform-correct rule, so this is a stale claim standing next to
  its own correction. The same wrong path appears in `GitBridge.swift:506`.
- **TECHNICAL.md:1279 calls the config/state writers "atomic".** They are
  neither atomic nor cross-process safe — see F24.
- **TECHNICAL.md:1242 and ROADMAP.md:123 both claim `run_timed` kills any
  subprocess that outlives its budget.** False — see F13 — and TECHNICAL.md
  additionally names a test as proof that cannot prove it.
- **TECHNICAL.md:1860 says "LeoGit reaches git through subprocesses and
  libgit2".** There is no `git2` in `Cargo.lock`; everything is a subprocess,
  which is the premise of §1.
- **TECHNICAL.md:1913 needs rewriting with F1**, for the reason given there.

**F21. `MAX_HIGHLIGHT_FILE_LINES` cannot prevent the read it appears to guard.**
The guard tests `last_wanted` — the highest line number the diff touches — but
it sits at [highlight.rs:251](../../core/src/highlight.rs#L251), *after* the two
`read_blob` calls at [:198](../../core/src/highlight.rs#L198) and
[:205](../../core/src/highlight.rs#L205). So the whole blob is buffered before
the guard runs, **for every file, always** — the wrong-axis problem is secondary
to the guard being downstream of the cost. The 4 MiB diff guard does not help
either: it bounds the *diff*, and `read_blob` reads the *blob*. Worth closing if
the guard is to mean what its doc comment says.

## 5. Not proposed, deliberately

- **A filesystem watcher.** §3. The poll is not the cost; startup is.
- **libgit2.** It is the structurally correct answer to §1 — a sibling project
  measured `git remote` at 659 ms → 0.55 ms over 70 repos by holding one open
  handle — but it is a rewrite of `git.rs`, it changes behaviour in subtle
  documented ways (partial clones and reftable repos cannot be opened at all),
  and F1 captures most of the same win for two days' work. Revisit only if the
  spawn budget ever becomes a real complaint.
- **`reqwest` client pooling.** §3.
- **Diff virtualization.** Already on ROADMAP with a decision recorded and the
  `CellMeasurer` approach named; unchanged by this audit.
- **A token cache for diff highlighting.** Already on ROADMAP, deferred pending
  measurement — which §6 would finally make possible.
- **A throttle on picker-open discovery.** §7 Q2. The walk never delays the
  picker; throttling it only removes the freshness it exists to provide.
- **A cancel affordance for transfers.** §7 Q3. A new control in both clients,
  against four docs that record its absence as settled design.
- **A `-uall` file-count ceiling.** §7 Q1. Every form of it changes what the
  Changes tab shows.

## 6. Sequencing

The ordering rule is the project's own: implement in the order the user flow
hits, and land the thing that makes the next thing measurable first.

**Step 0 — a measurement harness. DONE.** `just bench <repo> [--fetch]
[--scan <dir>]` (`core/examples/bench.rs`, release build, one warm-up then
three timed calls per operation), `just test`, `just lint`. Spawns are counted
at the one chokepoint every child passes through — `process::prepare_child`,
`prepare_child_async`, and a new `prepare_child_pty` for the terminal's shell,
which had been bypassing the hook — and read back via `process::spawn_count()`.
`probe_login_path()` was split out of `fix_path_env` so the startup probe can
be timed without writing the environment. Test count corrected: **228** (200
core + 25 bridge + 3 host), not the 234 an earlier draft claimed.

Baseline on this repo, warm, before any other change:

| operation | median ms | spawns/call |
|---|---|---|
| `git --version` (floor) | 7.9 | 1 |
| `get_status` | 18.2 | 2 |
| `repo_sync_status(fetch=false)` | 19.3 | 2 |
| `get_log` (50) | 19.9 | **2** |
| `get_diff` (one file) | 16.6 | **2** |
| `get_selected_diff` (5 files) | 83.9 | **10** |
| `discover_repos` (depth 3, 70 repos) | 0.9 | 0 |
| `probe_login_path` (startup) | 360 | 1 |

Three things the harness exposed that the audit had missed, now findings
F28–F30 in §4: `get_log` and every `get_diff` pay a second spawn for
`has_commits`; `run_timed` polls `try_wait` on a 50 ms tick so every network op
is quantised to 50 ms; and `discover_repos` is a pure filesystem walk, which
makes the withdrawn F9 throttle even less justified. Deferred on the way: the
workspace carries **161 pre-existing `clippy::pedantic` warnings** in core;
the rule for this plan is zero *new* warnings and fixing old ones only inside
functions being touched — a full sweep is its own task.

**Step 1 — the small fixes. DONE.** F4, F7 + F28, F11, F14, F16, F27.
Measured after, same repo, warm: `get_log` **19.9 → 12.0 ms (2 → 1 spawn)**,
`get_diff` **16.6 → 8.8 ms (2 → 1)**, `get_selected_diff` **2N → N spawns**
(18 files: 9.6 ms per file, was 16.8). Tests 228 → 235; core clippy 161 → 158.
`pnpm check` and `just mac-build` clean.

What landed, and where it departed from the finding as written:
- **F4 was not two lines.** Seven `#[tauri::command(async)]` sync commands can
  block for ≥100 ms and all now hop to the blocking pool through
  `process::run_blocking`: `repo_sync_status`, `gh_repo_list`, `check_auth`
  (a GitHub round trip), `known_repos` (tens of thousands of stats cold), and
  the three `os` launchers (a cold app launch, bounded at 15 s). The ~30
  local-git and pure-CPU commands stay `(async)`, correctly. Three false "Tauri
  makes this hop implicitly" comments were rewritten, not two.
- **F28** — `has_commits` reads `HEAD` and the ref store from disk, through a
  new `common_dir(git_dir)` helper that follows a worktree's `commondir`
  (F1 reuses it), falling back to the original `git rev-parse` for a reftable
  store, a symbolic-ref chain, or anything unreadable. Tightened over the
  finding: a loose ref answers `true` only if it holds an object id, so a torn
  ref file goes to git rather than to `fatal: bad revision`. Seven tests assert
  both which path answered and agreement with git's own answer, including a
  real reftable repo (git 2.50 supports `--ref-format=reftable` here).
- **F7** — `run_diff` takes the anchor; `get_selected_diff` resolves it once.
- **F14** — `ServerAliveInterval=10`, `CountMax=3`: 30 s of silence, chosen
  over the finding's 15 s so a user push over a briefly stalled link is not
  aborted, and longer than the background fetch's 12 s cap so the caller's
  budget always fires first.
- **F16** — 15 s bound plus `kill_on_drop` on the `claude` probes.
- **F27** — `Option<String>` end to end (core, FFI mirror, TS `string | null`,
  two Svelte and two Swift `?? body` reads); bindings regenerated.
- **F11** — both sets bound once above the `List`.

**Step 2 — correctness and durability. DONE.** F18, F19, F20, F24, F25.
Tests 235 → 243; core clippy 158 → 157; `pnpm check` and `just mac-build`
clean; the real config directory verified byte-identical before and after.

- **F18** — `RepoStore.open()` now drops `status` with the path, after the
  generation guard so a failed open still leaves the current repo intact. The
  poll's `refreshQuietly()` needed the same generation guard — without it a
  tick in flight across the switch put the stale status straight back — so it
  got one. Per §7 Q4 the Changes pane renders **blank** (a `Color.clear` that
  keeps the layout and passes clicks) until the first status lands, never
  "No changes"; every other reader of `status` was audited and already
  tolerates nil, and `SyncControls` maps nil to a disabled `.loading`, which
  closes the ⌘P case.
- **F19** — `BranchStore` has a generation counter bumped on `reset()` and on
  every `load()`; a stale listing is dropped. Every switch path already reset
  the store.
- **F20** — scan roots are de-duplicated by file identity (`dev, ino` on
  unix; canonical text on Windows), reusing the metadata already read.
  Confirmed on this machine that `Root/` and `root/` canonicalise to different
  strings with the same inode. Three tests, one of which fails without the fix.
- **F24** — `write_atomically`: sibling temp file created with `create_new`,
  renamed over the target, temp removed on every failure path, Windows retry
  on a sharing violation. `config.toml` is durable (`sync_all` + directory
  fsync); `repos-state.json` is atomic but **deliberately not durable**, since
  `sync_all` is `F_FULLFSYNC` on macOS and that file is written on the
  awaited repo-switch path. A cross-process `File::lock()` on a `<file>.lock`
  sidecar now orders the two clients (verified live with two processes);
  `Unsupported` degrades to unlocked. `load_config` heals a corrupt file by
  moving it aside as `config.toml.corrupt-<secs>` and writing defaults. Tests
  cover the writer's cleanup, the heal, and 100 concurrent updates with an
  unlocked reader racing them.
- **F25** — `record_recent_repo` also sets `last_opened_repo`; every caller in
  both clients is an open. The redundant second write is gone from Tauri's
  switch and launch paths and from Swift's `noteOpened`; `setLastOpened` had
  no other caller and is deleted. The Tauri switch now awaits the one write
  where it awaited the old first one.

*Adversarial review of Step 2 (two independent readers) found fifteen
follow-ups, all applied before Step 4 (tests 259 → 263, core clippy 156 →
155, `pnpm check` and `just mac-build` clean):* `loadRepoData` now takes its
generation from the caller and every `RepoStore` writer after an await is
guarded — `refreshWorkingTree` had none and, with F18's nil status, published
repo A's status and history under repo B on every switch that straddled it;
`BranchStore` records which repo it serves (`reset(for:)`) and refuses a
listing for any other before claiming a generation, so a merge finishing after
a switch neither publishes A's list under B nor blanks B's; the public
`load_config` takes both locks with a private `_locked` body for
`patch_config`, backups are named with pid and nanos, and the file lock is a
2 s `try_lock` loop that then proceeds unlocked; the F20 tests assert a real
`roots_walked` counter and fail with the check removed (verified by removing
it); `write_atomically` only deletes a temp it created, preserves the target's
mode on unix, and treats Windows errors 32/33/1224 as contended with a 310 ms
backoff; roots with `st_ino == 0` fall back to text identity;
`effective_scan_paths` de-dupes the same way; the count badge keeps the last
known count across the switch window (it is not actionable, and its absence
would shift the tab strip); and Tauri has exactly one state write per open,
in `MainLayout.initialize()` for the six mounting paths and in
`handleSwitchRepo` for the in-app switch.

**Step 3 — startup. DONE.** F22. Measured by the harness, same machine:
startup PATH resolution **449 ms → 0.3 ms, 1 spawn → 0** on a warm cache.
Tests 243 → 259; core clippy 157 → 156; `just mac-build` clean.

- New `core/src/path_cache.rs` owns the one question the cache raises — when
  is the cached `PATH` wrong. The key records every candidate rc file for the
  shell family *whether or not it exists* (so creating `~/.zshenv`
  invalidates), the `/etc/paths.d` entries, the version-manager state files
  (`nvm` default alias, `.tool-versions`, …), and the `SHELL`/`HOME`/
  `ZDOTDIR`/`XDG_CONFIG_HOME` values; a 7-day ceiling catches what the key
  structurally cannot see. `shell-path-cache.json` lives in the config dir
  (a wiped cache dir must not regress `PATH`) and is written atomically.
- `fix_path_env` keeps its contract and stays the only `set_var` caller:
  valid cache → install it; otherwise probe as before and write the cache.
  `spawn_path_reprobe()`, called by both hosts once the UI exists, re-probes
  on a named thread and, if the answer differs, stores it where every
  `prepare_child*` hook applies it to the child's own `PATH` — so a tool the
  fresh `PATH` knows is found by the very next child, not the next launch.
- **The `-i` stays.** The research settled it: `zsh -lc` does not read
  `~/.zshrc`, and on this machine that file supplies most of the `PATH`.
- **Two pre-existing probe bugs fixed on the way.** In fish, `"$PATH"` joins
  the list with spaces, so every fish user was installing one unusable
  space-separated blob; and any rc file that prints corrupted the value. The
  probe now brackets an `env` dump in per-call markers and reads its `PATH=`
  line, which is shell-agnostic. It is also bounded at 10 s and sets
  `LEOGIT_RESOLVING_ENVIRONMENT=1` (the VS Code convention).
- A `LEOGIT_CONFIG_DIR` override (checked after the test redirect) lets the
  bench and development runs keep out of the real config dir.
- Observed while testing, feeding Step 5: a login shell that `exec`s is
  bounded, but one whose *child* inherits the pipe is not — `run_timed`
  reported the timeout at 300 ms and returned after 30 s. That is F13.

**Step 4 — the poll's cost. DONE.** F1 and F5. Measured, warm, this repo:
`get_status` **18.2 → 9.4 ms, 2 → 1 spawn**; `repo_sync_status(fetch=false)`
**19.3 → 9.3 ms, 2 → 1**. On the 11.7k-file repo both drop by the ~8.9 ms a
`git remote` costs there. Tests 263 → 276; core clippy 155; core-only change,
so no client build was needed.

- **F1** — `first_remote`, `read_status` and `get_remote` read the remote
  names from `<common_dir>/config` through a port of git's own `config.c`
  lexer (headers, quoted subsections with escapes, legacy `[remote.Name]`
  lowercased, comments, continuation lines), byte-sorted and de-duplicated the
  way `git remote` prints them, and fall back to spawning `git remote` for
  every shape the file cannot answer: any `GIT_CONFIG*` variable in the
  process, an `include`/`includeIf` section, `extensions.worktreeConfig`, an
  unreadable or malformed file. `read_status` resolves `git_dir` once for
  both `merging` and the remote. Ten equivalence tests each assert which path
  answered *and* agreement with a spawned `git remote`.
- **Two things git does that the research had not found**, both now
  handled: the once-per-process probe for remotes defined in global or system
  config needs `--includes` (a scoped `git config` defaults to
  `--no-includes`, yet `git remote` honours them), and an `includeIf` in
  global config can only be answered from inside the matching repo, so its
  presence means permanent fallback for the process. Also: `remote.pushDefault`
  is filtered (it names no remote), `[remote ""]` declines rather than
  reproduce git's `.url` oddity, and `[remote "/x"]` is skipped as git skips
  it.
- **F5** — one `sort_by_cached_key` over `(root-first rank, lowercase path)`;
  stable, so ties keep their order; a test pins the spelled-out order.
- Cost corner, documented in place: a repo layout the filesystem cannot see
  now resolves `git_dir` through `rev-parse` before falling back, so an
  unusual repo can cost two spawns where it cost one.

**Step 5 — the network's cost. DONE.** F2 + F15, F13 + F29. Tests 276 →
279; core clippy 155; `pnpm check` and `just mac-build` clean. F3 is
deferred (no SSH remote on this machine to measure against — see F3).

- **F13 + F29** — `run_timed` and `run_timed_streaming` take a `KillScope`:
  `Group` for every git network op, every `gh` call and the PATH probe;
  `Child` for the `os` launchers, because `xdg-open` runs the user's browser
  in-group (the audit found `os.rs` *does* use the runner, so this had to be
  explicit). `Group` puts the child in its own process group with stdin
  nulled; a waiter thread owns the `Child` and its exit arrives over a
  channel, so `recv_timeout` *is* the budget and the 50 ms poll grid is gone
  (measured: fetch medians of 429.7 / 419.9 / 435.2 ms, no longer multiples of
  50). On timeout the whole group is killed **before** the reap (the zombie
  pins the pgid), then both readers are collected through one shared 2 s
  grace and detached on expiry — nothing can unblock a reader whose pipe a
  survivor holds. A child that exited normally still returns `Ok` when a
  reader had to be abandoned. Windows mirrors it with a duplicated process
  handle and, for `Group`, a job object with `KILL_ON_JOB_CLOSE`; that arm is
  written against the pinned `windows-sys` source but **unverified here** (no
  Windows target installed). Tests: the grandchild is dead after a timeout
  (`kill(pid, 0)` → `ESRCH`), a normal exit is not held open by a lingering
  grandchild, and a 20 ms child completes in under 48 ms. `libc` (unix) and
  `windows-sys` (windows) were added as direct dependencies: two lock lines,
  zero new crate versions.
- **F2 + F15** — one fetch stamp per client (`repoSync.ts`; a
  `FetchCooldown` owned by `RepoDirectoryStore` and injected into
  `SyncStore`, on a monotonic clock so it counts through sleep). Background
  fetches inside the 60 s window **downgrade to the local recompute**, never
  skip, and do not charge the breaker; every successful fetch stamps,
  including a pull's; a push never does; a user's own Fetch or Pull never
  consults it. The active repo's silent fetch now claims the same in-flight
  slot as the sweeps (Tauri) or an `isSilentFetching` flag (Swift), so the
  four native triggers cannot overlap. **Deviation from the finding, and the
  right one:** the auto-fetch loop's own tick does *not* consult the cooldown
  — its default cadence is 30 s, shorter than the window, and consulting it
  would have halved the cadence the user configured. Only catch-up triggers
  (cold open, wake, reconnect, refocus, switch) and the tier sweeps do.
- Pre-existing flaky test fixed on the way: the spawn-counter test now takes
  the minimum of sixteen readings, since parallel tests can only inflate it.
- TECHNICAL.md:1242 and ROADMAP.md:123, which F13 called false, are now true
  as written; the docs pass will say *why* they are true.

**Step 6 — the diff pipeline. DEFERRED, pending §7 Q6.** F23's design rested
on a premise that is false for Tauri (see the correction under F23); the three
ways forward each change where rendering happens, and none can be measured
with the tools this plan built. Nothing was implemented. F21, which touches
the same 35 lines, moves to Step 7 on its own.

**Step 7 — the rest. DONE.** F6, F8, F10, F21, F26. Tests 279 → 282; core
clippy 155 → 154; `pnpm check` and `just mac-build` clean. F15 landed with
Step 5; F12 is deferred (no trace — see F12).

- **F6** — `entry.file_type()` decides directory-vs-file from the `dirent`;
  `fs::metadata` survives only on the symlink branch. Under `~/Dev` at depth 3
  that is ~122 stats → 1 per walk; the row was sub-millisecond either way, as
  the finding said, and a symlinked project folder is still found (tested).
- **F8** — `RepoDirectoryStore` records a probe failure with a timestamp and
  re-probes that path only after 5 minutes; opening the repo clears the
  marker at once, and a failure never blanks a badge that was good. A row
  renders nothing for a failed or never-probed repo, exactly as before.
  Consequence worth naming: a repo that *recovers* refills its badge within
  5 min rather than 30 s.
- **F10** — the History effect is keyed on one `$derived` boolean under
  `untrack`, and `loadInitialLog` has an in-flight guard keyed on the path
  plus a post-await path check it did not have. Verified against svelte
  5.56.4's flush ordering: a switch on the History tab makes **exactly one**
  `git log`. Consequence worth naming: a *failed* first page is no longer
  retried on every 2 s tick — that retry re-armed its error modal each time —
  and recovers on a tab switch or when HEAD moves.
- **F21** — the line cap is checked on the wanted-line sets before either
  blob is read; the check inside `tokenize_file` became unreachable and is
  gone. A test proves a diff past the cap falls back without spawning
  `git show`.
- **F26** — the PTY child sits behind its own lock, so `close_terminal` and
  `reap_child` hold nothing a keystroke's `write_terminal` needs across the
  SIGHUP → grace → SIGKILL escalation. **Not `clone_killer()`**, which the
  finding suggested: in portable-pty 0.9 that handle sends a bare SIGHUP with
  no grace loop and no SIGKILL, so a shell ignoring SIGHUP would have outlived
  the panel and leaked. A structural test holds the child's lock and asserts a
  write returns in under 50 ms.

**Final verification — two adversarial reviews over the whole diff, one per
side, and their fixes.** Neither trusted §6; both were told to disprove it.

*Client side* found one **P1 that would have shipped**: the fetch cooldown
was stamped by the *fetch-less* badge sweeps, because core's `RepoSync.fetched`
is `true` when no fetch was requested. Opening the picker stamped every row,
and the repo the user then opened had its on-open fetch refused — with
auto-fetch off, it would never have been fetched. Fixed in both clients: a
stamp needs a requested fetch, a remote, and success. Also fixed: the native
silent-fetch claim was process-wide rather than per repo, so a fetch of A
surviving a switch blocked B's warm-up; the toolbar flashed on every switch
(the branch chip fell back to the literal "Branches" and the sync button lost
its chevron for the load window — the toolbar now draws from one held
`ToolbarStatus` mirror of the eight fields it shows, so every display-only
reader keeps its last known face, while everything actionable — the sync
button's enablement and the action it would run, the branch menu's checkmark
and merge target — still reads the live status, exactly today's nil gating);
the
held count could outlive a failed first status; Tauri's mount path awaited the
state write ahead of first paint; and `loadInitialLog`'s slot could be
released by the wrong caller.

*Rust side* found no P1 and verified the config lexer against fifteen
adversarial files and real git 2.50. Fixed from its list: a detached reader
kept growing its buffer for the survivor's lifetime; the `os` launchers paid
the full 2 s reader grace on Linux where `xdg-open`'s handler daemonises
holding the pipe; a timed-out call could overrun its budget by two grace
windows instead of one; `has_commits` answered "no commits" rather than
declining for a `HEAD` pointing outside `refs/heads/`; an empty `[]` header
was accepted where git rejects the file; the once-per-process outside-remote
probe held its mutex across two unbounded spawns; and a Windows job-assignment
failure would have failed every network command instead of degrading to a
plain kill. Each of the four new Rust tests was shown to fail against the
pre-fix code before being kept.

**Final state, verified independently of every agent that produced it:**
`cargo fmt` clean; **286 tests** passing (258 core + 25 bridge + 3 host, from
228 at the start); core `clippy::pedantic` **154** warnings (from 161, zero
new anywhere, zero in every other crate and the bench example); `pnpm check`
0 errors; `just mac-build` succeeded. Nothing is committed — that is the
user's call after the manual pass.

Steps 1–3 are where the user-visible wins are concentrated. §7 is settled, so
nothing above waits on a decision.

## 7. Decisions

All five were resolved on 2026-09-05 under one rule: **the current user
experience is the baseline, and no item in this plan may change it.** Where an
option would alter what the user sees, when they see it, or what they can do,
the other option wins even when it leaves performance on the table. Each entry
records the options that were on the table so the reasoning survives.

**Q1 — a file-count ceiling on `-uall` (F5). Decided: no ceiling.** A repo with
50k untracked files is pathological but reachable (a fresh clone before
`.gitignore` lands). The options were (a) fix only the sort and keep the cliff;
(b) fall back to `-unormal` above a threshold, which collapses untracked
directories to one row; (c) show the first N and a "…and 49 950 more" row.
(b) and (c) both change what the Changes tab shows, so **(a)** stands: the
`sort_by_cached_key` fix removes the allocation storm and the row count is
whatever the working tree holds, exactly as today. If Step 0's harness ever
shows a real cliff on a real repo, that is a new item with its own decision,
not a reopening of this one.

**Q2 — discovery throttle window (F9). Decided: no throttle; F9 is withdrawn.**
Both pickers already show the cached list the instant they open and let the
walk publish into it — the Tauri comment at
[MainLayout.svelte:1725](../../apps/tauri-app/src/lib/views/MainLayout.svelte#L1725)
says the re-walk exists precisely *"so a repo cloned from a terminal … is in
the list the user is about to read"*, and
[RepoSwitcher.swift:89](../../apps/swift-ui-app/Sources/LeoGit/Screens/RepoSwitcher.swift#L89)
runs the walk and the badge sweep concurrently for the same reason. The walk
is therefore off the UI path already; a throttle would buy nothing the user
can feel and would cost the one thing that comment promises. The walk's cost
is addressed where it lives: F6 cuts each walk's syscalls by roughly two
thirds without touching when it runs.

**Q3 — does F17 (cancel a transfer) belong on ROADMAP? Decided: no.** Four
living docs record "there is no cancel" as settled design and STYLE.md builds a
UI convention on it. A cancel affordance is a new control in both clients, which
is a change to the experience, not a preservation of it. F17 stays in Tier 2 as
a recorded, known gap; it gets a ROADMAP entry only if a user asks for it.

**Q4 — should Tier 3 be split out? Decided: no.** Neither option touches the
user, so the recommendation stands: F18–F21 stay here until Step 2 lands, then
the outcome is recorded in ROADMAP and the section deleted. One constraint
carried into Step 2 from the rule above: the F18 fix clears the stale `status`
so no action can fire on it, and lets the Changes tab show whatever it shows
today for a repo with no status yet — **no new loading placeholder**, since the
gap is 100–500 ms and a placeholder that flashes for that long is itself a
visible change.

**Q5 — the F23 fallback. Decided: the client re-sends on request; the backend
never re-derives content.** `highlight_diff` gains a lean request shape —
the two paths plus one `(line_type, old_line_no, new_line_no)` triple per row,
the 55 KiB the highlighter actually reads — and the blob path runs on it
unchanged. When the blob path cannot serve the request, the backend replies
with a distinct `NeedContent` result instead of tokens, and the client re-sends
the full `FileDiff` through the path that exists today. The fallback is
reachable in ordinary use, not just on error: a file past
`MAX_HIGHLIGHT_FILE_LINES` (20 000), a root commit (whose `{sha}^` does not
resolve), and any unreadable blob all land on it, so its output has to be
provably identical to today's, and re-sending the client's own copy is the only
design that makes that true by construction. Rejected: folding highlighting
into `get_diff`, because the diff would then wait for the highlighter instead of
appearing first; having the backend re-run the diff, because between the two
calls an autosave — the amplifier F23 itself names — can change the file and
the tokens would no longer line up with the rows on screen; a backend cache of
the last `FileDiff` keyed by stamp, because it puts state into a core that has
none and only pays off if the fallback turns out to be common, which Step 0
can measure later. Both clients adopt the same contract so `tokenize_diff`
keeps one signature.

**Q6 — F23 on Tauri, now that `render.rs` needs `content`. OPEN — deferred
by the implementation pass on 2026-09-05.** Q5's design serves the Swift
client's `tokenize_diff` but not Tauri's `highlight_diff`, which returns HTML
rendered from `content` and `intra_line_diff`. The options, none of which the
implementation may pick alone:
(a) **Tauri receives tokens and builds the spans in the WebView.** The only
option that delivers F23's stated win on the client where it was measured
worst — but it reverses the decision recorded at `highlight.rs:83-86`
(*"keeps the WebView main thread out of the span-building work"*), duplicates
`render_line`'s code-point and overlay splitting in TypeScript, and trades a
main-thread JSON parse for main-thread span construction of unknown cost on a
3 000-row diff. It needs a devtools trace, which this plan's harness cannot
provide, and a large diff hitching *more* would be a visible regression.
(b) **Lean request for Swift only; Tauri keeps sending `FileDiff`.** Safe,
small: ~188 KiB → ~44 KiB of memcpy per file selection on a concurrent
executor. Two wire shapes for one operation, for a win the finding itself
rated minor.
(c) **Lean request that also carries `content` and `intra_line_diff`.** Drops
only `text`, `file_header`, `hunk.header` and `is_binary` — roughly a tenth of
the payload. Not worth a wire-shape change.
(d) **Leave it.** Recorded so the next reader knows why the largest
per-action item in this plan did not ship.
*Recommendation: (d) until a trace of (a) exists; the Swift-only (b) is not
worth two shapes.*
