import { get, writable } from 'svelte/store'
import { appState } from '$lib/stores/app'
import { config } from '$lib/stores/config'
import { reposApi } from '$lib/api/commands'

/**
 * Whether a discovery walk is running. Only useful together with an empty
 * list: once a pass has published rows, later refreshes replace them in place
 * rather than blinking the picker through a spinner.
 */
export const discoveringRepos = writable(false)

// The pass in flight, so a second caller awaits it instead of starting another
// walk. Opening the dropdown while Settings' close is still walking is the
// common case, and two concurrent walks would publish twice for nothing.
let pass: Promise<void> | null = null

/**
 * Re-run discovery and republish the known-repo list.
 *
 * Discovery used to run once per launch, so a repo cloned from a terminal, a
 * folder added to the scan paths, or a repo deleted on disk only reached the
 * picker after a restart. Both phases call this now: the startup picker and the
 * main-view dropdown, each when it is about to be looked at, plus whenever
 * Settings closes (the scan paths are what discovery walks).
 *
 * Never rejects: the list already on screen is a better answer than an empty
 * one, so a failed walk is logged and the previous list stands. Callers are
 * fire-and-forget — nothing waits on a rediscovery to render.
 */
export function rediscoverRepos(): Promise<void> {
  if (pass) return pass
  pass = runPass().finally(() => {
    pass = null
    discoveringRepos.set(false)
  })
  return pass
}

async function runPass(): Promise<void> {
  discoveringRepos.set(true)
  const cfg = get(config)
  try {
    const repos = await reposApi.knownRepos(cfg?.scan_paths ?? [], cfg?.scan_depth ?? 3)
    appState.update((s) => ({
      ...s,
      // The open repo always keeps its row. It normally comes back in the union
      // via the MRU, but that write is fire-and-forget on switch, so a
      // rediscovery close behind one could otherwise drop the very repo on
      // screen out of the switcher.
      repos: !s.repoPath || repos.includes(s.repoPath) ? repos : [...repos, s.repoPath],
    }))
  } catch (error) {
    console.error('[repos] rediscovery failed', error)
  }
}
