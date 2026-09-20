<script lang="ts">
  /*
    The one-line strip under the header, above the content — the app's only
    banner shape (`STYLE.md`, *Status indicators*). It states something about
    the repository without taking the window, so the last good view stays
    readable behind it.

    Three conditions use it, each a line of its own so none can silence
    another: the poll's "this repository stopped being readable" (no ✕ — its
    recovery retires it), a dismissable notice, and the way back from the last
    History action. Call sites pass content and a tone; they do not pass style.
  */
  import Icon from './Icon.svelte'

  interface Props {
    /**
     * `warning` is something that went wrong or needs reading: the yellow
     * triangle on a wash of the same hue. `done` is the app reporting its own
     * finished work: a quiet checkmark and no wash, because nothing is wrong.
     */
    tone: 'warning' | 'done'
    /** The sentence. */
    message: string
    /** Git's own text after the sentence — data, so mono, muted and selectable. */
    detail?: string
    /** The one thing the strip offers to do, as a text link after the sentence. */
    action?: {
      label: string
      run: () => void
      /** Why it cannot run right now: the link goes inert and says so on hover. */
      blocked?: string | null
    }
    /** Retire the strip by hand. Omitted where only recovery may retire it. */
    onDismiss?: () => void
  }

  let { tone, message, detail, action, onDismiss }: Props = $props()
</script>

<div class="status-strip {tone}" role="status">
  <!-- Filled, not outlined: every warning banner on the native side uses
       `exclamationmark.triangle.fill` (`StatusStrip.swift`), and the outlined
       variant is reserved there for the full-pane `ContentUnavailableView`
       states. -->
  <Icon name={tone === 'warning' ? 'exclamationmark-triangle-fill' : 'checkmark-circle'} size={13} />
  <!-- With a detail the sentence keeps its width and git's text takes the
       ellipsis; alone, the sentence is what shrinks. The action follows the
       sentence as the next words of it, and the ✕ keeps the far edge. -->
  <span class="strip-message" class:fixed={detail !== undefined}>{message}</span>
  {#if detail !== undefined}
    <span class="strip-detail">{detail}</span>
  {/if}
  {#if action}
    <!-- `aria-disabled`, not `disabled`, as the context menu's items are: a
         disabled button swallows the hover that shows why it is off. -->
    <button
      type="button"
      class="strip-action"
      onclick={() => {
        if (!action.blocked) action.run()
      }}
      aria-disabled={!!action.blocked}
      title={action.blocked ?? undefined}
    >
      {action.label}
    </button>
  {/if}
  {#if onDismiss}
    <button class="strip-dismiss" onclick={onDismiss} aria-label="Dismiss">
      <Icon name="xmark" size={10} weight="semibold" />
    </button>
  {/if}
</div>

<style>
  /* One line under the header, above the content, so the data behind it stays
     visible. Deliberately not a toast and not a modal — what it says persists,
     so it must be able to sit there. */
  .status-strip {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-inactive);
    color: var(--text-primary);
    font-size: 12px;
  }

  .status-strip.warning {
    background: color-mix(in srgb, var(--status-yellow) 12%, transparent);
  }

  /* `:global` because the glyph is a child component's element, and a
     direct-child combinator so it tints only the strip's own mark: the
     descendant form also catches the dismiss button's ✕ and beats
     `.strip-dismiss`'s colour on specificity. `flex-shrink` lives in `Icon`. */
  .status-strip > :global(svg) {
    align-self: center;
  }

  .status-strip.warning > :global(svg) {
    color: var(--status-yellow);
  }

  .status-strip.done > :global(svg) {
    color: var(--text-secondary);
  }

  .strip-message {
    flex: 0 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    user-select: text;
  }

  .strip-message.fixed {
    flex: 0 0 auto;
    overflow: visible;
  }

  .strip-detail {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    user-select: text;
  }

  /* A text link, as the composer's *Stop Amending* is: the strip is one line
     of prose and a bordered button would make it a toolbar. */
  .strip-action {
    flex-shrink: 0;
    padding: 0;
    background: transparent;
    border: none;
    color: var(--border-active);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .strip-action:hover:not([aria-disabled='true']) {
    text-decoration: underline;
  }

  .strip-action[aria-disabled='true'] {
    color: var(--text-faint);
    cursor: not-allowed;
  }

  .strip-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    align-self: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    margin-left: auto;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
  }

  .strip-dismiss:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }
</style>
