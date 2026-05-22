import { writable } from 'svelte/store'
import { configApi, type Config } from '$lib/api/commands'

export const config = writable<Config | null>(null)

export async function refreshConfig(): Promise<Config | null> {
  try {
    const cfg = await configApi.loadConfig()
    config.set(cfg)
    applyTheme(cfg.theme)
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
