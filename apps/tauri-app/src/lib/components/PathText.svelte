<script lang="ts">
  interface Props {
    path: string
    /**
     * Mute the filename as well as the directory, for a path that is not the
     * one the eye should land on: a rename's "from" side, which the arrow
     * points away from, and a dirty submodule, which the parent repository
     * cannot stage. Truncation still keeps the filename and collapses the
     * directory, so both sides of a move read the same way. `isMuted` natively.
     */
    muted?: boolean
    /**
     * Raise the filename to medium — the cue on a row that is going into the
     * next commit. A prop and not a class the row sets from outside, because
     * a medium filename is wider than a regular one and a change of face has
     * to refit: the row's width does not move when a checkbox is ticked, so
     * nothing else would tell the fit to run again. `nameWeight` natively.
     */
    emphasized?: boolean
    /**
     * Take no more width than the whole path needs, rather than all the row
     * offers. A lone path fills its row either way; a rename's two sides both
     * hug, so they sit together as `old → new`, and a row that is short of
     * room lets the side that fits in its half keep its full width and gives
     * the rest to the other — two greedy sides split the row down the middle
     * and left a gap between them. `hugsPath` natively.
     */
    hug?: boolean
  }

  let { path, muted = false, emphasized = false, hug = false }: Props = $props()

  let container: HTMLDivElement
  // Two measuring spans, not one, because the two halves are drawn in two
  // different faces: `emphasized` raises the filename to medium, and a medium
  // filename is wider than the regular one. Measuring the whole string in a
  // single span would size an included row in the lighter face and promise a
  // fit the row then overflows. `PathText.swift` states the same rule ("Every
  // face that is drawn is also measured").
  let measureDirRef: HTMLSpanElement
  let measureNameRef: HTMLSpanElement
  let displayLength = $state<number | null>(null)
  // The whole path's width in the faces it is drawn in, plus the trailing pad
  // `fit` keeps clear: what a hugging label caps itself at. Rounded up to a
  // whole pixel, so the width the row gives back can never come out a hair
  // short of the path and cost it a character it had room for. `null` until
  // measured, which leaves the label greedy for its first frame.
  let naturalWidth = $state<number | null>(null)

  function truncateMid(value: string, length: number): string {
    if (value.length <= length) return value
    if (length <= 0) return ''
    if (length === 1) return '…'
    const mid = (length - 1) / 2
    const pre = value.substring(0, Math.floor(mid))
    const post = value.substring(value.length - Math.ceil(mid))
    return `${pre}…${post}`
  }

  interface PathParts {
    dir: string
    name: string
  }

  /**
   * Truncate a path into its muted directory part and bright filename part.
   * The directory shrinks first, down to a trailing "…/" bridge — but never
   * below a first-letter "b…/" hint, so a nested file can't be mistaken for
   * a root file. Only once the hint plus the full filename can't fit does
   * the filename itself middle-truncate. Building the parts here (instead of
   * splitting a truncated string) keeps directory characters from ever being
   * styled as filename or vice versa.
   */
  function truncatePathParts(p: string, length: number): PathParts {
    const lastSep = p.lastIndexOf('/')
    const dir = lastSep === -1 ? '' : p.substring(0, lastSep + 1)
    const name = p.substring(lastSep + 1)
    if (p.length <= length) return { dir, name }
    if (length <= 0) return { dir: '', name: '' }
    if (dir) {
      if (name.length + 3 <= length) {
        const keep = length - name.length - 2
        return { dir: `${dir.substring(0, keep)}…/`, name }
      }
      // A dir short enough to fit within the hint's footprint shows whole.
      const hint = dir.length <= 3 ? dir : `${dir[0]}…/`
      if (length > hint.length) {
        return { dir: hint, name: truncateMid(name, length - hint.length) }
      }
      // Trailing-slash paths (untracked directories) have no filename to keep.
      if (!name) return { dir: '…', name: '' }
    }
    return { dir: '', name: truncateMid(name, length) }
  }

  const parts = $derived(truncatePathParts(path, displayLength ?? path.length))
  const displayed = $derived(parts.dir + parts.name)
  const truncated = $derived(displayed.length < path.length)

  /** Width of a candidate split, each half measured in the face it is drawn in. */
  function measureParts({ dir, name }: PathParts): number {
    if (!measureDirRef || !measureNameRef) return 0
    measureDirRef.textContent = dir
    measureNameRef.textContent = name
    return (
      measureDirRef.getBoundingClientRect().width +
      measureNameRef.getBoundingClientRect().width
    )
  }

  const TRAILING_PAD = 2

  function fit() {
    if (!container || !measureDirRef || !measureNameRef) return
    const wholeWidth = measureParts(truncatePathParts(path, path.length))
    const natural = Math.ceil(wholeWidth) + TRAILING_PAD
    if (naturalWidth !== natural) naturalWidth = natural
    const available = container.clientWidth - TRAILING_PAD
    if (available <= 0) return
    if (wholeWidth <= available) {
      if (displayLength !== null) displayLength = null
      return
    }
    let lo = 1
    let hi = path.length
    let best = 1
    while (lo <= hi) {
      const mid = Math.floor((lo + hi) / 2)
      if (measureParts(truncatePathParts(path, mid)) <= available) {
        best = mid
        lo = mid + 1
      } else {
        hi = mid - 1
      }
    }
    if (displayLength !== best) displayLength = best
  }

  // Every input the fit reads that the container's size cannot report: the
  // path, and the filename's face. A hugging label's cap changes its own width,
  // which the observer does report, and which settles in one pass — the cap is
  // the whole path's width, a number no fit can move.
  $effect(() => {
    void path
    void emphasized
    if (!container) return
    const ro = new ResizeObserver(() => fit())
    ro.observe(container)
    fit()
    return () => ro.disconnect()
  })
</script>

<div
  class="path-text"
  bind:this={container}
  title={truncated ? path : undefined}
  style:max-width={hug && naturalWidth !== null ? `${naturalWidth}px` : undefined}
>
  <span class="path-visible" class:muted class:emphasized
    >{#if parts.dir}<span class="dirname">{parts.dir}</span>{/if}<span class="filename">{parts.name}</span></span
  >
  <!--
    The hidden ruler, built from the same two spans as the visible half so it
    inherits the same two faces — including the medium `.filename` of an
    emphasized path. Its text is written by `measureParts` rather than from the
    template: it changes once per binary-search step and must not drag a
    re-render behind it.
  -->
  <span class="path-visible path-measure" class:emphasized aria-hidden="true"
    ><span class="dirname" bind:this={measureDirRef}></span><span
      class="filename"
      bind:this={measureNameRef}
    ></span></span
  >
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

  /* `.secondary`, not `.tertiary`: `PathText.swift`'s `styled()` paints the
     directory with `foregroundColor = .secondary` and the filename with
     `.primary`. One step of contrast is all the split needs — it exists so the
     eye lands on the file's own name, and a directory dropped two steps reads
     as disabled rather than as context. */
  .dirname {
    color: var(--text-secondary);
  }

  .filename {
    color: var(--text-primary);
  }

  .path-visible.emphasized .filename {
    font-weight: 500;
  }

  /* A muted path drops to the directory's own level — a rename's "from" side,
     so the arrow points from something uniformly quiet to the current name,
     and a dirty submodule, which reads as inactive throughout. Native says the
     same thing as `isMuted`, which resolves the filename to `.secondary` — the
     colour the directory beside it already has. Filename stays legible (no
     strikethrough). */
  .path-visible.muted .filename {
    color: var(--text-secondary);
  }
</style>
