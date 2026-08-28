import { writable } from 'svelte/store'

/**
 * Kind of user-initiated network operation.
 *
 * `fetch` is here because the sync ladder made it reachable: it is the proposal
 * for an in-sync branch and a menu item in every split state. The automatic
 * fetch behind auto-fetch and the refocus resync deliberately claims no slot —
 * it is not something the user is waiting on, and taking the slot would disable
 * the whole action cluster on a timer.
 */
export type NetworkOpKind = 'fetch' | 'push' | 'pull' | 'publish'

/**
 * The user-initiated network operation in flight, or null when idle.
 *
 * Lives in a store (not Header-local state) so the rest of the app can react:
 * the 2 s status poll, auto-fetch, and the background badge scheduler all pause
 * while this is set — polling mid-transfer only spawns extra git processes that
 * contend with the push/pull for the repo's disk, locks, and bandwidth. It also
 * gives the ops mutual exclusion: starting a pull while a push is in flight is
 * refused at the handler level.
 */
export const activeNetworkOp = writable<NetworkOpKind | null>(null)

/** Live progress of the active push/pull, mirrored from `git-progress` events. */
export interface NetworkProgress {
  /** Aggregate 0–100 across the operation's phases. */
  percent: number
  /** Raw git line, e.g. "Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s". */
  text: string
}

export const networkProgress = writable<NetworkProgress | null>(null)

/** Enter a network op: claims the mutual-exclusion slot with a clean gauge. */
export function beginNetworkOp(kind: NetworkOpKind): void {
  activeNetworkOp.set(kind)
  networkProgress.set(null)
}

/** Leave the network op, dropping any progress so the UI returns to idle. */
export function endNetworkOp(): void {
  activeNetworkOp.set(null)
  networkProgress.set(null)
}
