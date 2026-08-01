import { get } from 'svelte/store'
import { appState } from '$lib/stores/app'
import { recentRepos } from '$lib/stores/reposState'
import { activeNetworkOp } from '$lib/stores/networkOps'
import { syncRepo } from '$lib/stores/repoSync'

/**
 * Tiered background refresh of every repo's pull/push badge. The more recently
 * a repo was used, the more often we fetch it — so the picker shows fresh
 * counts for repos you care about without hammering the network for the long
 * tail.
 *
 * Tiers (relative to the recents list, active repo excluded — it has its own
 * 30s auto-fetch + 2s status poll, so the scheduler never touches it):
 *   tier1  next 4   ("top 5")  → every 2 min  + on app refocus (throttled)
 *   tier2  next 5   ("top 10") → every 5 min
 *   tier3  next 10  ("top 20") → every 10 min
 *   rest                       → only when switched to (see syncOnSwitch)
 *
 * On-switch sync lives in the caller (MainLayout) so every repo — including the
 * untiered tail — refreshes the moment you open it.
 */

const TIER1_MS = 2 * 60_000
const TIER2_MS = 5 * 60_000
const TIER3_MS = 10 * 60_000

// Stagger the first refresh of each tier so badges appear soon after launch
// without three bursts of git fetches landing at once on top of the initial
// repo load.
const TIER1_KICK_MS = 1_500
const TIER2_KICK_MS = 4_000
const TIER3_KICK_MS = 8_000

// Don't re-run the tier1 refocus sync more than once per window — focus and
// visibilitychange can both fire on a single activation, and the user may
// alt-tab rapidly.
const REFOCUS_THROTTLE_MS = 30_000

type Timer = ReturnType<typeof setInterval>

let timers: Timer[] = []
let kicks: Timer[] = []
let lastRefocusTier1 = 0

interface Tiers {
  tier1: string[]
  tier2: string[]
  tier3: string[]
}

/** Slice the recents list (active excluded, missing paths dropped) into tiers. */
function computeTiers(): Tiers {
  const { repoPath: active, repos } = get(appState)
  const known = new Set(repos)
  const others = get(recentRepos).filter((p) => p !== active && known.has(p))
  return {
    tier1: others.slice(0, 4),
    tier2: others.slice(4, 9),
    tier3: others.slice(9, 19),
  }
}

/** Fetch + recompute each repo in a tier sequentially, to keep the number of
 * concurrent `git fetch` processes low. Marked `background` so the whole tier
 * goes quiet while offline / backing off (each `syncRepo` self-skips). Also
 * pauses while a user push/pull is in flight — badge fetches would steal
 * bandwidth from the transfer; the next tier interval picks the repos up. */
async function syncTier(repos: string[]): Promise<void> {
  for (const repo of repos) {
    if (get(activeNetworkOp)) return
    await syncRepo(repo, true, true)
  }
}

function stop(): void {
  for (const t of timers) clearInterval(t)
  for (const k of kicks) clearTimeout(k)
  timers = []
  kicks = []
}

function start(): void {
  stop()
  kicks = [
    setTimeout(() => void syncTier(computeTiers().tier1), TIER1_KICK_MS),
    setTimeout(() => void syncTier(computeTiers().tier2), TIER2_KICK_MS),
    setTimeout(() => void syncTier(computeTiers().tier3), TIER3_KICK_MS),
  ]
  timers = [
    setInterval(() => void syncTier(computeTiers().tier1), TIER1_MS),
    setInterval(() => void syncTier(computeTiers().tier2), TIER2_MS),
    setInterval(() => void syncTier(computeTiers().tier3), TIER3_MS),
  ]
}

/** Refresh the top tier when the app regains focus, throttled. */
function refocusSync(): void {
  const now = Date.now()
  if (now - lastRefocusTier1 < REFOCUS_THROTTLE_MS) return
  lastRefocusTier1 = now
  void syncTier(computeTiers().tier1)
}

/** Fetch + recompute a single repo the user just switched to. */
function syncOnSwitch(path: string): void {
  void syncRepo(path, true)
}

export const repoSyncScheduler = { start, stop, refocusSync, syncOnSwitch }
