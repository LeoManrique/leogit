import SwiftUI

/// The New Pull Request sheet: title (seeded from the head commit's
/// summary, the closest native equivalent of `gh pr create --fill`),
/// optional description and base branch, and a draft toggle.
///
/// Fire-and-find-out like publish: no pre-flight checks — an unpushed
/// branch, a missing GitHub remote, or an already-open PR all fail with
/// gh's own message, inline, with every field preserved for a retry.
struct CreatePullRequestSheet: View {
    let repoPath: String
    let currentBranch: String
    let seedTitle: String
    /// Called after a successful create, before dismissal settles — the
    /// caller reloads the open-PR list so the new PR appears in it.
    let onCreated: () async -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var title: String
    @State private var body_ = ""
    @State private var base = ""
    @State private var isDraft = false
    @State private var isCreating = false
    @State private var errorMessage: String?
    @FocusState private var isTitleFocused: Bool

    init(
        repoPath: String,
        currentBranch: String,
        seedTitle: String,
        onCreated: @escaping () async -> Void
    ) {
        self.repoPath = repoPath
        self.currentBranch = currentBranch
        self.seedTitle = seedTitle
        self.onCreated = onCreated
        _title = State(initialValue: seedTitle)
    }

    private var canCreate: Bool {
        !title.trimmingCharacters(in: .whitespaces).isEmpty && !isCreating
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("New Pull Request")
                .font(.title3.weight(.semibold))

            Group {
                LabeledContent("Title") {
                    TextField("Title", text: $title)
                        .textFieldStyle(.roundedBorder)
                        .labelsHidden()
                        .focused($isTitleFocused)
                }
                .labeledContentStyle(.vertical)

                LabeledContent("Description") {
                    TextField(
                        "Description",
                        text: $body_,
                        prompt: Text("Optional"),
                        axis: .vertical
                    )
                    .textFieldStyle(.roundedBorder)
                    .labelsHidden()
                    .lineLimit(3...8)
                }
                .labeledContentStyle(.vertical)

                LabeledContent("Base branch") {
                    TextField(
                        "Base branch",
                        text: $base,
                        prompt: Text("Repository default branch")
                    )
                    .textFieldStyle(.roundedBorder)
                    .labelsHidden()
                    .autocorrectionDisabled()
                }
                .labeledContentStyle(.vertical)

                Toggle("Create as draft", isOn: $isDraft)
            }
            .disabled(isCreating)

            Text("Opens a pull request for “\(currentBranch)”. The branch must already be pushed.")
                .font(.caption)
                .foregroundStyle(.secondary)

            if isCreating {
                ProgressView()
                    .progressViewStyle(.linear)
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
                    .disabled(isCreating)
                Button(isCreating ? "Creating…" : "Create Pull Request", action: create)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canCreate)
            }
        }
        .padding(16)
        .frame(width: 460)
        .interactiveDismissDisabled(isCreating)
        .onAppear { isTitleFocused = true }
    }

    private func create() {
        guard canCreate else { return }
        errorMessage = nil
        isCreating = true
        Task {
            defer { isCreating = false }
            do {
                _ = try await GitBridge.openPullRequest(
                    in: repoPath,
                    title: title.trimmingCharacters(in: .whitespaces),
                    body: body_,
                    base: base,
                    draft: isDraft
                )
                dismiss()
                await onCreated()
            } catch {
                // Fields survive; the sheet stays up for a corrected retry.
                errorMessage = error.displayMessage
            }
        }
    }
}
