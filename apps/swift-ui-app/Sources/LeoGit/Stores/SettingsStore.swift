import Foundation

/// Form state for the Settings window: the shared `config.toml` fields the
/// native client actually consumes, held as editable values while the window
/// is open.
///
/// Only fields with a native consumer get a control. Two are absent
/// deliberately: `theme`, because the native app follows the system appearance
/// and a stored theme is a web-only concept (FRONTEND.md §8); and
/// `side_by_side_diff`, which is written from the diff's own header in both
/// clients — the arrangement is a property of the diff you are reading, not a
/// preference you go and find. A third, `ai_provider`, has a control here but
/// no field: it is read and written straight through `AppConfigStore`, because
/// the composer's picker edits the same setting and two owners meant the two
/// pickers could disagree while both were on screen.
///
/// **Each control writes its own field and nothing else.** A patch that named
/// every field the window holds would post them as they looked when it
/// *opened*, so a `tab_size` the Tauri client wrote meanwhile was reverted by
/// an unrelated toggle here — and naming `side_by_side_diff` would revert
/// whatever the diff header last chose. `patch_config` cannot protect against
/// that; a field-wise writer only helps a caller that names fields field-wise.
///
/// Two rules follow from having no Save button, and both are load-bearing:
/// - **A write that fails puts its control back.** With nothing pending, a
///   control still showing the rejected value would be claiming a setting that
///   isn't on disk.
/// - **Every write re-seeds its control from the config core handed back**, so
///   a clamp or a trim is visible rather than silent.
///
/// Writes are debounced a moment *per field* and fired by the controls
/// themselves, never by `load()` — so opening the window writes nothing.
@MainActor
@Observable
final class SettingsStore {
    /// The settings this window owns, one per control.
    ///
    /// The unit of everything here: a patch names one of these, a debounce is
    /// keyed by one, and "has this changed since we last looked at the file?"
    /// is asked about one. Scan paths are on the list even though they commit
    /// through Edit ▸ Done rather than a debounce — Done writes through the
    /// same path.
    enum Field: CaseIterable, Hashable {
        case autoFetch
        case fetchInterval
        case hideWhitespace
        case syntaxHighlighting
        case tabSize
        case scanPaths
        case scanDepth
        case shell
        case claudeModel
        case claudeTimeout
        case ollamaModel
        case ollamaURL
        case ollamaTimeout
    }

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
    static let aiTimeoutRange = Int(bounds.aiTimeoutSecs.min)...Int(bounds.aiTimeoutSecs.max)

    private static let writeDebounce = Duration.milliseconds(300)

    // Git
    var autoFetch = true
    var fetchIntervalSeconds = 30

    // Diff
    var hideWhitespace = false
    var syntaxHighlighting = true
    var tabSize = 4

    // Repository discovery
    var scanDepth = 3
    private(set) var scanPathsText = ""

    // Terminal — a shell id, or "" for Automatic (`terminal_shell: None`).
    var shellSelection = ""
    private(set) var shells: [ShellOption] = []

    // AI. Each provider keeps its own model and timeout: one shared model
    // meant setting `sonnet` and switching to Ollama produced a request
    // against a model that doesn't exist there.
    var claudeModel = ""
    var claudeTimeoutSeconds = 120
    var ollamaModel = ""
    var ollamaURL = ""
    var ollamaTimeoutSeconds = 120

    /// Set when a write fails or the config can't be read; cleared by the next
    /// successful write.
    private(set) var errorMessage: String?

    /// The app-wide config owner: this window's only reader and only writer.
    /// A dependency rather than window state, hence unobserved, and handed in
    /// at construction so there is no moment where it hasn't arrived yet.
    @ObservationIgnored private let configStore: AppConfigStore

    init(config: AppConfigStore) {
        configStore = config
    }

    /// Whether there is a configuration to edit at all.
    ///
    /// False renders the failure and *no controls*: a form showing struct
    /// defaults behind "Could not read the configuration file." is offering to
    /// edit settings that aren't the user's, and every keystroke in it is
    /// silently inert.
    var isLoaded: Bool { configStore.config != nil }

    /// The provider both pickers show — this window's and the composer's.
    var aiProvider: String { configStore.aiProvider }

    /// What each field looked like on disk the last time it was seeded from
    /// there, recorded as the patch that field would send. Comparing the two
    /// answers "has this control changed since we last looked at the file?",
    /// which is the one question behind three behaviours: whether to write at
    /// all, what to flush on close, and which controls an edit made elsewhere
    /// may repaint.
    ///
    /// Recorded *through* `patch(for:)` rather than from the config, so a
    /// value the form normalizes on its way out — a trimmed model name, a
    /// re-parsed path list — reads as unchanged instead of writing itself back
    /// on every visit.
    private var seeded: [Field: ConfigPatch] = [:]

    /// The debounce counting down for each field, kept only so a newer change
    /// can cancel it. A finished task left in the table is harmless — the
    /// question of whether a field still owes a write is `isDirty`, not this.
    private var pendingWrites: [Field: Task<Void, Never>] = [:]

    /// The picker's first row: what "Automatic" resolves to on this machine.
    var automaticShellLabel: String {
        shells.first.map { "Automatic (\($0.label))" } ?? "Automatic"
    }

    // MARK: Scan paths — locked until Edit

    /// Whether the folder list is being edited. Read-only otherwise, the macOS
    /// list-editor pattern: this is the one setting that decides which
    /// repositories the app can see at all, and a half-typed line is a
    /// different folder rather than a shorter one.
    private(set) var isEditingScanPaths = false

    /// The list mid-edit. Nothing is written until Done, so leaving the window
    /// by any route discards it.
    var scanPathsDraft = ""

    /// What the field shows: the draft while editing, the setting otherwise.
    var displayedScanPaths: String { isEditingScanPaths ? scanPathsDraft : scanPathsText }

    /// Edit ▸ Done. Done parses, applies, and locks again.
    func toggleScanPathsEditing() {
        guard isEditingScanPaths else {
            scanPathsDraft = scanPathsText
            isEditingScanPaths = true
            return
        }
        isEditingScanPaths = false
        scanPathsText = scanPathsDraft
        Task { await write(.scanPaths) }
    }

    // MARK: Loading

    /// Populate every control from the shared config and the probed shells.
    ///
    /// Re-reads the file rather than trusting what the store already holds:
    /// the other client may have written it since this process last looked.
    func load() async {
        async let probed = GitBridge.shellOptions()
        let read = await configStore.reload()
        // Before seeding: a stored shell id is only honoured when the shell it
        // names is one this machine actually has.
        shells = await probed
        errorMessage = read ? nil : "Could not read the configuration file."
        guard let config = configStore.config else { return }
        for field in Field.allCases { seed(field, from: config) }
    }

    /// Repaint from a config this window didn't write — the other client's
    /// save, or the composer's provider picker.
    ///
    /// A field with an edit of its own outstanding keeps it: the user's
    /// uncommitted change is newer than the file, and is what its own write
    /// will carry.
    func adoptExternalChanges() {
        guard let config = configStore.config else { return }
        for field in Field.allCases where !isDirty(field) {
            seed(field, from: config)
        }
    }

    // MARK: Writing

    /// Coalesce rapid changes to one control (stepper clicks, a held key) into
    /// one write shortly after the last of them. Per field, so a stepper still
    /// counting down never delays the toggle beside it.
    func scheduleWrite(_ field: Field) {
        guard isLoaded else { return }
        pendingWrites[field]?.cancel()
        pendingWrites[field] = Task { [weak self] in
            try? await Task.sleep(for: Self.writeDebounce)
            guard !Task.isCancelled else { return }
            await self?.write(field)
        }
    }

    /// Write anything the window is closing on top of.
    ///
    /// One rule covers both cases, and only the first used to be handled: a
    /// debounce still counting down, and — the one that lost work — a text
    /// field the user typed into and never left. Text fields commit on focus
    /// change or Return, and ⌘W is neither, so the typed model or Ollama URL
    /// was dropped with no feedback in the one surface whose whole premise is
    /// "you never press Save". Asking each field whether it still differs from
    /// disk covers both without rewriting the file on every visit.
    func flush() {
        for task in pendingWrites.values { task.cancel() }
        pendingWrites.removeAll()
        let owing = Field.allCases.filter(isDirty)
        guard !owing.isEmpty else { return }
        // Sequential: the writes queue behind each other in `AppConfigStore`
        // anyway, and awaiting them in order keeps the error line describing
        // the last one rather than whichever finished last.
        Task { for field in owing { await write(field) } }
    }

    /// Patch one field, then put whatever the file now says back into its
    /// control.
    private func write(_ field: Field) async {
        guard isDirty(field) else { return }
        let outgoing = patch(for: field)
        do {
            try await configStore.patch(outgoing)
            errorMessage = nil
            // Discovery hangs off the settings that change what it walks, not
            // off this window closing: the repo list is what a scan-path edit
            // is *for*, and the screen most likely to be showing it — the
            // picker, with its "Choose Folders to Search" action — has no
            // switcher to re-open and would otherwise sit on "No repositories
            // found" until the app was restarted.
            if field == .scanPaths || field == .scanDepth {
                NotificationCenter.default.post(name: .leogitScanPathsChanged, object: nil)
            }
        } catch {
            errorMessage = error.displayMessage
        }
        // Core's clamp on success, the untouched previous value on refusal —
        // either way the file is what the control must show. Unless the form
        // moved while the write was in flight, in which case the user's edit is
        // newer than this answer and the write it schedules will carry it.
        guard patch(for: field) == outgoing, let config = configStore.config else { return }
        seed(field, from: config)
    }

    /// Persist the provider through its owner, which both pickers read.
    ///
    /// Synchronous, so the picker shows the choice in the same layout pass it
    /// was made in; only the reporting waits.
    func setAIProvider(_ provider: String) {
        guard let write = configStore.setAIProvider(provider) else { return }
        Task {
            do {
                try await write.value
                errorMessage = nil
            } catch {
                errorMessage = error.displayMessage
            }
        }
    }

    // MARK: Fields

    /// Whether this control has changed since the file was last read into it.
    /// A field never seeded owes nothing — the window is still loading.
    private func isDirty(_ field: Field) -> Bool {
        guard let recorded = seeded[field] else { return false }
        return patch(for: field) != recorded
    }

    /// This control's value as a patch naming only its own field.
    ///
    /// Numbers are clamped to the control bounds here so the form matches what
    /// lands; blanks travel as `""`, which core reads as "no value" — the rule
    /// that keeps an emptied model box from becoming `--model ""`.
    private func patch(for field: Field) -> ConfigPatch {
        switch field {
        case .autoFetch:
            ConfigPatch(autoFetch: autoFetch)
        case .fetchInterval:
            ConfigPatch(
                fetchIntervalMs: UInt32(
                    fetchIntervalSeconds.clamped(to: Self.fetchIntervalRange) * 1000))
        case .hideWhitespace:
            ConfigPatch(hideWhitespace: hideWhitespace)
        case .syntaxHighlighting:
            ConfigPatch(syntaxHighlighting: syntaxHighlighting)
        case .tabSize:
            ConfigPatch(tabSize: UInt32(tabSize.clamped(to: Self.tabSizeRange)))
        case .scanPaths:
            ConfigPatch(scanPaths: parsedScanPaths)
        case .scanDepth:
            ConfigPatch(scanDepth: UInt32(scanDepth.clamped(to: Self.scanDepthRange)))
        case .shell:
            ConfigPatch(terminalShell: shellSelection)
        case .claudeModel:
            ConfigPatch(claudeModel: claudeModel.cleaned)
        case .claudeTimeout:
            ConfigPatch(
                claudeTimeoutSecs: UInt32(claudeTimeoutSeconds.clamped(to: Self.aiTimeoutRange)))
        case .ollamaModel:
            ConfigPatch(ollamaModel: ollamaModel.cleaned)
        case .ollamaURL:
            ConfigPatch(ollamaServerUrl: ollamaURL.cleaned)
        case .ollamaTimeout:
            ConfigPatch(
                ollamaTimeoutSecs: UInt32(ollamaTimeoutSeconds.clamped(to: Self.aiTimeoutRange)))
        }
    }

    /// Put the file's value for one field into its control, and record what
    /// that field now owes.
    private func seed(_ field: Field, from config: Config) {
        switch field {
        case .autoFetch:
            autoFetch = config.autoFetch
        case .fetchInterval:
            fetchIntervalSeconds = max(Int(config.fetchIntervalMs) / 1000, 1)
        case .hideWhitespace:
            hideWhitespace = config.hideWhitespace
        case .syntaxHighlighting:
            syntaxHighlighting = config.syntaxHighlighting
        case .tabSize:
            tabSize = Int(config.tabSize)
        case .scanPaths:
            scanPathsText = config.scanPaths.joined(separator: "\n")
        case .scanDepth:
            scanDepth = Int(config.scanDepth)
        case .shell:
            // A stored id whose shell is gone renders as Automatic, exactly
            // like the Tauri picker — core would fall back to the best shell
            // on this machine anyway.
            let stored = config.terminalShell
            shellSelection = shells.contains { $0.id == stored } ? (stored ?? "") : ""
        case .claudeModel:
            claudeModel = config.claude.model ?? ""
        case .claudeTimeout:
            claudeTimeoutSeconds = Int(config.claude.timeoutSecs)
        case .ollamaModel:
            ollamaModel = config.ollama.model ?? ""
        case .ollamaURL:
            ollamaURL = config.ollama.serverUrl
        case .ollamaTimeout:
            ollamaTimeoutSeconds = Int(config.ollama.timeoutSecs)
        }
        seeded[field] = patch(for: field)
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

extension String {
    /// A free-text setting on its way to the config file: surrounding
    /// whitespace is never part of a model name or a server URL, and leaving
    /// it in would make the same value read as changed on every visit.
    fileprivate var cleaned: String {
        trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
