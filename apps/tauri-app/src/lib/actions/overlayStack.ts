import { writable } from 'svelte/store'
import type { Action } from 'svelte/action'

/**
 * Who Escape belongs to right now.
 *
 * Every dismissable surface — the popovers, the settings and help overlays,
 * every confirmation, the context menu — registers itself for as long as it is
 * on screen, and Escape goes to whichever registered *last*. Registration order
 * is the stacking order, because a dialog opened from a popover mounts after
 * it; nothing has to be told which surface is on top.
 *
 * It replaces two hand-written lists of overlay flags, one per host, which had
 * already drifted apart and which closed **every** overlay at once — so
 * dismissing a confirmation also dismissed the popover that raised it, and a
 * surface added to one host was silently undismissable from the other. There is
 * no list to forget a surface any more: a surface registers where it is built.
 */
type Entry = { dismiss: () => void }

const stack: Entry[] = []

/**
 * How many surfaces are stacked over the repository view.
 *
 * The app's own chords read this before firing: the question a dialog is asking
 * is the only one on screen, and answering it with a ⌘↩ that means "commit"
 * would answer something else entirely. It used to be a hand-written list of
 * overlay flags, which four surfaces had never been added to — a commit chord
 * fired straight through the embedded-repo confirmation asking about that very
 * commit. Registration can't be forgotten the way a list entry can.
 */
export const overlayDepth = writable(0)

/**
 * Register this element's surface for the lifetime of the element. The
 * parameter is what Escape should do — usually the surface's own cancel, and
 * for a surface with an inner mode (the branch picker) the step back out of it.
 */
export const dismissOnEscape: Action<HTMLElement, () => void> = (_node, dismiss) => {
  const entry: Entry = { dismiss }
  stack.push(entry)
  overlayDepth.set(stack.length)
  return {
    update(next: () => void) {
      entry.dismiss = next
    },
    destroy() {
      const at = stack.indexOf(entry)
      if (at !== -1) stack.splice(at, 1)
      overlayDepth.set(stack.length)
    },
  }
}

/**
 * Hand Escape to the topmost registered surface; returns whether one took it.
 *
 * A surface mid-operation takes the key and does nothing with it — a clone in
 * flight can't be abandoned by hiding its dialog — because falling through to
 * whatever sits underneath would dismiss the wrong thing. "Consumed" therefore
 * means *addressed*, not *closed*.
 */
export function dismissTopOverlay(): boolean {
  const top = stack[stack.length - 1]
  if (!top) return false
  top.dismiss()
  return true
}
