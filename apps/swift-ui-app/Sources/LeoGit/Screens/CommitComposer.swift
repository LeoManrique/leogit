import SwiftUI

/// The commit box under the changes list: summary, optional description, and
/// the Commit button. Pure presentation — all state lives in `CommitStore`,
/// and the actual commit is the owner's `onSubmit` so it can gate it (e.g.
/// behind the embedded-repo confirmation) and refresh afterwards.
struct CommitComposer: View {
    @Bindable var store: CommitStore

    /// How many files the next commit would contain, for the button label.
    let includedCount: Int

    let onSubmit: () -> Void

    private var canCommit: Bool {
        !store.summary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && includedCount > 0
            && !store.isCommitting
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Summary (required)", text: $store.summary)
                .textFieldStyle(.roundedBorder)

            TextField("Description", text: $store.details, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(2...6)

            HStack(alignment: .center, spacing: 8) {
                if let errorMessage = store.errorMessage {
                    Text(errorMessage)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(3)
                        .textSelection(.enabled)
                }

                Spacer(minLength: 0)

                if store.isCommitting {
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

    private var commitLabel: String {
        switch includedCount {
        case 0: "Commit"
        case 1: "Commit 1 File"
        default: "Commit \(includedCount) Files"
        }
    }
}
