<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import { listen, type UnlistenFn } from '@tauri-apps/api/event'
  import { terminalApi } from '$lib/api/commands'
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
  let unlisteners: UnlistenFn[] = []
  let resizeObserver: ResizeObserver | null = null
  let resizeTimer: ReturnType<typeof setTimeout> | null = null
  // Flips true once xterm is open so the focus effect never runs before `term`
  // exists. Stays false on a fresh remount ({#key}) until that instance mounts.
  let mounted = $state(false)
  // Set once teardown starts so async setup steps bail instead of resurrecting
  // a PTY for a component that is already gone.
  let disposed = false

  // A panel drag fires ResizeObserver every frame. Each fit() that changes the
  // grid pushes a ResizePseudoConsole down to the shell, and PSReadLine repaints
  // its whole edit buffer per resize — so an unthrottled observer turns one drag
  // into hundreds of repaints and a visibly corrupted prompt. Coalesce to the
  // final size instead; Windows Terminal debounces for the same reason.
  const RESIZE_DEBOUNCE_MS = 80

  onMount(() => {
    void init()

    return () => {
      disposed = true
      if (resizeTimer !== null) clearTimeout(resizeTimer)
      resizeTimer = null
      resizeObserver?.disconnect()
      resizeObserver = null
      unlisteners.forEach((fn) => fn())
      unlisteners = []
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
      windowsPty,
    })

    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon())

    // Everything the shell could want goes to the shell — Ctrl+P is
    // readline's previous-history, Escape is vim's normal mode. The panel's
    // own toggle is the single exception: returning false leaves the event
    // unhandled here so it reaches the window listener that owns it. The app's
    // global handlers make the same cut from their side (`utils/keyboard.ts`),
    // so each of these keys has exactly one owner at any moment.
    term.attachCustomKeyEventHandler((e) => !((e.ctrlKey || e.metaKey) && e.key === '`'))

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

  async function initBackend() {
    if (!term) return
    try {
      const started = await terminalApi.start(repoPath, shellId)
      if (disposed) {
        // Unmounted while the shell was starting; don't leak the PTY.
        terminalApi.close(started.pid).catch(() => {})
        return
      }
      pid = started.pid
      onShellResolved?.(started.shell_label)

      const u1 = await listen<string>(`terminal-output-${pid}`, (e) => {
        term?.write(e.payload)
      })
      // The shell exited on its own (`exit`, Ctrl+D, or a crash). Null the pid
      // first so unmount cleanup skips close_terminal — the backend already
      // dropped the session before emitting. A clean exit lets the parent tear
      // the panel down as before; anything else keeps the dead terminal on
      // screen with the reason, VS Code-style, so a shell that dies instantly
      // (a broken .zshrc) no longer flashes its error away. The panel's own ✕
      // still closes it — unmount cleanup is a no-op once the pid is null.
      const u2 = await listen<{ exit_code: number; signal: string | null }>(
        `terminal-closed-${pid}`,
        (e) => {
          pid = null
          const { exit_code, signal } = e.payload
          if (exit_code === 0 && !signal) {
            onExit?.()
          } else {
            const reason = signal
              ? `terminated by signal: ${signal}`
              : `exited with code ${exit_code}`
            term?.writeln(`\r\n\x1b[31m[Process ${reason}]\x1b[0m`)
          }
        }
      )
      unlisteners.push(u1, u2)

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
      if (pid !== null) {
        terminalApi.resize(pid, term.cols, term.rows).catch(console.error)
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
</style>
