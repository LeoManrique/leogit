import SwiftUI

/// The commit box under the changes list: summary, optional description, the
/// AI generate controls, and the Commit button. Pure presentation — all state
/// lives in `CommitStore`, and the actual commit and generation are the
/// owner's closures so it can gate them (e.g. behind the embedded-repo
/// confirmation) and pass the checked files.
struct CommitComposer: View {
    @Bindable var store: CommitStore

    /// How many files the next commit would contain, for the button label.
    let includedCount: Int

    let onSubmit: () -> Void

    /// Generate a commit message with AI from the checked files' diff.
    let onGenerate: () -> Void

    private var canCommit: Bool {
        !store.summary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && includedCount > 0
            && !isBusy
    }

    /// Committing and generating share one busy treatment: the fields lock
    /// (typing mid-generate would be overwritten by the result), and each
    /// button excludes the other — the store's guards enforce the same.
    private var isBusy: Bool {
        store.isCommitting || store.isGenerating
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Summary (required)", text: $store.summary)
                .textFieldStyle(.roundedBorder)
                .disabled(isBusy)

            TextField("Description", text: $store.details, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(2...6)
                .disabled(isBusy)

            if let errorMessage = store.errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(3)
                    .textSelection(.enabled)
            }

            HStack(alignment: .center, spacing: 8) {
                Picker("AI provider", selection: providerBinding) {
                    Text("Claude").tag("claude")
                    Text("Ollama").tag("ollama")
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .fixedSize()
                .disabled(isBusy)
                .help("AI provider used by Generate")

                Button(store.isGenerating ? "Generating…" : "Generate") {
                    onGenerate()
                }
                .keyboardShortcut("g", modifiers: .command)
                .disabled(isBusy || includedCount == 0)
                .help("Generate a commit message from the checked files (⌘G)")

                Spacer(minLength: 0)

                if isBusy {
                    ProgressView()
                        .controlSize(.small)
                }

                Button(action: onSubmit) {
                    Text(commitLabel)
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(!canCommit)
                .help("Commit the checked files (⌘↩)")
            }
        }
        .padding(10)
    }

    /// The picker writes through the store so a change persists to the
    /// shared config file (and reverts if that save fails).
    private var providerBinding: Binding<String> {
        Binding {
            store.aiProvider
        } set: { provider in
            Task { await store.setAIProvider(provider) }
        }
    }

    private var commitLabel: String {
        switch includedCount {
        case 0: "Commit"
        case 1: "Commit 1 File"
        default: "Commit \(includedCount) Files"
        }
    }
}
