import Foundation

/// Observable state for the Settings window: the shared `config.toml` fields
/// the native client actually consumes, held as editable values while the
/// window is open.
///
/// Only fields with a native consumer get a control — auto-fetch cadence,
/// the diff rendering settings, repo discovery, the terminal shell, and the
/// AI knobs. The remaining fields cross the bridge untouched: `save()` loads
/// a fresh `Config`, overlays just the managed fields, and writes the whole
/// file back, so the other client's settings survive every save. Two are
/// exempt deliberately (FRONTEND.md §8): `theme` permanently, because the
/// native app follows the system appearance and a stored theme is a web-only
/// concept; `side_by_side_diff` until the split layout gets its own design
/// pass (tracked in ROADMAP).
///
/// Saves are debounced a moment and fired by the controls themselves (via
/// `scheduleSave()`), never by `load()` — so opening the window writes
/// nothing. Each successful save also reloads the shared `AppConfigStore`,
/// which is how an edit here reaches the open diff and the auto-fetch loop
/// without a restart.
@MainActor
@Observable
final class SettingsStore {
    /// Bounds shown by the controls and enforced on save, matching the Tauri
    /// form's HTML constraints — which that client never actually enforced.
    static let fetchIntervalRange = 5...3600
    static let scanDepthRange = 1...10
    static let tabSizeRange = 1...16

    private static let defaultOllamaURL = "http://localhost:11434"
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

    // AI
    var aiProvider = "claude"
    var aiModel = ""
    var ollamaURL = SettingsStore.defaultOllamaURL

    private(set) var isLoaded = false

    /// Set when a save fails; cleared by the next successful one.
    private(set) var errorMessage: String?

    /// The app-wide config owner, reloaded after every successful save so
    /// the change applies live in this process. Injected by `SettingsView`
    /// (environment values can't reach a store's init); a dependency, not
    /// window state, hence unobserved.
    @ObservationIgnored var configStore: AppConfigStore?

    private var pendingSave: Task<Void, Never>?

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
        aiProvider = config.aiProvider == "ollama" ? "ollama" : "claude"
        aiModel = config.aiModel ?? ""
        ollamaURL = config.ollamaServerUrl
        isLoaded = true
    }

    /// Coalesce rapid control changes (stepper clicks, toggles) into one
    /// write shortly after the last of them.
    func scheduleSave() {
        guard isLoaded else { return }
        pendingSave?.cancel()
        pendingSave = Task { [weak self] in
            try? await Task.sleep(for: Self.saveDebounce)
            guard !Task.isCancelled else { return }
            await self?.save()
        }
    }

    /// Write any edit that is still waiting on the debounce — the window
    /// closing must not swallow the last change.
    func flushPendingSave() {
        guard let pendingSave else { return }
        pendingSave.cancel()
        self.pendingSave = nil
        Task { await save() }
    }

    /// Load the file fresh, overlay the managed fields, and write it back —
    /// then reload the shared `AppConfigStore`, which is how the main window
    /// observes the change: the open diff re-keys and the auto-fetch loop
    /// re-arms within one interval, no restart.
    private func save() async {
        guard isLoaded else { return }
        do {
            let fresh = try await GitBridge.appConfig()
            try await GitBridge.saveAppConfig(applying(to: fresh))
            errorMessage = nil
            await configStore?.reload()
        } catch {
            errorMessage = error.displayMessage
        }
    }

    /// This window's fields over `fresh`, normalized: numbers clamped to
    /// their control bounds, an emptied model back to `nil` (Tauri persists
    /// `""` there — a bug not worth replicating), an emptied Ollama URL back
    /// to core's default so Generate can never be pointed at "".
    private func applying(to fresh: Config) -> Config {
        let interval = fetchIntervalSeconds.clamped(to: Self.fetchIntervalRange)
        let trimmedModel = aiModel.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedURL = ollamaURL.trimmingCharacters(in: .whitespacesAndNewlines)
        return Config(
            theme: fresh.theme,
            fetchIntervalMs: UInt32(interval * 1000),
            aiProvider: aiProvider == "ollama" ? "ollama" : "claude",
            aiModel: trimmedModel.isEmpty ? nil : trimmedModel,
            aiApiKey: fresh.aiApiKey,
            autoFetch: autoFetch,
            syntaxHighlighting: syntaxHighlighting,
            scanPaths: parsedScanPaths,
            scanDepth: UInt32(scanDepth.clamped(to: Self.scanDepthRange)),
            sideBySideDiff: fresh.sideBySideDiff,
            hideWhitespace: hideWhitespace,
            tabSize: UInt32(tabSize.clamped(to: Self.tabSizeRange)),
            claudeTimeoutSecs: fresh.claudeTimeoutSecs,
            ollamaServerUrl: trimmedURL.isEmpty ? Self.defaultOllamaURL : trimmedURL,
            terminalShell: shellSelection.isEmpty ? nil : shellSelection
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
