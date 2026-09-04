<script lang="ts">
  /**
   * Settings — instant-apply, like the native window and like macOS settings
   * generally.
   *
   * Every control patches its own field as it changes: discrete controls
   * (switches, pickers) on the click, text and numeric fields when they lose
   * focus or take a Return, which is what an `<input>`'s `change` event already
   * means. There is no Save button, so there is nothing to forget to press and
   * nothing to cancel — the config on disk is what the form shows.
   *
   * Two things follow from that and are load-bearing:
   * - **A patch names only the field that changed.** The whole-object write
   *   this replaced posted the config as it looked when the dialog *opened*,
   *   silently reverting whatever the other client had saved meanwhile. Core
   *   clamps and normalizes, and hands the result back, so an out-of-range
   *   entry corrects itself in front of the user instead of being dropped.
   * - **A write that fails puts its control back.** With no Save button
   *   pending, a control still showing the rejected value would be claiming a
   *   setting that isn't on disk.
   *
   * Scan paths are the one field outside instant-apply, behind an Edit ▸ Done
   * cycle: they decide which repositories exist as far as the app is concerned,
   * and half a typed line is a different setting rather than a smaller one.
   *
   * The shape of the window is the native `Form { … }.formStyle(.grouped)` in
   * `Screens/SettingsView.swift`: a section label, then a rounded card holding
   * that section's rows hairline-separated, then the section's explanatory
   * sentence as a footnote *below* the card. Every row is one shape — label at
   * the leading edge, control at the trailing edge — because that is the one
   * arrangement all five kinds of row here can share: a `Toggle`'s label is a
   * whole sentence ("Automatically fetch from remotes") and cannot live in the
   * narrow right-aligned column a two-word label like "Tab size" would want.
   */
  import {
    configApi,
    terminalApi,
    type ConfigBounds,
    type ConfigPatch,
    type ShellOption,
  } from '$lib/api/commands'
  import { config as configStore, patchConfig, refreshConfig } from '$lib/stores/config'
  import { rediscoverRepos } from '$lib/services/repoDiscovery'
  import { dismissOnEscape } from '$lib/actions/overlayStack'
  import Icon from '$lib/components/Icon.svelte'

  interface Props {
    isOpen: boolean
    onClose: () => void
  }

  let { isOpen, onClose }: Props = $props()

  /**
   * The one config in the client. Reading the shared store rather than keeping
   * a second copy is what stops the form and the rest of the app from
   * disagreeing — and it means a change made in the native client, which
   * arrives on the next window activation, shows up here rather than being
   * overwritten by a stale form.
   */
  const config = $derived($configStore)

  let error = $state('')
  /** Shells probed on this machine, best-first. Empty until loaded. */
  let shells = $state<ShellOption[]>([])

  /**
   * Bounds for the numeric fields, read from core — the same declaration the
   * writer clamps against, rather than a third copy of these numbers in a
   * third unit. The `min`/`max` attributes on `<input type=number>` are
   * advisory only: typing 999 or clearing the field both pass straight
   * through. Core clamps on write and hands back the corrected config, which
   * the form re-renders from, so the correction is visible rather than silent.
   * They are not decoration even so — the stepper's ▲▼ are the DOM's own
   * spinner arithmetic, which reads exactly these attributes, so the arrows
   * stop where core would have clamped instead of walking past it.
   */
  let bounds = $state<ConfigBounds | null>(null)

  /** What "Automatic" resolves to, so the choice isn't a mystery. */
  const autoShellLabel = $derived(shells[0]?.label ?? '')

  /** A preference whose shell is no longer installed shows as Automatic,
   *  matching what the backend would actually launch. */
  const shellChoice = $derived(
    shells.some((s) => s.id === config?.terminal_shell) ? (config?.terminal_shell ?? '') : '',
  )

  /**
   * Seconds on screen, milliseconds on the wire — the native window's split,
   * so neither client makes the user count zeroes in `1800000`. Floored at one
   * second so a hand-edited sub-second value still renders as a number.
   */
  const fetchIntervalSeconds = $derived(Math.max(Math.floor((config?.fetch_interval_ms ?? 0) / 1000), 1))
  const intervalBounds = $derived(
    bounds
      ? {
          min: Math.floor(bounds.fetch_interval_ms.min / 1000),
          max: Math.floor(bounds.fetch_interval_ms.max / 1000),
        }
      : null,
  )

  /** Scan paths, mid-edit. Nothing is written until Done, so leaving the
   *  dialog — by any route — discards the draft. */
  let pathsEditing = $state(false)
  let pathsDraft = $state('')

  /** One option of a picker row, in the `<option>` order it renders in. */
  interface PickerOption {
    value: string
    label: string
  }

  /**
   * The four row shapes below are the whole form. They exist because the
   * markup for a row — label, control column, the ids that tie them together —
   * repeats eleven times across six sections, and eleven copies of it is
   * eleven places for the register to drift.
   */
  interface SwitchRowProps {
    id: string
    label: string
    checked: boolean
    onchange: (checked: boolean) => void
  }

  interface StepperRowProps {
    id: string
    label: string
    value: number
    /** Advisory bounds; absent until `configBounds()` answers. */
    min?: number
    max?: number
    /** The native `Stepper`'s own increment for this field. */
    step?: number
    /** Trails the value the way the native's `"\(seconds) s"` does. */
    unit?: string
    disabled?: boolean
    /** What the field snaps back to when it is emptied. */
    fallback: number
    apply: (value: number) => void
  }

  interface SelectRowProps {
    id: string
    label: string
    value: string
    options: PickerOption[]
    onchange: (value: string) => void
  }

  interface TextRowProps {
    id: string
    label: string
    value: string
    placeholder: string
    onchange: (value: string) => void
  }

  /** The shell picker's options: Automatic first, then what was found. */
  const shellOptions: PickerOption[] = $derived([
    { value: '', label: `Automatic${autoShellLabel ? ` (${autoShellLabel})` : ''}` },
    ...shells.map((shell) => ({ value: shell.id, label: shell.label })),
  ])

  async function loadSettings() {
    pathsEditing = false
    error = ''
    try {
      const [cfg, limits, available] = await Promise.all([
        // Re-read on open rather than trusting the store: the other client may
        // have written the file since this window last looked at it.
        refreshConfig(),
        configApi.configBounds(),
        // Non-fatal: an empty list just leaves the picker with "Automatic".
        terminalApi.listShells().catch((e) => {
          console.warn('[settings] shell discovery failed', e)
          return [] as ShellOption[]
        }),
      ])
      bounds = limits
      shells = available
      if (!cfg) error = 'Could not read the configuration file.'
    } catch (e) {
      error = String(e)
    }
  }

  /**
   * Bumped to rebuild every control from the config on disk.
   *
   * The controls render from `config`, so they repaint whenever it changes —
   * which covers most of what core's clamp does. The two cases it doesn't are
   * both cases where the *DOM* holds a value the config never took: a rejected
   * write, and an entry core clamped back to the value it already had (999
   * typed into a field already at its maximum). Neither changes `config`, so
   * nothing would repaint without this.
   */
  let formSeq = $state(0)

  /**
   * This form's writer: the shared chained one, plus what only a form needs —
   * an error line, a rebuild of the control that asked, and the discovery walk
   * a scan-path edit implies.
   */
  function patch(fields: ConfigPatch): void {
    void patchConfig(fields).then(
      ({ before, updated }) => {
        error = ''
        if (before && JSON.stringify(before) === JSON.stringify(updated)) formSeq += 1
        // The scan paths are what discovery walks and the depth is how far, so
        // a change to either re-walks now: the setting takes effect where it
        // was made, rather than on a later dialog dismissal.
        if (fields.scan_paths !== undefined || fields.scan_depth !== undefined) {
          void rediscoverRepos()
        }
      },
      (e: unknown) => {
        // Nothing landed, so the control must stop claiming it did.
        error = String(e)
        formSeq += 1
      },
    )
  }

  /**
   * Commit a numeric field, or put it back.
   *
   * An emptied or unparseable `<input type=number>` reads as `NaN`, and a patch
   * naming it would reach the backend as `null` and fail with a raw serde
   * error. Snapping the control back to what is on disk says "that was not a
   * number" without a sentence about it.
   *
   * Takes the element rather than the event because both routes into a numeric
   * setting end here — the field's own `change`, and a click on the stepper
   * beside it — and only one of those has an event whose target is the field.
   */
  function commitNumber(
    input: HTMLInputElement,
    fallback: number,
    apply: (value: number) => void,
  ): void {
    const value = input.valueAsNumber
    if (Number.isNaN(value)) {
      input.value = String(fallback)
      return
    }
    apply(Math.round(value))
  }

  /**
   * One click of a stepper arrow.
   *
   * `stepUp`/`stepDown` are the DOM's own spinner arithmetic, so the arrows
   * move by exactly the `step` the field declares and stop at its `min`/`max`
   * — core's bounds, already on the element — rather than this file keeping a
   * second copy of either number. The commit that follows is the field's own,
   * so a stepper click and a typed-then-blurred value reach core by the same
   * path and get the same clamp, the same error handling and the same repaint.
   */
  function nudge(
    button: HTMLElement,
    delta: 1 | -1,
    fallback: number,
    apply: (value: number) => void,
  ): void {
    const input = button.closest('.stepper-field')?.querySelector('input')
    if (!(input instanceof HTMLInputElement)) return
    // An emptied field has no value to step from, and `stepUp` would read it
    // as zero. Put the saved value back first so ▲ from blank means "one above
    // what is on disk" rather than "one above nothing".
    if (Number.isNaN(input.valueAsNumber)) input.value = String(fallback)
    if (delta > 0) input.stepUp()
    else input.stepDown()
    commitNumber(input, fallback, apply)
  }

  function togglePathsEdit(): void {
    if (!config) return
    if (!pathsEditing) {
      pathsDraft = config.scan_paths.join('\n')
      pathsEditing = true
      return
    }
    pathsEditing = false
    patch({ scan_paths: pathsDraft.split('\n').map((s) => s.trim()).filter(Boolean) })
  }

  $effect(() => {
    if (isOpen) loadSettings()
  })
</script>

<!-- The row shapes. Each is one `.row`: the `<label>` names the control by id,
     which is what makes the label text a click target and the accessible name,
     and the control sits in a trailing column of its own. -->

{#snippet switchRow({ id, label, checked, onchange }: SwitchRowProps)}
  <div class="row">
    <label class="row-label" for={id}>{label}</label>
    <span class="row-control">
      <!-- A real checkbox, drawn as a switch by the sibling beside it rather
           than by restyling the control's own box. `app.css` sets out at
           length why that box is unreachable: neither engine devolves a
           checkbox on an author `background`/`border`/`border-radius` — both
           fall through to `default: return false` in `isControlStyled` — so
           every declaration aimed at it is discarded rather than honoured.
           What survives is the input's *geometry*, so it is stretched over the
           track at zero opacity: the hit area, the keyboard, the checked state
           and the accessibility tree all stay the platform's, and only the
           paint is ours. -->
      <span class="switch">
        <input
          {id}
          type="checkbox"
          {checked}
          onchange={(e) => onchange(e.currentTarget.checked)}
        />
        <span class="switch-track"></span>
      </span>
    </span>
  </div>
{/snippet}

{#snippet stepperRow({
  id,
  label,
  value,
  min,
  max,
  step,
  unit,
  disabled,
  fallback,
  apply,
}: StepperRowProps)}
  <div class="row">
    <label class="row-label" for={id}>{label}</label>
    <span class="row-control">
      <span class="stepper-field">
        <input
          {id}
          type="number"
          {value}
          {min}
          {max}
          {step}
          {disabled}
          onchange={(e) => commitNumber(e.currentTarget, fallback, apply)}
        />
        {#if unit}<span class="unit">{unit}</span>{/if}
        <!-- Hidden from assistive technology and skipped by Tab on purpose:
             the field beside them already publishes its value, bounds and step
             and already moves on ↑/↓, so these are a second *pointer*
             affordance for one control, not a second control. They stay
             clickable — `tabindex="-1"` is focusable but not tabbable, which
             is what `aria-hidden` permits.

             `aria-hidden` is repeated on each button rather than left to the
             wrapper alone. It has to be: the compiler's "a button should have
             a label" check reads the element itself and never its ancestors,
             so the wrapper silences the tree without silencing the warning,
             and labelling them instead would put two controls in the AT tree
             where the native `Stepper` is one. -->
        <span class="stepper" aria-hidden="true">
          <button
            type="button"
            class="stepper-btn stepper-btn--up"
            tabindex="-1"
            aria-hidden="true"
            {disabled}
            onclick={(e) => nudge(e.currentTarget, 1, fallback, apply)}
          ></button>
          <button
            type="button"
            class="stepper-btn stepper-btn--down"
            tabindex="-1"
            aria-hidden="true"
            {disabled}
            onclick={(e) => nudge(e.currentTarget, -1, fallback, apply)}
          ></button>
        </span>
      </span>
    </span>
  </div>
{/snippet}

{#snippet selectRow({ id, label, value, options, onchange }: SelectRowProps)}
  <div class="row">
    <label class="row-label" for={id}>{label}</label>
    <span class="row-control row-control--field">
      <span class="select-field">
        <select {id} {value} onchange={(e) => onchange(e.currentTarget.value)}>
          {#each options as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </span>
    </span>
  </div>
{/snippet}

{#snippet textRow({ id, label, value, placeholder, onchange }: TextRowProps)}
  <div class="row">
    <label class="row-label" for={id}>{label}</label>
    <span class="row-control row-control--field">
      <input {id} type="text" {value} {placeholder} onchange={(e) => onchange(e.currentTarget.value)} />
    </span>
  </div>
{/snippet}

{#if isOpen}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose()
    }}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1" use:dismissOnEscape={onClose}>
      <div class="modal-header">
        <h2>Settings</h2>
        <button class="close-btn" onclick={onClose} aria-label="Close">
          <Icon name="xmark" size={11} weight="semibold" />
        </button>
      </div>

      <div class="modal-body">
        {#if error}
          <!-- Its own card, so a failure sits in the form's own register
               instead of floating above it — the native puts the message in a
               `Section` of its own for the same reason. -->
          <div class="card error-card">{error}</div>
        {/if}

        {#key formSeq}
        {#if config}
          <!-- Theme has no native counterpart: macOS follows the system
               appearance and the native client never asks. It is a real
               platform divergence rather than a defect, so it keeps its
               section and wears the same register as the rest. -->
          <section class="section">
            <h3>Appearance</h3>
            <div class="card">
              {@render selectRow({
                id: 'theme-select',
                label: 'Theme',
                value: config.theme,
                options: [
                  { value: 'dark', label: 'Dark' },
                  { value: 'light', label: 'Light' },
                ],
                onchange: (theme) => patch({ theme }),
              })}
            </div>
          </section>

          <section class="section">
            <h3>Git</h3>
            <div class="card">
              {@render switchRow({
                id: 'auto-fetch',
                label: 'Automatically fetch from remotes',
                checked: config.auto_fetch,
                onchange: (auto_fetch) => patch({ auto_fetch }),
              })}
              {@render stepperRow({
                id: 'fetch-interval',
                label: 'Fetch interval',
                value: fetchIntervalSeconds,
                min: intervalBounds?.min,
                max: intervalBounds?.max,
                // 5 s a click, the native `Stepper`'s own step for this field.
                step: 5,
                unit: 's',
                disabled: !config.auto_fetch,
                fallback: fetchIntervalSeconds,
                apply: (n) => patch({ fetch_interval_ms: n * 1000 }),
              })}
            </div>
            <p class="section-footer">
              Applies to the open repository within one interval — no restart needed.
            </p>
          </section>

          <section class="section">
            <h3>Diff</h3>
            <div class="card">
              {@render switchRow({
                id: 'hide-whitespace',
                label: 'Hide whitespace changes',
                checked: config.hide_whitespace,
                onchange: (hide_whitespace) => patch({ hide_whitespace }),
              })}
              {@render switchRow({
                id: 'syntax-highlighting',
                label: 'Syntax highlighting',
                checked: config.syntax_highlighting,
                onchange: (syntax_highlighting) => patch({ syntax_highlighting }),
              })}
              {@render stepperRow({
                id: 'tab-size',
                label: 'Tab size',
                value: config.tab_size,
                min: bounds?.tab_size.min,
                max: bounds?.tab_size.max,
                fallback: config.tab_size,
                apply: (n) => patch({ tab_size: n }),
              })}
            </div>
            <p class="section-footer">Applies to the open diff immediately.</p>
          </section>

          <section class="section">
            <h3>Repository Discovery</h3>
            <div class="card">
              <!-- Read-only until Edit, the macOS list-editor pattern: this is
                   the one setting that decides which repositories the app can
                   see at all, and a half-typed line is a different folder rather
                   than a shorter one. Nothing is written until Done. -->
              <div class="row row--top">
                <label class="row-label" for="scan-paths">Folders to scan</label>
                <span class="row-control row-control--field">
                  <textarea
                    id="scan-paths"
                    class="paths-input"
                    readonly={!pathsEditing}
                    value={pathsEditing ? pathsDraft : config.scan_paths.join('\n')}
                    oninput={(e) => (pathsDraft = e.currentTarget.value)}
                    rows="6"
                  ></textarea>
                </span>
              </div>
              <!-- Its own row, hairline and all, because the native gives it
                   one: a `LabeledContent("")` whose only content is the
                   button. It is an action on the field above, not a control
                   the field has to share its line with. -->
              <div class="row">
                <span class="row-label"></span>
                <span class="row-control">
                  <button class="btn-secondary" onclick={togglePathsEdit}>
                    {pathsEditing ? 'Done' : 'Edit'}
                  </button>
                </span>
              </div>
              {@render stepperRow({
                id: 'scan-depth',
                label: 'Scan depth',
                value: config.scan_depth,
                min: bounds?.scan_depth.min,
                max: bounds?.scan_depth.max,
                fallback: config.scan_depth,
                apply: (n) => patch({ scan_depth: n }),
              })}
            </div>
            <p class="section-footer">
              One folder per line (~ allowed). The repository switcher searches these for git
              repositories.
            </p>
          </section>

          <section class="section">
            <h3>Terminal</h3>
            <div class="card">
              {@render selectRow({
                id: 'terminal-shell',
                label: 'Shell',
                value: shellChoice,
                options: shellOptions,
                onchange: (terminal_shell) => patch({ terminal_shell }),
              })}
            </div>
            <p class="section-footer">
              Only shells found on this machine are listed. Applies to new terminal sessions.
            </p>
          </section>

          <section class="section">
            <h3>AI Commit Messages</h3>
            <div class="card">
              {@render selectRow({
                id: 'provider-select',
                label: 'Provider',
                value: config.ai_provider,
                options: [
                  { value: 'claude', label: 'Claude' },
                  { value: 'ollama', label: 'Ollama' },
                ],
                onchange: (ai_provider) => patch({ ai_provider }),
              })}
              <!-- One model field per provider: a single shared one meant a model
                   set for Claude was handed to Ollama, which has never heard of
                   it, so Generate failed with nothing on screen explaining why. -->
              {#if config.ai_provider === 'ollama'}
                {@render textRow({
                  id: 'ollama-model',
                  label: 'Model',
                  value: config.ollama.model ?? '',
                  placeholder: 'tavernari/git-commit-message:latest',
                  onchange: (ollama_model) => patch({ ollama_model }),
                })}
                {@render textRow({
                  id: 'ollama-url',
                  label: 'Ollama server URL',
                  value: config.ollama.server_url,
                  placeholder: 'http://localhost:11434',
                  onchange: (ollama_server_url) => patch({ ollama_server_url }),
                })}
                {@render stepperRow({
                  id: 'ollama-timeout',
                  label: 'Timeout',
                  value: config.ollama.timeout_secs,
                  min: bounds?.ai_timeout_secs.min,
                  max: bounds?.ai_timeout_secs.max,
                  // 10 s a click, the native timeout `Stepper`'s own step.
                  step: 10,
                  unit: 's',
                  fallback: config.ollama.timeout_secs,
                  apply: (n) => patch({ ollama_timeout_secs: n }),
                })}
              {:else}
                {@render textRow({
                  id: 'claude-model',
                  label: 'Model',
                  value: config.claude.model ?? '',
                  placeholder: 'sonnet',
                  onchange: (claude_model) => patch({ claude_model }),
                })}
                {@render stepperRow({
                  id: 'claude-timeout',
                  label: 'Timeout',
                  value: config.claude.timeout_secs,
                  min: bounds?.ai_timeout_secs.min,
                  max: bounds?.ai_timeout_secs.max,
                  step: 10,
                  unit: 's',
                  fallback: config.claude.timeout_secs,
                  apply: (n) => patch({ claude_timeout_secs: n }),
                })}
              {/if}
            </div>
            <p class="section-footer">
              Used by Generate in the commit composer. Each provider keeps its own model; leave it
              empty for that provider's default.
            </p>
          </section>
        {/if}
        {/key}
      </div>

      <div class="modal-footer">
        <button class="btn-secondary" onclick={onClose}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  /* 480px and 540px are the native window's own frame — `.frame(width: 480)`
     and `.frame(minHeight: 540)` on the `Form` in `SettingsView.swift`. The
     native takes the height as a *minimum* and grows with its content; a modal
     has a viewport to stay inside instead, so the floor is capped at the same
     85vh the ceiling is and the body scrolls past it. */
  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 10px;
    width: 90%;
    max-width: 480px;
    min-height: min(540px, 85vh);
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-popover);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-inactive);
  }

  .close-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--surface-hover);
  }

  /* The form's ground is *recessed* relative to its cards, which is the whole
     of what makes a grouped form read as one: macOS paints the window under a
     grouped `Form` in the page background and the sections on top of it in a
     raised control fill. `--bg-elevated` over `--bg-secondary` is the one
     token pair that keeps that relationship in both themes — the card lands
     lighter than its ground in dark (#3a3a3c on #252525) and in light (#ffffff
     on #f5f5f7). Painting the card `--bg-secondary` on the modal's own
     `--bg-elevated` would invert it and sink the sections into the sheet. */
  .modal-body {
    flex: 1;
    padding: 14px 16px 16px;
    overflow-y: auto;
    background: var(--bg-secondary);
  }

  .section + .section {
    margin-top: 18px;
  }

  /* 13px semibold is the Settings section label (STYLE.md, *Section headers*):
     the native lets a grouped `Form` size its own headers, and this client has
     no `Form` to inherit one from — Apple publishes the arrangement but not
     the type. Above the card and outside it, which Apple does publish:
     "sections will visually group their content below their headers" (WWDC22,
     *What's new in SwiftUI*). Sentence case, not upper: the uppercased grouped
     header is an iOS list convention and macOS does not do it. */
  .modal-body h3 {
    margin: 0 0 6px 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  /* One section's rows, in the rounded fill a `Section` gets from a grouped
     `Form`. 6px is the app's chrome radius (STYLE.md, *Spacing, radii,
     focus*); the native's is the system's own and appears in no source file.
     `overflow: hidden` is what makes the first and last rows take the corners
     rather than square them off. */
  .card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-inactive);
    border-radius: 6px;
    overflow: hidden;
  }

  /* Between rows and after none of them: a hairline belongs to the join, not
     to the row, so the last row keeps the card's own bottom edge. */
  .row + .row {
    border-top: 1px solid var(--border-inactive);
  }

  .error-card {
    padding: 8px 10px;
    margin-bottom: 18px;
    color: var(--status-red);
    font-size: 12px;
  }

  /* Every row is this shape: the label at the leading edge, the control at the
     trailing one, and whatever space is left in between. That is the grouped
     style's own rule, not a choice — "rows in a grouped rows form have leading
     aligned labels and trailing aligned controls within visually grouped
     sections" (`FormStyle.grouped`). The *other* macOS style is the one that
     right-aligns a column of labels (`FormStyle.columns`, "a trailing aligned
     column of labels next to a leading aligned column of values"), and it is
     the one most write-ups describe — porting its alignment here would be
     copying the wrong form. It is also the only arrangement that fits this
     form: a `Toggle`'s label is a whole sentence, and a label column wide
     enough for "Automatically fetch from remotes" would strand "Tab size" a
     third of the way across the card. */
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    /* The floor, not the height. A switch row is a line of text tall and a
       picker row is its control tall, so the rows in one card are not all the
       same height — which is what the native does too, rather than padding
       every row out to the tallest control in the form. */
    padding: 4px 10px;
    min-height: 32px;
  }

  /* The one row whose control is taller than a line. Its label sits on the
     field's first line rather than halfway down a six-row box. */
  .row--top {
    align-items: flex-start;
  }

  .row--top .row-label {
    padding-top: 5px;
  }

  /* 13px is the app's body size for settings copy (STYLE.md, *Typography*),
     and the row label is at full strength because it is the control's own
     label, the way a `LabeledContent` or `Toggle` label is natively. */
  .row-label {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 13px;
    color: var(--text-primary);
  }

  .row-control {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
  }

  /* Text-shaped controls — the pickers, the text fields, the path list — take
     a column of their own instead of shrinking onto their content, so the four
     of them line up down the form. It yields to the label when the modal is
     narrower than its 480px maximum. */
  .row-control--field {
    flex: 0 1 280px;
  }

  .row-control--field > .select-field,
  .row-control--field > input,
  .row-control--field > textarea {
    flex: 1;
    min-width: 0;
  }

  /* What a whole section applies to, and when — a footnote *below* the card,
     leading-aligned with the section label above it, which is where the native
     puts it (`Section`'s `footer:`, and `settingsFooter()`'s
     `alignment: .leading`). Per *section*, not per control: the scope of
     "immediately" is the group, not any one switch in it.

     10px is the native's own: `settingsFooter()` sets `.font(.caption)`, and
     macOS resolves Caption 1 to 10 pt / 13 pt leading — not the 12 pt the same
     Apple table gives for iOS, which is the number that gets ported by
     mistake. The 1.4 leading is the wrapped-paragraph exception to
     `line-height: normal`; these sentences run to three lines at this width. */
  .section-footer {
    margin: 6px 0 0 0;
    font-size: 10px;
    line-height: 1.4;
    color: var(--text-muted);
  }

  /* The switch: a track and a knob drawn beside a checkbox that is still doing
     all the work. See the markup for why the input cannot draw itself.

     It is the *mini* size class, not the full one: a grouped form "adapts"
     its toggles, and what it adapts them into is "trailing mini switches"
     (WWDC22, *What's new in SwiftUI*), which the HIG repeats — "within a
     grouped form, consider using a mini switch to control the setting in a
     single row." 26×15 is that class's footprint. The exact pixels are not a
     number from the native side: a `Toggle`'s switch is AppKit chrome and its
     geometry appears in no source file and in no Apple document, so this is
     sized to sit a hair above the 14px checkbox it replaces and leave a
     boolean row the height it already had.

     Flat accent fill when on, white knob, no gradient (STYLE.md, *Forms*). */
  .switch {
    position: relative;
    display: inline-flex;
    flex: none;
    width: 26px;
    height: 15px;
  }

  /* Stretched over the track at zero opacity: the pointer, the keyboard and
     the accessibility tree all still land on a real checkbox, and the engine's
     own painting of it — which no author style can reach — is simply not
     visible. Zero opacity also swallows the `outline` ring `app.css` puts on a
     focused checkbox, so the ring is re-drawn on the track below instead of
     appearing twice. */
  .switch input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .switch-track {
    width: 100%;
    height: 100%;
    /* Half the height: a stadium, which is the shape rather than a radius
       choice, so it is exempt from the radius scale. */
    border-radius: 7.5px;
    /* Off. A translucent token rather than a solid one so one value reads on
       both themes — it composites to a light pill on the dark card and a grey
       one on the white card, which no flat `--bg-*` does in both directions. */
    background: var(--border-strong);
    /* Under the input, which is what has to receive the click. */
    pointer-events: none;
    transition: background 150ms ease;
  }

  .switch-track::after {
    content: '';
    position: absolute;
    top: 1px;
    left: 1px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    /* White in both themes, which is what the token is for and what macOS
       does — the knob does not follow the appearance. */
    background: var(--on-accent);
    transition: transform 150ms ease;
  }

  .switch input:checked ~ .switch-track {
    background: var(--border-active);
  }

  .switch input:checked ~ .switch-track::after {
    /* 26 − 13 − 1 − 1: the track's width less the knob's and the two 1px
       insets. */
    transform: translateX(11px);
  }

  /* The checkbox focus ring from `app.css`, moved onto the element that is
     actually visible. Same two values, so a switch and a checkbox wear the
     same ring. */
  .switch input:focus-visible ~ .switch-track {
    outline: 2px solid var(--border-active);
    outline-offset: 2px;
  }

  /* A numeric setting, its unit and its arrows are one control as far as the
     user is concerned, so they travel together: the wrapper is what the
     stepper's click handler walks up to in order to find the field it belongs
     to, and what the disabled rule below dims as a unit. */
  .stepper-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* The field register — recessed fill, border, radius, padding — is
     `app.css`'s, taken along with the text fields and the selects so the
     column reads as one rule. All this adds is a width that fits the largest
     value core will accept and the right alignment a stepper pairs with: the
     digits sit against the unit and the arrows rather than drifting away from
     them. Tabular figures so a step doesn't shift the column. */
  .stepper-field input[type='number'] {
    width: 60px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    /* The engine's own spinner would be a second pair of arrows beside the
       one drawn below. */
    appearance: textfield;
  }

  .stepper-field input[type='number']::-webkit-outer-spin-button,
  .stepper-field input[type='number']::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }

  .stepper-field input:disabled,
  .stepper-field:has(input:disabled) .unit {
    opacity: 0.5;
  }

  /* The unit the user thinks in, trailing the value the way the native's
     `LabeledContent("Fetch interval", value: "30 s")` does — in the row, not
     bracketed into the label. */
  .unit {
    font-size: 13px;
    color: var(--text-secondary);
  }

  /* Two half-height buttons in one bezel, the shape of an `NSStepper`. Its
     metrics are AppKit's and appear in no source file, so 15×24 is chosen to
     stand the same height as the field it belongs to. */
  .stepper {
    display: inline-flex;
    flex-direction: column;
    flex: none;
    width: 15px;
    height: 24px;
  }

  .stepper-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    padding: 0;
    /* The pair reads as one control, so only the outer corners round and only
       the outer edge is drawn; the seam between them is a single hairline. */
    border-radius: 0;
    color: var(--text-secondary);
  }

  .stepper-btn--up {
    border-radius: 4px 4px 0 0;
    border-bottom-width: 0;
  }

  .stepper-btn--down {
    border-radius: 0 0 4px 4px;
  }

  .stepper-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Two borders of a box, the way `app.css` draws the select's chevron, rather
     than the ▲ and ▼ characters: those are drawn by whichever font the host
     happens to have for them and come out at a different weight and a
     different size on each of the three platforms this ships to. Filled rather
     than stroked, which is the one place this departs from *Icons*' preference
     for line glyphs in chrome — a 7px stroked arrowhead is mush at 1x. */
  .stepper-btn::before {
    content: '';
    width: 0;
    height: 0;
    border-left: 3.5px solid transparent;
    border-right: 3.5px solid transparent;
  }

  .stepper-btn--up::before {
    border-bottom: 3.5px solid currentColor;
  }

  .stepper-btn--down::before {
    border-top: 3.5px solid currentColor;
  }

  /* Monospaced because these are paths: a stray space or a doubled slash is
     only visible in a fixed-width face. The fill, border and radius are
     `app.css`'s textarea register. */
  .paths-input {
    padding: 6px 8px;
    font-size: 12px;
    line-height: 1.5;
    font-family: var(--font-mono);
    /* Only vertical, and never below four rows plus padding: the handle used
       to collapse the box to a sliver that hid every configured path. The
       `rows` attribute sets the taller starting height. */
    resize: vertical;
    min-height: 88px;
  }

  .paths-input:read-only {
    color: var(--text-secondary);
    background: var(--bg-secondary);
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-inactive);
  }

  .btn-secondary {
    padding: 3px 14px;
    font-size: 12px;
    font-weight: 500;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    cursor: pointer;
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background: var(--surface-hover);
  }
</style>
