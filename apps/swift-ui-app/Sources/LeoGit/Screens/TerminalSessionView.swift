import AppKit
import SwiftTerm
import SwiftUI

/// One live PTY session rendered by SwiftTerm — the native counterpart of
/// the Tauri client's `Terminal.svelte`.
///
/// Owns the whole Swift side of the terminal seam: launches the PTY on
/// appear, relays output from the Rust reader thread into the emulator,
/// sends keystrokes and grid sizes back on a serial I/O queue, and kills the
/// child when it leaves the hierarchy. Mounted with an explicit `.id`
/// (`repoPath:generation`), so a new session or a repo switch replaces the
/// whole view and start/cleanup ride structural identity — the way the
/// Tauri client remounts its component under `{#key}`.
struct TerminalSessionView: View {
    let repoPath: String
    let isExpanded: Bool
    let onStarted: (StartedTerminal) -> Void
    let onExit: () -> Void

    @State private var controller = TerminalController()

    var body: some View {
        TerminalHostView(controller: controller)
            .task {
                if isExpanded { controller.focus() }
                await controller.start(repoPath: repoPath, onStarted: onStarted, onExit: onExit)
            }
            .onDisappear { controller.shutdown() }
            .onChange(of: isExpanded) { _, expanded in
                // Focus follows the panel: showing it hands the terminal the
                // keyboard so typing works without clicking into it, and
                // collapsing hands it back — a collapsed panel is zero-height
                // but still mounted, so a terminal that kept first responder
                // would swallow keystrokes into an invisible shell.
                if expanded {
                    controller.focus()
                } else {
                    controller.resignFocus()
                }
            }
    }
}

/// Bridges one SwiftTerm view to one core PTY session.
///
/// `@MainActor` because it touches the view; the blocking bridge calls are
/// pushed onto `ioQueue`, a serial queue that both keeps them off the main
/// actor and preserves typing order — unordered `Task`s could deliver
/// keystrokes out of sequence.
@MainActor
final class TerminalController {
    /// Core's synthetic session id — nil before launch and after exit.
    /// Nulled *first* on a natural exit so `shutdown()` never double-closes
    /// a session core has already dropped: the Tauri client's ordering.
    private var pid: UInt32?

    /// Set by `TerminalHostView` when it builds the view.
    weak var terminalView: TerminalView?

    private let ioQueue = DispatchQueue(label: "com.leomanrique.leogit.terminal-io")
    private var onExit: (() -> Void)?
    private var relay: TerminalRelay?

    /// A focus request that could not be honoured yet. The dock asks for
    /// focus as the panel opens, which on the first expand is before AppKit
    /// has attached the emulator to a window — and `makeFirstResponder` on a
    /// windowless view is a silent no-op, which is why the terminal used to
    /// come up unfocused. The request is remembered and replayed from
    /// `viewDidAttach()`.
    private var wantsFocus = false

    /// Read the shared config's shell preference and spawn the PTY. On
    /// failure the error is printed into the emulator in red — the Tauri
    /// client's only in-terminal message.
    func start(
        repoPath: String,
        onStarted: @escaping (StartedTerminal) -> Void,
        onExit: @escaping () -> Void
    ) async {
        self.onExit = onExit
        let relay = TerminalRelay(
            deliver: { [weak self] bytes in self?.feed(bytes) },
            closed: { [weak self] exit in self?.sessionDidClose(exit) }
        )
        self.relay = relay

        // Fresh read per session, so a preference saved in the Tauri client
        // applies to the next session here — "applies to new terminal
        // sessions", as its Settings hint puts it. Core resolves an
        // uninstalled or unknown id to the best shell on this machine.
        let shellID = (try? await GitBridge.appConfig())?.terminalShell

        do {
            let started = try await GitBridge.launchTerminal(
                in: repoPath, shellID: shellID, listener: relay)
            if Task.isCancelled {
                // Unmounted while spawning: close the just-born session
                // immediately so it cannot leak (the Tauri `disposed` guard).
                ioQueue.async { try? GitBridge.killTerminal(pid: started.pid) }
                return
            }
            pid = started.pid
            onStarted(started)
            pushCurrentSize()
        } catch {
            feed(Array("\r\n\u{1B}[31mTerminal error: \(error.displayMessage)\u{1B}[0m\r\n".utf8))
        }
    }

    /// Keystrokes (and pastes) from the emulator, in order.
    func send(_ data: String) {
        guard let pid else { return }
        ioQueue.async {
            do {
                try GitBridge.sendTerminalInput(pid: pid, data: data)
            } catch {
                print("[terminal] write to session \(pid) failed: \(error.displayMessage)")
            }
        }
    }

    /// The emulator's grid changed; tell the PTY. A collapsed panel reports
    /// a degenerate grid — skipped, like the Tauri client swallowing
    /// zero-size fits, so re-expanding restores the exact prompt instead of
    /// a one-column wrap.
    func resize(cols: Int, rows: Int) {
        guard let pid, cols >= 2, rows >= 2 else { return }
        ioQueue.async {
            try? GitBridge.resizeTerminalGrid(
                pid: pid,
                cols: UInt16(clamping: cols),
                rows: UInt16(clamping: rows)
            )
        }
    }

    /// Kill the child on unmount, queued behind any pending writes. A no-op
    /// when the shell already exited — `sessionDidClose` nulled the pid.
    func shutdown() {
        guard let pid else { return }
        self.pid = nil
        ioQueue.async { try? GitBridge.killTerminal(pid: pid) }
    }

    /// Give the terminal the keyboard, now or as soon as it can hold it.
    func focus() {
        wantsFocus = true
        applyPendingFocus()
    }

    /// AppKit attached the emulator to a window: the earliest moment it can
    /// become first responder, so a request made before the mount finished
    /// lands here.
    func viewDidAttach() {
        applyPendingFocus()
    }

    /// Drop the keyboard when the panel closes, leaving the window itself as
    /// first responder. A pending request is dropped too: a panel collapsed
    /// before it ever drew must not steal focus later.
    func resignFocus() {
        wantsFocus = false
        guard let terminalView, let window = terminalView.window,
            window.firstResponder === terminalView
        else { return }
        window.makeFirstResponder(nil)
    }

    private func applyPendingFocus() {
        guard wantsFocus, let terminalView, let window = terminalView.window else { return }
        wantsFocus = false
        // Queued behind the update that mounted or revealed the view: SwiftUI
        // settles its own first responder as the pass ends, so a synchronous
        // call here can be undone a moment later.
        Task { @MainActor in window.makeFirstResponder(terminalView) }
    }

    /// The PTY opens at 80×24; push the grid the emulator actually laid out.
    /// Belt-and-braces for the case where the view settled before the
    /// session existed and `sizeChanged` therefore never fired.
    private func pushCurrentSize() {
        guard let terminal = terminalView?.getTerminal() else { return }
        resize(cols: terminal.cols, rows: terminal.rows)
    }

    private func feed(_ bytes: [UInt8]) {
        terminalView?.feed(byteArray: bytes[...])
    }

    /// The child exited. A clean exit tears the panel down as before;
    /// anything else keeps the dead terminal on screen with the reason,
    /// VS Code-style, so a shell that dies instantly (a broken `.zshrc`) no
    /// longer flashes its error away. Either way the pid is nulled first, so
    /// `shutdown()` — and the dock's ✕, which unmounts this view — never
    /// double-closes a session core has already dropped.
    private func sessionDidClose(_ exit: TerminalExit) {
        pid = nil
        if exit.exitCode == 0, exit.signal == nil {
            onExit?()
        } else {
            let reason =
                exit.signal.map { "terminated by signal: \($0)" }
                ?? "exited with code \(exit.exitCode)"
            feed(Array("\r\n\u{1B}[31m[Process \(reason)]\u{1B}[0m\r\n".utf8))
        }
    }
}

/// Receives one session's events on its Rust reader thread and coalesces
/// them onto the main actor.
///
/// The reader emits once per 4 KiB read with no throttling, so a flood —
/// `cat` on a large file — would otherwise queue one main-actor hop per
/// chunk. Instead chunks append to a locked buffer with at most one drain in
/// flight: bursts batch into a single `feed`, the callback stays cheap on
/// the reader thread, and the main actor sees bounded work. This is the
/// port plan's answer to byte-stream throughput over a UniFFI callback.
private final class TerminalRelay: TerminalEventListener, @unchecked Sendable {
    private let lock = NSLock()
    private var buffer: [UInt8] = []
    private var drainScheduled = false

    private let deliver: @MainActor ([UInt8]) -> Void
    private let closed: @MainActor (TerminalExit) -> Void

    init(
        deliver: @escaping @MainActor ([UInt8]) -> Void,
        closed: @escaping @MainActor (TerminalExit) -> Void
    ) {
        self.deliver = deliver
        self.closed = closed
    }

    func onOutput(pid _: UInt32, data: String) {
        lock.lock()
        buffer.append(contentsOf: data.utf8)
        let shouldSchedule = !drainScheduled
        drainScheduled = true
        lock.unlock()
        guard shouldSchedule else { return }
        Task { @MainActor in self.drain() }
    }

    func onClosed(pid _: UInt32, exit: TerminalExit) {
        Task { @MainActor in
            // Flush whatever the shell printed on its way out before
            // reporting the exit.
            self.drain()
            self.closed(exit)
        }
    }

    @MainActor
    private func drain() {
        lock.lock()
        let bytes = buffer
        buffer.removeAll(keepingCapacity: true)
        drainScheduled = false
        lock.unlock()
        if !bytes.isEmpty { deliver(bytes) }
    }
}

/// The AppKit `TerminalView`, dressed to match the Tauri terminal: black
/// background, light-grey foreground, 12 pt medium monospace, default ANSI
/// palette. The delegate conformance is `@preconcurrency`: SwiftTerm's
/// protocol is not actor-isolated, but the view only calls it on the main
/// thread.
private struct TerminalHostView: NSViewRepresentable {
    let controller: TerminalController

    func makeCoordinator() -> Coordinator {
        Coordinator(controller: controller)
    }

    func makeNSView(context: Context) -> TerminalView {
        let view = HostedTerminalView(frame: .zero)
        view.onAttach = { [controller] in controller.viewDidAttach() }
        view.terminalDelegate = context.coordinator
        view.font = .monospacedSystemFont(ofSize: 12, weight: .medium)
        view.nativeBackgroundColor = .black
        view.nativeForegroundColor = NSColor(srgbRed: 0.898, green: 0.898, blue: 0.898, alpha: 1)
        controller.terminalView = view
        return view
    }

    func updateNSView(_ nsView: TerminalView, context: Context) {}

    /// SwiftTerm's view plus the one hook it does not expose: notice of the
    /// moment AppKit puts it in a window, which is when a focus request made
    /// during the mount can finally be honoured.
    private final class HostedTerminalView: TerminalView {
        var onAttach: (() -> Void)?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window != nil { onAttach?() }
        }
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency TerminalViewDelegate {
        private let controller: TerminalController

        init(controller: TerminalController) {
            self.controller = controller
        }

        func send(source: TerminalView, data: ArraySlice<UInt8>) {
            controller.send(String(decoding: data, as: UTF8.self))
        }

        func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
            controller.resize(cols: newCols, rows: newRows)
        }

        func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
            guard let url = URL(string: link) else { return }
            NSWorkspace.shared.open(url)
        }

        /// OSC 52 — a program inside the terminal setting the clipboard.
        func clipboardCopy(source: TerminalView, content: Data) {
            guard let text = String(data: content, encoding: .utf8) else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        }

        func setTerminalTitle(source: TerminalView, title: String) {}
        func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
        func scrolled(source: TerminalView, position: Double) {}
        func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
    }
}
