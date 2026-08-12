/**
 * One-shot startup check for a newer leogit release (the LeoSync updater,
 * ported). Asks the backend — one GitHub Releases API request — once per app
 * session; while it can't get an answer (offline, rate-limited, GitHub down)
 * it retries on a slow interval, and after the first completed check it stays
 * quiet: releases don't ship often enough to warrant polling.
 *
 * The result only feeds the header's update chip; failures are logged, never
 * surfaced. The outcome deliberately does NOT feed the connectivity breaker:
 * a rate-limited GitHub API answer says nothing about the git remotes the
 * breaker guards.
 */
import { updateApi } from '$lib/api/commands'
import { availableUpdate } from '$lib/stores/update'
import { shouldAttemptBackground } from './connectivity'

const RETRY_INTERVAL_MS = 30 * 60_000

let timer: ReturnType<typeof setInterval> | null = null
let checked = false

async function attempt(): Promise<void> {
  // While the OS says offline (or the breaker is open) don't even spawn the
  // request — it would only time out and burn a retry slot.
  if (checked || !shouldAttemptBackground()) return
  try {
    const info = await updateApi.checkForUpdate()
    checked = true
    stop()
    if (info) {
      console.log(`[update] v${info.version} available`)
      availableUpdate.set(info)
    }
  } catch (error) {
    console.warn('[update] check failed, will retry:', error)
  }
}

/** Retry the moment the OS says we're back online rather than waiting out the
 *  30-minute window — launching offline (a plane, a captive portal) is exactly
 *  when the first attempt is skipped. */
const handleOnline = (): void => void attempt()

/** Kick the startup check; retries keep running until one check completes. */
function start(): void {
  if (timer || checked) return
  void attempt()
  timer = setInterval(() => void attempt(), RETRY_INTERVAL_MS)
  window.addEventListener('online', handleOnline)
}

function stop(): void {
  window.removeEventListener('online', handleOnline)
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

export const updateChecker = { start, stop }
