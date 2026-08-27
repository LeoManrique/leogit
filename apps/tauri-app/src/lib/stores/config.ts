import { writable } from 'svelte/store'
import { configApi, gitApi, type Config, type FileEntry } from '$lib/api/commands'

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

export async function refreshConfig(): Promise<Config | null> {
  try {
    const cfg = await configApi.loadConfig()
    config.set(cfg)
    applyTheme(cfg.theme)
    // Path arithmetic in core, so this is cheap — but a failure here must not
    // fail the config load: search then matches whole paths and the empty
    // state just can't say where it looked.
    scanFolders.set(await gitApi.effectiveScanPaths(cfg.scan_paths ?? []).catch(() => []))
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
