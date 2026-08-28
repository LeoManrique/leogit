/**
 * A repeating background job whose cadence can change between runs.
 *
 * `setInterval` cannot express that. Its period is fixed when it is armed, so a
 * poll that wants to slow down when the window is hidden has to be torn down
 * and rebuilt, and a run that outlives its period stacks a second one on top of
 * the first. This is the shape the native client's loops already have — sleep,
 * work, then decide when to sleep again — written once here because three
 * loops need it: the status poll, the automatic fetch, and the repo-badge tier
 * scheduler.
 *
 * Three properties come from re-deciding after every run rather than up front:
 *
 * - **Runs never overlap.** The next delay is armed when the previous run
 *   settles, so a slow tick delays the next one instead of racing it. No
 *   in-flight guard is needed.
 * - **A cadence change applies immediately.** {@link PacedLoop.reschedule}
 *   re-asks `dueAt` and re-arms against the run that already happened, so
 *   dropping from a 30 s cadence to 2 s does not wait out the 30 s first.
 * - **Parking is expressible.** A `dueAt` of `Infinity` arms nothing at all —
 *   what "auto-fetch is switched off" means — and the loop starts again on the
 *   next `reschedule` without anything having ticked in between.
 */

/**
 * Smallest delay the loop will arm, so a `dueAt` already in the past cannot
 * spin. Far below every cadence in the app (the shortest is the 2 s status
 * poll), so it never shapes a real schedule.
 */
const MIN_DELAY_MS = 250

export interface PacedLoop {
  /** Begin. The first run happens one cadence from now, never immediately. */
  start(): void
  /** Stop and forget the pending run. Safe to call when never started. */
  stop(): void
  /** Re-decide when the next run is due — after a cadence or config change. */
  reschedule(): void
}

export interface PacedLoopOptions {
  /**
   * When the next run is due, as an epoch timestamp, given when the previous
   * one finished (or when the loop started). Return `Infinity` to park.
   */
  dueAt: (lastRunAt: number) => number
  /** The work. Rejections are logged and do not stop the loop. */
  run: () => Promise<void>
  /** Name for the failure log. */
  label: string
  /**
   * Added to the first run's delay only — a start-up skew, so two clients
   * launched together do not stay in phase forever.
   */
  skewFirstMs?: number
}

export function pacedLoop(options: PacedLoopOptions): PacedLoop {
  let timer: ReturnType<typeof setTimeout> | null = null
  let lastRunAt = 0
  let started = false
  let running = false
  let ranOnce = false

  function arm(): void {
    if (timer !== null) {
      clearTimeout(timer)
      timer = null
    }
    // A run in flight arms the next delay itself when it settles; arming here
    // as well is what would let two runs overlap.
    if (!started || running) return
    const due = options.dueAt(lastRunAt)
    if (!Number.isFinite(due)) return
    const skew = ranOnce ? 0 : (options.skewFirstMs ?? 0)
    timer = setTimeout(tick, Math.max(MIN_DELAY_MS, due + skew - Date.now()))
  }

  async function tick(): Promise<void> {
    timer = null
    running = true
    try {
      await options.run()
    } catch (error) {
      console.warn(`[${options.label}] tick failed:`, error)
    } finally {
      running = false
      ranOnce = true
      lastRunAt = Date.now()
      arm()
    }
  }

  return {
    start(): void {
      started = true
      // Sleep first, work second: whoever started the loop has just done the
      // thing it repeats (the initial status read, the launch fetch), so the
      // first tick belongs one cadence away. `ranOnce` deliberately survives a
      // restart — the skew is once per session, not once per repository, and
      // re-applying it on every switch would just delay each new repo's first
      // fetch for no reason.
      lastRunAt = Date.now()
      arm()
    },
    stop(): void {
      started = false
      if (timer !== null) {
        clearTimeout(timer)
        timer = null
      }
    },
    reschedule: arm,
  }
}
