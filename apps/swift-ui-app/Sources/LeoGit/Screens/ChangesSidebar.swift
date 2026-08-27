import SwiftUI

/// The Changes tab's half of the sidebar: the changed-file list with its
/// include-all header, the row context menu, and the commit composer pinned
/// underneath. Always on screen while the tab is selected — a clean working
/// tree empties the list but keeps the composer, exactly like the Tauri
/// sidebar, so an amend (which may carry no files at all) and a draft
/// message have somewhere to live.
///
/// Checkboxes mean "include this file in the next commit" — there is no
/// staging-area concept, matching the Tauri client: nothing touches the index
/// until Commit, and core's `commit` then resets and re-stages exactly the
/// checked files.
struct ChangesSidebar: View {
    let repoPath: String
    let files: [FileEntry]

    /// Owned by the repository screen, not by this view: the tab bar swaps
    /// tabs by rebuilding the pane, which would take an in-progress draft —
    /// and amend mode, which the History tab is what puts the composer into —
    /// down with it.
    @Bindable var commitStore: CommitStore

    /// The file whose diff the detail pane shows, keyed by repo-relative
    /// path (`FileEntry.id`) so it survives a status reload that replaces
    /// every row value. Owned by the repository screen: the diff lives on the
    /// far side of the split, and the selection must outlive a tab switch.
    @Binding var selectedPath: String?

    /// Called after a commit, discard, or ignore so the owner reloads status
    /// and history.
    let onWorkingTreeChanged: () async -> Void

    /// Failures from the row actions, which have nowhere of their own to
    /// report: the owner shows them in the screen's error banner.
    let onError: (String) -> Void

    /// Snapshot of the files to commit while the embedded-repo confirmation
    /// is up, so the commit operates on what the user was shown.
    @State private var pendingFiles: [FileEntry] = []
    @State private var isConfirmingEmbedded = false

    /// The file the discard confirmation is about; `nil` when it's closed.
    @State private var fileToDiscard: FileEntry?
    /// What discarding `fileToDiscard` would actually do, as core decides it.
    /// `nil` until the answer arrives — the dialog opens on the row click and
    /// fills its message a moment later rather than guessing in the meantime.
    @State private var discardPlan: DiscardPlan?

    /// The composer's height — the one piece of sidebar geometry the user
    /// sets by hand, persisted like the Tauri client's `leogit:commitHeight`
    /// (same default and bounds). `UserDefaults` rather than the shared
    /// config: layout is per client, and the Tauri client keeps its own in
    /// `localStorage`. Lives here rather than in `CommitComposer` so the
    /// list and the handle, which share the space, read the same value.
    @AppStorage("commitComposerHeight") private var composerHeight = 220.0

    /// Measured height of this pane, so a tall stored height can't overflow
    /// a short window: the list keeps a floor and the composer yields.
    @State private var availableHeight: CGFloat = 0

    private static let composerHeightRange: ClosedRange<CGFloat> = 180...600
    /// The least the list keeps when the composer is at its tallest.
    private static let listMinHeight: CGFloat = 80

    var body: some View {
        VStack(spacing: 0) {
            if files.isEmpty {
                // No header either: "0 of 0 files included" is a sentence
                // about nothing, and the Tauri sidebar drops its select-all
                // row the same way.
                EmptyListPlaceholder(text: "No changes")
            } else {
                listHeader
                Divider()
                fileList
            }
            RowResizeHandle(height: composerHeightBinding, range: composerHeightBounds)
            CommitComposer(
                store: commitStore,
                includedCount: includedFiles.count,
                autoSummary: CommitStore.autoSummary(for: includedFiles),
                onSubmit: submit,
                onGenerate: generate
            )
            .frame(height: effectiveComposerHeight)
        }
        .onGeometryChange(for: CGFloat.self) { proxy in
            proxy.size.height
        } action: { height in
            availableHeight = height
        }
        .onChange(of: files.map(\.path), initial: true) {
            // Keep something selected: first file on arrival, and again
            // when a reload drops the previously selected path.
            if selectedPath == nil || !files.contains(where: { $0.path == selectedPath }) {
                selectedPath = files.first?.path
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
        .task(id: fileToDiscard?.path) {
            guard let file = fileToDiscard else {
                discardPlan = nil
                return
            }
            discardPlan = await GitBridge.discardPlan(in: repoPath, files: [file])
        }
    }

    // MARK: Composer height

    /// What the handle may drag to right now: the fixed range, capped by
    /// what fits above the list's floor. Capping the *drag* too — not only
    /// the rendered height — keeps the stored value within reach, so a drag
    /// back down moves the divider at once instead of first spending an
    /// invisible surplus. Unmeasured (the first frame) means uncapped, so
    /// the stored height doesn't flash through the minimum before geometry
    /// arrives.
    private var composerHeightBounds: ClosedRange<CGFloat> {
        let range = Self.composerHeightRange
        guard availableHeight > 0 else { return range }
        let cap = max(range.lowerBound, availableHeight - Self.listMinHeight)
        return range.lowerBound...min(range.upperBound, cap)
    }

    /// The stored height, clamped into today's bounds — a window that grows
    /// again gets the user's full height back without a fresh drag.
    private var effectiveComposerHeight: CGFloat {
        let bounds = composerHeightBounds
        return min(max(composerHeight, bounds.lowerBound), bounds.upperBound)
    }

    /// `@AppStorage` stores a `Double`; the handle speaks geometry.
    private var composerHeightBinding: Binding<CGFloat> {
        Binding {
            CGFloat(composerHeight)
        } set: { height in
            composerHeight = Double(height)
        }
    }

    // MARK: List

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
        Button("Discard Changes…", role: .destructive) {
            discardPlan = nil
            fileToDiscard = file
        }

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
    ///
    /// The answer comes from core, which decides it from actual `HEAD`
    /// membership, and is the same decision the discard itself runs on. The
    /// status letter cannot answer it: a staged re-add of a path that exists
    /// in HEAD is restorable, a rename whose original is *not* in HEAD is not,
    /// and under an unborn HEAD nothing is — three cases the old guess got
    /// wrong, each of them a promise the action then broke.
    private func discardWarning(for file: FileEntry) -> String {
        guard let plan = discardPlan else {
            return "Working out what this will do…"
        }
        let restored = plan.restore.joined(separator: ", ")
        let trashed = plan.trash.joined(separator: ", ")
        return switch (restored.isEmpty, trashed.isEmpty) {
        case (false, false):
            "\(restored) comes back and \(trashed) moves to the Trash. This can't be undone."
        case (false, true):
            "\(restored) goes back to its committed state. This can't be undone."
        case (true, false):
            "\(trashed) was never committed, so there is nothing to restore it to — "
                + "it moves to the Trash instead."
        case (true, true):
            "There is nothing to discard in \(file.path)."
        }
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
