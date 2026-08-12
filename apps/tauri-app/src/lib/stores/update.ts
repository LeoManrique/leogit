import { writable } from 'svelte/store'
import type { UpdateInfo } from '$lib/api/commands'

/**
 * A newer release found by the startup check (`updateChecker`), or null while
 * this build is current / the check hasn't completed. Read by the header's
 * update chip.
 */
export const availableUpdate = writable<UpdateInfo | null>(null)

/**
 * Session-scoped dismissal of the update chip. Deliberately not persisted:
 * like the check itself it resets on relaunch, so a skipped release
 * resurfaces on the next start instead of being forgotten forever.
 */
export const updateDismissed = writable(false)
