<script lang="ts" module>
  /*
    Every glyph in the app, in one place.

    ── Why this exists ──────────────────────────────────────────────────────
    The icons are the loudest single thing deciding whether this client and the
    native one read as the same product, and the native client draws its chrome
    with SF Symbols. Every glyph therefore lives here and nowhere else: no
    component inlines an `<svg>` of its own. That is not tidiness for its own
    sake — an icon set spread across call sites has no place to enforce a grid,
    a stroke weight or an optical centre, so the same arrow drifts to a
    different weight in each component that draws it and the family resemblance
    is the first thing lost. One registry is the only version of this that can
    hold.

    ── Why the geometry looks the way it does ───────────────────────────────
    SF Symbols are drawn as glyphs in a font, on a fixed design grid, and they
    align to the *cap height* of the text they sit beside rather than being
    centred in their own bounding box — that is what makes an SF Symbol look
    settled next to a label and a Feather-style icon look like it is floating.
    So: a 16-unit viewBox for every icon, shapes kept inside a ~12-unit optical
    area around (8, 8), and directional glyphs (arrows, chevrons) drawn a touch
    larger than square ones, because a shape that tapers reads smaller than a
    shape that does not.

    The stroke weight is the part the old icons got most wrong. An SF Symbol's
    weight tracks the weight of the text beside it: a Regular symbol next to
    13pt body text carries a stroke of roughly 1.2–1.3pt, not the 2px that
    hand-drawn Feather-derived icons carry by default. Scaled onto this
    16-unit grid that is ~1.2 units, which is what `regular` below is. The
    heavier steps exist because the native client genuinely asks for them —
    `HistorySidebar.swift:217` draws its unpushed marker at
    `.font(.system(size: 9, weight: .bold))`, and a bold symbol has a
    proportionally fatter stroke. `weight` is therefore a real design axis,
    not a fudge factor: pick the step that matches the text it sits beside.

    Because stroke-width is in viewBox units, the rendered thickness scales
    with `size` — an 8px icon gets a 0.6px stroke at `regular`. That is the
    correct behaviour and matches how a symbol thins out at small point sizes;
    it is also why the two 8px count badges in the header ask for `bold`.

    Round caps and joins throughout, which is what SF Symbols' terminals look
    like, and `shape-rendering="geometricPrecision"` so a sub-pixel stroke is
    not snapped to the pixel grid and left looking muddy — font glyphs get
    hinting, SVG paths do not, and this is the closest substitute.

    ── What this deliberately is NOT ────────────────────────────────────────
    None of these paths are Apple's artwork, traced or otherwise. SF Symbols
    are licensed for use in software running on Apple platforms, and the
    licence forbids redistributing them or using them as the basis for
    derivative glyphs; a Linux/Windows Tauri build could not ship them even if
    we wanted to. Every shape below was drawn from scratch to *read* as the
    same idea at the same optical weight — a folder, a branch, a refresh loop.
    That puts a hard ceiling on how close the match can get, and it is the
    reason a few of these are recognisably "a folder" rather than "Apple's
    folder". The alignment we can control — grid, optical centring, stroke
    weight, terminal shape — is where the family resemblance actually comes
    from, and that is what this file spends its effort on.

    ── Scope ────────────────────────────────────────────────────────────────
    This renders an `<svg>` and nothing else. No button, no tooltip, no label,
    no state. Call sites keep owning their own `title=`/`aria-label` on the
    interactive element, exactly as they do today.

    ── One trap worth knowing ───────────────────────────────────────────────
    Svelte's scoped CSS cannot reach into a child component's DOM, so a
    parent's `.icon { color: … }` will NOT apply to the svg this renders even
    when the class is passed through — the parent's scope hash is only stamped
    onto elements in the parent's own template. Tint via `color:` on the
    surrounding element (the svg inherits through `currentColor`), or wrap the
    parent's rule in `:global(…)`. The `spin` prop exists for the same reason:
    a keyframe animation defined in a parent could never have reached here.
  */

  /** Every glyph the app can draw. A typo here is a compile error, which is
      the entire reason this is a union and not a `string`. */
  export type IconName =
    // Repository chrome
    | 'folder'
    | 'branch'
    | 'commit'
    // Navigation and window controls
    | 'chevron-up'
    | 'chevron-down'
    | 'chevron-left'
    | 'xmark'
    | 'plus'
    | 'minus'
    | 'gear'
    | 'question-circle'
    // The sync ladder
    | 'arrow-up'
    | 'arrow-down'
    | 'arrow-up-circle'
    | 'arrow-clockwise'
    | 'arrow-2-circlepath'
    | 'icloud-arrow-up'
    // Status
    | 'checkmark'
    | 'checkmark-circle'
    | 'exclamationmark-triangle'
    | 'exclamationmark-triangle-fill'
    // Sorting
    | 'clock'
    | 'textformat-abc'
    // Documents and diffs
    | 'doc'
    | 'doc-text'
    | 'doc-on-doc'
    | 'doc-zipper'
    | 'doc-text-magnifyingglass'
    | 'arrow-turn-down-right'
    | 'rectangle-grid-1x2'
    | 'rectangle-split-2x1'
    // Terminal and app identity
    | 'terminal'
    | 'app-mark'

  /**
   * The four steps that exist, named after the SF Symbols weights they stand
   * in for. Values are stroke widths on the 16-unit grid, so they scale with
   * `size` the way a symbol's stroke scales with point size.
   */
  export type IconWeight = 'regular' | 'medium' | 'semibold' | 'bold'

  const STROKE: Record<IconWeight, number> = {
    regular: 1.2,
    medium: 1.4,
    semibold: 1.6,
    bold: 1.9,
  }

  /**
   * A glyph is a list of shapes. Almost everything is a `<path>`; the one
   * exception is `textformat-abc`, whose subject *is* letterforms — SF draws
   * them in the system typeface, so setting real text in `--font-ui` is a
   * closer match than any path we could trace, as well as the only version
   * that stays right in every locale and at every weight.
   */
  type Shape =
    | { path: string; fill?: boolean; dash?: string }
    | { text: string; y: number; size: number }

  /** A circle as two arcs, so every drawn shape in the registry is one path
      and the template needs exactly one element type. */
  function ring(cx: number, cy: number, r: number): string {
    return `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${r * 2} 0a${r} ${r} 0 1 0 ${-r * 2} 0`
  }

  function dot(cx: number, cy: number, r: number): Shape {
    return { path: ring(cx, cy, r), fill: true }
  }

  /** The page outline shared by `doc` and its variants, so the four document
      glyphs cannot drift apart the way the two refresh loops did. */
  const DOC_PAGE =
    'M3.4 3.5a1.15 1.15 0 0 1 1.15-1.15h3.9l3.15 3.15v7a1.15 1.15 0 0 1-1.15 ' +
    '1.15h-5.9A1.15 1.15 0 0 1 3.4 12.5Z'
  const DOC_FOLD = 'M8.45 2.35v2.15a1 1 0 0 0 1 1h2.15'

  /** The rounded frame shared by the two diff-layout glyphs, which differ only
      in where the divider goes — the same relationship `rectangle.grid.1x2`
      and `rectangle.split.2x1` have on the native side. */
  const PANEL =
    'M2.35 4.6a1.5 1.5 0 0 1 1.5-1.5h8.3a1.5 1.5 0 0 1 1.5 1.5v6.8a1.5 1.5 0 0 ' +
    '1-1.5 1.5h-8.3a1.5 1.5 0 0 1-1.5-1.5Z'

  const ICONS: Record<IconName, Shape[]> = {
    // ── Repository chrome ──
    folder: [
      {
        path:
          'M2.3 4.6a1.2 1.2 0 0 1 1.2-1.2h2.55a1.2 1.2 0 0 1 .87.37l.84.86h4.74a1.2 ' +
          '1.2 0 0 1 1.2 1.2v5.81a1.2 1.2 0 0 1-1.2 1.2H3.5a1.2 1.2 0 0 1-1.2-1.2Z',
      },
    ],
    // Two commits on a trunk with a third on a branch off it. The native
    // `arrow.triangle.branch` is arrows rather than nodes, but the node form is
    // what every Git client draws and what this client already drew; keeping it
    // costs nothing in family resemblance and keeps the meaning obvious.
    branch: [
      { path: ring(4.4, 3.6, 1.35) },
      { path: ring(4.4, 12.4, 1.35) },
      { path: ring(11.6, 5.6, 1.35) },
      { path: 'M4.4 4.95v6.1' },
      { path: 'M10.25 5.6H9A4.6 4.6 0 0 0 4.4 10.2' },
    ],
    // A commit with the branch line passing through it. Nothing draws this:
    // the branch chip deliberately shows `branch` whether or not HEAD is
    // detached, following `BranchMenu.swift:70`, and reports the detachment in
    // its label instead. Kept because "a commit" is a shape this app will want
    // again, and because re-deriving it is the expensive part.
    commit: [{ path: ring(8, 8, 2.4) }, { path: 'M2.3 8h3.3' }, { path: 'M10.4 8h3.3' }],

    // ── Navigation and window controls ──
    'chevron-up': [{ path: 'M4.25 9.65 8 5.9l3.75 3.75' }],
    'chevron-down': [{ path: 'M4.25 6.35 8 10.1l3.75-3.75' }],
    'chevron-left': [{ path: 'M9.65 4.25 5.9 8l3.75 3.75' }],
    xmark: [{ path: 'M4.15 4.15 11.85 11.85M11.85 4.15 4.15 11.85' }],
    plus: [{ path: 'M8 3.1v9.8M3.1 8h9.8' }],
    minus: [{ path: 'M3.1 8h9.8' }],
    // No native counterpart: macOS puts Settings in the app menu, so the
    // SwiftUI client never draws a gear at all. Radial teeth rather than a
    // scalloped outline, because scallops turn to mush below 14px.
    gear: [
      { path: ring(8, 8, 2.35) },
      {
        path:
          'M8 1.85v1.9M8 12.25v1.9M14.15 8h-1.9M3.75 8h-1.9M12.35 3.65 11 5M5 11 ' +
          '3.65 12.35M12.35 12.35 11 11M5 5 3.65 3.65',
      },
    ],
    // Also no native counterpart — the shortcut sheet is a Tauri-only surface.
    'question-circle': [
      { path: ring(8, 8, 5.8) },
      { path: 'M6.15 6.4a1.85 1.85 0 1 1 1.85 1.85v1.1' },
      dot(8, 11.15, 0.72),
    ],

    // ── The sync ladder ──
    'arrow-up': [{ path: 'M8 12.95V3.55M4.35 7.2 8 3.55l3.65 3.65' }],
    'arrow-down': [{ path: 'M8 3.05v9.4M4.35 8.8 8 12.45l3.65-3.65' }],
    'arrow-up-circle': [
      { path: ring(8, 8, 5.8) },
      { path: 'M8 11.15V5.2M5.85 7.35 8 5.2l2.15 2.15' },
    ],
    // One loop with one head: a plain refresh, matching the native Clone
    // sheet's `arrow.clockwise`.
    'arrow-clockwise': [
      { path: 'M13.35 8a5.35 5.35 0 1 1-1.6-3.8' },
      { path: 'M11.3 1.65 11.75 4.2 9.2 3.75' },
    ],
    // Two arcs, two heads: a *sync*, which is a different statement from a
    // refresh, and the one the native sync control makes.
    'arrow-2-circlepath': [
      { path: 'M2.85 8A5.15 5.15 0 0 1 8 2.85a5.15 5.15 0 0 1 4.42 2.5' },
      { path: 'M12.6 2.85 12.42 5.35 10.13 4.33' },
      { path: 'M13.15 8A5.15 5.15 0 0 1 8 13.15a5.15 5.15 0 0 1-4.42-2.5' },
      { path: 'M3.4 13.15 3.58 10.65 5.87 11.67' },
    ],
    'icloud-arrow-up': [
      {
        path:
          'M11.4 8.8a2.4 2.4 0 0 0 .12-4.79 3.3 3.3 0 0 0-6.28-1 3 3 0 0 0-.64 5.79Z',
      },
      { path: 'M8 14.1V9.3M6.2 11.1 8 9.3l1.8 1.8' },
    ],

    // ── Status ──
    checkmark: [{ path: 'M3.15 8.55 6.35 11.75 12.85 4.6' }],
    'checkmark-circle': [{ path: ring(8, 8, 5.8) }, { path: 'M5.35 8.15 7.15 9.95 10.65 6.2' }],
    'exclamationmark-triangle': [
      {
        path:
          'M8.87 2.4a1 1 0 0 0-1.74 0L1.4 12.32a1 1 0 0 0 .87 1.48h11.46a1 1 0 0 0 ' +
          '.87-1.48Z',
      },
      { path: 'M8 6.35v3.3' },
      dot(8, 11.6, 0.72),
    ],
    // The bar and the dot are knocked out of the filled triangle with
    // `fill-rule="evenodd"` rather than painted in the background colour, so
    // the glyph stays correct over any surface — the banner it sits in is
    // tinted, not `--bg-primary`.
    'exclamationmark-triangle-fill': [
      {
        fill: true,
        path:
          'M8.87 2.4a1 1 0 0 0-1.74 0L1.4 12.32a1 1 0 0 0 .87 1.48h11.46a1 1 0 0 0 ' +
          '.87-1.48ZM8 5.6a.78.78 0 0 0-.78.83l.23 3.3a.55.55 0 0 0 1.1 0l.23-3.3A.78 ' +
          '.78 0 0 0 8 5.6Z' +
          ring(8, 11.55, 0.88),
      },
    ],

    // ── Sorting ──
    clock: [{ path: ring(8, 8, 5.55) }, { path: 'M8 4.7V8.2l2.35 1.4' }],
    'textformat-abc': [{ text: 'abc', y: 10.9, size: 9 }],

    // ── Documents and diffs ──
    doc: [{ path: DOC_PAGE }, { path: DOC_FOLD }],
    'doc-text': [
      { path: DOC_PAGE },
      { path: DOC_FOLD },
      { path: 'M5.55 7.9h4.9M5.55 10.15h4.9' },
    ],
    'doc-on-doc': [
      {
        path:
          'M5.6 6.85a1.25 1.25 0 0 1 1.25-1.25h5A1.25 1.25 0 0 1 13.1 6.85v5.55A1.25 ' +
          '1.25 0 0 1 11.85 13.65h-5A1.25 1.25 0 0 1 5.6 12.4Z',
      },
      {
        path:
          'M10.9 5.6V4.1a1.25 1.25 0 0 0-1.25-1.25h-5A1.25 1.25 0 0 0 3.4 4.1v5.55A1.25 ' +
          '1.25 0 0 0 4.65 10.9H5.6',
      },
    ],
    // No corner fold on this one: the zipper runs down the centre, and a fold
    // in the same corner as the pull-tab made both unreadable at 13px.
    'doc-zipper': [
      {
        path:
          'M3.65 3.5a1.15 1.15 0 0 1 1.15-1.15h6.4A1.15 1.15 0 0 1 12.35 3.5v9A1.15 ' +
          '1.15 0 0 1 11.2 13.65H4.8A1.15 1.15 0 0 1 3.65 12.5Z',
      },
      { path: 'M8 4.1v1.05M8 6.35v1.05M8 8.6v1.05M8 10.85v1.05' },
    ],
    // The page is drawn open on its right side so the lens sits *over* it
    // rather than beside it, which is how the composed SF variants read.
    'doc-text-magnifyingglass': [
      { path: 'M11.35 7.05V5.4L8.4 2.35H4.5A1.1 1.1 0 0 0 3.4 3.45v9.1a1.1 1.1 0 0 0 1.1 1.1h3.2' },
      { path: 'M8.3 2.35v2.1a1 1 0 0 0 1 1h2.05' },
      { path: 'M5.7 7.7h2.9' },
      { path: ring(11.25, 10.55, 2.35) },
      { path: 'M12.95 12.25 14.5 13.8' },
    ],
    'arrow-turn-down-right': [
      { path: 'M3.6 2.9v5.35a2.1 2.1 0 0 0 2.1 2.1H12.6M10.4 8.15 12.6 10.35l-2.2 2.2' },
    ],
    'rectangle-grid-1x2': [{ path: PANEL }, { path: 'M2.35 8h11.3' }],
    'rectangle-split-2x1': [{ path: PANEL }, { path: 'M8 3.1v9.8' }],

    // ── Terminal and app identity ──
    terminal: [
      {
        path:
          'M2.2 4.6a1.6 1.6 0 0 1 1.6-1.6h8.4a1.6 1.6 0 0 1 1.6 1.6v6.8a1.6 1.6 0 0 ' +
          '1-1.6 1.6H3.8a1.6 1.6 0 0 1-1.6-1.6Z',
      },
      { path: 'M4.95 6.3 7.05 8.35 4.95 10.4' },
      { path: 'M8.6 10.55h2.75' },
    ],
    // Three connected points — the same "graph of commits" idea the native
    // welcome screen leans on, drawn as filled nodes with a dotted path.
    'app-mark': [
      dot(8, 3.35, 1.45),
      dot(3.4, 11.95, 1.45),
      dot(12.6, 11.95, 1.45),
      { path: 'M7.3 4.65 4.1 10.65M8.7 4.65l3.2 6M4.85 11.95h6.3', dash: '1.1 1.5' },
    ],
  }
</script>

<script lang="ts">
  import type { ClassValue } from 'svelte/elements'

  interface Props {
    /** Which glyph to draw. */
    name: IconName
    /**
     * Rendered edge length in px. The default is 12 because that is what the
     * majority of this app's existing call sites had settled on by hand — it
     * is the size the header chips, the sync ladder and the terminal controls
     * all use, so it is the one that needs no argument.
     */
    size?: number
    /**
     * How heavy the strokes are, matching the weight of the text the icon sits
     * beside. Leave it alone next to body text; step up beside bold or very
     * small text, where a `regular` stroke goes faint.
     */
    weight?: IconWeight
    /**
     * An accessible name. Supply it only when the icon is the *only* carrier
     * of its meaning; leave it off (the default) and the icon is marked
     * decorative, which is right whenever a visible label or an `aria-label`
     * on the enclosing button already says the same thing. Naming it twice is
     * worse than not naming it, so the default is silence.
     */
    title?: string
    /** Turns the glyph continuously — for `arrow-clockwise` and
        `arrow-2-circlepath` while a transfer is in flight. */
    spin?: boolean
    /** Passthrough for layout classes. Note the scoping trap in the header
        comment: a parent's scoped rule needs `:global(…)` to land here. */
    class?: ClassValue
  }

  let {
    name,
    size = 12,
    weight = 'regular',
    title,
    spin = false,
    class: klass,
  }: Props = $props()

  const shapes = $derived(ICONS[name])
</script>

<svg
  class={klass}
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width={STROKE[weight]}
  stroke-linecap="round"
  stroke-linejoin="round"
  shape-rendering="geometricPrecision"
  data-spin={spin ? 'true' : undefined}
  role={title ? 'img' : undefined}
  aria-label={title}
  aria-hidden={title ? undefined : 'true'}
>
  {#each shapes as shape, i (i)}
    {#if 'text' in shape}
      <text x="8" y={shape.y} font-size={shape.size} text-anchor="middle">{shape.text}</text>
    {:else}
      <path
        d={shape.path}
        fill={shape.fill ? 'currentColor' : 'none'}
        fill-rule={shape.fill ? 'evenodd' : undefined}
        stroke={shape.fill ? 'none' : 'currentColor'}
        stroke-dasharray={shape.dash}
      />
    {/if}
  {/each}
</svg>

<style>
  /* `display: block` rather than the inline default: an inline svg sits on the
     text baseline and drags the line box down by its descender, which is what
     the scattered `line-height: 0` workarounds around the app were fighting.
     `flex-shrink: 0` because every call site is inside a flex row and an icon
     that squashes is worse than one that overflows. */
  svg {
    display: block;
    flex-shrink: 0;
  }

  /* Letterform glyphs are set in the app's own UI font — see the note on
     `textformat-abc` above. */
  text {
    font-family: var(--font-ui);
    font-weight: 500;
    fill: currentColor;
    stroke: none;
  }

  /* Targeted by attribute rather than by class on purpose: a keyframe defined
     in a parent component can never reach this element (Svelte scopes styles
     to the component that declares them), so the spinner has to live here. */
  svg[data-spin='true'] {
    animation: spin 0.9s linear infinite;
  }

  /* Not stopped outright under reduced motion, because this spinner is load
     bearing — it is the only thing on screen saying a transfer is still
     running. Slowed instead, which is the accepted compromise for an
     indicator whose motion carries the meaning. */
  @media (prefers-reduced-motion: reduce) {
    svg[data-spin='true'] {
      animation-duration: 2.4s;
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
