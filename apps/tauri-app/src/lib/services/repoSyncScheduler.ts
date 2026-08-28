import { get } from 'svelte/store'
import { appState } from '$lib/stores/app'
import { recentRepos } from '$lib/stores/reposState'
import { repoSync, syncRepo } from '$lib/stores/repoSync'
import { activityState, canRunRepoSweeps } from '$lib/services/backgroundPolicy'
import { pacedLoop } from '$lib/services/pacedLoop'

/**
 * Tiered background refresh of every repo's pull/push badge. The more recently
 * a repo was used, the more often we fetch it — so the picker shows fresh
 * counts for repos you care about without hammering the network for the long
 * tail.
 *
 * Tiers (relative to the recents list, active repo excluded — it has its own
 * auto-fetch and status poll, so the scheduler never touches it):
 *   tier1  next 4   ("top 5")  → every 2 min  + on app refocus (throttled)
 *   tier2  next 5   ("top 10") → every 5 min
 *   tier3  next 10  ("top 20") → every 10 min
 *   rest                       → when switched to (see syncOnSwitch) and via
 *                                the fetch-less sweep while the repo list is
 *                                on screen (see syncVisibleRepos)
 *
 * **One loop, three deadlines.** The tiers used to be three independent
 * `setInterval`s, which meant they collided: every ten minutes all three came
 * due in the same turn and three `git fetch` fan-outs ran at once, on top of
 * whatever the active repo's auto-fetch was doing. The "sequential" the tier
 * runner promises was only ever true *within* a tier. One loop sleeping to the
 * nearest deadline can't do that — a tier that comes due while another is
 * running simply waits its turn.
 *
 * On-switch sync lives in the caller (MainLayout) so every repo — including the
 * untiered tail — refreshes the moment you open it.
 */

interface Tier {
  /** How often this tier refreshes. */
  intervalMs: number
  /** First run, staggered so badges fill soon after launch without three
   *  bursts of fetches landing on top of the initial repo load. */
  kickMs: number
  /** Slice of the recents list (active excluded) this tier covers. */
  range: [number, number]
}

const TIERS: Tier[] = [
  { intervalMs: 2 * 60_000, kickMs: 1_500, range: [0, 4] },
  { intervalMs: 5 * 60_000, kickMs: 4_000, range: [4, 9] },
  { intervalMs: 10 * 60_000, kickMs: 8_000, range: [9, 19] },
]

// Don't re-run the refocus kick more than once per window — the user may
// alt-tab rapidly, and each kick is a fetch per repo in the top tier.
const REFOCUS_THROTTLE_MS = 30_000

/** When each tier next comes due, as epoch ms. Seeded by `start`. */
let dueAt: number[] = []
let lastRefocusKick = 0

/** The eligible repos for one tier: recents, active excluded, missing dropped. */
function tierMembers(tier: Tier): string[] {
  const { repoPath: active, repos } = get(appState)
  const known = new Set(repos)
  const eligible = get(recentRepos).filter((p) => p !== active && known.has(p))
  return eligible.slice(tier.range[0], tier.range[1])
}

/**
 * Fetch + recompute each repo in a tier sequentially, to keep the number of
 * concurrent `git fetch` processes low. Marked `background` so the whole tier
 * goes quiet while offline / backing off (each `syncRepo` self-skips).
 *
 * The policy is re-checked before every repo, not once at entry: a push
 * starting, or the window going away, abandons the rest of the tier rather than
 * finishing a fan-out nobody is waiting for. The tier's next deadline is
 * already armed, so it simply retries.
 */
async function syncTier(repos: string[]): Promise<void> {
  for (const repo of repos) {
    if (!canRunRepoSweeps()) return
    await syncRepo(repo, true, true)
  }
}

const loop = pacedLoop({
  label: 'repo-sync',
  // Parked outright while the window is neither focused nor on screen: badges
  // for repos the user isn't looking at are the one genuinely deferrable thing
  // the app does. Deadlines keep passing while parked, so waking up runs
  // whatever came due — one sequential catch-up, not a lost cycle.
  //
  // The *activity* half of the policy only. A network op also forbids sweeps,
  // but parking on it would need something to un-park the loop when the
  // transfer ends, and nothing raises an event for that; the per-repo guard
  // inside `syncTier` already makes such a tick cost nothing.
  dueAt: () => (activityState() === 'active' ? Math.min(...dueAt) : Number.POSITIVE_INFINITY),
  run: async () => {
    for (const [index, tier] of TIERS.entries()) {
      // The clock is re-read per tier, not captured once for the pass: a tier
      // that took a minute to walk has let the next one come due meanwhile, and
      // its own next deadline should be a full interval from when it actually
      // finished rather than from when the pass started.
      const now = Date.now()
      if (now < dueAt[index]) continue
      // Re-armed before the tier runs, so a slow tier delays the next one
      // rather than shifting its whole schedule.
      dueAt[index] = now + tier.intervalMs
      await syncTier(tierMembers(tier))
    }
  },
})

function start(): void {
  const now = Date.now()
  dueAt = TIERS.map((tier) => now + tier.kickMs)
  loop.start()
}

function stop(): void {
  loop.stop()
}

/** Re-decide the next run after the window's activity state moved. */
function onActivityChange(): void {
  loop.reschedule()
}

/**
 * Bring the top tier forward — the catch-up after an absence, and after the
 * network comes back. Throttled, because a rapid alt-tab would otherwise fetch
 * the top five repos on every pass.
 */
function kickTopTier(): void {
  const now = Date.now()
  if (dueAt.length === 0) return // not started yet — `start` sets its own kicks
  if (now - lastRefocusKick < REFOCUS_THROTTLE_MS) return
  lastRefocusKick = now
  dueAt[0] = now
  loop.reschedule()
}

/** Fetch + recompute a single repo the user just switched to. */
function syncOnSwitch(path: string): void {
  void syncRepo(path, true)
}

// Full-list sweeps re-run at most once per window; rows with no cached entry
// (which the tiers never cover) are always filled in regardless.
const LIST_SWEEP_THROTTLE_MS = 30_000
let lastListSweep = 0

/** Fetch-less recompute of the repos currently shown in the repo list.
 * The dropdown lists every discovered repo, but the tiers only keep the ~19
 * most recent fresh — so when the list is actually on screen, sweep the rest
 * (and, throttled, the whole list) with local `git status` recomputes. No
 * network is involved, so this also works offline, where the tier fetches go
 * quiet. Sequential like the tiers to keep the disk polite; dots and badges
 * pop in row by row as results land. */
async function syncVisibleRepos(repos: string[]): Promise<void> {
  const active = get(appState).repoPath
  const cached = get(repoSync)
  const refreshAll = Date.now() - lastListSweep >= LIST_SWEEP_THROTTLE_MS
  if (refreshAll) lastListSweep = Date.now()
  const targets = repos.filter((p) => p !== active && (refreshAll || !cached.has(p)))
  for (const repo of targets) {
    if (!canRunRepoSweeps()) return
    await syncRepo(repo, false)
  }
}

export const repoSyncScheduler = {
  start,
  stop,
  onActivityChange,
  kickTopTier,
  syncOnSwitch,
  syncVisibleRepos,
}
