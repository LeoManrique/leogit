import SwiftUI

/// Working-tree changes for the open repository: the changed-file list and
/// commit box on the left, the selected file's diff on the right.
///
/// Checkboxes mean "include this file in the next commit" — there is no
/// staging-area concept, matching the Tauri client: nothing touches the index
/// until Commit, and core's `commit` then resets and re-stages exactly the
/// checked files.
struct ChangesView: View {
    let repoPath: String
    let files: [FileEntry]
    let statusEpoch: Int

    /// Called after a successful commit so the owner reloads status + history.
    let onCommitted: () async -> Void

    /// Selection is the file's repo-relative path (`FileEntry.id`), so it
    /// survives a status reload that replaces every row value.
    @State private var selectedPath: String?

    @State private var commitStore = CommitStore()

    /// Snapshot of the files to commit while the embedded-repo confirmation
    /// is up, so the commit operates on what the user was shown.
    @State private var pendingFiles: [FileEntry] = []
    @State private var isConfirmingEmbedded = false

    var body: some View {
        Group {
            if files.isEmpty {
                ContentUnavailableView(
                    "No Changes",
                    systemImage: "checkmark.circle",
                    description: Text("The working tree is clean.")
                )
                // Must claim the full space like the split view it replaces:
                // left to its own (small) ideal size, the surrounding VStack
                // would center vertically, shoving the tab picker into the
                // middle of the window and re-sizing it after a commit.
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    // Same frame as the History commit list, so the divider
                    // sits in the same place across tabs; the diff dominates.
                    changesPane
                        .frame(minWidth: 260, idealWidth: 280, maxWidth: 420)
                    detail
                        .frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
                }
                .onChange(of: files.map(\.path), initial: true) {
                    // Keep something selected: first file on arrival, and again
                    // when a reload drops the previously selected path.
                    if selectedPath == nil || !files.contains(where: { $0.path == selectedPath }) {
                        selectedPath = files.first?.path
                    }
                }
            }
        }
        .onChange(of: repoPath) {
            // A different repository must not inherit the previous one's
            // draft message or checkbox opt-outs.
            commitStore.reset()
        }
        .task {
            // The provider picker mirrors the shared config file; one read
            // per appearance is enough (it's machine-global, not per-repo).
            await commitStore.loadAIProvider()
        }
        .confirmationDialog(
            "Commit Embedded Repositories?",
            isPresented: $isConfirmingEmbedded
        ) {
            Button("Commit") { performCommit(pendingFiles) }
            Button("Cancel", role: .cancel) { pendingFiles = [] }
        } message: {
            Text(embeddedWarning)
        }
    }

    // MARK: Left pane

    private var changesPane: some View {
        VStack(spacing: 0) {
            listHeader
            Divider()
            fileList
            Divider()
            CommitComposer(
                store: commitStore,
                includedCount: includedFiles.count,
                autoSummary: CommitStore.autoSummary(for: includedFiles),
                onSubmit: submit,
                onGenerate: generate
            )
        }
    }

    private var listHeader: some View {
        HStack(spacing: 8) {
            Toggle("Include all files", isOn: allIncludedBinding)
                .toggleStyle(.checkbox)
                .labelsHidden()
                .disabled(committableFiles.isEmpty)
                .help("Include or exclude every file")

            Text("\(includedFiles.count) of \(committableFiles.count) files included")
                .font(.caption)
                .foregroundStyle(.secondary)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }

    private var fileList: some View {
        ChangedFileList(files: files, selectedPath: $selectedPath) { file in
            Toggle("Include \(file.displayName)", isOn: includedBinding(for: file))
                .toggleStyle(.checkbox)
                .labelsHidden()
                .disabled(!CommitStore.isCommittable(file))
        }
    }

    @ViewBuilder
    private var detail: some View {
        if let file = files.first(where: { $0.path == selectedPath }) {
            DiffView(repoPath: repoPath, file: file, target: .workingTree(epoch: statusEpoch))
        } else {
            ContentUnavailableView(
                "No File Selected",
                systemImage: "doc.text",
                description: Text("Select a file to see its changes.")
            )
        }
    }

    // MARK: Inclusion state

    private var committableFiles: [FileEntry] {
        files.filter(CommitStore.isCommittable)
    }

    private var includedFiles: [FileEntry] {
        commitStore.includedFiles(from: files)
    }

    private var allIncludedBinding: Binding<Bool> {
        Binding {
            !committableFiles.isEmpty && committableFiles.allSatisfy { commitStore.isIncluded($0) }
        } set: { include in
            commitStore.setAllIncluded(include, in: files)
        }
    }

    private func includedBinding(for file: FileEntry) -> Binding<Bool> {
        Binding {
            commitStore.isIncluded(file)
        } set: { include in
            commitStore.setIncluded(file, include)
        }
    }

    // MARK: Commit

    private func submit() {
        let files = includedFiles
        if files.contains(where: \.embedded) {
            // Committing a nested repository records only a gitlink — usually
            // a mistake, so make it a deliberate choice (as the Tauri client
            // does with its confirmation modal).
            pendingFiles = files
            isConfirmingEmbedded = true
        } else {
            performCommit(files)
        }
    }

    private func performCommit(_ files: [FileEntry]) {
        Task {
            // Recomputed from the snapshot, not the live list — the message
            // must describe what this commit contains.
            let fallback = CommitStore.autoSummary(for: files)
            if await commitStore.commit(repoPath: repoPath, files: files, autoSummary: fallback) {
                await onCommitted()
            }
        }
    }

    private func generate() {
        Task {
            await commitStore.generate(repoPath: repoPath, files: includedFiles)
        }
    }

    private var embeddedWarning: String {
        let names = pendingFiles.filter(\.embedded).map(\.displayName)
        return "\(names.joined(separator: ", ")): nested git "
            + (names.count == 1 ? "repository" : "repositories")
            + " commit as a pointer (gitlink), not as files. Their contents stay out of this repository."
    }
}
