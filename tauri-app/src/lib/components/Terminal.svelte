<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import { invoke } from '@tauri-apps/api/core'
  import { listen, type UnlistenFn } from '@tauri-apps/api/event'
  import '@xterm/xterm/css/xterm.css'

  let { repoPath }: { repoPath: string } = $props()

  let container: HTMLDivElement | undefined = $state()
  let term: Terminal | null = null
  let fitAddon: FitAddon | null = null
  let pid: number | null = null
  let unlisteners: UnlistenFn[] = []
  let resizeObserver: ResizeObserver | null = null

  onMount(() => {
    if (!container) return

    term = new Terminal({
      fontFamily: "ui-monospace, 'SF Mono', Menlo, Monaco, monospace",
      fontSize: 12,
      theme: {
        background: '#000000',
        foreground: '#e5e5e5',
      },
      cursorBlink: true,
    })

    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon())
    term.open(container)

    // Defer fit() until the container has its real size.
    setTimeout(() => fitAddon?.fit(), 0)

    initBackend()

    resizeObserver = new ResizeObserver(() => {
      try {
        fitAddon?.fit()
      } catch {
        // fit can throw if container is unmounted mid-frame; ignore.
      }
    })
    resizeObserver.observe(container)

    return () => {
      resizeObserver?.disconnect()
      resizeObserver = null
      unlisteners.forEach((fn) => fn())
      unlisteners = []
      if (pid !== null) {
        const closingPid = pid
        pid = null
        invoke('close_terminal', { pid: closingPid }).catch(() => {})
      }
      term?.dispose()
      term = null
      fitAddon = null
    }
  })

  async function initBackend() {
    if (!term) return
    try {
      pid = await invoke<number>('start_terminal', { repoPath })

      const u1 = await listen<string>(`terminal-output-${pid}`, (e) => {
        term?.write(e.payload)
      })
      const u2 = await listen(`terminal-closed-${pid}`, () => {
        pid = null
      })
      unlisteners.push(u1, u2)

      term.onData((data) => {
        if (pid !== null) {
          invoke('write_terminal', { pid, data }).catch(console.error)
        }
      })

      term.onResize(({ cols, rows }) => {
        if (pid !== null) {
          invoke('resize_terminal', { pid, cols, rows }).catch(console.error)
        }
      })

      // Push the initial size down to the PTY in case xterm settled
      // before the backend session was ready.
      if (pid !== null) {
        invoke('resize_terminal', {
          pid,
          cols: term.cols,
          rows: term.rows,
        }).catch(console.error)
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
