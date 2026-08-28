/**
 * Which desktop this WebView is running on.
 *
 * The client ships to macOS, Windows and Linux from one bundle, and a handful
 * of strings have to name what the user actually presses or where a file
 * actually opens. Kept in one place so those strings can't disagree about what
 * platform they are on — the previous arrangement had the test written out at
 * each site.
 */

/** Whether this is macOS, where the modifier is ⌘ and the file manager Finder. */
export function isMac(): boolean {
  return navigator.userAgent.includes('Mac')
}

/** Whether this is Windows, where the file manager is Explorer. */
export function isWindows(): boolean {
  return navigator.userAgent.includes('Win')
}
