import SwiftUI

/// The Settings window (⌘,), backed by the shared `config.toml` both clients
/// read — an edit here shows up in the Tauri client and vice versa.
///
/// Discrete controls (toggles, steppers, pickers) save through a short
/// debounce as they change; text fields commit when focus leaves them or on
/// Return — the standard macOS settings behaviour, in place of the Tauri
/// overlay's explicit Save/Cancel pair.
struct SettingsView: View {
    @State private var store = SettingsStore()
    @FocusState private var focusedField: TextInput?

    /// The free-text controls, tracked so leaving one commits it.
    private enum TextInput: Hashable {
        case scanPaths
        case aiModel
        case ollamaURL
    }

    var body: some View {
        Form {
            gitSection
            discoverySection
            terminalSection
            aiSection
            if let message = store.errorMessage {
                Section {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .frame(minHeight: 540)
        .task { await store.load() }
        .onChange(of: focusedField) { previous, _ in
            // Leaving any text field commits it; entering the first one
            // commits nothing (previous == nil).
            if previous != nil { store.scheduleSave() }
        }
        .onDisappear { store.flushPendingSave() }
    }

    private var gitSection: some View {
        Section {
            Toggle("Automatically fetch from remotes", isOn: saving($store.autoFetch))
            Stepper(
                value: saving($store.fetchIntervalSeconds),
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

    private var discoverySection: some View {
        Section {
            TextField(
                "Folders to scan",
                text: $store.scanPathsText,
                prompt: Text("~/Dev"),
                axis: .vertical
            )
            .lineLimit(3...8)
            .focused($focusedField, equals: .scanPaths)
            .onSubmit { store.scheduleSave() }
            Stepper(
                value: saving($store.scanDepth),
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
            Picker("Shell", selection: saving($store.shellSelection)) {
                Text(store.automaticShellLabel).tag("")
                ForEach(store.shells, id: \.id) { shell in
                    Text(shell.label).tag(shell.id)
                }
            }
        } header: {
            Text("Terminal")
        } footer: {
            Text("Applies to new terminal sessions.")
                .settingsFooter()
        }
    }

    private var aiSection: some View {
        Section {
            Picker("Provider", selection: saving($store.aiProvider)) {
                Text("Claude").tag("claude")
                Text("Ollama").tag("ollama")
            }
            TextField(
                "Model",
                text: $store.aiModel,
                prompt: Text(store.aiProvider == "ollama" ? "tavernari/git-commit-message:latest" : "sonnet")
            )
            .focused($focusedField, equals: .aiModel)
            .onSubmit { store.scheduleSave() }
            TextField(
                "Ollama server URL",
                text: $store.ollamaURL,
                prompt: Text("http://localhost:11434")
            )
            .focused($focusedField, equals: .ollamaURL)
            .onSubmit { store.scheduleSave() }
        } header: {
            Text("AI Commit Messages")
        } footer: {
            Text("Used by Generate in the commit composer. Leave the model empty for the provider's default.")
                .settingsFooter()
        }
    }

    /// A binding that also schedules a save when the control writes — how
    /// toggles, steppers, and pickers persist without a Save button. Only
    /// user interaction goes through the setter, so `load()` never saves.
    private func saving<Value>(_ binding: Binding<Value>) -> Binding<Value> {
        Binding(
            get: { binding.wrappedValue },
            set: { value in
                binding.wrappedValue = value
                store.scheduleSave()
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
