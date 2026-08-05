/**
 * Helpers for *filesystem* paths — repo roots, launch targets — which the
 * backend hands over in the platform's own form. On Windows that means
 * backslashes, so anything splitting on '/' alone sees the whole path as a
 * single segment: a repo row without a GitHub remote to name it ended up
 * labelled `C:\Users\Leo\Dev\ryubing\Ryubing` instead of `Ryubing`.
 *
 * Deliberately NOT for git's paths. Git reports paths with forward slashes on
 * every platform, and `PathText` / `fileActions` rely on that — a
 * separator-agnostic split there would cut a filename that legitimately
 * contains a backslash on Linux or macOS.
 */

/**
 * Last segment of a filesystem path, tolerating either separator and any
 * number of trailing ones. Falls back to the whole path when there's no
 * segment to take (a bare root), so a label is never empty.
 */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts[parts.length - 1] || path
}
