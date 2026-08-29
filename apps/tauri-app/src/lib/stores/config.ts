import { get, writable } from 'svelte/store'
import { configApi, gitApi, type Config, type ConfigPatch, type FileEntry } from '$lib/api/commands'

export const config = writable<Config | null>(null)

/**
 * Letter and label per file status, from core — so the two clients can't
 * invent different glyphs again, which they had, on the conflicted row.
 *
 * Fetched once at startup rather than per row: a changed-file list draws these
 * on every repaint. Empty until the first load; the badge falls back to the
 * status name's initial, which is only ever visible for the first frame of a
 * cold start.
 */
export const fileStatusStyles = writable<
  Record<FileEntry['status'], { letter: string; label: string }>
>({} as Record<FileEntry['status'], { letter: string; label: string }>)

export async function loadFileStatusStyles(): Promise<void> {
  try {
    const styles = await gitApi.fileStatusStyles()
    fileStatusStyles.set(
      Object.fromEntries(
        styles.map((s) => [s.status, { letter: s.letter, label: s.label }])
      ) as Record<FileEntry['status'], { letter: string; label: string }>
    )
  } catch (e) {
    console.error('[status] glyph table load failed', e)
  }
}

/**
 * The folders discovery actually walks: `scan_paths` with `~` expanded and the
 * stock fallbacks filled in when the list is empty. Derived from the config, so
 * it refreshes with it — repo search strips these prefixes before matching a
 * path, and the picker's empty state names them.
 */
export const scanFolders = writable<string[]>([])

/**
 * Publish a config the backend has just handed back: the shared store, the
 * theme it names, and the scan folders it implies.
 *
 * `patch_config` returns the whole normalized config, so a settings control
 * that just wrote one field already holds the new state and needs no second
 * `load_config` round trip to publish it. Everything downstream reads `config`,
 * which is why a diff setting changed here reaches the open diff without anyone
 * telling it to.
 *
 * Resolving the scan folders is the one part that costs a command, so it only
 * re-runs when the paths actually moved — a theme change doesn't move them.
 */
export async function applyConfig(cfg: Config): Promise<void> {
  const previous = get(config)
  config.set(cfg)
  applyTheme(cfg.theme)
  const paths = cfg.scan_paths ?? []
  if (previous && sameStrings(previous.scan_paths ?? [], paths)) return
  // Path arithmetic in core, so this is cheap — but a failure here must not
  // fail the config load: search then matches whole paths and the empty
  // state just can't say where it looked.
  scanFolders.set(await gitApi.effectiveScanPaths(paths).catch(() => []))
}

function sameStrings(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((value, i) => value === b[i])
}

/**
 * Writes in flight, chained. Two quick edits must land in the order they were
 * made, and that has to hold *across* surfaces now that the diff header writes
 * its layout while the Settings dialog can be standing open on the same file.
 * A failed write must not strand the ones behind it, so the chain is kept on
 * the swallowed copy.
 */
let writes: Promise<unknown> = Promise.resolve()

/**
 * Apply a field-wise patch and publish the config core hands back.
 *
 * The one writer. A surface patches the fields it owns and nothing else, so it
 * cannot revert what another surface — or the other client — changed while it
 * was open. The returned config is normalized, which is what lets a caller
 * re-seed its control from the answer rather than from what it asked for.
 *
 * Resolves with `before` as well, read **inside** the chain: it is what this
 * write replaced, not what was on screen when the caller asked. A form that
 * compares the two to decide whether anything moved has to compare against the
 * config immediately preceding its own write, or an earlier queued patch shows
 * up as the difference and the comparison silently stops being about this one.
 */
export function patchConfig(
  fields: ConfigPatch
): Promise<{ before: Config | null; updated: Config }> {
  const write = writes.then(async () => {
    const before = get(config)
    const updated = await configApi.patchConfig(fields)
    await applyConfig(updated)
    return { before, updated }
  })
  writes = write.catch(() => {})
  return write
}

export async function refreshConfig(): Promise<Config | null> {
  try {
    const cfg = await configApi.loadConfig()
    await applyConfig(cfg)
    return cfg
  } catch (e) {
    console.error('config load failed', e)
    return null
  }
}

export function applyTheme(theme: string): void {
  if (typeof document === 'undefined') return
  document.documentElement.dataset.theme = theme === 'light' ? 'light' : 'dark'
}
