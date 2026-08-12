import { join } from '@tauri-apps/api/path'
import { gitApi, osApi, type FileEntry } from '$lib/api/commands'

// Stateless helpers behind the Changes-tab file context menu. The repo-state
// mutations (discard, ignore) still go through MainLayout so they can confirm /
// refresh; these are the side-effect-only actions plus the small pure helpers
// the menu needs to build its labels.

/** Copy the file's absolute path to the clipboard. */
export async function copyAbsolutePath(repoPath: string, file: FileEntry): Promise<void> {
  // join() is platform-aware, so this yields native separators on Windows.
  const abs = await join(repoPath, file.path)
  await navigator.clipboard.writeText(abs)
}

/** Copy the file's repo-relative path (as git reports it) to the clipboard. */
export async function copyRelativePath(file: FileEntry): Promise<void> {
  await navigator.clipboard.writeText(file.path)
}

/** Reveal the file in the OS file manager. */
export function revealFile(repoPath: string, file: FileEntry): Promise<void> {
  return osApi.revealPath(repoPath, file.path)
}

/** Open the file with its default OS application. */
export function openWithDefault(repoPath: string, file: FileEntry): Promise<void> {
  return osApi.openPath(repoPath, file.path)
}

/** Add a single file's exact path to .gitignore (the backend escapes its glob
 * metacharacters so the rule matches the file verbatim). */
export function ignoreFile(repoPath: string, file: FileEntry): Promise<void> {
  return gitApi.ignorePaths(repoPath, [file.path])
}

/** Add a `*.<ext>` rule to .gitignore so every file of that type is ignored. */
export function ignoreExtension(repoPath: string, ext: string): Promise<void> {
  return gitApi.appendToGitignore(repoPath, [`*${ext}`])
}

/**
 * The file's extension *with* the leading dot (e.g. `.md`), or null when it has
 * none. A leading-dot name like `.gitignore` counts as no extension (matching
 * how an "Ignore all .X files" rule would otherwise be nonsensical).
 */
export function fileExtension(path: string): string | null {
  const name = path.slice(path.lastIndexOf('/') + 1)
  const dot = name.lastIndexOf('.')
  if (dot <= 0) return null
  return name.slice(dot)
}

/** Platform-appropriate label for the "reveal in file manager" menu item. */
export function revealLabel(): string {
  const ua = navigator.userAgent
  if (ua.includes('Mac')) return 'Reveal in Finder'
  if (ua.includes('Win')) return 'Show in Explorer'
  return 'Show in File Manager'
}

/**
 * Intents the Changes-tab context menu raises. Implemented by the view that
 * owns the repo path, the confirm dialog, and the post-mutation status refresh
 * (MainLayout); passed down to FileList, which only decides what to show and
 * which intent each item fires. When undefined, FileList shows no menu.
 */
export interface FileContextActions {
  /** Discard working-tree changes for one or more files (asks to confirm). */
  discard: (files: FileEntry[]) => void
  /** Add this file's exact path to .gitignore. */
  ignoreFile: (file: FileEntry) => void
  /** Add a `*.<ext>` rule to .gitignore (ext includes the leading dot). */
  ignoreExtension: (ext: string) => void
  /** Copy the file's absolute path to the clipboard. */
  copyPath: (file: FileEntry) => void
  /** Copy the file's repo-relative path to the clipboard. */
  copyRelativePath: (file: FileEntry) => void
  /** Reveal the file in the OS file manager. */
  reveal: (file: FileEntry) => void
  /** Open the file with its default OS application. */
  openWithDefault: (file: FileEntry) => void
}
