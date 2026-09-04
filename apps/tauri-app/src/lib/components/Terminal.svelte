<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import { Channel } from '@tauri-apps/api/core'
  import { writeText } from '@tauri-apps/plugin-clipboard-manager'
  import { osApi, terminalApi, type TerminalEvent } from '$lib/api/commands'
  import { isMac } from '$lib/utils/platform'
  import '@xterm/xterm/css/xterm.css'

  let {
    repoPath,
    shellId,
    expanded = true,
    onExit,
    onShellResolved,
  }: {
    repoPath: string
    /** Persisted shell preference; the backend falls back if it's uninstalled. */
    shellId?: string
    expanded?: boolean
    onExit?: () => void
    /** Reports the shell that actually launched, so the panel can label itself. */
    onShellResolved?: (label: string) => void
  } = $props()

  let container: HTMLDivElement | undefined = $state()
  let term: Terminal | null = null
  let fitAddon: FitAddon | null = null
  let pid: number | null = null
  let resizeObserver: ResizeObserver | null = null
  let resizeTimer: ReturnType<typeof setTimeout> | null = null
  // Flips true once xterm is open so the focus effect never runs before `term`
  // exists. Stays false on a fresh remount ({#key}) until that instance mounts.
  let mounted = $state(false)
  // Set once teardown starts so async setup steps bail instead of resurrecting
  // a PTY for a component that is already gone.
  let disposed = false
  // The shell reported its exit. Since output and the close event now share one
  // channel, "closed" can land *before* `start_terminal` has returned the pid —
  // a shell that dies on a broken `.zshrc` does exactly that — and adopting a
  // pid after the backend has already dropped the session would leave the panel
  // holding a handle to nothing.
  let sessionClosed = false
  // A command handed in before the shell was ready (see `runCommand`).
  let queuedCommand: string | null = null
  // The "Follow link" affordance, created on first hover and reused after.
  let linkHint: HTMLDivElement | null = null

  // A panel drag fires ResizeObserver every frame. Each fit() that changes the
  // grid pushes a ResizePseudoConsole down to the shell, and PSReadLine repaints
  // its whole edit buffer per resize — so an unthrottled observer turns one drag
  // into hundreds of repaints and a visibly corrupted prompt. Coalesce to the
  // final size instead; Windows Terminal debounces for the same reason.
  const RESIZE_DEBOUNCE_MS = 80

  // Which modifier turns a hover into a click-through. ⌘ on macOS because ⌃
  // there is a right-click, `Ctrl` everywhere else — Terminal.app, iTerm and
  // VS Code all draw the line in the same place, and SwiftTerm already does.
  const LINK_MODIFIER = isMac() ? '⌘' : 'Ctrl'

  onMount(() => {
    void init()
    window.addEventListener('focus', restoreCaret)
    document.addEventListener('visibilitychange', restoreCaret)

    return () => {
      disposed = true
      window.removeEventListener('focus', restoreCaret)
      document.removeEventListener('visibilitychange', restoreCaret)
      if (resizeTimer !== null) clearTimeout(resizeTimer)
      resizeTimer = null
      resizeObserver?.disconnect()
      resizeObserver = null
      hideLinkHint()
      if (pid !== null) {
        const closingPid = pid
        pid = null
        terminalApi.close(closingPid).catch(() => {})
      }
      term?.dispose()
      term = null
      fitAddon = null
    }
  })

  async function init() {
    if (!container) return

    // Must be fetched before constructing Terminal: xterm reads `windowsPty`
    // when it builds the buffer, so setting it afterwards has no effect.
    let windowsPty: { backend: 'conpty' | 'winpty'; buildNumber: number } | undefined
    try {
      const info = await terminalApi.ptyInfo()
      if (info.backend && info.build_number) {
        windowsPty = {
          backend: info.backend as 'conpty' | 'winpty',
          buildNumber: info.build_number,
        }
      }
    } catch (e) {
      // Non-fatal: xterm falls back to its heuristic wrapping mode.
      console.warn('[terminal] pty info unavailable, reflow disabled', e)
    }
    if (disposed || !container) return

    term = new Terminal({
      fontFamily: "ui-monospace, 'SF Mono', Menlo, Monaco, monospace",
      fontSize: 12,
      fontWeight: 500,
      theme: {
        background: '#000000',
        foreground: '#e5e5e5',
      },
      cursorBlink: true,
      // Chosen rather than inherited: xterm defaults to 1 000 and SwiftTerm to
      // 500, which had the two clients remembering different amounts of the
      // same shell. 1 000 is the one that survives a `git log --stat`, and what
      // VS Code ships.
      scrollback: 1000,
      windowsPty,
    })

    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    // A URL needs the modifier before it opens, and says so on hover. Plain
    // click belongs to the *selection* — dragging across a line that happens to
    // contain a URL used to navigate away from it — which is why every terminal
    // emulator asks for a modifier, and why the affordance is what answers the
    // discoverability cost rather than dropping the modifier.
    term.loadAddon(
      new WebLinksAddon(openLink, {
        hover: showLinkHint,
        leave: hideLinkHint,
      })
    )
    term.parser.registerOscHandler(52, handleClipboardRequest)

    // Everything the shell could want goes to the shell — Ctrl+P is
    // readline's previous-history, Escape is vim's normal mode. The panel's
    // own toggle is the single exception: returning false leaves the event
    // unhandled here so it reaches the window listener that owns it. The app's
    // global handlers make the same cut from their side (`utils/keyboard.ts`),
    // so each of these keys has exactly one owner at any moment.
    //
    // `Ctrl` only, matching the native client and VS Code: ⌘` is macOS's own
    // window-cycling chord, so answering to it took a system gesture away from
    // every user of a macOS build. The phase is checked because xterm runs this
    // handler for keyup and keypress too, and releasing an event xterm had
    // already begun processing is not the same as declining it.
    term.attachCustomKeyEventHandler(
      (e) => !(e.type === 'keydown' && e.ctrlKey && e.key === '`')
    )

    term.open(container)

    // Defer fit() until the container has its real size.
    setTimeout(() => safeFit(), 0)

    await initBackend()
    if (disposed) return

    resizeObserver = new ResizeObserver(() => {
      if (resizeTimer !== null) clearTimeout(resizeTimer)
      resizeTimer = setTimeout(() => {
        resizeTimer = null
        safeFit()
      }, RESIZE_DEBOUNCE_MS)
    })
    resizeObserver.observe(container)

    mounted = true
  }

  function safeFit() {
    try {
      fitAddon?.fit()
    } catch {
      // fit can throw if the container is unmounted mid-frame; ignore.
    }
  }

  // Focus the terminal whenever the section is showing: on first open, on a new
  // session (remounted via {#key}), and when re-expanded from minimized. Guard on
  // `mounted` so focus() never runs before xterm exists, and skip while collapsed
  // since the container is display:none then and a hidden element can't take focus.
  $effect(() => {
    if (mounted && expanded) {
      term?.focus()
    }
  })

  /**
   * Put the caret back in the shell when the app becomes active again.
   *
   * The web platform does not restore it: coming back to the window leaves
   * xterm painted as focused — its border lit, its cursor block drawn — while
   * the hidden textarea behind it holds nothing, so keystrokes disappear until
   * the user clicks. AppKit hands the native client its first responder back;
   * this is the same behaviour by hand.
   *
   * The condition is read *now* rather than latched on the way out. A flag set
   * from `focusin` is only ever cleared by another `focusin`, and clicking a
   * plain div raises none — so the flag would strand `true` and the terminal
   * would take the caret back from whatever the user had actually moved to.
   */
  function restoreCaret(): void {
    if (!mounted || !expanded || document.hidden) return
    if (container?.contains(document.activeElement)) term?.focus()
  }

  /** Whether this click carries the modifier that means "follow the link". */
  function linkModifierHeld(e: MouseEvent): boolean {
    return isMac() ? e.metaKey : e.ctrlKey
  }

  /** Hand a clicked URL to the OS browser, as the update chip's link does. */
  function openLink(e: MouseEvent, uri: string): void {
    if (!linkModifierHeld(e)) return
    osApi.openUrl(uri).catch((error) => {
      console.error('[terminal] could not open link:', error)
    })
  }

  /**
   * Name the gesture over the link the pointer is on.
   *
   * `xterm-hover` is xterm's marker class for an overlay of our own: its own
   * mouse tracking stops at any element carrying it, so the tooltip can sit
   * over the row it describes without the link flickering out from under it.
   * The class carries no styling of its own — that is entirely ours.
   */
  function showLinkHint(e: MouseEvent, _uri: string): void {
    const host = term?.element
    if (!host) return
    if (!linkHint) {
      linkHint = document.createElement('div')
      linkHint.className = 'xterm-hover terminal-link-hint'
      linkHint.textContent = `Follow link (${LINK_MODIFIER} + click)`
      host.appendChild(linkHint)
    }
    const box = host.getBoundingClientRect()
    // Centred on the pointer, then pulled back inside the panel so a link near
    // either edge still reads its own hint.
    const half = linkHint.offsetWidth / 2
    const limit = Math.max(box.width - half, half)
    const top = e.clientY - box.top
    linkHint.style.left = `${Math.min(Math.max(e.clientX - box.left, half), limit)}px`
    linkHint.style.top = `${top}px`
    // Above the pointer, except on the first row or two, where the panel's own
    // overflow would clip it.
    linkHint.classList.toggle('below', top < 28)
  }

  function hideLinkHint(): void {
    linkHint?.remove()
    linkHint = null
  }

  /**
   * OSC 52 — the escape sequence a shell, `tmux` or `vim` uses to put its
   * selection on the system clipboard, including from the far side of an SSH
   * session where nothing else can reach it. SwiftTerm honours it already; this
   * is the Tauri half.
   *
   * **Write-only, deliberately.** The sequence also defines a *read*, which
   * types the clipboard back down the TTY — so anything that can print to the
   * terminal could exfiltrate whatever the user last copied. Every emulator
   * worth trusting refuses to answer it, and so does this: the request is
   * swallowed (returning `true`) rather than declined, so no other handler
   * answers it either.
   *
   * The write goes through the OS rather than `navigator.clipboard`, which
   * WebKit refuses without a recent click — and a shell asking for the
   * clipboard is not a click.
   */
  function handleClipboardRequest(payload: string): boolean {
    // xterm has already stripped `52;`, so what arrives is `Pc;Pd`. Only the
    // first separator is ours — the rest, if any, is the payload's problem.
    const split = payload.indexOf(';')
    if (split === -1) return true
    const targets = payload.slice(0, split)
    const encoded = payload.slice(split + 1).replace(/\s/g, '')
    if (encoded === '?') return true
    // `Pc` is a set of selections, empty meaning the spec's default. The system
    // clipboard is the only one with anywhere to go here: X11's PRIMARY and the
    // numbered cut buffers have no cross-platform analogue.
    if (targets !== '' && !targets.includes('c') && !targets.includes('s')) return true
    let text: string
    try {
      const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0))
      text = new TextDecoder().decode(bytes)
    } catch {
      // Per the spec anything that isn't base64 clears the selection. There is
      // nothing to clear, so it is a no-op rather than a failure to report.
      return true
    }
    writeText(text).catch((e) => {
      console.warn('[terminal] clipboard write refused', e)
    })
    return true
  }

  /**
   * Type a command into this shell and run it, on behalf of somewhere else in
   * the app that knows the command but not the terminal (today: the composer's
   * "fix the AI provider" button).
   *
   * Queued when the shell is still starting, so the caller never has to know
   * whether this panel is warm — it may have been created by the very click
   * that is calling this.
   */
  export function runCommand(command: string) {
    if (pid === null) {
      queuedCommand = command
      return
    }
    terminalApi.write(pid, `${command}\r`).catch(console.error)
  }

  /**
   * Handle one message from this session's stream.
   *
   * Registered on the channel *before* `start_terminal` is invoked, which is the
   * whole reason the session has a channel: the reader thread starts emitting
   * the moment the shell is spawned, and a subscription taken out afterwards
   * would miss everything printed in between — a fast shell's first prompt, or
   * the entire life of one that dies on a broken `.zshrc`.
   */
  function onSessionEvent(message: TerminalEvent): void {
    if (message.event === 'output') {
      term?.write(message.data)
      return
    }
    // The shell exited on its own (`exit`, Ctrl+D, or a crash). Null the pid
    // first so unmount cleanup skips close_terminal — the backend already
    // dropped the session before sending this. A clean exit lets the parent tear
    // the panel down; anything else keeps the dead terminal on screen with the
    // reason, VS Code-style, so a shell that dies instantly no longer flashes
    // its error away. The panel's own ✕ still closes it — unmount cleanup is a
    // no-op once the pid is null.
    sessionClosed = true
    pid = null
    const { exit_code, signal } = message.exit
    if (exit_code === 0 && !signal) {
      onExit?.()
      return
    }
    const reason = signal ? `terminated by signal: ${signal}` : `exited with code ${exit_code}`
    term?.writeln(`\r\n\x1b[31m[Process ${reason}]\x1b[0m`)
  }

  async function initBackend() {
    if (!term) return
    try {
      const started = await terminalApi.start(
        repoPath,
        shellId,
        new Channel<TerminalEvent>(onSessionEvent)
      )
      if (disposed) {
        // Unmounted while the shell was starting; don't leak the PTY.
        terminalApi.close(started.pid).catch(() => {})
        return
      }
      onShellResolved?.(started.shell_label)
      // The session may already be over — its close event can overtake this
      // return. Taking the pid now would hand the panel a handle the backend
      // has dropped, and its ✕ would then try to kill a session that is gone.
      if (sessionClosed) return
      pid = started.pid

      term.onData((data) => {
        if (pid !== null) {
          terminalApi.write(pid, data).catch(console.error)
        }
      })

      term.onResize(({ cols, rows }) => {
        if (pid !== null) {
          terminalApi.resize(pid, cols, rows).catch(console.error)
        }
      })

      // Push the initial size down to the PTY in case xterm settled
      // before the backend session was ready.
      terminalApi.resize(pid, term.cols, term.rows).catch(console.error)

      if (queuedCommand !== null) {
        const command = queuedCommand
        queuedCommand = null
        runCommand(command)
      }
    } catch (e) {
      term?.writeln(`\r\n\x1b[31mTerminal error: ${e}\x1b[0m`)
    }
  }
</script>

<div class="terminal" bind:this={container}></div>

<style>
  .terminal {
    width: 100%;
    height: 100%;
    background: #000000;
    padding: 4px;
    box-sizing: border-box;
    overflow: hidden;
  }

  :global(.xterm) {
    font-family: ui-monospace, 'SF Mono', Menlo, Monaco, monospace;
    height: 100%;
  }

  :global(.xterm-viewport) {
    background-color: #000000 !important;
  }

  /* Built by hand into xterm's own element, so it can't be scoped by Svelte. */
  :global(.terminal-link-hint) {
    position: absolute;
    z-index: 10;
    transform: translate(-50%, calc(-100% - 8px));
    padding: 3px 7px;
    border-radius: 5px;
    border: 1px solid var(--border-inactive);
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-family: var(--font-ui);
    font-size: 11px;
    line-height: 1.4;
    white-space: nowrap;
    pointer-events: none;
    box-shadow: var(--shadow-popover);
  }

  :global(.terminal-link-hint.below) {
    transform: translate(-50%, 18px);
  }
</style>
