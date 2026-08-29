import Foundation

/// The single native owner of the shared `config.toml` contents — the Tauri
/// client's `$config` store. Everything outside the Settings window that
/// needs a setting observes this store instead of re-reading the file, so "a
/// setting changed" is one observation with one staleness window, not N
/// ad-hoc `GitBridge.appConfig()` calls with N of them.
///
/// Reload sites, and why they suffice:
/// - launch (`LeoGitApp`) — the first read every consumer starts from;
/// - every Settings save (`SettingsStore.save`) — an edit made *in this
///   process* is visible the moment it lands on disk, which is what re-keys
///   an open diff and re-arms the auto-fetch interval live;
/// - the activation resync (`ContentView.resyncOnActivate`) — edits made
///   from the *Tauri* client are picked up on return to the app, the same
///   moment everything else catches up.
///
/// The derived accessors carry the Tauri client's fallback defaults
/// (`$config?.x ?? default` in `MainLayout.svelte`), so the moments before
/// the launch read lands render identically in both clients.
@MainActor
@Observable
final class AppConfigStore {
    private(set) var config: Config?

    /// Diff: suppress whitespace-only changes (`git diff -w`).
    var hideWhitespace: Bool { config?.hideWhitespace ?? false }

    /// Diff: run the syntax-colour phase.
    var syntaxHighlighting: Bool { config?.syntaxHighlighting ?? true }

    /// Diff: tab stop width, in columns.
    var tabSize: Int { config.map { Int($0.tabSize) } ?? 4 }

    /// Diff: the split (side-by-side) layout rather than the unified one.
    ///
    /// Reads through `pendingSideBySide` so the toggle in the diff header
    /// answers on the click rather than after a file write, and so a *refused*
    /// write is an observable change back — the control is driven by this
    /// value, and a setter that left the store untouched would leave the
    /// segment the user pressed showing a layout that never took.
    var sideBySideDiff: Bool { pendingSideBySide ?? config?.sideBySideDiff ?? false }

    /// What the in-flight layout write is asking for. Cleared when it lands,
    /// success or not: on success `config` already says the same thing, and on
    /// failure the truth on disk is what the control must show.
    private var pendingSideBySide: Bool?

    /// Persist the diff layout, leaving every other setting untouched.
    ///
    /// The one writer outside the Settings window, and it patches the single
    /// field it owns — the same discipline every control in that window
    /// follows, which is what lets this write while that window stands open.
    /// The returned config is normalized and authoritative, so it replaces the
    /// held one rather than being merged into it.
    ///
    /// **Synchronous on purpose.** A control writes through a `Binding` and
    /// re-reads it in the same layout pass; a `Task` does not start there, so
    /// deferring the pending value would leave the segment snapping back for a
    /// frame — defeating the shadow whose entire job is to answer on the click.
    ///
    /// Writes are **chained**, not fired in parallel: a double-click would
    /// otherwise put two patches in flight against one file, where core's lock
    /// decides the order and the loser is what persists.
    func setSideBySideDiff(_ enabled: Bool) {
        guard enabled != sideBySideDiff else { return }
        pendingSideBySide = enabled
        writeGeneration += 1
        let previous = layoutWrite
        layoutWrite = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            do {
                config = try await GitBridge.patchAppConfig(ConfigPatch(sideBySideDiff: enabled))
            } catch {
                print("[config] could not save the diff layout: \(error.displayMessage)")
            }
            // Only the write that set it may clear it: a later click has
            // already claimed the slot for the value it is still writing.
            if pendingSideBySide == enabled { pendingSideBySide = nil }
        }
    }

    /// The layout write in flight, so the next one queues behind it.
    private var layoutWrite: Task<Void, Never>?

    /// Bumped when a write starts, so a read that overtook it can be dropped.
    private var writeGeneration = 0

    /// Re-read the file. Failures keep the last good config — a transient
    /// read error must not snap live views back to the defaults.
    ///
    /// A read that started before a write and resolved after it is discarded:
    /// it holds the file from *before* the write, and the write already
    /// published core's own normalized answer, which is newer. The two race
    /// because the read runs off the main actor, and this store now writes as
    /// well as reads.
    func reload() async {
        let started = writeGeneration
        if let fresh = try? await GitBridge.appConfig() {
            guard started == writeGeneration else { return }
            config = fresh
        }
    }
}
