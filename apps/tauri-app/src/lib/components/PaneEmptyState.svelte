<script lang="ts">
  /*
    What a pane says when it has nothing to show.

    ── Why this is one component ────────────────────────────────────────────
    The native client draws all thirteen of these states with a single
    unmodified `ContentUnavailableView`, so on that side they cannot drift:
    there is no per-site styling to disagree with. This client had hand-written
    the same pattern thirteen times across two files and arrived at four
    disagreeing definitions of it, with three different title sizes, two
    different body text colours, two different font stacks and no icon
    anywhere. That is the failure mode this exists to end: not duplication for
    its own sake, but thirteen separate chances for a pane to look like it
    belongs to a different app.

    ── Where the numbers come from, and why they live here ──────────────────
    Apple publishes no metrics for `ContentUnavailableView` — none, on any
    platform. There is no table in the HIG, nothing in the SwiftUI reference,
    and no design resource that names a single one of these values. They were
    extracted from the shipping SwiftUI binary (7.5.3, macOS 26.6.2) by
    resolving the compiled constants the view lays itself out with.

    That provenance is the entire reason every number is in this one file. They
    are not documented, so there is nothing to check them against; Apple can
    change any of them in an OS update and nothing will tell us. When that
    happens the fix has to be one edit here, applied to all thirteen states at
    once — which is only true while no call site is allowed its own opinion
    about size, spacing or colour. Call sites pass content. They do not pass
    style.

    Resolved values, all of them from that extraction:
      · centred on both axes, text centre-aligned
      · content capped at 400, 20 of padding all round
      · icon 36, then 22 to the title, then 12 to the description, then 12
        before the actions
      · title 26 bold (`.largeTitle`), description 13 (`.body`)
      · title and description at the secondary text level, the icon one step
        fainter than them

    The 26px title is the value most likely to look like a mistake, because the
    CSS it replaces used 15px. It is not a mistake. In a side-by-side against
    the native pane the heading stands at roughly twice the height of its own
    description line, and 26-against-13 is the only ratio that reproduces that;
    15px produces a heading the eye reads as a slightly emphasised sentence.
  */
  import type { Snippet } from 'svelte'
  import Icon, { type IconName } from './Icon.svelte'

  interface Props {
    /** The glyph standing in for the native's `systemImage:`. Decorative, so
     *  it is drawn `aria-hidden`: the title beside it already carries the
     *  meaning, and naming it twice is worse than not naming it. */
    icon: IconName
    /** The heading — the native's `Label` title. */
    title: string
    /** The plain explanatory sentence under the heading, in the app's own
     *  voice. Optional because a few states are self-evident from the heading
     *  alone. */
    detail?: string
    /**
     * Git's own error text, passed through untouched. Separate from `detail`
     * because it is data rather than prose and gets the treatment data gets —
     * mono, boxed, left-aligned, selectable — matching how the error modal
     * reports the same strings. A state can carry both: the sentence explains,
     * the box quotes.
     */
    verbatim?: string
    /** Buttons under the state, for the pane that offers a way out of it
     *  ("Show Diff Anyway"). Rendered in the caller's scope, so the caller
     *  keeps owning the button's own markup and handlers. */
    actions?: Snippet
  }

  let { icon, title, detail, verbatim, actions }: Props = $props()
</script>

<!--
  `flex: 1` and an explicit `--bg-primary`, both load-bearing (STYLE.md, *Empty
  states*): sized to its own content this block lets the layout centre the
  pane's whole stack, which drags a header meant to sit at the top into the
  middle of the pane. Claiming the full height is what stops that.
-->
<div class="pane-empty-state">
  <div class="content">
    <!-- Tinted from this wrapper rather than from a rule on the icon itself:
         Svelte's scoped CSS cannot reach into a child component's DOM, so a
         `.glyph svg` rule here would never land. `color` inherits across the
         boundary and the svg paints from `currentColor` (Icon.svelte). -->
    <span class="glyph">
      <Icon name={icon} size={36} />
    </span>
    <p class="title">{title}</p>
    {#if detail}
      <p class="detail">{detail}</p>
    {/if}
    {#if verbatim}
      <p class="verbatim">{verbatim}</p>
    {/if}
    {#if actions}
      <div class="actions">{@render actions()}</div>
    {/if}
  </div>
</div>

<style>
  /* `font-family` is re-stated rather than inherited because this component is
     also used inside `DiffViewer`, which sets `--font-mono` across its whole
     subtree. These states are prose about the pane, not diff content, so they
     have to opt back out of it — the same reason the two states this replaces
     inside that file each carried a `font-family` of their own. */
  .pane-empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    background: var(--bg-primary);
    font-family: var(--font-ui);
    text-align: center;
  }

  /* The 400px cap is on the column, not on the individual lines: capping each
     line separately lets a long description sit wider than a long quoted
     error and the block loses its centre line. */
  .content {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 400px;
    min-width: 0;
  }

  .glyph {
    display: block;
    color: var(--text-muted);
  }

  /* Spacing is per-element rather than one `gap`, because the gaps are not
     equal: 22 under the icon, 12 between everything after it. */
  .title {
    margin-top: 22px;
    font-size: 26px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  /* One of the two local leading overrides STYLE.md sanctions: a wrapped
     paragraph that wants air. The app's `line-height: normal` is tuned to make
     a *line of chrome* sit at the native's height, and it is right there; a
     three-line sentence set at it reads as a wall. The title keeps `normal` —
     it is a single line, where a ratio would be neither. */
  .detail {
    margin-top: 12px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .verbatim {
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--bg-secondary);
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    text-align: left;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  /* The state's title register does not reach its actions: a button is a
     control and keeps the 13px body size. Pinned here against the global
     `button { font-size: inherit }`, and on the wrapper rather than on the
     button, which is authored in the caller's scope and out of reach. */
  .actions {
    margin-top: 12px;
    font-size: 13px;
  }
</style>
