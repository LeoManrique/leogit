<script lang="ts">
  interface Props {
    path: string
    /**
     * Render as the "from" side of a rename: whole path muted (filename
     * included). Truncation still keeps the filename and collapses the
     * directory, so both sides of a move read the same way.
     */
    dim?: boolean
  }

  let { path, dim = false }: Props = $props()

  let container: HTMLDivElement
  let measureRef: HTMLSpanElement
  let displayLength = $state<number | null>(null)

  function truncateMid(value: string, length: number): string {
    if (value.length <= length) return value
    if (length <= 0) return ''
    if (length === 1) return '…'
    const mid = (length - 1) / 2
    const pre = value.substring(0, Math.floor(mid))
    const post = value.substring(value.length - Math.ceil(mid))
    return `${pre}…${post}`
  }

  function truncatePath(p: string, length: number): string {
    if (p.length <= length) return p
    if (length <= 0) return ''
    if (length === 1) return '…'
    const lastSep = p.lastIndexOf('/')
    if (lastSep === -1) return truncateMid(p, length)
    const filenameLen = p.length - lastSep - 1
    if (filenameLen + 2 > length) return truncateMid(p, length)
    const pre = p.substring(0, length - filenameLen - 2)
    const post = p.substring(lastSep)
    return `${pre}…${post}`
  }

  function splitDisplay(displayed: string): { dir: string; name: string } {
    const lastSep = displayed.lastIndexOf('/')
    if (lastSep === -1) return { dir: '', name: displayed }
    return {
      dir: displayed.substring(0, lastSep + 1),
      name: displayed.substring(lastSep + 1),
    }
  }

  const displayed = $derived(
    displayLength === null ? path : truncatePath(path, displayLength),
  )
  const parts = $derived(splitDisplay(displayed))
  const truncated = $derived(displayed.length < path.length)

  function measureWidth(text: string): number {
    if (!measureRef) return 0
    measureRef.textContent = text
    return measureRef.getBoundingClientRect().width
  }

  const TRAILING_PAD = 2

  function fit() {
    if (!container || !measureRef) return
    const available = container.clientWidth - TRAILING_PAD
    if (available <= 0) return
    const fullWidth = measureWidth(path)
    if (fullWidth <= available) {
      if (displayLength !== null) displayLength = null
      return
    }
    let lo = 1
    let hi = path.length
    let best = 1
    while (lo <= hi) {
      const mid = Math.floor((lo + hi) / 2)
      const w = measureWidth(truncatePath(path, mid))
      if (w <= available) {
        best = mid
        lo = mid + 1
      } else {
        hi = mid - 1
      }
    }
    if (displayLength !== best) displayLength = best
  }

  $effect(() => {
    void path
    if (!container) return
    const ro = new ResizeObserver(() => fit())
    ro.observe(container)
    fit()
    return () => ro.disconnect()
  })
</script>

<div class="path-text" bind:this={container} title={truncated ? path : undefined}>
  <span class="path-visible" class:dim
    >{#if parts.dir}<span class="dirname">{parts.dir}</span>{/if}<span class="filename">{parts.name}</span></span
  >
  <span bind:this={measureRef} class="path-measure" aria-hidden="true"></span>
</div>

<style>
  .path-text {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    white-space: nowrap;
  }

  .path-visible {
    display: inline-block;
    white-space: nowrap;
  }

  .path-measure {
    position: absolute;
    visibility: hidden;
    pointer-events: none;
    top: 0;
    left: 0;
    white-space: nowrap;
  }

  .dirname {
    color: var(--text-muted);
  }

  .filename {
    color: var(--text-primary);
  }

  /* "From" side of a rename: whole path muted so the "to" side reads as the
     current name. Filename stays legible (no strikethrough). */
  .path-visible.dim .filename {
    color: var(--text-muted);
  }
</style>
