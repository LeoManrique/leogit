/**
 * Where a key event came from, for the app's global keyboard handlers.
 *
 * The embedded terminal is not a text field — it is a PTY, and the shell owns
 * every key the app hasn't explicitly reserved. xterm.js hosts its input in a
 * hidden `<textarea>`, so a plain "is this a text input?" test both misfires
 * (Ctrl+P is readline's previous-history *and* would kick a push) and
 * misses (the panel's own toggle became unreachable from inside the panel).
 *
 * One rule, applied by every window-level handler: **while the terminal has
 * focus, the only global chord is the panel's own toggle.** Everything else —
 * Escape, Ctrl+P, ⌘R, the overlay chords — belongs to the shell.
 */

/** Whether a key event was raised inside the embedded terminal emulator. */
export function isFromTerminal(e: Event): boolean {
  const target = e.target
  // `.xterm` is the class xterm.js puts on the element it was opened into, so
  // this matches the emulator and its hidden textarea and nothing else — the
  // dock's own header buttons stay ordinary app chrome.
  return target instanceof Element && target.closest('.xterm') !== null
}
