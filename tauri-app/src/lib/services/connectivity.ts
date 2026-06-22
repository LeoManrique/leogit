/**
 * Offline / flaky-connection awareness for *background* network work.
 *
 * The backend already time-boxes every remote git call (so nothing hangs the
 * UI), but when we're offline it's still wasteful — and not "smooth" — to keep
 * firing fetches that we know will fail. This module decides whether a
 * background fetch is worth attempting, from two signals:
 *
 *   1. navigator.onLine — the WebView's view of connectivity. When it's false we
 *      skip instantly, with no request spawned at all.
 *   2. A consecutive-failure circuit breaker — for a *flaky* link (onLine still
 *      true, but fetches time out), we stop retrying every tick and back off
 *      with an exponential window. When the window expires exactly one attempt
 *      is allowed through to probe for recovery; success closes the breaker.
 *
 * Only *automatic/background* network is gated. User-initiated actions (Pull,
 * Push, switching repo) always attempt — they're already bounded by the backend
 * timeout — and their outcome still feeds the breaker so a successful manual
 * pull immediately re-opens background syncing.
 *
 * By design there is no UI surface here: this only shapes *when* requests fire,
 * keeping the app responsive offline without nagging the user with warnings.
 */

// Consecutive background failures tolerated before we start backing off. >1 so a
// single transient blip doesn't trip the breaker.
const FAILURE_THRESHOLD = 2
// First backoff window once the breaker opens; doubles per extra failure…
const BACKOFF_BASE_MS = 30_000
// …capped here so we still probe for recovery a few times an hour when down.
const BACKOFF_MAX_MS = 5 * 60_000

let consecutiveFailures = 0
// Epoch ms before which background attempts are suppressed (breaker "open").
let openUntil = 0

function isBrowserOffline(): boolean {
  return typeof navigator !== 'undefined' && navigator.onLine === false
}

function backoffMs(): number {
  // failures past the threshold lengthen the window: 30s, 60s, 120s, … capped.
  const steps = Math.max(0, consecutiveFailures - FAILURE_THRESHOLD)
  return Math.min(BACKOFF_BASE_MS * 2 ** steps, BACKOFF_MAX_MS)
}

/**
 * Whether a *background* network git op should be attempted right now. False
 * when the OS reports offline or the breaker is open; once the backoff window
 * lapses this returns true for the next caller (the recovery probe), and the
 * caller's `recordResult` either closes the breaker or re-opens it longer.
 */
export function shouldAttemptBackground(): boolean {
  if (isBrowserOffline()) return false
  return Date.now() >= openUntil
}

/**
 * Feed a network op's outcome to the breaker. Success resets it; failure counts
 * toward the threshold and, once reached, opens the breaker for a backoff window.
 * Call this for every real network attempt — background *and* user-initiated —
 * so a successful manual action promptly re-enables background syncing.
 */
export function recordResult(ok: boolean): void {
  if (ok) {
    consecutiveFailures = 0
    openUntil = 0
    return
  }
  consecutiveFailures += 1
  if (consecutiveFailures >= FAILURE_THRESHOLD) {
    openUntil = Date.now() + backoffMs()
  }
}

/**
 * Wire the OS `online` event so the moment connectivity returns we reset the
 * breaker and run `onRecover` (e.g. an immediate resync) instead of waiting out
 * the current backoff window. Returns a teardown function.
 */
export function initConnectivity(onRecover: () => void): () => void {
  const handleOnline = (): void => {
    consecutiveFailures = 0
    openUntil = 0
    onRecover()
  }
  window.addEventListener('online', handleOnline)
  return () => window.removeEventListener('online', handleOnline)
}
