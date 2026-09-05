<script lang="ts">
  import { untrack } from 'svelte'
  import { get } from 'svelte/store'
  import { repoState, canCommit } from '$lib/stores/repo'
  import { appState } from '$lib/stores/app'
  import { gitApi, aiApi, configApi, type Config, type FileEntry } from '$lib/api/commands'
  import { config } from '$lib/stores/config'
  import { basename } from '$lib/utils/path'
  import EmbeddedRepoConfirm from './EmbeddedRepoConfirm.svelte'

  interface Props {
    onCommitted?: () => void
    onStopAmending?: () => void
    /**
     * Run a shell command in the app's own terminal. Supplied by the view that
     * owns the terminal panel; the composer knows the command that would fix an
     * unready AI provider but nothing about where to run it.
     */
    onRunInTerminal?: (command: string) => void
  }

  let { onCommitted, onStopAmending, onRunInTerminal }: Props = $props()

  let summary = $state('')
  let description = $state('')
  // The active AI provider is sourced from — and persisted to — the shared
  // config store, so the choice survives restarts and stays in sync with the
  // Settings overlay. Any unrecognized stored value falls back to Claude.
  const provider = $derived<'claude' | 'ollama'>(
    $config?.ai_provider === 'ollama' ? 'ollama' : 'claude',
  )
  let isGenerating = $state(false)
  let isCommitting = $state(false)
  let error = $state<string | null>(null)
  let charCount = $derived(summary.length)

  // Files staged for the commit that's awaiting embedded-repo confirmation.
  // Non-empty only while the EmbeddedRepoConfirm modal is open; cleared once the
  // user confirms or cancels. The modal lists the embedded entries within.
  let pendingFiles = $state<FileEntry[]>([])
  const pendingEmbedded = $derived(pendingFiles.filter((f) => f.embedded))

  // "A commit is under way" for every lockout. The embedded-repo confirmation
  // is a *pause* inside the commit, not a state before it: `isCommitting` is
  // still false while the dialog waits, and its Confirm calls `performCommit`
  // directly — past `canSubmit`. Without folding the pending state in here, the
  // composer stays live behind the dialog and Generate can still be started,
  // landing its result on a composer the confirmed commit has just cleared.
  const isCommitInProgress = $derived(isCommitting || pendingFiles.length > 0)
  // Committing and generating share one busy treatment, as they do natively
  // (CommitComposer.swift:60-62): the fields lock, and one spinner in the
  // button row stands for whichever of the two is running. Purely a display
  // condition — every gate below still asks its own question.
  const isBusy = $derived(isGenerating || isCommitInProgress)
  // Outer repo name (repoPath basename) for the warning copy.
  const outerRepoName = $derived(
    $appState.repoPath ? basename($appState.repoPath) : 'this repository',
  )

  // Co-author trailers preserved from the commit being amended, re-applied via
  // format_commit_message on commit. Plain trailers, e.g. "Name <email>".
  let amendCoAuthors = $state<string[]>([])

  // True while the user is editing the most recent commit instead of creating
  // a new one. Driven by repoState.commitToAmend (set from the History context
  // menu via MainLayout).
  const isAmending = $derived($repoState.commitToAmend !== null)

  // When exactly one file is staged, default the summary to a GitHub-Desktop
  // style "Create/Delete/Update <file>" so the most common commit (one file)
  // needs zero typing. Empty when 0 or many files are staged, so multi-file
  // commits still require a real summary.
  const autoSummary = $derived.by(() => {
    const sel = $repoState.selectedFiles
    if (sel.size !== 1) return ''
    const [path] = sel
    const file = $repoState.status.files.find((f) => f.path === path)
    if (!file) return ''
    const verb =
      file.status === 'New' ? 'Create' : file.status === 'Deleted' ? 'Delete' : 'Update'
    return `${verb} ${file.display_name}`
  })

  // What actually gets committed: the typed summary if any, otherwise the
  // single-file auto-summary. Also drives the input placeholder so the user
  // sees the message they'll commit before typing.
  const effectiveSummary = $derived(summary.trim() || autoSummary)
  // "(required)" is the native composer's wording (CommitComposer.swift:80).
  // The disabled Commit button already carries the requirement, so the bare
  // noun was defensible — but the two clients shipped different words for one
  // control, and the reference is the one that decides.
  const summaryPlaceholder = $derived(autoSummary || 'Summary (required)')

  // How many files the next commit would contain. Resolved against the live
  // status list rather than read off `selectedFiles.size`, so a path that has
  // left the working tree since the last status read cannot be advertised in
  // the button's label and then quietly dropped by `handleCommit`, which
  // resolves its file list exactly the same way.
  const includedCount = $derived(
    $repoState.status.files.filter((f) => $repoState.selectedFiles.has(f.path)).length,
  )

  // The Commit button names what it is about to do, in the native's wording and
  // title case (CommitComposer.swift:314-323): the count spelled out, "File"
  // singular at one. Zero drops the count rather than saying "Commit 0 Files" —
  // the button is disabled there anyway, and a count of nothing is not a
  // sentence. Amending names the rewrite instead of a count, because the file
  // list is not what an amend is about, and it is the only state with its own
  // in-progress title: everywhere else the spinner beside the button carries
  // that, so the label can keep telling the user what they are committing.
  const commitLabel = $derived.by(() => {
    if (isAmending) return isCommitting ? 'Amending…' : 'Amend Commit'
    if (includedCount === 0) return 'Commit'
    return includedCount === 1 ? 'Commit 1 File' : `Commit ${includedCount} Files`
  })

  // Relaxed submit gate when amending: git allows --amend with no staged files
  // (message-only edit). Outside amend mode, canCommit requires file selection.
  // Uses effectiveSummary so a one-file commit can submit with a blank input.
  //
  // Locked while generating, in both directions (Generate is already disabled
  // while committing): otherwise a commit can land mid-request and the late AI
  // result writes a message describing already-committed changes into the
  // composer the commit just cleared.
  const canSubmit = $derived(
    effectiveSummary.length > 0 && !isGenerating && (isAmending || $canCommit),
  )

  // ---- Provider readiness ---------------------------------------------------
  // Why the selected provider can't serve a request, when it can't. Core asks
  // two questions, not one — installed, *and* able to answer — because the first
  // on its own let an installed Claude CLI with an expired session light the
  // button up and fail every generate.
  //
  // Only the blocked case is stored, tagged with the provider it describes.
  // `null` therefore covers both "ready" and "not asked yet", which is the same
  // thing to the gate: refusing on "not known" is worse than letting a doomed
  // request report itself. Tagging is what makes a switched provider drop its
  // predecessor's block on the spot, with no clearing step to forget.
  interface ProviderBlock {
    provider: 'claude' | 'ollama'
    reason: string
    fixCommand: string
    /** The failed request this was read out of, when it came from one. */
    detail: string
  }
  let providerBlock = $state<ProviderBlock | null>(null)
  let probeSeq = 0

  async function probeProvider(target: 'claude' | 'ollama') {
    const seq = ++probeSeq
    try {
      // The same wait Generate does: the picker's own write may still be in
      // flight, and `load_ai_config` reads the file it is writing.
      await providerWrite
      const cfg = await aiApi.loadAiConfig()
      // A newer probe started, or the file still names the provider we're
      // replacing — either way this answer describes the wrong one.
      if (seq !== probeSeq || cfg.provider !== target) return
      const status = await aiApi.checkProviderStatus(cfg.provider, cfg)
      // Assigned only once the answer is in hand. Clearing on the way *into*
      // the probe is what made the remedy blink out and back on every window
      // focus — the re-probe runs on exactly that event, and asking Claude
      // costs two process spawns.
      if (seq !== probeSeq) return
      providerBlock = status.ready
        ? null
        : {
            provider: target,
            reason: status.reason,
            fixCommand: status.fix_command,
            detail: '',
          }
    } catch (err) {
      // Core raises only for an unknown provider name; a probe that fails for
      // any other reason is a wiring failure, not an answer — so it is logged
      // and nothing changes. It must not clear a block a real failed request
      // proved, which would put Generate back in front of a provider already
      // known to be dead.
      //
      // Logged, not silent: an unanswered probe leaves the gate open, and a
      // host that predates this command ("no such command") would otherwise
      // read as a working probe that found nothing wrong.
      console.error('[ai] provider probe failed; leaving Generate enabled', err)
    }
  }

  /** Re-ask, for the triggers below. Reads `provider` untracked on purpose. */
  function reprobeProvider() {
    untrack(() => void probeProvider(provider))
  }

  // Depends on `provider` alone. Reading the whole config here would re-probe
  // on every unrelated Settings change, and reading anything inside the body
  // would subscribe to it — hence `untrack`, the same shape the clone dialog's
  // re-arm effect needs.
  $effect(() => {
    const target = provider
    untrack(() => void probeProvider(target))
  })

  // The block while it still describes the selected provider — one narrowing
  // that the three reads below and the gate all share.
  const blocked = $derived(providerBlock?.provider === provider ? providerBlock : null)
  const blockedReason = $derived(blocked?.reason ?? '')
  const fixCommand = $derived(blocked?.fixCommand ?? '')
  // The raw provider text the remedy was read out of. Kept as the block's
  // tooltip rather than as a second line: "Claude couldn't authenticate. Sign
  // in again." and "Failed to authenticate: OAuth session expired" are one
  // fact, and stacking both is what made this strip unreadable.
  const blockedDetail = $derived(blocked?.detail ?? '')
  // The native's own fallback hint (CommitComposer.swift:110-111), with its
  // shortcut written the way this client's hosts write it.
  const generateHint = $derived(
    blockedReason || 'Generate a commit message from the checked files (Ctrl+G)',
  )
  const canGenerate = $derived(
    !isGenerating && !isCommitInProgress && !blocked && $repoState.selectedFiles.size > 0,
  )

  /*
    Re-probe when the window comes back, but only while something is blocking —
    so a ready provider costs nothing on every focus.

    This is the trigger that makes a *disabled* Generate safe to ship: every way
    of fixing an unready provider leaves this window. Signing in opens a browser
    and comes back; starting Ollama or installing the CLI happens in a terminal
    that takes focus of its own. Without it the button stays dead after the user
    has already fixed the problem, which is worse than never having disabled it.
  */
  $effect(() => {
    if (!blocked) return
    const onFocus = () => reprobeProvider()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  })

  function runFixCommand() {
    if (fixCommand) onRunInTerminal?.(fixCommand)
  }

  // When entering amend mode, pre-fill the composer from the target commit.
  // The backend pre-parses `co_authors` / `body_without_coauthors` off the
  // commit's trailers, so no trailer parsing happens here. When leaving
  // (commit-to-amend → null), clear the composer back to empty so the user
  // doesn't accidentally re-submit the amended message as a new commit.
  let lastAmendSha = $state<string | null>(null)
  $effect(() => {
    const target = $repoState.commitToAmend
    if (target !== null && target.sha !== lastAmendSha) {
      lastAmendSha = target.sha
      amendCoAuthors = target.co_authors
      summary = target.summary
      description = target.body_without_coauthors ?? target.body
    } else if (target === null && lastAmendSha !== null) {
      lastAmendSha = null
      amendCoAuthors = []
      summary = ''
      description = ''
    }
  })

  // One-shot prefill seed used by Undo Commit. MainLayout sets it on the store
  // after `git reset --mixed`; we copy values into the composer and clear the
  // seed so the effect doesn't re-fire on subsequent renders.
  $effect(() => {
    const seed = $repoState.restoreMessage
    if (seed !== null) {
      // Clear immediately — Svelte 5 re-runs the effect with seed === null,
      // and the early return prevents an infinite loop.
      repoState.update((s) => ({ ...s, restoreMessage: null }))
      summary = seed.summary
      description = seed.description
      amendCoAuthors = seed.coAuthors
    }
  })

  async function handleGenerate() {
    const state = get(repoState)
    const repoPath = $appState.repoPath
    if (!repoPath) return

    if (state.selectedFiles.size === 0) {
      error = 'No files selected'
      return
    }

    isGenerating = true
    error = null

    try {
      const files = Array.from(state.selectedFiles)
        .map((path) => state.status.files.find((f) => f.path === path))
        .filter((f): f is NonNullable<typeof f> => Boolean(f))

      const diffStr = await gitApi.getSelectedDiff(repoPath, files)

      // Read fresh per generate, and resolved for the selected provider by
      // core — so the model and server URL always belong to the provider
      // about to run. Splicing a picker value over a separately-loaded config
      // is how the two clients drifted, and how a Claude model reached Ollama.
      // `setProvider` persists the picker's choice; waiting for that write is
      // what makes this read reflect it, rather than the value it is replacing.
      await providerWrite
      const cfg = await aiApi.loadAiConfig()
      const message = await aiApi.generateCommitMessage(diffStr, cfg.provider, cfg)
      summary = message.title
      description = message.description
    } catch (err) {
      error = `Generate failed: ${String(err)}`
      // Read the failure itself, rather than re-running the probe. For an
      // expired session the probe is *wrong*: the credentials are still on
      // disk, so it reports a signed-in CLI, and this failure is the only place
      // that state is ever visible. Core owns the reading.
      void classifyFailure(String(err))
    } finally {
      isGenerating = false
    }
  }

  async function classifyFailure(message: string) {
    const target = provider
    const seq = ++probeSeq
    try {
      const status = await aiApi.providerStatusFromFailure(target, message)
      // Only ever to *raise* a remedy. A failure core doesn't recognize says
      // nothing about the provider, so it must not clear a block the probe
      // already found.
      if (seq !== probeSeq || status.ready) return
      providerBlock = {
        provider: target,
        reason: status.reason,
        fixCommand: status.fix_command,
        detail: message,
      }
      // The remedy replaces the raw failure rather than stacking under it. Both
      // describe one state, and the remedy is the half the user can act on; the
      // provider's own wording stays in the block's tooltip and in this log.
      console.warn('[ai] provider blocked by a failed request', message)
      error = null
    } catch (err) {
      console.error('[ai] could not classify the generate failure', err)
    }
  }

  // Persist a provider change (optimistic local update, then write). A patch
  // naming only `ai_provider`: the whole-object write this replaces posted the
  // config as the store had cached it — possibly hours old — and reverted
  // every field the other client had changed since.
  /**
   * The in-flight provider write. Generate awaits it: the picker's `onchange`
   * doesn't, so clicking Generate immediately after switching would otherwise
   * read the *previous* provider back off disk while the picker already shows
   * the new one.
   */
  let providerWrite: Promise<unknown> = Promise.resolve()

  async function setProvider(next: 'claude' | 'ollama') {
    const cfg = $config
    if (!cfg || cfg.ai_provider === next) return
    config.set({ ...cfg, ai_provider: next })
    providerWrite = configApi.patchConfig({ ai_provider: next })
    try {
      config.set((await providerWrite) as Config)
    } catch (err) {
      error = `Failed to save provider: ${String(err)}`
      // Put the picker back where the file still has it, rather than leaving
      // an optimistic value lying until the next restart — and put back only
      // the picker. Restoring the whole snapshot would revert every other
      // field the store has learned since, which is the lost update
      // `patch_config` exists to prevent, re-introduced one layer up.
      config.update((current) =>
        current ? { ...current, ai_provider: cfg.ai_provider } : current,
      )
    }
  }

  async function handleCommit() {
    // effectiveSummary falls back to the single-file auto-summary, so a blank
    // input is fine as long as something resolves; only a truly empty message
    // (no input, no auto-summary) is rejected.
    if (!effectiveSummary) {
      error = 'Summary is required'
      return
    }
    const repoPath = $appState.repoPath
    if (!repoPath) return

    const state = get(repoState)
    const files = Array.from(state.selectedFiles)
      .map((path) => state.status.files.find((f) => f.path === path))
      .filter((f): f is NonNullable<typeof f> => Boolean(f))
    // Amend allows a message-only commit, so an empty file list is only an
    // error in the non-amend path.
    if (files.length === 0 && !isAmending) {
      error = 'No files selected'
      return
    }

    // Committing an embedded git repository stages a gitlink, not the folder's
    // files — a surprising outcome we confirm before proceeding. Defer to the
    // modal, which calls performCommit on confirm.
    if (files.some((f) => f.embedded)) {
      pendingFiles = files
      return
    }

    await performCommit(files)
  }

  async function performCommit(files: FileEntry[]) {
    const repoPath = $appState.repoPath
    if (!repoPath) return

    isCommitting = true
    error = null

    try {
      const fullMessage = await gitApi.formatCommitMessage(effectiveSummary, description, amendCoAuthors)
      await gitApi.commit(repoPath, fullMessage, files, isAmending)
      summary = ''
      description = ''
      amendCoAuthors = []
      repoState.update((s) => ({
        ...s,
        selectedFiles: new Set(),
        userDeselected: new Set(),
        commitToAmend: null,
      }))
      lastAmendSha = null
      onCommitted?.()
    } catch (err) {
      error = `${isAmending ? 'Amend' : 'Commit'} failed: ${String(err)}`
    } finally {
      isCommitting = false
      // Close the confirm modal (if this commit came from it) whether it
      // succeeded or failed, so a failure's error message isn't hidden behind it.
      pendingFiles = []
    }
  }

  // A single-line <input> scrolls horizontally as the caret moves but ignores
  // wheel / trackpad gestures, so a long summary can't be swiped through. Map a
  // wheel delta onto scrollLeft (dominant axis, so both vertical scroll and a
  // horizontal swipe work) — only when there's actually overflow, and only then
  // do we preventDefault so we don't hijack the surrounding scroll otherwise.
  function handleSummaryWheel(e: WheelEvent) {
    const el = e.currentTarget as HTMLInputElement
    if (el.scrollWidth <= el.clientWidth) return
    const delta = Math.abs(e.deltaX) >= Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    if (delta === 0) return
    el.scrollLeft += delta
    e.preventDefault()
  }

  // ---- Keyboard entry points ------------------------------------------------
  // Called by the window-level handler in MainLayout rather than by a listener
  // on the fields: a chord scoped to the field it belongs to is only reachable
  // once you have already clicked into the composer, which is the one moment
  // you don't need a shortcut. Both gate exactly as their buttons do — a
  // keyboard route past the lockout is still a way to have a late AI result
  // overwrite a composer the commit just cleared.

  /** ⌘↩ / Ctrl+↩ from anywhere in the repo view. */
  export function requestCommit() {
    if (canSubmit && !isCommitInProgress) handleCommit()
  }

  /** ⌘G / Ctrl+G from anywhere in the repo view. */
  export function requestGenerate() {
    if (canGenerate) handleGenerate()
  }
</script>

<div class="commit-message-container" role="form" aria-label="Commit message">
  {#if isAmending}
    <div class="amend-notice" role="status">
      <span class="amend-notice-text">
        Your changes will modify your <strong>most recent commit</strong>.
      </span>
      <button
        type="button"
        class="stop-amending-link"
        onclick={() => onStopAmending?.()}
        disabled={isCommitInProgress}
      >
        Stop Amending
      </button>
    </div>
  {/if}

  <div class="summary-section">
    <input
      id="summary-input"
      type="text"
      class="summary-input"
      placeholder={summaryPlaceholder}
      bind:value={summary}
      maxlength="200"
      disabled={isGenerating || isCommitInProgress}
      onwheel={handleSummaryWheel}
    />
    <span class="char-count" class:warning={charCount > 72}>{charCount}/72</span>
  </div>

  <div class="description-section">
    <textarea
      id="description-input"
      class="description-input"
      placeholder="Description"
      bind:value={description}
      disabled={isGenerating || isCommitInProgress}
    ></textarea>
  </div>

  <!--
    One strip for everything that went wrong, so a failure and the state behind
    it read as a single message instead of as two unrelated lines at opposite
    ends of the box.

    Both rows are independent: a commit failure has to stay visible while the AI
    provider is separately blocked. Only the *generate* failure that produced a
    remedy is folded away, and that happens in `classifyFailure`, not here.
  -->
  {#if error || blockedReason}
    <div class="composer-status" role="status">
      {#if error}
        <p class="status-error">{error}</p>
      {/if}
      {#if blockedReason}
        <!--
          Why Generate is greyed out, stated rather than left to a hover. The
          offer sits immediately after the sentence that explains it, and reads
          as a sentence too — "Run" is prose, and only the command itself is the
          control. The command is spelled out because the app is about to type
          exactly that into the user's shell.
        -->
        <p class="status-remedy" title={blockedDetail}>
          <span>{blockedReason}</span>
          {#if fixCommand && onRunInTerminal}
            <span class="fix-offer">
              Run <button
                type="button"
                class="fix-command"
                onclick={runFixCommand}
                title="Run this in the terminal below"
              >{fixCommand}</button>
            </span>
          {/if}
        </p>
      {/if}
    </div>
  {/if}

  <div class="button-bar">
    <div class="button-group">
      <div class="select-field">
        <select
          class="provider-select"
          value={provider}
          onchange={(e) => setProvider(e.currentTarget.value as 'claude' | 'ollama')}
          disabled={isGenerating || isCommitInProgress}
        >
          <option value="claude">Claude</option>
          <option value="ollama">Ollama</option>
        </select>
      </div>
      <button
        class="action-button"
        onclick={handleGenerate}
        disabled={!canGenerate}
        title={generateHint}
      >
        {isGenerating ? 'Generating…' : 'Generate'}
      </button>
    </div>

    <!--
      The spinner sits between the row's trailing edge and the Commit button,
      where the native puts it (CommitComposer.swift:113-118). It is what lets
      the button keep naming the commit while one is running instead of
      swapping its label out for "Committing…" — the label answers "what will
      this do?", which is still worth reading mid-flight, and the spinner
      answers "is something happening?".
    -->
    <div class="commit-group">
      {#if isBusy}
        <span class="commit-progress" role="progressbar" aria-label="Working"></span>
      {/if}

      <!--
        No `aria-label`: the visible text now carries the file count, and an
        override reading only "Commit" would hide that from a screen reader and
        put the accessible name at odds with the label on screen.
      -->
      <button
        class="commit-button"
        onclick={handleCommit}
        disabled={!canSubmit || isCommitInProgress}
        title={isAmending
          ? 'Rewrite the most recent commit (Ctrl+Enter)'
          : 'Commit the checked files (Ctrl+Enter)'}
      >
        {commitLabel}
      </button>
    </div>
  </div>
</div>

{#if pendingEmbedded.length > 0}
  <EmbeddedRepoConfirm
    repos={pendingEmbedded}
    outerRepo={outerRepoName}
    {isCommitting}
    onConfirm={() => performCommit(pendingFiles)}
    onCancel={() => (pendingFiles = [])}
  />
{/if}

<style>
  /* 10px inset and an 8px rhythm between the rows: the native composer's
     `.padding(10)` and its `VStack(alignment: .leading, spacing: 8)`
     (CommitComposer.swift:73, :133).

     No `border-top` of its own. The rule above the composer belongs to the
     resize handle, exactly as it does natively — there the handle *is* a
     `Divider` (RowResizeHandle.swift:41) and `CommitComposer` draws nothing at
     its edge. `.commit-resize-handle` in `MainLayout.svelte` already paints
     that line, so a second one here read as a double rule with a 2px gutter
     between the two. */
  .commit-message-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: var(--bg-secondary);
    height: 100%;
    min-height: 0;
    box-sizing: border-box;
  }

  .amend-notice {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 4px 8px;
    border-left: 2px solid var(--status-yellow);
    background: transparent;
    font-size: 11px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .amend-notice-text {
    flex: 1;
    min-width: 0;
  }

  .amend-notice-text strong {
    color: var(--text-primary);
    font-weight: 500;
  }

  .stop-amending-link {
    padding: 0;
    background: transparent;
    border: none;
    color: var(--border-active);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .stop-amending-link:hover:not(:disabled) {
    text-decoration: underline;
  }

  .stop-amending-link:disabled {
    color: var(--text-faint);
    cursor: not-allowed;
  }

  /* The counter sits *beside* the field, not on top of it, and the 6px between
     them is the native row's `HStack(spacing: 6)` (CommitComposer.swift:78).
     Overlaying it inside the input is what the native deliberately refused to
     do (CommitComposer.swift:143-146), and for a reason that applies just as
     well here: a single-line input scrolls its own text under the caret, so a
     long summary ends up running underneath the digits counting it. */
  .summary-section {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .description-section {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  /* macOS `.caption` with monospaced digits, in the tertiary rank
     (CommitComposer.swift:148-153). `flex-shrink: 0` is that view's
     `.fixedSize()`: the digits never compress, the field yields instead. */
  .char-count {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  /* Advisory only, and only past git's conventional 72. Nothing is truncated
     and nothing is blocked — the native says why (CommitComposer.swift:136-141):
     a silent hard cap chops pasted and AI-generated summaries with no warning. */
  .char-count.warning {
    color: var(--status-orange);
  }

  /* `min-width: 0` is the field's half of the row's bargain, and the native
     needs the same thing badly enough to subclass for it: `ScrollingTextField`
     drops its intrinsic width (WheelScrollableTextField.swift:64-70) so a long
     summary cannot push the layout wider. Without it a flex item refuses to
     shrink below its content and the counter gets squeezed off the row. */
  .summary-input {
    flex: 1;
    min-width: 0;
    height: 28px;
    font-size: 13px;
    padding: 4px 8px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .summary-input:focus,
  .description-input:focus,
  .provider-select:focus {
    outline: none;
    border-color: var(--border-active);
    box-shadow: 0 0 0 2px var(--cursor-bg);
  }

  .summary-input:disabled,
  .description-input:disabled,
  .provider-select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* Deliberately no min-height of its own. The composer already has one floor
     — its 180px minimum, enforced by the resize handle and by MainLayout's
     clamp — and a second floor here can only disagree with it: the textarea
     refused to shrink when the status strip appeared, overflowed its flex slot,
     and painted over the strip below. One floor, in one place.

     The inset is 4px down the top and 9px in from the leading edge, which is
     where the native editor puts its first character and its placeholder
     (CommitComposer.swift:286, :294-296): a 4pt scroll-content margin on every
     side, plus the text view's own 5pt line-fragment padding on the two
     horizontal ones. */
  .description-input {
    flex: 1;
    min-height: 0;
    font-size: 13px;
    padding: 4px 9px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
    resize: none;
    overflow-y: auto;
  }

  /* One bordered block, so a failure and the remedy for it read as a single
     message. A left rule instead of a filled banner, matching the amend
     notice: the composer is a dense stack of fields and a tinted slab here
     reads as another one. */
  .composer-status {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border-left: 2px solid var(--status-red);
    font-size: 11px;
    line-height: 1.4;
    flex-shrink: 0;
  }

  .status-error {
    margin: 0;
    color: var(--status-red);
    /* Provider errors can run long; keep the composer's width rather than
       letting an un-wrappable token push the whole box wider. */
    overflow-wrap: anywhere;
  }

  /* A standing condition, not a failed attempt — so it reads as a caption
     rather than in the red the error line above uses. Baseline-aligned, so the
     inline command chip sits on the same line as the words around it. */
  .status-remedy {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    /* One word space between the reason and the offer, not a gutter: they are
       one sentence, and a gap wide enough to read as a column break undoes
       that. The row gap is what a wrapped line falls by. */
    gap: 2px;
    margin: 0;
    color: var(--text-secondary);
  }

  /* "Run" is prose at the caption's own size and colour — it is grammar, not a
     control. Only the command is clickable. */
  .fix-offer {
    flex-shrink: 0;
  }

  /* The command is the control: its own tinted chip, in mono, in the accent —
     a thing you can tell apart from the sentence carrying it without leaving
     that sentence. Deliberately not a bordered button: the chrome made the
     whole phrase read as one oversized control and buried which part was
     actually clickable. */
  .fix-command {
    padding: 1px 5px;
    background: var(--bg-elevated);
    border: none;
    border-radius: 4px;
    color: var(--border-active);
    font-family: var(--font-mono);
    /* Mono renders visually larger than the UI face at the same value, so
       matching the strip's size would not match on screen. */
    font-size: 0.92em;
    line-height: inherit;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .fix-command:hover {
    background: var(--surface-hover);
    text-decoration: underline;
  }

  /* 8px between every control in this row. Natively it is one
     `HStack(alignment: .center, spacing: 8)` with a `Spacer` doing the split
     (CommitComposer.swift:94, :113), so the picker-to-Generate gap and the
     spinner-to-Commit gap are the same measure; the two groups here only exist
     to put the `Spacer` between them. */
  .button-bar {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
  }

  .button-group,
  .commit-group {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* The native's `ProgressView().controlSize(.small)`
     (CommitComposer.swift:115-118), drawn with this codebase's own spinner ring
     rather than the system's thinner arcs — so it is stepped below AppKit's
     16pt small indicator to carry the same visual weight beside a 23px button. */
  .commit-progress {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    border: 2px solid var(--border-inactive);
    border-top-color: var(--border-active);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* A spinner is the one thing here that must not be the only channel: it says
     "busy" by turning. Held still it still reads as a distinct object in a row
     that is otherwise empty, and the disabled controls beside it say the same
     thing. */
  @media (prefers-reduced-motion: reduce) {
    .commit-progress {
      animation: none;
    }
  }

  /* Sized and filled as a button rather than as a field: this one sits in the
     composer's button bar, so it takes `--bg-elevated` and the row's 12px like
     the two buttons beside it, not the recessed register the Settings selects
     wear. Everything else about it — `appearance: none`, the chevron and its
     wrapper — is `app.css`'s, and the trailing padding is restated only because
     this shorthand would otherwise close the gap the chevron sits in. */
  .provider-select {
    font-size: 12px;
    padding: 3px 24px 3px 8px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    font-family: inherit;
  }

  .action-button,
  .commit-button {
    padding: 3px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
    border: 1px solid var(--border-strong);
  }

  .action-button {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .action-button:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  /* A bordered button keeps its bezel when disabled and dims only what is
     printed on it, which is what AppKit does to a plain `Button`'s title. The
     prominent one below cannot be treated this way — see there. */
  .action-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .commit-button {
    background: var(--border-active);
    color: var(--on-accent);
    border-color: var(--border-active);
    padding: 3px 16px;
    font-weight: 500;
  }

  .commit-button:hover:not(:disabled) {
    background: var(--accent-secondary);
    border-color: var(--accent-secondary);
  }

  /* A disabled prominent button gives up the accent entirely, the way
     `.buttonStyle(.borderedProminent)` does under `.disabled(!canCommit)`
     (CommitComposer.swift:123, :125). Fading the blue instead — which is all
     `opacity` can do — leaves a button that still reads as the accent-coloured
     thing you are meant to press, so the composer looked available with an
     empty summary and the click did nothing. Losing the fill is the signal;
     the dimming is only what follows from it. */
  .commit-button:disabled {
    background: var(--bg-tertiary);
    border-color: var(--border-inactive);
    color: var(--text-muted);
    cursor: not-allowed;
  }
</style>
