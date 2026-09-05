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
/// checked files. **Inclusion and selection are different things**: the
/// checkbox column is what the commit will contain, the highlight is what the
/// pointer and keyboard are pointing at, and either can move without the other.
struct ChangesSidebar: View {
    let repoPath: String
    let files: [FileEntry]

    /// Whether this repository's first `git status` has landed.
    ///
    /// `files.isEmpty` cannot tell "nothing has changed" from "we haven't
    /// looked yet", and the list asserts the first of those out loud. Opening a
    /// repository drops the previous one's status with its path, so without
    /// this the pane would claim *No changes* for the width of that first read
    /// — a false statement, on every switch. `RepoStore.historyLoaded` is the
    /// same gate on the History side, and exists for the same sentence.
    let statusLoaded: Bool

    /// Owned by the repository screen, not by this view: the tab bar swaps
    /// tabs by rebuilding the pane, which would take an in-progress draft —
    /// and amend mode, which the History tab is what puts the composer into —
    /// down with it.
    @Bindable var commitStore: CommitStore

    /// The highlighted rows, by repo-relative path (`FileEntry.id`) so they
    /// survive a status reload that replaces every row value. Owned by the
    /// repository screen for the same reason as the draft: a tab switch
    /// rebuilds this view, and a selection that died with it would drop the
    /// user back at the top of the list every time they looked at History.
    @Binding var selection: Set<String>

    /// The file whose diff the detail pane shows. Derived from `selection`
    /// through `FileListSelection`, which is the one place that rule lives.
    @Binding var selectedPath: String?

    /// Called after a commit: HEAD moved, so status *and* history are stale.
    let onCommitted: () async -> Void

    /// Called after a discard or an ignore: the working tree changed but
    /// history cannot have, so only the status is re-read.
    let onWorkingTreeChanged: () async -> Void

    /// A hand-off to another program that didn't take — FRONTEND §6.13's
    /// second class, which the screen shows in its dismissible strip. Failures
    /// of the actions that *write* stay here, in a modal or in the dialog that
    /// raised them.
    let onNotice: (String) -> Void

    /// A write the user was waiting on that failed — §6.13's first class. Like
    /// the discard sheet, it is *presented* by the repository screen: a window
    /// shows one thing at a time, and this view can already be behind a sheet
    /// when the answer arrives.
    let onFailure: (ActionFailure) -> Void

    /// Ask for the discard confirmation over these files. The sheet itself is
    /// presented by the repository screen: a window has one sheet slot, and a
    /// second `.sheet` further down the tree is not a second one.
    let onDiscard: ([FileEntry]) -> Void

    /// Run a shell command in the terminal dock — the composer's offer to fix
    /// an unready AI provider. Owned by the repository screen, which is where
    /// the dock lives.
    let onRunInTerminal: (String) -> Void

    /// Snapshot of the files to commit while the embedded-repo confirmation
    /// is up, so the commit operates on what the user was shown.
    @State private var pendingFiles: [FileEntry] = []
    @State private var isConfirmingEmbedded = false

    /// The composer's height — the one piece of sidebar geometry the user
    /// sets by hand, persisted like the Tauri client's `leogit:commitHeight`
    /// (same default and bounds). `UserDefaults` rather than the shared
    /// config: layout is per client, and the Tauri client keeps its own in
    /// `localStorage`. Lives here rather than in `CommitComposer` so the
    /// list and the handle, which share the space, read the same value.
    @AppStorage("commitComposerHeight") private var composerHeight = 220.0

    /// The height while a drag is in progress, before it is worth writing
    /// down. A drag produces a value per frame and only the last one is worth
    /// keeping, so `UserDefaults` is written once, on release.
    @State private var draggingHeight: CGFloat?

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
                //
                // Before the first status lands there is no sentence to say:
                // the pane holds the space and stays quiet. Deliberately not a
                // spinner or a "Loading…" line — a `git status` usually answers
                // in a few hundred milliseconds, and a placeholder that flashes
                // for that long on every repo switch is more visible than the
                // wrong word it replaces.
                //
                // It is not a bounded wait, though: a repository whose *first*
                // status read fails stays blank until one succeeds, which for a
                // deleted or unreadable folder is indefinitely. That is the
                // right silence rather than a missing state — the pane cannot
                // say what is wrong, and the thing that can is already saying
                // it, in the error banner above the split.
                //
                // `Color.clear` takes the same flexible space
                // `EmptyListPlaceholder` does, so the composer below it does not
                // move when the answer arrives. Nothing sits behind it to click
                // through to, so it is left as it is.
                if statusLoaded {
                    EmptyListPlaceholder(text: "No changes")
                } else {
                    Color.clear
                }
            } else {
                listHeader
                Divider()
                fileList
            }
            RowResizeHandle(
                height: composerHeightBinding,
                range: composerHeightBounds,
                onCommit: persistComposerHeight
            )
            CommitComposer(
                store: commitStore,
                includedCount: includedFiles.count,
                autoSummary: CommitStore.autoSummary(for: includedFiles),
                isConfirmationPending: isConfirmingEmbedded,
                onSubmit: submit,
                onGenerate: generate,
                onRunFixCommand: onRunInTerminal
            )
            .frame(height: effectiveComposerHeight)
        }
        .onGeometryChange(for: CGFloat.self) { proxy in
            proxy.size.height
        } action: { height in
            availableHeight = height
        }
        // A drag that is cancelled rather than ended — the window deactivating
        // mid-gesture — never reaches `onCommit`, and the height would be lost
        // at the next launch. Leaving is the other moment it is settled.
        .onDisappear(perform: persistComposerHeight)
        .onChange(of: files.map(\.path), initial: true) { reseat() }
        .onChange(of: selection) {
            selectedPath = FileListSelection.activePath(
                in: selection,
                of: files,
                keeping: selectedPath
            )
        }
        .task(id: commitStore.aiProvider) {
            // Re-asked whenever the picker moves, so the gate always describes
            // the provider Generate would actually run. Keyed on the provider
            // rather than the whole config: an unrelated Settings save is not
            // a reason to spawn `claude --version` again.
            await commitStore.refreshProviderStatus()
        }
        .confirmationDialog(embeddedTitle, isPresented: $isConfirmingEmbedded) {
            Button("Commit as Link") { performCommit(pendingFiles) }
            Button("Cancel", role: .cancel) { pendingFiles = [] }
        } message: {
            Text(embeddedWarning)
        }
    }

    // MARK: Selection

    /// Keep the highlight and the open diff describing files that still exist.
    ///
    /// Two things, in order. Rows that have left the working tree are pruned —
    /// committed, discarded, `git rm`'d in a terminal — because a highlight
    /// pointing at nothing would still be counted by every action that reads it.
    /// Then, and only if nothing is left, the list re-seats on the first row.
    /// Those are the two conditions and no others (STYLE.md): a file the user
    /// chose is never overridden, and a tick that changes only a row's content
    /// or status is not a re-seat — the trigger is the *set of paths* changing.
    private func reseat() {
        let live = Set(files.map(\.path))
        let survivors = selection.intersection(live)
        if survivors.isEmpty {
            selection = files.first.map { [$0.path] } ?? []
        } else if survivors != selection {
            selection = survivors
        }
        selectedPath = FileListSelection.activePath(
            in: selection,
            of: files,
            keeping: selectedPath
        )
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

    /// The height in force: the live drag if there is one, else what was
    /// stored — clamped into today's bounds, so a window that grows again
    /// gets the user's full height back without a fresh drag.
    private var effectiveComposerHeight: CGFloat {
        let bounds = composerHeightBounds
        let height = draggingHeight ?? CGFloat(composerHeight)
        return min(max(height, bounds.lowerBound), bounds.upperBound)
    }

    /// The handle writes to the transient value; `persistComposerHeight`
    /// moves it into `UserDefaults` once the gesture is over.
    private var composerHeightBinding: Binding<CGFloat> {
        Binding {
            draggingHeight ?? CGFloat(composerHeight)
        } set: { height in
            draggingHeight = height
        }
    }

    private func persistComposerHeight() {
        if let draggingHeight {
            composerHeight = Double(draggingHeight)
        }
        draggingHeight = nil
    }

    // MARK: List

    private var listHeader: some View {
        HStack(spacing: 8) {
            // The multi-source form of `Toggle`, which is the only way to a
            // *mixed* checkbox in SwiftUI. A two-state select-all lies the
            // moment one row is unchecked — it reads "off" over a list that is
            // mostly on — and this is the control people use to answer "what
            // is going in?" at a glance.
            Toggle("Include all files", sources: inclusions, isOn: \.isIncluded)
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
        ChangedFileList(
            files: files,
            selection: $selection,
            isIncluded: { commitStore.isIncluded($0) },
            onToggleSelection: toggleSelectionInclusion
        ) { file in
            Toggle("Include \(file.displayName)", isOn: includedBinding(for: file))
                .toggleStyle(.checkbox)
                .labelsHidden()
                .disabled(!CommitStore.isCommittable(file))
                // Offered here too, though a disabled control may not raise a
                // tooltip at all: the ↪ badge beside it is enabled and carries
                // the same sentence, so the answer is reachable either way.
                .help(file.repositoryEntryHint ?? "Include in the next commit")
        } menu: { targets in
            rowMenu(for: targets)
        }
    }

    // MARK: Row actions

    /// The right-clicked rows' menu.
    ///
    /// A multi-row selection collapses to the single action that means
    /// anything across files. Ignoring several paths at once and copying
    /// several are not actions either client offers — one `.gitignore` rule per
    /// right-click is a decision, a dozen at once is an accident.
    @ViewBuilder
    private func rowMenu(for targets: [FileEntry]) -> some View {
        if targets.count > 1 {
            Button("Discard \(targets.count) Selected Changes…", role: .destructive) {
                onDiscard(targets)
            }
        } else if let file = targets.first {
            singleRowMenu(for: file)
        }
    }

    /// One row's menu, in the Tauri client's order and wording: the two writes
    /// that need confirming or that change the repo first, then the copies,
    /// then the hand-offs to the system.
    @ViewBuilder
    private func singleRowMenu(for file: FileEntry) -> some View {
        Button("Discard Changes…", role: .destructive) {
            onDiscard([file])
        }

        Divider()

        Button("Ignore File (Add to .gitignore)") {
            write { try await GitBridge.ignoreFiles(in: repoPath, paths: [file.path]) }
        }
        if let ext = Self.fileExtension(of: file.path) {
            Button("Ignore All \(ext) Files (Add to .gitignore)") {
                write { try await GitBridge.ignorePatterns(in: repoPath, patterns: ["*\(ext)"]) }
            }
        }

        Divider()

        Button("Copy File Path") { Clipboard.copy(absolutePath(of: file)) }
        Button("Copy Relative File Path") { Clipboard.copy(file.path) }

        Divider()

        // A deleted file has nothing left on disk to show or open. Disabled
        // rather than hidden, so the menu keeps a stable shape.
        Button("Reveal in Finder") {
            handOff { try await GitBridge.revealInFileManager(in: repoPath, relativePath: file.path) }
        }
        .disabled(file.status == .deleted)

        Button("Open with Default Program") {
            handOff { try await GitBridge.openWithDefaultApp(in: repoPath, relativePath: file.path) }
        }
        .disabled(file.status == .deleted)
    }

    /// A row action that writes to the repository — the two `.gitignore` ones;
    /// discard has its own sheet, which keeps its own refusal. The user is
    /// waiting on it, so a failure takes the window (§6.13's first class) and
    /// offers the same attempt again: these fail on a write race far more often
    /// than on anything the user would have to change first. Success re-reads
    /// the status only, since appending a rule cannot move `HEAD`.
    private func write(_ action: @escaping () async throws -> Void) {
        Task {
            do {
                try await action()
                await onWorkingTreeChanged()
            } catch {
                onFailure(ActionFailure(error.displayMessage) { write(action) })
            }
        }
    }

    /// A hand-off to another program. It changes nothing here, so a failure is
    /// reported and stepped over: taking the window because Finder wouldn't
    /// open is a bigger interruption than the thing that failed.
    private func handOff(_ action: @escaping () async throws -> Void) {
        Task {
            do {
                try await action()
            } catch {
                onNotice(error.displayMessage)
            }
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

    /// One committable row's inclusion, in the shape `Toggle`'s multi-source
    /// initializer wants: a collection it can project a `Binding<Bool>` out of.
    private struct RowInclusion {
        var isIncluded: Bool
    }

    private var inclusions: Binding<[RowInclusion]> {
        // Filtered once and captured, not re-derived inside each closure:
        // SwiftUI drives a multi-source toggle element by element, so a
        // select-all over N rows calls both closures N times — and capturing is
        // also what guarantees the getter and the setter are indexing one list.
        let committable = committableFiles
        return Binding {
            committable.map { RowInclusion(isIncluded: commitStore.isIncluded($0)) }
        } set: { rows in
            for (file, row) in zip(committable, rows)
            where commitStore.isIncluded(file) != row.isIncluded {
                commitStore.setIncluded(file, row.isIncluded)
            }
        }
    }

    private func includedBinding(for file: FileEntry) -> Binding<Bool> {
        Binding {
            commitStore.isIncluded(file)
        } set: { include in
            commitStore.setIncluded(file, include)
        }
    }

    /// Space over the list: include or exclude every highlighted row at once —
    /// the highest-frequency action in the app, which had no keyboard route
    /// here at all.
    ///
    /// The target state is the select-all checkbox's own sentence, deliberately
    /// the same one: *any excluded → include them all, otherwise exclude them
    /// all*. A mixed selection resolving to "include" is what makes the key
    /// usable as a sweep — press it twice and you have exactly the rows you
    /// swept, whatever state they were in.
    /// Returns whether it changed anything, so a press over a selection of
    /// nothing but dirty submodules is *not* swallowed — a key that silently
    /// does nothing is worse than one the system beeps at.
    private func toggleSelectionInclusion() -> Bool {
        let targets = files.filter { selection.contains($0.path) && CommitStore.isCommittable($0) }
        guard !targets.isEmpty else { return false }
        let include = targets.contains { !commitStore.isIncluded($0) }
        commitStore.setAllIncluded(include, in: targets)
        return true
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
            pendingFiles = []
        }
    }

    private func generate() {
        Task {
            await commitStore.generate(repoPath: repoPath, files: includedFiles)
        }
    }

    private var embeddedRepos: [FileEntry] {
        pendingFiles.filter(\.embedded)
    }

    private var embeddedTitle: String {
        embeddedRepos.count == 1
            ? "Commit nested repository as a link?"
            : "Commit nested repositories as a link?"
    }

    /// The Tauri dialog's copy, which is the better of the two: it names the
    /// outer repository, states the consequence for whoever clones it, and
    /// says what a gitlink *is* rather than assuming the word. Native's
    /// container — a system confirmation — is the better of the two, so the
    /// two halves are combined.
    private var embeddedWarning: String {
        let repos = embeddedRepos
        let many = repos.count > 1
        let names = repos.map(\.displayName).joined(separator: ", ")
        let subject = many
            ? "These folders are their own Git repositories"
            : "This folder is its own Git repository"
        return """
            \(names)

            \(subject). \(many ? "They’ll" : "It’ll") be committed as a link to the current \
            commit — the \(many ? "folders’" : "folder’s") files won’t be copied into \
            \(outerRepoName).

            Anyone cloning \(outerRepoName) won’t get those files unless \
            \(many ? "each is" : "it’s") set up as a submodule.
            """
    }

    private var outerRepoName: String {
        let name = URL(fileURLWithPath: repoPath, isDirectory: true).lastPathComponent
        return name.isEmpty ? "this repository" : name
    }
}
