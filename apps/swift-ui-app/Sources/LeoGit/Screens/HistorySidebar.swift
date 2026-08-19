import SwiftUI

/// The History tab's half of the sidebar: the commit list with its row menu,
/// paginating as the user reaches the end. No composer here — the Tauri
/// sidebar shows only the list on History, and amending is what sends the
/// user to Changes.
struct HistorySidebar: View {
    let commits: [CommitInfo]
    /// Drives the ↑ badge and gates the row menu's history-rewriting items:
    /// `headSha` says which row is `HEAD`, `unpushedShas` and `upstream` say
    /// whether the last commit is still safely local.
    let status: RepoStatus?

    /// The commit whose detail the main content shows, keyed by sha so it
    /// survives a log refresh that replaces every row value (same idea as
    /// the Changes tab's path selection). Owned by the repository screen:
    /// the detail lives on the far side of the split.
    @Binding var selectedSha: String?

    /// Ask the owner for another page when the list reaches its last row.
    let onReachEnd: () -> Void

    /// Put the composer into amend mode for this commit and show it.
    let onAmend: (CommitInfo) -> Void
    /// Drop this commit, keeping its changes and message for a new one.
    let onUndo: (CommitInfo) -> Void
    /// Check the commit out, detaching HEAD.
    let onCheckout: (CommitInfo) -> Void

    /// The commit the checkout confirmation is about; `nil` when it's closed.
    @State private var commitToCheckout: CommitInfo?

    private var unpushedShas: Set<String> { Set(status?.unpushedShas ?? []) }

    var body: some View {
        Group {
            if commits.isEmpty {
                EmptyListPlaceholder(text: "No commits")
            } else {
                commitList
            }
        }
        .onChange(of: commits.map(\.sha), initial: true) {
            // Keep something selected: newest commit on arrival, and again
            // when a refresh drops the selected sha (an amend).
            if selectedSha == nil || !commits.contains(where: { $0.sha == selectedSha }) {
                selectedSha = commits.first?.sha
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
