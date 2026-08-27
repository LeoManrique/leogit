import Foundation

/// Observable state for the Settings window: the shared `config.toml` fields
/// the native client actually consumes, held as editable values while the
/// window is open.
///
/// Only fields with a native consumer get a control — auto-fetch cadence,
/// the diff rendering settings, repo discovery, the terminal shell, and the
/// AI knobs. `save()` sends a *patch* naming exactly those, so every field
/// this window doesn't manage survives untouched by construction rather than
/// by remembering to reload first. Two are exempt deliberately
/// (FRONTEND.md §8): `theme` permanently, because the native app follows the
/// system appearance and a stored theme is a web-only concept;
/// `side_by_side_diff` until the split layout gets its own design pass
/// (tracked in ROADMAP).
///
/// Saves are debounced a moment and fired by the controls themselves (via
/// `scheduleSave()`), never by `load()` — so opening the window writes
/// nothing. Each successful save also reloads the shared `AppConfigStore`,
/// which is how an edit here reaches the open diff and the auto-fetch loop
/// without a restart.
@MainActor
@Observable
final class SettingsStore {
    /// The ranges every writer enforces, read from core rather than restated
    /// here — a control that offered a value the writer then clamped away was
    /// the exact symptom of three copies of these numbers in two units.
    private static let bounds = configBounds()

    /// Seconds, because that is what the control shows; milliseconds stay on
    /// the wire. Rounded outward so the displayed range never excludes a value
    /// core would accept.
    static let fetchIntervalRange =
        Int(bounds.fetchIntervalMs.min / 1000)...Int(bounds.fetchIntervalMs.max / 1000)
    static let scanDepthRange = Int(bounds.scanDepth.min)...Int(bounds.scanDepth.max)
    static let tabSizeRange = Int(bounds.tabSize.min)...Int(bounds.tabSize.max)

    private static let saveDebounce = Duration.milliseconds(300)

    // Git
    var autoFetch = true
    var fetchIntervalSeconds = 30

    // Diff
    var hideWhitespace = false
    var syntaxHighlighting = true
    var tabSize = 4

    // Repository discovery
    var scanDepth = 3
    var scanPathsText = ""

    // Terminal — a shell id, or "" for Automatic (`terminal_shell: None`).
    var shellSelection = ""
    private(set) var shells: [ShellOption] = []

    // AI. Each provider keeps its own model: one shared field meant setting
    // `sonnet` and switching to Ollama produced a request against a model
    // that doesn't exist there.
    var aiProvider = "claude"
    var claudeModel = ""
    var ollamaModel = ""
    var ollamaURL = ""


    private(set) var isLoaded = false

    /// Set when a save fails; cleared by the next successful one.
    private(set) var errorMessage: String?

    /// The app-wide config owner, reloaded after every successful save so
    /// the change applies live in this process. Injected by `SettingsView`
    /// (environment values can't reach a store's init); a dependency, not
    /// window state, hence unobserved.
    @ObservationIgnored var configStore: AppConfigStore?

    private var pendingSave: Task<Void, Never>?

    /// What this window's fields amounted to at the last load or save.
    /// `flushPendingSave()` compares the current fields against it to answer
    /// "has anything changed since then?", which is how closing the window can
    /// commit a text field that never scheduled a save without rewriting the
    /// file on every visit.
    private var lastPersisted: ConfigPatch?

    /// Ordinal of the most recently scheduled debounce, so a completed one can
    /// clear `pendingSave` without clearing a newer one that replaced it. Left
    /// set, `pendingSave` would stay non-nil for the rest of the session and
    /// make every window close save unconditionally.
    private var saveGeneration = 0

    /// The picker's first row: what "Automatic" resolves to on this machine.
    var automaticShellLabel: String {
        shells.first.map { "Automatic (\($0.label))" } ?? "Automatic"
    }

    /// Populate every control from the shared config and the probed shells.
    func load() async {
        async let configResult = GitBridge.appConfig()
        shells = await GitBridge.shellOptions()

        guard let config = try? await configResult else {
            errorMessage = "Could not read the configuration file."
            return
        }
        autoFetch = config.autoFetch
        fetchIntervalSeconds = max(Int(config.fetchIntervalMs) / 1000, 1)
        hideWhitespace = config.hideWhitespace
        syntaxHighlighting = config.syntaxHighlighting
        tabSize = Int(config.tabSize)
        scanDepth = Int(config.scanDepth)
        scanPathsText = config.scanPaths.joined(separator: "\n")
        // A stored id whose shell is gone renders as Automatic, exactly like
        // the Tauri picker — core would fall back to the best shell anyway.
        if let stored = config.terminalShell, shells.contains(where: { $0.id == stored }) {
            shellSelection = stored
        } else {
            shellSelection = ""
        }
        aiProvider = config.aiProvider
        claudeModel = config.claude.model ?? ""
        ollamaModel = config.ollama.model ?? ""
        ollamaURL = config.ollama.serverUrl
        isLoaded = true
        // After `isLoaded`, so `currentPatch` describes fields that are live.
        lastPersisted = currentPatch
    }

    /// Coalesce rapid control changes (stepper clicks, toggles) into one
    /// write shortly after the last of them.
    func scheduleSave() {
        guard isLoaded else { return }
        saveGeneration += 1
        let generation = saveGeneration
        pendingSave?.cancel()
        pendingSave = Task { [weak self] in
            try? await Task.sleep(for: Self.saveDebounce)
            guard !Task.isCancelled else { return }
            await self?.save()
            self?.retireScheduledSave(generation)
        }
    }

    /// A debounce that ran to completion is no longer pending — unless another
    /// has replaced it since, which the generation check is what detects.
    private func retireScheduledSave(_ generation: Int) {
        guard generation == saveGeneration else { return }
        pendingSave = nil
    }

    /// Write anything the window is closing on top of.
    ///
    /// Two cases, and only the first used to be handled: a debounced save
    /// still counting down, and — the one that lost work — a text field the
    /// user typed into and never left. Text fields commit on focus change or
    /// Return, and ⌘W is neither, so the typed model, Ollama URL, or scan
    /// paths were dropped with no feedback in the one surface whose whole
    /// premise is "you never press Save". Comparing against `lastPersisted`
    /// keeps that from turning into a write on every open-and-close.
    func flushPendingSave() {
        if let pendingSave {
            pendingSave.cancel()
            self.pendingSave = nil
            Task { await save() }
            return
        }
        guard isLoaded, currentPatch != lastPersisted else { return }
        Task { await save() }
    }

    /// Patch the fields this window manages — then reload the shared
    /// `AppConfigStore`, which is how the main window observes the change: the
    /// open diff re-keys and the auto-fetch loop re-arms within one interval,
    /// no restart.
    ///
    /// A patch, not a whole-file write: the load-fresh-then-edit discipline
    /// that used to protect the other client's settings is now structural, so
    /// there is nothing left here to get wrong.
    private func save() async {
        guard isLoaded else { return }
        let patch = currentPatch
        do {
            try await GitBridge.patchAppConfig(patch)
            lastPersisted = patch
            errorMessage = nil
            await configStore?.reload()
        } catch {
            errorMessage = error.displayMessage
        }
    }

    /// This window's fields as a patch. Numbers are clamped to the control
    /// bounds here so the form matches what lands; blanks travel as `""`,
    /// which core reads as "no value" — the rule that keeps an emptied model
    /// box from becoming `--model ""`.
    private var currentPatch: ConfigPatch {
        // `theme`, `sideBySideDiff` and the two timeouts are absent on
        // purpose: this window has no control for them, and a patch that
        // named them would be claiming an opinion it doesn't have.
        ConfigPatch(
            fetchIntervalMs: UInt32(
                fetchIntervalSeconds.clamped(to: Self.fetchIntervalRange) * 1000),
            aiProvider: aiProvider,
            autoFetch: autoFetch,
            syntaxHighlighting: syntaxHighlighting,
            scanPaths: parsedScanPaths,
            scanDepth: UInt32(scanDepth.clamped(to: Self.scanDepthRange)),
            hideWhitespace: hideWhitespace,
            tabSize: UInt32(tabSize.clamped(to: Self.tabSizeRange)),
            terminalShell: shellSelection,
            claudeModel: claudeModel.trimmingCharacters(in: .whitespacesAndNewlines),
            ollamaModel: ollamaModel.trimmingCharacters(in: .whitespacesAndNewlines),
            ollamaServerUrl: ollamaURL.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    /// One folder per line, trimmed, blanks dropped — the Tauri textarea's
    /// parsing rule.
    private var parsedScanPaths: [String] {
        scanPathsText
            .split(separator: "\n", omittingEmptySubsequences: true)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }
}

extension Int {
    /// The nearest value inside `range`.
    func clamped(to range: ClosedRange<Int>) -> Int {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}
