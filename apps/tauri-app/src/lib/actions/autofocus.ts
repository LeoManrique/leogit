/**
 * Focus an element as soon as it mounts.
 *
 * Use `use:autofocus` in place of the HTML `autofocus` attribute: Svelte flags
 * the attribute (a11y_autofocus) and it is unreliable for elements that mount
 * inside conditional blocks (overlays, dropdowns) where the browser may not
 * re-apply it. The action runs exactly when the node appears in the DOM.
 */
export function autofocus(node: HTMLElement) {
  node.focus()
}
