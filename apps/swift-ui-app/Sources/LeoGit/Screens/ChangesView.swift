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

    /// Owned by the repository screen, not by this view: the tab bar swaps
    /// tabs by rebuilding the pane, which would take an in-progress draft —
    /// and amend mode, which the History tab is what puts the composer into —
    /// down with it.
    @Bindable var commitStore: CommitStore

    /// Called after a commit, discard, or ignore so the owner reloads status
    /// and history.
    let onWorkingTreeChanged: () async -> Void

    /// Failures from the row actions, which have nowhere of their own to
    /// report: the owner shows them in the screen's error banner.
    let onError: (String) -> Void

    /// Selection is the file's repo-relative path (`FileEntry.id`), so it
    /// survives a status reload that replaces every row value.
    @State private var selectedPath: String?

    /// Snapshot of the files to commit while the embedded-repo confirmation
    /// is up, so the commit operates on what the user was shown.
    @State private var pendingFiles: [FileEntry] = []
    @State private var isConfirmingEmbedded = false

    /// The file the discard confirmation is about; `nil` when it's closed.
    @State private var fileToDiscard: FileEntry?

    var body: some View {
        Group {
            // Amending keeps the pane up on a clean tree: a message-only
            // amend has no files by definition, and the composer is the only
            // way to finish — or abandon — the mode the History tab started.
            if files.isEmpty, !commitStore.isAmending {
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
        .confirmationDialog(
            "Discard Changes?",
            isPresented: discardConfirmationBinding,
            presenting: fileToDiscard
        ) { file in
            Button("Discard Changes", role: .destructive) { discard(file) }
            Button("Cancel", role: .cancel) {}
        } message: { file in
            Text(discardWarning(for: file))
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
        } menu: { file in
            rowMenu(for: file)
        }
    }

    // MARK: Row actions

    /// The right-clicked file's menu, in the Tauri client's order and
    /// wording: the two writes that need confirming or that change the repo
    /// first, then the copies, then the hand-offs to the system.
    @ViewBuilder
    private func rowMenu(for file: FileEntry) -> some View {
        Button("Discard Changes…", role: .destructive) { fileToDiscard = file }

        Divider()

        Button("Ignore File (Add to .gitignore)") {
            run { try await GitBridge.ignoreFiles(in: repoPath, paths: [file.path]) }
        }
        if let ext = Self.fileExtension(of: file.path) {
            Button("Ignore All \(ext) Files (Add to .gitignore)") {
                run { try await GitBridge.ignorePatterns(in: repoPath, patterns: ["*\(ext)"]) }
            }
        }

        Divider()

        Button("Copy File Path") { Clipboard.copy(absolutePath(of: file)) }
        Button("Copy Relative File Path") { Clipboard.copy(file.path) }

        Divider()

        // A deleted file has nothing left on disk to show or open. Disabled
        // rather than hidden, so the menu keeps a stable shape.
        Button("Reveal in Finder") {
            run(refresh: false) {
                try await GitBridge.revealInFileManager(in: repoPath, relativePath: file.path)
            }
        }
        .disabled(file.status == .deleted)

        Button("Open with Default Program") {
            run(refresh: false) {
                try await GitBridge.openWithDefaultApp(in: repoPath, relativePath: file.path)
            }
        }
        .disabled(file.status == .deleted)
    }

    /// Run a row action, reporting failure through the screen's banner.
    /// `refresh` reloads status afterwards — every action that writes to the
    /// repository changes what the list should show.
    private func run(refresh: Bool = true, _ action: @escaping () async throws -> Void) {
        Task {
            do {
                try await action()
                if refresh { await onWorkingTreeChanged() }
            } catch {
                onError(error.displayMessage)
            }
        }
    }

    private func discard(_ file: FileEntry) {
        run { try await GitBridge.discardChanges(in: repoPath, files: [file]) }
    }

    /// `presenting:` needs the value to outlive the dismissal animation, so
    /// the dialog's own binding clears it rather than the buttons doing so.
    private var discardConfirmationBinding: Binding<Bool> {
        Binding {
            fileToDiscard != nil
        } set: { isPresented in
            if !isPresented { fileToDiscard = nil }
        }
    }

    /// Discard does one of two things per path, and which one is not obvious
    /// from the row — so the dialog says it outright rather than asking the
    /// user to guess whether their file is recoverable.
    private func discardWarning(for file: FileEntry) -> String {
        if let original = file.origPath {
            return "\(original) comes back and \(file.path) moves to the Trash. "
                + "This can't be undone."
        }
        if file.status == .new {
            return "\(file.path) was never committed, so there is nothing to restore it to — "
                + "it moves to the Trash instead."
        }
        return "\(file.path) goes back to its committed state. This can't be undone."
    }

    private func absolutePath(of file: FileEntry) -> String {
        URL(fileURLWithPath: repoPath, isDirectory: true)
            .appending(path: file.path)
            .path(percentEncoded: false)
    }

    /// The file's extension *with* its leading dot, or nil when it has none.
    /// A dotfile like `.gitignore` counts as having none — "ignore all
    /// .gitignore files" is not a rule anyone means. The Tauri menu builds
    /// its label from the same rule.
    private static func fileExtension(of path: String) -> String? {
        let name = path.lastIndex(of: "/").map { String(path[path.index(after: $0)...]) } ?? path
        guard let dot = name.lastIndex(of: "."), dot != name.startIndex else { return nil }
        return String(name[dot...])
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
                await onWorkingTreeChanged()
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
