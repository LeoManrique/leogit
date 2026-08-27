import { get } from 'svelte/store'
import { configApi } from '$lib/api/commands'
import { config } from '$lib/stores/config'
import { patchReposState } from '$lib/stores/reposState'

/**
 * Where the Clone dialog should offer to put the new repo: the folder the last
 * clone landed in, else the first configured scan path (so the clone shows up
 * in the picker afterwards), else `~/Dev`.
 *
 * Shared because the dialog is reachable from two phases now — the startup
 * picker and the main-view dropdown — and a seed that differed between them
 * would quietly clone into two different places depending on where you started.
 */
export async function resolveCloneDefaultDir(): Promise<string> {
  let lastDir: string | undefined
  try {
    lastDir = (await configApi.loadState()).last_clone_dir
  } catch {
    // A missing or unreadable state file just means "no remembered folder".
  }
  return lastDir || get(config)?.scan_paths?.[0] || '~/Dev'
}

/** Remember where a clone landed, so the next one is offered the same folder. */
export async function rememberCloneDir(parentDir: string): Promise<void> {
  await patchReposState({ last_clone_dir: parentDir })
}
