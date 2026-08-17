import { writable } from 'svelte/store'
import { configApi, gitApi, type Config } from '$lib/api/commands'

export const config = writable<Config | null>(null)

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
