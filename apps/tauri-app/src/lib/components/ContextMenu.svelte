<script lang="ts" module>
  export interface ContextMenuItem {
    label: string
    action: () => void
    enabled?: boolean
    destructive?: boolean
    separator?: boolean
  }
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { dismissOnEscape } from '$lib/actions/overlayStack'

  interface Props {
    x: number
    y: number
    items: ContextMenuItem[]
    onClose: () => void
  }

  let { x, y, items, onClose }: Props = $props()

  // Small offset so the cursor sits just outside the menu's first item — avoids
  // accidental activation and gives the menu a bit of breathing room from the
  // click point.
  const CURSOR_OFFSET = 4

  let menuEl: HTMLDivElement | undefined = $state()
  let focusIdx = $state(0)

  // Viewport-clamped position, applied after the menu has measurable
  // dimensions. Null until measured (and reset whenever the anchor x/y change),
  // so the menu first renders at the click point, then nudges in-bounds. Base
  // position derives from the props so reusing the instance at a new anchor
  // moves it immediately, without waiting for the measure pass.
  let clamped = $state<{ x: number; y: number } | null>(null)
  const posX = $derived(clamped ? clamped.x : x + CURSOR_OFFSET)
  const posY = $derived(clamped ? clamped.y : y + CURSOR_OFFSET)

  const focusable = $derived(
    items.map((it, i) => ({ it, i })).filter(({ it }) => !it.separator && it.enabled !== false),
  )

  // Recompute position when x/y change (same component instance reused across
  // right-clicks) AND clamp to the viewport.
  // IMPORTANT: this component must NOT be rendered inside any ancestor with a
  // `transform`, `filter`, `perspective`, or `will-change: transform`, because
  // those establish a containing block for `position: fixed` descendants. The
  // CommitList virtual-scroll wrapper (`transform: translateY`) is the obvious
  // trap — render the menu as a sibling of that wrapper, not a child.
  $effect(() => {
    // Re-read the anchor so this re-runs when the menu is reused at a new
    // position; drop any prior clamp so the base (click-point) position shows
    // immediately before the measure pass refines it.
    const cx = x
    const cy = y
    clamped = null
    if (!menuEl) return
    tick().then(() => {
      if (!menuEl) return
      const rect = menuEl.getBoundingClientRect()
      const margin = 6
      const vw = window.innerWidth
      const vh = window.innerHeight
      let nx = cx + CURSOR_OFFSET
      let ny = cy + CURSOR_OFFSET
      if (nx + rect.width + margin > vw) nx = Math.max(margin, vw - rect.width - margin)
      if (ny + rect.height + margin > vh) ny = Math.max(margin, vh - rect.height - margin)
      clamped = { x: nx, y: ny }
    })
  })

  onMount(() => {
    const first = focusable[0]
    if (first) focusIdx = first.i

    function onPointer(e: MouseEvent) {
      if (!menuEl) return
      if (!menuEl.contains(e.target as Node)) onClose()
    }
    // Escape is the overlay stack's, not this listener's. Handling it here as
    // well as there closed the menu *and* the popover it was opened over, which
    // is the whole class of bug the stack exists to end.
    function onKey(e: KeyboardEvent) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        if (focusable.length === 0) return
        const order = focusable.map(({ i }) => i)
        const here = order.indexOf(focusIdx)
        const next =
          e.key === 'ArrowDown'
            ? order[(here + 1 + order.length) % order.length]
            : order[(here - 1 + order.length) % order.length]
        focusIdx = next
        return
      }
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        const target = items[focusIdx]
        if (target && !target.separator && target.enabled !== false) {
          target.action()
          onClose()
        }
      }
    }
    // Use capture so we catch clicks before they bubble to other handlers.
    window.addEventListener('mousedown', onPointer, true)
    window.addEventListener('contextmenu', onPointer, true)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('mousedown', onPointer, true)
      window.removeEventListener('contextmenu', onPointer, true)
      window.removeEventListener('keydown', onKey)
    }
  })

  function activate(item: ContextMenuItem) {
    if (item.separator || item.enabled === false) return
    item.action()
    onClose()
  }
</script>

<div
  bind:this={menuEl}
  class="context-menu"
  style="left: {posX}px; top: {posY}px;"
  role="menu"
  tabindex="-1"
  use:dismissOnEscape={onClose}
>
  {#each items as item, i (i)}
    {#if item.separator}
      <div class="separator" role="separator"></div>
    {:else}
      <button
        class="menu-item"
        class:destructive={item.destructive}
        class:focused={i === focusIdx}
        disabled={item.enabled === false}
        onclick={() => activate(item)}
        onmouseenter={() => {
          if (item.enabled !== false) focusIdx = i
        }}
        role="menuitem"
      >
        <span class="menu-label">{item.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  /*
    An AppKit context menu, reproduced. Every number below is a measurement of a
    live `NSMenu` on macOS 26 rather than a value chosen here, because the native
    client shows the system menu and this one has to arrive at the same picture
    by hand. `docs/plans/tauri-reskin.md` §6.2 tabulates each one and where it
    came from.

    The one thing that cannot be reproduced is the surface itself: the real menu
    is a Liquid Glass window, and STYLE.md rules `backdrop-filter` out because
    these windows are opaque. `--bg-elevated` and a hairline stand in for the
    material and for the edge the glass gives it — which is also why the paddings
    here are one pixel short of the measured insets. That 1px border is part of
    the 5px the highlight is inset by and part of the 16px the label sits in
    from the menu's outer edge, so the box model has to give it back.
  */
  .context-menu {
    position: fixed;
    z-index: 2000;
    /* Load-bearing across files: `Header.svelte` anchors both of its menus at
       `rect.right - 200` so they hang from the right edge of their chevron, and
       only this minimum makes that land. */
    min-width: 200px;
    /* `max-content` rather than the shrink-to-fit a positioned box would take:
       that fits the menu to `viewport - left`, so a menu opened near the right
       edge would measure narrow, be clamped left on that measurement, and then
       silently re-expand past the margin the clamp had just reserved. Sizing to
       the content instead makes the measured width the final one at any `left`.
       The cap is the clamp's own 6px margin doubled, so a menu can never be
       wider than the clamp is able to bring back on screen; a label longer than
       that ellipsizes, which is what AppKit does with one too. */
    width: max-content;
    max-width: calc(100vw - 12px);
    /* 5px of content inset, less the 1px border. */
    padding: 4px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    /* 12pt, and continuous rather than circular in the original — CSS has no
       squircle, so this is the closest the reproduction gets. */
    border-radius: 12px;
    box-shadow: var(--shadow-popover);
    display: flex;
    flex-direction: column;
  }

  .menu-item {
    display: flex;
    align-items: center;
    /* 24pt rows, and 11px of text inset that puts the label 16px in from the
       menu's outer edge once the padding and the border are counted. */
    height: 24px;
    padding: 0 11px;
    background: transparent;
    color: var(--text-primary);
    border: none;
    /* The highlight's own radius, inside the menu's 12px. */
    border-radius: 7px;
    /* `NSFont.menuFont(ofSize: 0)` is the 13pt system face at regular weight,
       which is what the app's body font already is. */
    font-size: 13px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    /* A menu highlight is instant in AppKit, and at accent strength a fade
       smears into a trail when the pointer sweeps down the list. Stated rather
       than omitted, because the global `button` rule would otherwise lend this
       one its 120ms. */
    transition: none;
  }

  /* Cancels the global `button:hover` / `button:active` washes from `app.css`,
     which outrank `.menu-item`'s own transparent background — an element in the
     selector beats a class — and would otherwise tint a row the pointer reached
     without firing `mouseenter`, the menu being re-positioned under a still
     pointer being the case that does it. The highlight below outranks this. */
  .menu-item:hover {
    background: transparent;
  }

  /* The label keeps the app's inherited line-height. AppKit's own box is 16pt
     in the 24pt row, but pinning that here would buy nothing — the row centres
     the glyphs either way — while `overflow: hidden` would then clip a font
     whose natural line box is taller than 16px at 13px. Linux falls back to
     exactly such a face (Noto Sans runs ~17.7px), and it is a ship platform. */
  .menu-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Ordered ahead of the highlight deliberately: a highlighted destructive item
     is white on the accent in AppKit, not red on it, and the rule below both
     outranks this one and follows it. */
  .menu-item.destructive {
    color: var(--status-red);
  }

  /* The pointer and the keyboard cursor draw the same highlight, because AppKit
     gives them one treatment — the accent fill, `selectedMenuItemTextColor`
     (white in both appearances) on top.

     Exactly one row wears it, and the rule is keyed on `focusIdx` alone to keep
     it that way: `mouseenter` moves that index, so the hovered row *is* the
     focused row in every ordinary interaction. Naming `:hover` here as well
     would light a second row the moment an arrow key follows a hover — the
     pointer's row and the row Return acts on, both in full accent, with nothing
     saying which is which. AppKit highlights one item at a time. */
  .menu-item.focused:not(:disabled) {
    background: var(--border-active);
    color: var(--on-accent);
  }

  /* Last, so a disabled item that is also destructive reads disabled: the two
     rules carry equal specificity, and this is the one that must win. A
     disabled row takes no highlight either, which is what the `:not(:disabled)`
     above enforces. */
  .menu-item:disabled {
    color: var(--text-faint);
    cursor: default;
  }

  /* An 11pt separator row: 5px, a hairline, 5px. The 11px inset lines it up
     with the labels' leading edge instead of running to the menu's own — the
     separator is inset further than the highlight, not less. */
  .separator {
    height: 1px;
    margin: 5px 11px;
    background: var(--border-inactive);
  }
</style>
