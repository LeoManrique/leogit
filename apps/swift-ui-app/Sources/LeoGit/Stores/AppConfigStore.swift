import Foundation

/// The single native owner of the shared `config.toml` — the Tauri client's
/// `$config` store, in both of its roles.
///
/// **One reader.** Everything that needs a setting observes this store instead
/// of re-reading the file, so "a setting changed" is one observation with one
/// staleness window, not N ad-hoc `GitBridge.appConfig()` calls with N of them.
///
/// **One writer.** Every surface that changes a setting patches through
/// `patch(_:)`, which queues writes rather than only relying on core's lock:
/// the lock keeps the file coherent but decides *nothing* about order, so two
/// patches in flight against one file are two patches whose winner is the
/// scheduler's. A patch names only the fields its surface owns, which is what
/// lets the Settings window, the diff header and the composer's provider
/// picker all write while the others stand open.
///
/// Reload sites, and why they suffice:
/// - launch (`LeoGitApp`) — the first read every consumer starts from;
/// - the Settings window opening (`SettingsStore.load`) — the other client may
///   have written the file since this process last looked at it;
/// - the activation resync (`ContentView.resyncOnActivate`) — edits made from
///   the *Tauri* client are picked up on return to the app, the same moment
///   everything else catches up.
///
/// A write needs no reload of its own: `patch_config` hands back the whole
/// normalized config, which is newer than anything a re-read could produce.
///
/// One consumer deliberately reads the file directly: a new terminal session
/// takes `terminal_shell` from `GitBridge.appConfig()` in
/// `TerminalController.start`. That setting's promise is "applies to new
/// terminal sessions", which a preference saved in the other client seconds
/// ago has to satisfy, and one read per spawned shell costs nothing.
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

    /// Discovery: the folders the repository walk starts from, as written —
    /// core expands `~` and fills in its own defaults for an empty list.
    var scanPaths: [String] { config?.scanPaths ?? [] }

    /// Discovery: how far under each scan path the walk descends.
    var scanDepth: UInt32 { config?.scanDepth ?? 3 }

    /// Diff: the split (side-by-side) layout rather than the unified one.
    ///
    /// Reads through `pendingSideBySide` so the toggle in the diff header
    /// answers on the click rather than after a file write, and so a *refused*
    /// write is an observable change back — the control is driven by this
    /// value, and a setter that left the store untouched would leave the
    /// segment the user pressed showing a layout that never took.
    var sideBySideDiff: Bool { pendingSideBySide ?? config?.sideBySideDiff ?? false }

    /// AI: the provider Generate runs against.
    ///
    /// Held here rather than in `CommitStore` because it has two controls —
    /// the composer's picker and the Settings window's — and two owners meant
    /// they could disagree while both were open. Shadowed like the layout, for
    /// the same reason: a picker must answer on the click.
    var aiProvider: String { pendingProvider ?? config?.aiProvider ?? "claude" }

    /// What the in-flight layout write is asking for. Cleared when it lands,
    /// success or not: on success `config` already says the same thing, and on
    /// failure the truth on disk is what the control must show.
    private var pendingSideBySide: Bool?

    /// The same shadow for the provider picker.
    private var pendingProvider: String?

    /// The write in flight, so the next one queues behind it.
    private var writes: Task<Config, Error>?

    /// Bumped when a write starts, so a read that overtook it can be dropped.
    private var writeGeneration = 0

    /// Apply a field-wise patch and publish the config core hands back.
    ///
    /// The returned config is normalized and authoritative — a caller can
    /// re-seed its control from it rather than from what it asked for, which
    /// is what makes core's clamp visible instead of silent.
    @discardableResult
    func patch(_ fields: ConfigPatch) async throws -> Config {
        try await enqueue(fields).value
    }

    /// Persist the diff layout, leaving every other setting untouched. Its
    /// failure is logged rather than raised: the segment putting itself back
    /// is the report, and there is no surface a layout write belongs in.
    func setSideBySideDiff(_ enabled: Bool) {
        guard enabled != sideBySideDiff else { return }
        let write = showWhileWriting(
            enabled, in: \.pendingSideBySide, as: ConfigPatch(sideBySideDiff: enabled))
        Task {
            if case let .failure(error) = await write.result {
                print("[config] could not save the diff layout: \(error.displayMessage)")
            }
        }
    }

    /// Persist the AI provider. `nil` when it is already selected and there is
    /// nothing to wait for; otherwise the write, so the surface that asked can
    /// say why it failed — the picker's failure is the user's to see.
    @discardableResult
    func setAIProvider(_ provider: String) -> Task<Void, Error>? {
        guard provider != aiProvider else { return nil }
        return showWhileWriting(
            provider, in: \.pendingProvider, as: ConfigPatch(aiProvider: provider))
    }

    /// Write one field while showing its new value immediately.
    ///
    /// **Synchronous up to the point the value is on screen and the write is
    /// queued**, on purpose: a control writes through a `Binding` and re-reads
    /// it in the same layout pass, and a `Task` does not start there — so a
    /// shadow set from inside one leaves the control the user just pressed
    /// reverting for a frame, defeating the shadow whose whole job is to answer
    /// on the click. Queueing has to happen here for the same reason
    /// `enqueue` is synchronous.
    ///
    /// The shadow is dropped when the write lands, success or not: on success
    /// `config` already says the same thing, and on failure the truth on disk
    /// is what the control must show.
    private func showWhileWriting<Value: Equatable>(
        _ value: Value,
        in shadow: ReferenceWritableKeyPath<AppConfigStore, Value?>,
        as fields: ConfigPatch
    ) -> Task<Void, Error> {
        self[keyPath: shadow] = value
        let write = enqueue(fields)
        return Task { @MainActor [weak self] in
            defer {
                // Only the write that set it may clear it: a later click has
                // already claimed the slot for the value it is still writing.
                if let self, self[keyPath: shadow] == value { self[keyPath: shadow] = nil }
            }
            _ = try await write.value
        }
    }

    /// Wait for the writes already queued to land.
    ///
    /// For a caller that is about to read the config file *through core* — a
    /// generate resolves its provider there — rather than through this store,
    /// and so would otherwise read the value a write in flight is replacing.
    func settle() async {
        _ = await writes?.result
    }

    /// Re-read the file. Failures keep the last good config — a transient
    /// read error must not snap live views back to the defaults — and are
    /// reported so a surface that opened on one can say so.
    ///
    /// A read that started before a write and resolved after it is discarded:
    /// it holds the file from *before* the write, and the write already
    /// published core's own normalized answer, which is newer. The two race
    /// because the read runs off the main actor, and this store writes as well
    /// as reads. Being overtaken is not a failure — the config in hand is the
    /// fresher one — so it reports success.
    @discardableResult
    func reload() async -> Bool {
        let started = writeGeneration
        guard let fresh = try? await GitBridge.appConfig() else { return false }
        guard started == writeGeneration else { return true }
        config = fresh
        return true
    }

    /// Queue one write behind those already in flight.
    ///
    /// Synchronous up to the point the chain is registered, on purpose: the
    /// order patches land in has to be the order they were *asked for*, and a
    /// caller that reached this through a `Task` of its own would be joining
    /// the queue in scheduler order instead. The generation is bumped here for
    /// the same reason — a `reload()` already awaiting must see that a write
    /// has started, not learn it a hop later.
    private func enqueue(_ fields: ConfigPatch) -> Task<Config, Error> {
        writeGeneration += 1
        let previous = writes
        let write = Task { @MainActor [weak self] in
            // A failure ahead must not strand the ones behind it, so the
            // result is awaited rather than the value.
            _ = await previous?.result
            let updated = try await GitBridge.patchAppConfig(fields)
            self?.config = updated
            return updated
        }
        writes = write
        return write
    }
}
