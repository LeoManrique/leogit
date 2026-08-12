import SwiftUI

/// Commit history for the open repository.
///
/// A `Table` rather than a `List`: columns, header sorting affordances and
/// keyboard selection are what a native macOS history view is expected to have,
/// and `Table` is lazy so a full page of commits costs nothing to render.
struct HistoryView: View {
    let commits: [CommitInfo]

    @State private var selection: CommitInfo.ID?

    var body: some View {
        if commits.isEmpty {
            ContentUnavailableView(
                "No Commits",
                systemImage: "clock",
                description: Text("This repository has no commit history yet.")
            )
        } else {
            Table(commits, selection: $selection) {
                TableColumn("Summary") { commit in
                    HStack(spacing: 6) {
                        Text(commit.summary)
                            .lineLimit(1)
                        ForEach(commit.tags, id: \.self) { tag in
                            Text(tag)
                                .font(.caption2)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(.tint.opacity(0.15), in: .capsule)
                        }
                    }
                }
                .width(min: 220, ideal: 420)

                TableColumn("Author") { commit in
                    Text(commit.authorName)
                        .lineLimit(1)
                        .foregroundStyle(.secondary)
                }
                .width(min: 100, ideal: 150)

                TableColumn("When") { commit in
                    Text(CommitDate.relative(commit.authorDate))
                        .foregroundStyle(.secondary)
                        .help(CommitDate.absolute(commit.authorDate))
                }
                .width(min: 90, ideal: 120)

                TableColumn("Commit") { commit in
                    Text(commit.shortSha)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                .width(min: 70, ideal: 80)
            }
            .tableStyle(.inset)
            .alternatingRowBackgrounds()
        }
    }
}
