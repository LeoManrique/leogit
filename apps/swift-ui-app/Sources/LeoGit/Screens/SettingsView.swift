import SwiftUI

/// The Settings window (⌘,), backed by the shared `config.toml` both clients
/// read — an edit here shows up in the Tauri client and vice versa.
///
/// Instant-apply, like macOS settings generally and like the Tauri overlay:
/// discrete controls (toggles, steppers, pickers) write through a short
/// debounce as they change, text fields when focus leaves them or on Return.
/// Each control writes its own field and nothing else, so a setting the other
/// client — or the diff header, or the composer's provider picker — changes
/// while this window stands open survives. Scan paths are the one field
/// outside instant-apply, behind an Edit ▸ Done cycle.
struct SettingsView: View {
    /// The shared config owner — this window's only reader and only writer.
    /// A parameter rather than an environment value because the store below is
    /// built with it in `init`.
    private let appConfig: AppConfigStore

    @State private var store: SettingsStore
    @FocusState private var focusedField: SettingsStore.Field?

    init(appConfig: AppConfigStore) {
        self.appConfig = appConfig
        _store = State(initialValue: SettingsStore(config: appConfig))
    }

    var body: some View {
        Form {
            if let message = store.errorMessage {
                Section {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }
            }
            // No configuration, no controls. Rendering the struct defaults
            // behind the failure offered settings that were never the user's,
            // editable and silently inert.
            if store.isLoaded {
                gitSection
                diffSection
                discoverySection
                terminalSection
                aiSection
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .frame(minHeight: 540)
        .task { await store.load() }
        // An edit made anywhere else — the other client, the composer's
        // provider picker — repaints the controls that aren't mid-change.
        .onChange(of: appConfig.config) { _, _ in store.adoptExternalChanges() }
        .onChange(of: focusedField) { previous, _ in
            // Leaving a text field commits it; entering the first one commits
            // nothing (previous == nil).
            if let previous { store.scheduleWrite(previous) }
        }
        .onDisappear { store.flush() }
    }

    private var gitSection: some View {
        Section {
            Toggle("Automatically fetch from remotes", isOn: saving($store.autoFetch, .autoFetch))
            Stepper(
                value: saving($store.fetchIntervalSeconds, .fetchInterval),
                in: SettingsStore.fetchIntervalRange,
                step: 5
            ) {
                LabeledContent("Fetch interval", value: "\(store.fetchIntervalSeconds) s")
            }
            .disabled(!store.autoFetch)
        } header: {
            Text("Git")
        } footer: {
            Text("Applies to the open repository within one interval — no restart needed.")
                .settingsFooter()
        }
    }

    private var diffSection: some View {
        Section {
            Toggle("Hide whitespace changes", isOn: saving($store.hideWhitespace, .hideWhitespace))
            Toggle(
                "Syntax highlighting",
                isOn: saving($store.syntaxHighlighting, .syntaxHighlighting))
            Stepper(
                value: saving($store.tabSize, .tabSize),
                in: SettingsStore.tabSizeRange
            ) {
                LabeledContent("Tab size", value: "\(store.tabSize)")
            }
        } header: {
            Text("Diff")
        } footer: {
            Text("Applies to the open diff immediately.")
                .settingsFooter()
        }
    }

    private var discoverySection: some View {
        Section {
            // Locked until Edit, the macOS list-editor pattern. Monospaced
            // because these are paths: a stray space or a doubled slash is
            // only visible in a fixed-width face.
            TextField(
                "Folders to scan",
                text: Binding(
                    get: { store.displayedScanPaths },
                    set: { store.scanPathsDraft = $0 }
                ),
                prompt: Text("~/Dev"),
                axis: .vertical
            )
            .lineLimit(3...8)
            .monospaced()
            .disabled(!store.isEditingScanPaths)
            LabeledContent("") {
                Button(store.isEditingScanPaths ? "Done" : "Edit") {
                    store.toggleScanPathsEditing()
                }
            }
            Stepper(
                value: saving($store.scanDepth, .scanDepth),
                in: SettingsStore.scanDepthRange
            ) {
                LabeledContent("Scan depth", value: "\(store.scanDepth)")
            }
        } header: {
            Text("Repository Discovery")
        } footer: {
            Text(
                "One folder per line (~ allowed). The repository switcher searches these for git repositories."
            )
            .settingsFooter()
        }
    }

    private var terminalSection: some View {
        Section {
            Picker("Shell", selection: saving($store.shellSelection, .shell)) {
                Text(store.automaticShellLabel).tag("")
                ForEach(store.shells, id: \.id) { shell in
                    Text(shell.label).tag(shell.id)
                }
            }
        } header: {
            Text("Terminal")
        } footer: {
            Text("Only shells found on this machine are listed. Applies to new terminal sessions.")
                .settingsFooter()
        }
    }

    private var aiSection: some View {
        Section {
            // The provider is the one control here that doesn't own its field:
            // the composer has a picker for the same setting, so both read and
            // write the shared owner and can't drift apart while both are open.
            Picker(
                "Provider",
                selection: Binding(
                    get: { store.aiProvider },
                    set: { store.setAIProvider($0) }
                )
            ) {
                Text("Claude").tag("claude")
                Text("Ollama").tag("ollama")
            }
            // One model and one timeout per provider: a single shared model
            // meant a model set for Claude was handed to Ollama, which has
            // never heard of it, so Generate failed with nothing on screen
            // explaining why.
            if store.aiProvider == "ollama" {
                TextField(
                    "Model",
                    text: $store.ollamaModel,
                    prompt: Text("tavernari/git-commit-message:latest")
                )
                .focused($focusedField, equals: .ollamaModel)
                .onSubmit { store.scheduleWrite(.ollamaModel) }
                TextField(
                    "Ollama server URL",
                    text: $store.ollamaURL,
                    prompt: Text("http://localhost:11434")
                )
                .focused($focusedField, equals: .ollamaURL)
                .onSubmit { store.scheduleWrite(.ollamaURL) }
                timeoutStepper(
                    value: saving($store.ollamaTimeoutSeconds, .ollamaTimeout),
                    seconds: store.ollamaTimeoutSeconds
                )
            } else {
                TextField("Model", text: $store.claudeModel, prompt: Text("sonnet"))
                    .focused($focusedField, equals: .claudeModel)
                    .onSubmit { store.scheduleWrite(.claudeModel) }
                timeoutStepper(
                    value: saving($store.claudeTimeoutSeconds, .claudeTimeout),
                    seconds: store.claudeTimeoutSeconds
                )
            }
        } header: {
            Text("AI Commit Messages")
        } footer: {
            Text(
                "Used by Generate in the commit composer. Each provider keeps its own model and timeout; leave the model empty for that provider's default."
            )
            .settingsFooter()
        }
    }

    /// The generate timeout, identical for both providers — one control, two
    /// bindings, rather than the same six lines written twice.
    private func timeoutStepper(value: Binding<Int>, seconds: Int) -> some View {
        Stepper(value: value, in: SettingsStore.aiTimeoutRange, step: 10) {
            LabeledContent("Timeout", value: "\(seconds) s")
        }
    }

    /// A binding that also schedules `field`'s write when the control changes
    /// — how toggles, steppers, and pickers persist without a Save button.
    /// Only user interaction goes through the setter, so `load()` never writes.
    private func saving<Value>(_ binding: Binding<Value>, _ field: SettingsStore.Field) -> Binding<
        Value
    > {
        Binding(
            get: { binding.wrappedValue },
            set: { value in
                binding.wrappedValue = value
                store.scheduleWrite(field)
            }
        )
    }
}

extension Text {
    /// Shared style for the section footers' explanatory lines.
    fileprivate func settingsFooter() -> some View {
        font(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
