/**
 * Who currently owns text entry, for the handlers that have to care.
 *
 * Two questions ask it, about two different subjects, and both go through the
 * same predicate so they cannot drift: the window key handler asks it about the
 * *event target* ("was this chord typed into a field?"), and the auto-fetch
 * loop asks it about `document.activeElement` ("is someone typing right now?").
 */

/**
 * Whether `node` is somewhere text is being typed — an `<input>`, a
 * `<textarea>`, or any `contenteditable` host.
 *
 * xterm.js's hidden `<textarea>` matches, deliberately: for the typing guard
 * that is the right answer (a fetch reordering the file list under a running
 * shell is exactly as unwelcome as under the commit composer, and the native
 * client suppresses on terminal focus on purpose). Handlers that need the
 * terminal to be *distinguishable* rather than merely included ask
 * `isFromTerminal` first — see `utils/keyboard.ts`.
 */
export function isTextInputElement(node: EventTarget | null): boolean {
  if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) return true
  return node instanceof HTMLElement && node.isContentEditable
}

/**
 * Whether someone is typing into this window *at this instant*.
 *
 * Asked at the moment the answer is needed rather than latched from
 * `focusin`/`focusout`, because a latch strands: removing a focused element
 * raises no `focusout`, so closing the terminal panel mid-session left the flag
 * stuck at `true` and auto-fetch silently dead for the rest of the session.
 * Nothing here is stored, so there is nothing to get stuck.
 *
 * The window's own focus is half the question. `document.activeElement` does
 * **not** clear when the window loses focus — that is exactly what
 * `document.hasFocus()` is for — so asking about the element alone would say
 * "still typing" for as long as the user was away in another app, and hold back
 * every automatic fetch for that whole time. The native client gets the same
 * answer from AppKit for free: `NSApp.keyWindow` is nil while the app is
 * inactive, so its first responder is nil and nobody is typing.
 */
export function isTextInputFocused(): boolean {
  return document.hasFocus() && isTextInputElement(document.activeElement)
}
