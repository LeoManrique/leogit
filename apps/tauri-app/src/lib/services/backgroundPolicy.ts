import { get } from 'svelte/store'
import { activeNetworkOp } from '$lib/stores/networkOps'

/**
 * The one answer to "may background work run right now, and how often?" — every
 * background loop names the predicate it obeys instead of composing its own
 * boolean out of scattered state. The native client's
 * `BackgroundSchedulingPolicy`, ported table-for-table:
 *
 * | Work                                   | Pauses on network op | Pauses when app inactive | Pauses when window hidden |
 * |----------------------------------------|----------------------|--------------------------|---------------------------|
 * | Status poll (active repo, local)       | yes                  | no — slows 2 s → 10 s    | no — slows to 30 s        |
 * | Auto-fetch loop (active repo, network) | yes                  | no                       | no — interval ×3          |
 * | Tier scheduler + sweeps (other repos)  | yes                  | yes                      | yes                       |
 *
 * The asymmetry is the point. A visible-but-unfocused window keeps telling the
 * truth — stale in plain sight is the failure this exists to prevent — and a
 * hidden one keeps refreshing slowly so returning to it reveals a current
 * screen instead of a catch-up in front of the user. The multi-repo fan-out is
 * the only genuinely deferrable work, so it is the only thing that stops, which
 * is GitHub Desktop's model for its own indicator sweep.
 *
 * ## What "hidden" can mean here
 *
 * The native policy reads AppKit occlusion; this reads `document.hidden`, which
 * the WebView sets when the window is minimized or the app is hidden and may
 * also set when the window is fully covered. When it does not, the window
 * reports `inactive` and polls at 10 s — slower than frontmost, faster than the
 * hidden rung, and never wrong in a way the user can see.
 *
 * The hidden rung is also the one the host can overrule: a WebView is free to
 * throttle timers in a backgrounded document, so the real hidden cadence is
 * "30 s or slower". That only ever makes hidden work cheaper, and the resync
 * that runs the moment the window wakes up is what actually guarantees a
 * current screen — the ladder is an optimization on top of it, not the
 * mechanism. Moving the tick to a Rust-side ticker would pin the cadence
 * exactly; it would also add a backend event nothing else needs.
 */
export type ActivityState = 'active' | 'inactive' | 'hidden'

/** Status poll cadence per state. Native's ladder, number for number. */
const STATUS_POLL_MS: Record<ActivityState, number> = {
  active: 2_000,
  inactive: 10_000,
  hidden: 30_000,
}

/** Auto-fetch interval multiplier while the window is off screen. */
const HIDDEN_FETCH_MULTIPLIER = 3

/**
 * A once-per-session offset, 0–30 s, added to the *first* automatic fetch.
 *
 * GitHub Desktop's trick, and it earns its keep here for a reason the web
 * clients don't have: LeoGit's two clients read the same repositories from the
 * same machine, so two windows launched together would otherwise fetch, and
 * contend for `index.lock`, in lockstep forever. The status poll is
 * deliberately *not* skewed — delaying the first local read by up to half a
 * minute after launch would be visible, and a `git status` is cheap enough that
 * phase alignment costs nothing.
 */
export const SESSION_FETCH_SKEW_MS = Math.floor(Math.random() * 30_000)

/** How awake the window is, ranked: a resync is worth running when it rises. */
const RANK: Record<ActivityState, number> = { hidden: 0, inactive: 1, active: 2 }

function compute(): ActivityState {
  if (document.hidden) return 'hidden'
  return document.hasFocus() ? 'active' : 'inactive'
}

let current: ActivityState = compute()
const listeners = new Set<(state: ActivityState, previous: ActivityState) => void>()

function handleChange(): void {
  const next = compute()
  if (next === current) return
  const previous = current
  current = next
  for (const listener of listeners) listener(next, previous)
}

/** Where the window is right now, read on the spot rather than cached. */
export function activityState(): ActivityState {
  return current
}

/**
 * Whether the window just became *more* awake — visible again, or frontmost
 * again. The resync-on-activation condition: coming back deserves a catch-up,
 * going away does not.
 */
export function wokeUp(state: ActivityState, previous: ActivityState): boolean {
  return RANK[state] > RANK[previous]
}

/**
 * Watch the window's activity state. Returns a teardown; the DOM listeners are
 * attached with the first watcher and removed with the last, so every
 * *scheduling* decision reads one shared answer. Components still ask the
 * document about themselves — the terminal about its caret, the history list
 * about whether its rows are on screen — which is a different question.
 */
export function observeActivity(
  listener: (state: ActivityState, previous: ActivityState) => void
): () => void {
  if (listeners.size === 0) {
    document.addEventListener('visibilitychange', handleChange)
    window.addEventListener('focus', handleChange)
    window.addEventListener('blur', handleChange)
    // The listeners can have missed a change while nobody was watching.
    current = compute()
  }
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
    if (listeners.size > 0) return
    document.removeEventListener('visibilitychange', handleChange)
    window.removeEventListener('focus', handleChange)
    window.removeEventListener('blur', handleChange)
  }
}

/**
 * The active repository's status poll. Never stops for focus or visibility —
 * {@link statusPollIntervalMs} slows it instead, so a window the user returns
 * to is already current.
 */
export function canPollStatus(): boolean {
  return get(activeNetworkOp) === null
}

/**
 * The active repository's automatic fetch. Same condition as
 * {@link canPollStatus} today, and named separately on purpose: they are two
 * rows of the table above, and a change to one must not silently move the
 * other.
 */
export function canAutoFetch(): boolean {
  return get(activeNetworkOp) === null
}

/**
 * The other repositories' badge machinery — the tier scheduler and the
 * switcher's row sweep. The deferrable fan-out, and the only work that pauses
 * outright: nobody is looking at a badge for a repository in a window that is
 * neither focused nor on screen, and the wake-up resync is its catch-up path.
 */
export function canRunRepoSweeps(): boolean {
  return get(activeNetworkOp) === null && current === 'active'
}

/** How long until the next status poll, given where the window is. */
export function statusPollIntervalMs(): number {
  return STATUS_POLL_MS[current]
}

/**
 * The automatic fetch's interval: what the user configured while the window is
 * on screen, stretched while it is not. Stretching beats pausing — ahead/behind
 * is then already right on return rather than a minute of catch-up away.
 */
export function autoFetchIntervalMs(configuredMs: number): number {
  return current === 'hidden' ? configuredMs * HIDDEN_FETCH_MULTIPLIER : configuredMs
}
