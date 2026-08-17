import SwiftUI

/// Commit history for the open repository: the commit list on the left, the
/// selected commit's detail — metadata, changed files, per-file diff — on the
/// right, mirroring the Tauri client's History tab.
struct HistoryView: View {
    let repoPath: String
    let commits: [CommitInfo]
    /// Drives the ↑ badge and gates the row menu's history-rewriting items:
    /// `headSha` says which row is `HEAD`, `unpushedShas` and `upstream` say
    /// whether the last commit is still safely local.
    let status: RepoStatus?
    /// Ask the owner for another page when the list reaches its last row.
    let onReachEnd: () -> Void

    /// Put the composer into amend mode for this commit and show it.
    let onAmend: (CommitInfo) -> Void
    /// Drop this commit, keeping its changes and message for a new one.
    let onUndo: (CommitInfo) -> Void
    /// Check the commit out, detaching HEAD.
    let onCheckout: (CommitInfo) -> Void

    /// Selection is the sha, so it survives a log refresh that replaces
    /// every row value (same idea as the Changes tab's path selection).
    @State private var selectedSha: String?

    /// The commit the checkout confirmation is about; `nil` when it's closed.
    @State private var commitToCheckout: CommitInfo?

    private var unpushedShas: Set<String> { Set(status?.unpushedShas ?? []) }

    var body: some View {
        if commits.isEmpty {
            ContentUnavailableView(
                "No Commits",
                systemImage: "clock",
                description: Text("This repository has no commit history yet.")
            )
            // Claim the full space like the split view it replaces, so the
            // tab picker stays pinned under the toolbar (see ChangesView).
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            HSplitView {
                commitList
                    // Same frame as the Changes pane, so the divider sits in
                    // the same place across tabs and the diff dominates.
                    .frame(minWidth: 260, idealWidth: 280, maxWidth: 420)
                detail
                    .frame(minWidth: 400, maxWidth: .infinity, maxHeight: .infinity)
            }
            .onChange(of: commits.map(\.sha), initial: true) {
                // Keep something selected: newest commit on arrival, and
                // again when a refresh drops the selected sha (an amend).
                if selectedSha == nil || !commits.contains(where: { $0.sha == selectedSha }) {
                    selectedSha = commits.first?.sha
                }
            }
        }
    }

    private var commitList: some View {
        List(commits, selection: $selectedSha) { commit in
            CommitRow(commit: commit, isUnpushed: unpushedShas.contains(commit.sha))
                .onAppear {
                    // Rows materialise lazily, so the last one appearing
                    // means the user scrolled to the end of what we have.
                    if commit.sha == commits.last?.sha { onReachEnd() }
                }
        }
        .listStyle(.inset)
        .alternatingRowBackgrounds()
        .contextMenu(forSelectionType: String.self) { shas in
            if let sha = shas.first, let commit = commits.first(where: { $0.sha == sha }) {
                rowMenu(for: commit)
            }
        }
        .confirmationDialog(
            "Check Out This Commit?",
            isPresented: checkoutConfirmationBinding,
            presenting: commitToCheckout
        ) { commit in
            Button("Check Out") { onCheckout(commit) }
            Button("Cancel", role: .cancel) {}
        } message: { commit in
            Text(
                "\(commit.shortSha) — \(commit.summary)\n\n"
                    + "This detaches HEAD: you'll be on no branch until you pick one from the "
                    + "branch menu. Commits made meanwhile are easy to lose."
            )
        }
    }

    // MARK: Row actions

    /// The right-clicked commit's menu. The two history-rewriting items only
    /// make sense on `HEAD`, and Checkout only on anything else, so exactly
    /// one of the two groups is ever live for a given row — they're shown
    /// disabled rather than hidden so the menu keeps a stable shape.
    @ViewBuilder
    private func rowMenu(for commit: CommitInfo) -> some View {
        let isHead = commit.sha == status?.headSha

        Button("Amend Last Commit…") { onAmend(commit) }
            .disabled(!isHead)

        Button("Undo Last Commit") { onUndo(commit) }
            .disabled(!isHead || !canUndo(commit))

        Button("Check Out Commit…") { commitToCheckout = commit }
            .disabled(isHead)

        Divider()

        Button("Copy SHA") { Clipboard.copy(commit.sha) }
        Button("Copy Tag") { Clipboard.copy(commit.tags.joined(separator: " ")) }
            .disabled(commit.tags.isEmpty)
    }

    /// Undo is offered only while the commit is believed to be local: either
    /// it's provably unpushed, or no upstream resolved at all — in which case
    /// nothing can prove it *was* pushed either. Undoing a published commit
    /// would leave the branch behind its remote and needing a force push.
    private func canUndo(_ commit: CommitInfo) -> Bool {
        let hasResolvedUpstream = !(status?.upstream ?? "").isEmpty
        return !hasResolvedUpstream || unpushedShas.contains(commit.sha)
    }

    /// `presenting:` needs the value to outlive the dismissal animation, so
    /// the dialog's own binding clears it rather than the buttons doing so.
    private var checkoutConfirmationBinding: Binding<Bool> {
        Binding {
            commitToCheckout != nil
        } set: { isPresented in
            if !isPresented { commitToCheckout = nil }
        }
    }

    @ViewBuilder
    private var detail: some View {
        if let commit = commits.first(where: { $0.sha == selectedSha }) {
            CommitDetailView(repoPath: repoPath, commit: commit)
        } else {
            ContentUnavailableView(
                "No Commit Selected",
                systemImage: "clock",
                description: Text("Select a commit to see its changes.")
            )
        }
    }
}

/// One commit in the list: summary with tag chips and the unpushed badge,
/// then author · relative time — the Tauri list's two-line row.
private struct CommitRow: View {
    let commit: CommitInfo
    let isUnpushed: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(commit.summary)
                    .lineLimit(1)

                Spacer(minLength: 0)

                // First tag plus an overflow count, like the Tauri list — a
                // narrow row can't afford a parade of chips.
                if let tag = commit.tags.first {
                    chip(tag)
                        .help(commit.tags.joined(separator: ", "))
                    if commit.tags.count > 1 {
                        chip("+\(commit.tags.count - 1)")
                            .help(commit.tags.joined(separator: ", "))
                    }
                }

                if isUnpushed {
                    // A 16×16 plate rather than a bare glyph — the Tauri
                    // list's unpushed badge, which shares the tag chips'
                    // height and corner radius so the row's indicator
                    // cluster reads as one family.
                    Image(systemName: "arrow.up")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 16, height: 16)
                        .background(.quaternary, in: .rect(cornerRadius: 5))
                        .help("Not yet pushed")
                }
            }

            Text("\(commit.authorName) · \(CommitDate.relative(commit.authorDate))")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.vertical, 3)
        .help(CommitDate.absolute(commit.authorDate))
    }

    private func chip(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10, design: .monospaced))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.tint.opacity(0.15), in: .capsule)
            .fixedSize()
    }
}

/// The right side of History: the commit's metadata card on top, then its
/// changed files beside the selected file's diff — the same layout the Tauri
/// client gives a selected commit.
private struct CommitDetailView: View {
    let repoPath: String
    let commit: CommitInfo

    @State private var store = CommitDetailStore()

    var body: some View {
        VStack(spacing: 0) {
            CommitHeader(commit: commit, fileCount: store.files.count, stats: store.stats)
            Divider()
            content
        }
        .task(id: commit.sha) {
            await store.load(repoPath: repoPath, sha: commit.sha)
        }
    }

    @ViewBuilder
    private var content: some View {
        if let errorMessage = store.errorMessage {
            ContentUnavailableView(
                "Couldn't Load Commit",
                systemImage: "exclamationmark.triangle",
                description: Text(errorMessage)
            )
        } else if store.files.isEmpty {
            if store.isLoading {
                // Kept quiet: the file list usually lands fast enough that a
                // spinner would be flicker, not information.
                Color.clear
            } else {
                ContentUnavailableView(
                    "No Changed Files",
                    systemImage: "doc",
                    description: Text("This commit doesn't change any files — an empty or merge commit.")
                )
            }
        } else {
            HSplitView {
                // Tighter than the outer panes — plain path rows need less
                // room, and this diff should dominate its split too.
                ChangedFileList(files: store.files, selectedPath: $store.selectedPath)
                    .frame(minWidth: 200, idealWidth: 240, maxWidth: 360)
                fileDiff
                    .frame(minWidth: 320, maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    @ViewBuilder
    private var fileDiff: some View {
        if let file = store.files.first(where: { $0.path == store.selectedPath }) {
            DiffView(repoPath: repoPath, file: file, target: .commit(sha: commit.sha))
        } else {
            ContentUnavailableView(
                "No File Selected",
                systemImage: "doc.text",
                description: Text("Select a file to see its changes.")
            )
        }
    }
}

/// The commit's metadata: summary and +/− totals, the full message body,
/// author identity and date, the sha with a copy button, and any trailers.
private struct CommitHeader: View {
    let commit: CommitInfo
    let fileCount: Int
    let stats: CommitStats?

    @State private var copied = false
    @State private var copyReset: Task<Void, Never>?
    /// Measured height of the body text, so the scrollable block hugs a
    /// short body and caps a long one (the Tauri card's `max-height: 140px`).
    @State private var bodyHeight: CGFloat = 0
    @Environment(\.colorScheme) private var colorScheme

    private static let maxBodyHeight: CGFloat = 140

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(commit.summary)
                    .font(.headline)
                    .textSelection(.enabled)

                Spacer(minLength: 8)

                if let stats, stats.additions > 0 || stats.deletions > 0 {
                    HStack(spacing: 6) {
                        if stats.additions > 0 {
                            Text("+\(stats.additions)")
                                .foregroundStyle(palette.addGlyph)
                        }
                        if stats.deletions > 0 {
                            Text("−\(stats.deletions)")
                                .foregroundStyle(palette.removeGlyph)
                        }
                    }
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .fixedSize()
                }
            }

            if !commit.body.isEmpty {
                messageBody
            }

            HStack(spacing: 6) {
                Text(commit.authorName)
                    .fontWeight(.medium)
                Text(commit.authorEmail)
                    .foregroundStyle(.secondary)
                Spacer(minLength: 8)
                Text(CommitDate.absolute(commit.authorDate))
                    .foregroundStyle(.secondary)
            }
            .font(.caption)
            .textSelection(.enabled)
            .lineLimit(1)

            HStack(spacing: 8) {
                Text(commit.sha)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Button(action: copySha) {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc")
                        .foregroundStyle(copied ? Color.green : Color.secondary)
                }
                .buttonStyle(.borderless)
                .help(copied ? "Copied" : "Copy SHA")

                Spacer(minLength: 8)

                if fileCount > 0 {
                    Text("\(fileCount) \(fileCount == 1 ? "file" : "files") changed")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize()
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    /// The full message body below the summary — trailers included, exactly
    /// as `git log` reports it. Scrolls once it outgrows its cap so a
    /// paragraph-long description can't push the file list off screen.
    private var messageBody: some View {
        ScrollView {
            Text(commit.body)
                .font(.system(size: 12, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
                .onGeometryChange(for: CGFloat.self) { proxy in
                    proxy.size.height
                } action: { height in
                    bodyHeight = height
                }
        }
        .frame(height: min(bodyHeight, Self.maxBodyHeight))
        .background(.quaternary.opacity(0.5), in: .rect(cornerRadius: 6))
    }

    private func copySha() {
        Clipboard.copy(commit.sha)
        copied = true
        copyReset?.cancel()
        copyReset = Task {
            try? await Task.sleep(for: .seconds(1.2))
            guard !Task.isCancelled else { return }
            copied = false
        }
    }

    private var palette: DiffPalette {
        DiffPalette(colorScheme)
    }
}
