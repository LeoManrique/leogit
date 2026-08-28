import SwiftUI

/// "Create a repository here?" — what a `leogit <dir>` invocation raises when
/// the folder it names is not a repository yet.
///
/// The alternative was for the invocation to do nothing, which is the one
/// outcome a user who just typed a command cannot diagnose. Wording is the
/// Tauri dialog's, unchanged: the sentence about files staying where they are
/// is the whole reassurance the prompt exists to give.
///
/// A sheet rather than a `confirmationDialog` because `git init` can fail
/// (permissions, a file in the way) and FRONTEND §6.13 requires a refused
/// dialog to keep its context and state the refusal in place — a
/// confirmation dialog has nowhere to put one, and dismissing to a banner
/// would lose the folder the user was asked about.
struct InitRepoSheet: View {
    let path: String
    /// Called with the repository path core returns — the folder itself, or
    /// the enclosing repository if one appeared in the meantime.
    let onCreated: (String) -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var isCreating = false
    @State private var errorMessage: String?

    private var folderName: String {
        URL(fileURLWithPath: path, isDirectory: true).lastPathComponent
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create a repository here?")
                .font(.title3.weight(.semibold))

            VStack(alignment: .leading, spacing: 2) {
                Text(folderName)
                    .font(.callout.weight(.semibold))
                Text(path)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(3)
                    .truncationMode(.middle)
            }

            Text(
                "This folder isn’t a Git repository yet. Creating one leaves your files exactly where they are — nothing is committed until you commit it."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

            if let errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isCreating)
                Button(isCreating ? "Creating…" : "Create repository", action: create)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(isCreating)
            }
        }
        .padding(16)
        .frame(width: 420)
        .interactiveDismissDisabled(isCreating)
    }

    private func create() {
        guard !isCreating else { return }
        isCreating = true
        errorMessage = nil
        Task {
            defer { isCreating = false }
            do {
                let repoPath = try await GitBridge.initRepository(at: path)
                dismiss()
                onCreated(repoPath)
            } catch {
                // The sheet stays up naming the same folder, so a fixed
                // permission can be retried without re-running the command.
                errorMessage = error.displayMessage
            }
        }
    }
}
