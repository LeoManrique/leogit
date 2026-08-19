import SwiftUI

/// The History tab's half of the main content: the selected commit's detail
/// — metadata card, changed files, per-file diff — or the reason there isn't
/// one. Every branch claims the whole slot — it shares a column with the
/// terminal dock, and an empty state left to its own size would let the dock
/// float up to meet it.
struct HistoryDetailPane: View {
    let repoPath: String
    let commits: [CommitInfo]
    let selectedSha: String?

    var body: some View {
        Group {
            if let commit = commits.first(where: { $0.sha == selectedSha }) {
                CommitDetailView(repoPath: repoPath, commit: commit)
            } else if commits.isEmpty {
                ContentUnavailableView(
                    "No Commits",
                    systemImage: "clock",
                    description: Text("This repository has no commit history yet.")
                )
            } else {
                ContentUnavailableView(
                    "No Commit Selected",
                    systemImage: "clock",
                    description: Text("Select a commit to see its changes.")
                )
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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
