import SwiftUI

/// The Publish sheet — the native counterpart of the Tauri
/// `PublishRepository` dialog: name the GitHub repository (seeded with the
/// folder name), optionally describe it, choose visibility, and let
/// `gh repo create` do create + wire `origin` + push in one shot.
///
/// On failure the sheet stays open with every field preserved, the error
/// inline — correct the name and retry without retyping (the Tauri dialog
/// survives its error modal the same way). There is no cancel once
/// publishing starts, so every exit is disabled while the operation runs.
struct PublishSheet: View {
    let store: SyncStore
    let repoPath: String
    /// Called after a successful publish — the status refresh that flips
    /// the toolbar from Publish to Push + Pull.
    let onPublished: () async -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var name: String
    @State private var repoDescription = ""
    /// Private by default, matching the Tauri dialog (and GitHub Desktop).
    @State private var isPrivate = true
    @State private var errorMessage: String?
    @FocusState private var isNameFocused: Bool

    init(store: SyncStore, repoPath: String, onPublished: @escaping () async -> Void) {
        self.store = store
        self.repoPath = repoPath
        self.onPublished = onPublished
        // One-shot seed, like the Tauri dialog's `defaultName`: for a
        // remote-less repo that is always the on-disk folder name.
        _name = State(initialValue: URL(fileURLWithPath: repoPath).lastPathComponent)
    }

    private var isPublishing: Bool { store.activeOperation == .publish }

    private var canPublish: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty && !isPublishing
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Publish Repository")
                .font(.title3.weight(.semibold))

            Group {
                LabeledContent("Name") {
                    TextField("Name", text: $name)
                        .textFieldStyle(.roundedBorder)
                        .labelsHidden()
                        .autocorrectionDisabled()
                        .focused($isNameFocused)
                }
                .labeledContentStyle(.vertical)

                LabeledContent("Description") {
                    TextField("Description", text: $repoDescription, prompt: Text("Optional"))
                        .textFieldStyle(.roundedBorder)
                        .labelsHidden()
                }
                .labeledContentStyle(.vertical)

                Toggle("Keep this code private", isOn: $isPrivate)
            }
            .disabled(isPublishing)

            Text("Publishes to github.com under your gh account — use owner/name to target an organization.")
                .font(.caption)
                .foregroundStyle(.secondary)

            if isPublishing {
                VStack(alignment: .leading, spacing: 4) {
                    ProgressView()
                        .progressViewStyle(.linear)
                    Text("Creating the GitHub repository and pushing…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if let errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isPublishing)
                Button(isPublishing ? "Publishing…" : "Publish Repository", action: publish)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canPublish)
            }
        }
        .padding(16)
        .frame(width: 420)
        .interactiveDismissDisabled(isPublishing)
        .onAppear { isNameFocused = true }
    }

    private func publish() {
        guard canPublish else { return }
        errorMessage = nil
        Task {
            let outcome = await store.publish(
                repoPath: repoPath,
                name: name.trimmingCharacters(in: .whitespaces),
                description: repoDescription.trimmingCharacters(in: .whitespaces),
                isPrivate: isPrivate
            )
            switch outcome {
            case .succeeded:
                dismiss()
                await onPublished()
            case let .failed(message):
                // The sheet stays up with the fields intact for a retry.
                errorMessage = message
            // Nothing was published, so the sheet stays exactly as it is — it
            // used to dismiss here and report a repository that never existed.
            case .refusedBusy:
                break
            }
        }
    }
}
