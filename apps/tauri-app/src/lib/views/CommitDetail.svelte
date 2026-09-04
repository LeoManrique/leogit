<script lang="ts">
  import type { CommitInfo, CommitStats } from '$lib/api/commands'
  import Icon from '$lib/components/Icon.svelte'

  interface Props {
    commit: CommitInfo | null
    fileCount?: number
    stats?: CommitStats | null
  }

  let { commit = null, fileCount = 0, stats = null }: Props = $props()

  let copied = $state(false)

  // The card's absolute date, in the same abbreviated shape the native card
  // uses. A bare `toLocaleString()` spells the month out in full and carries
  // seconds, which is precision this line never wanted: it exists so the
  // reader can place the commit on a calendar, and the seconds a commit landed
  // on are noise beside the relative age the list already gives.
  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  }

  async function copySha() {
    if (!commit) return
    try {
      await navigator.clipboard.writeText(commit.sha)
      copied = true
      setTimeout(() => (copied = false), 1200)
    } catch {}
  }
</script>

{#if commit}
  <div class="commit-card">
    <div class="title-row">
      <h2 class="title">{commit.summary}</h2>
      {#if stats && (stats.additions > 0 || stats.deletions > 0)}
        <span class="commit-counts">
          {#if stats.additions > 0}<span class="add-count">+{stats.additions}</span>{/if}
          <!-- U+2212, not a hyphen: STYLE.md's minus, and the character the
               native client already renders on the same figure. -->
          {#if stats.deletions > 0}<span class="del-count">−{stats.deletions}</span>{/if}
        </span>
      {/if}
    </div>

    <!--
      The message body exactly as `git log` reports it — trailers included,
      because `%b` already contains them. The card used to render `commit.body`
      and then `commit.trailers` underneath, so every Co-Authored-By and
      Signed-off-by line appeared twice, the second time stripped of the
      paragraph it belongs to.
    -->
    {#if commit.body}
      <pre class="body">{commit.body}</pre>
    {/if}

    <!--
      Identity and date. No separator glyph between the name and the address:
      the native card sets them side by side in an `HStack(spacing: 6)`
      (`HistoryDetailPane.swift:173-177`) and lets the gap do the separating,
      then pushes the date to the trailing edge with a `Spacer` (`:178-180`) —
      which is `margin-left: auto` here. A middot between the two would be the
      one punctuation mark on the card that the reference never draws.
    -->
    <div class="meta-row identity-row">
      <span class="author">{commit.author_name}</span>
      <span class="email">{commit.author_email}</span>
      <span class="date">{formatDate(commit.author_date)}</span>
    </div>

    <div class="meta-row sha-row">
      <code class="sha">{commit.sha}</code>
      <button class="copy-btn" class:copied onclick={copySha} title={copied ? 'Copied' : 'Copy SHA'} aria-label="Copy SHA">
        <!-- Bare checkmark, not the circled one the update chip uses: this is
             the confirmation `HistoryDetailPane.swift:195` shows, where the
             tick replaces `doc.on.doc` inside the same small button. -->
        {#if copied}
          <Icon name="checkmark" weight="semibold" />
        {:else}
          <Icon name="doc-on-doc" />
        {/if}
      </button>
      {#if fileCount > 0}
        <span class="files-count">{fileCount} {fileCount === 1 ? 'file' : 'files'} changed</span>
      {/if}
    </div>

  </div>
{/if}

<style>
  /* 10px vertical / 16px horizontal and an 8px stack gap: the native card's
     own `.padding(.vertical, 10)`, `.padding(.horizontal, 16)` and
     `VStack(spacing: 8)` (`HistoryDetailPane.swift:145,211-212`). */
  .commit-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-inactive);
    background: var(--bg-primary);
  }

  /* `HStack(alignment: .firstTextBaseline, spacing: 8)`
     (`HistoryDetailPane.swift:146`): the +/− totals sit on the summary's
     baseline, not on its top edge, so the two runs read as one line even
     though the mono digits carry different metrics from the UI face. */
  .title-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  /* Detail-pane heading register: the commit summary is the counterpart of the
     native pane's `.headline`, which macOS draws at 13pt. Semibold, not bold —
     the status plate is the app's only bold. */
  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    min-width: 0;
  }

  /* Commit-level +adds/−dels, pinned to the trailing edge at the title's
     baseline. 12px semibold mono is the native run's own font
     (`HistoryDetailPane.swift:164`) and the 6px gap its `HStack(spacing: 6)`
     (`:154`); `margin-left: auto` stands in for the `Spacer(minLength: 8)`
     (`:151`), which the row's own 8px gap already satisfies. No `line-height`:
     this is a single-line label, and STYLE.md's leading rule leaves those to
     the app's `normal`, so the mono digits keep the same box height as the UI
     text beside them instead of standing a point taller than it.

     `align-items: baseline` inside is load-bearing rather than cosmetic: a flex
     container only exposes a baseline when one of its own items is
     baseline-aligned, so under `center` this box would hand `.title-row` a
     baseline synthesised from its border edge and the run would sit low
     against the summary. */
  .commit-counts {
    margin-left: auto;
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .add-count { color: var(--diff-add-fg); }
  .del-count { color: var(--diff-remove-fg); }

  /* 12px mono on a recessed 6px plate, capped at 140px and scrolling past it —
     the native block's `.font(.system(size: 12, design: .monospaced))`, uniform
     `.padding(8)`, `cornerRadius: 6` and `min(bodyHeight, maxBodyHeight)` cap
     (`HistoryDetailPane.swift:142,221-232`). The padding is 8 on all four
     sides there, so it is 8 on all four sides here. */
  .body {
    margin: 0;
    padding: 8px;
    background: var(--bg-secondary);
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-wrap: break-word;
    max-height: 140px;
    overflow-y: auto;
  }

  /* Both metadata rows are one-line runs: the native card puts `.lineLimit(1)`
     on the identity row (`HistoryDetailPane.swift:184`) and on the sha
     (`:191`), so nothing here wraps to a second line. Each run clips itself
     rather than the row clipping them, which keeps the copy button's focus
     halo — an outside `box-shadow` — out of an `overflow: hidden` box. Their
     gaps differ, so each row carries its own: 6 for the identity row's
     `HStack(spacing: 6)` (`:173`), 8 for the sha row's (`:186`). */
  .meta-row {
    display: flex;
    align-items: center;
    min-width: 0;
    color: var(--text-secondary);
  }

  /* `.font(.caption)` on the whole row (`HistoryDetailPane.swift:182`), which
     macOS draws at 10pt regular — the same register as the sidebar row's
     second line, since both are the quiet line under a subject. */
  .identity-row {
    gap: 6px;
    font-size: 10px;
  }

  .sha-row {
    gap: 8px;
  }

  /* The only run on either row that is not `.secondary`: the native name takes
     `.fontWeight(.medium)` at the default label colour
     (`HistoryDetailPane.swift:174-175`), so the person is the one thing here
     that reads at full strength. */
  .author {
    color: var(--text-primary);
    font-weight: 500;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .email {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* `margin-left: auto` is the native `Spacer(minLength: 8)` at
     `HistoryDetailPane.swift:178`: the date is trailing-aligned, not merely
     spaced, so it holds the same edge whatever the name and address measure. */
  .date {
    flex: 0 0 auto;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    margin-left: auto;
  }

  /* 11px mono `.secondary` on one line (`HistoryDetailPane.swift:187-191`).
     Native middle-truncates it (`.truncationMode(.middle)`, `:192`), which CSS
     has no equivalent for, so a narrow pane loses the tail rather than the
     middle — the full sha is 40 mono characters and clears any realistic pane
     width, so the two agree in practice. */
  .sha {
    font-family: var(--font-mono);
    font-size: 11px;
    background: transparent;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .copy-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    /* The row no longer wraps, so every item on it is a shrinkable flex item
       and the deficit a long sha creates is shared out by basis. Without this
       the plate collapses toward the width of the glyph inside it — a 12×20
       hit target on a wide pane, which is exactly where there is *most* room
       for it. */
    flex-shrink: 0;
    padding: 0;
    background: transparent;
    /* `.secondary`, the tint the native glyph carries until it flips to green
       (`HistoryDetailPane.swift:196`). */
    color: var(--text-secondary);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: color 120ms ease, background 120ms ease;
  }

  .copy-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .copy-btn.copied {
    color: var(--status-green);
  }

  /* `.font(.caption)` again — 10pt — behind the sha row's own
     `Spacer(minLength: 8)` and `.fixedSize()`
     (`HistoryDetailPane.swift:201,204-207`), so the count holds the trailing
     edge at its full width and the sha is what gives way. */
  .files-count {
    margin-left: auto;
    flex: 0 0 auto;
    white-space: nowrap;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

</style>
