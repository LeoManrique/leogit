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
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 2000;
    min-width: 200px;
    padding: 4px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 8px;
    box-shadow: var(--shadow-popover);
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .menu-item {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 10px;
    background: transparent;
    color: var(--text-primary);
    border: none;
    border-radius: 4px;
    font-size: 13px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 80ms ease;
  }

  .menu-item.focused:not(:disabled),
  .menu-item:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .menu-item:disabled {
    color: var(--text-faint);
    cursor: default;
  }

  .menu-item.destructive {
    color: var(--status-red);
  }

  .separator {
    height: 1px;
    margin: 4px 6px;
    background: var(--border-inactive);
  }
</style>
