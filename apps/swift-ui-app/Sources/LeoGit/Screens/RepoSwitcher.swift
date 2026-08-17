import SwiftUI

/// The toolbar repo chip and its popover: every known repository one click
/// away, with per-repo indicators — a dirty dot for uncommitted changes, ↓
/// for commits to pull, ↑ for commits to push — so the state of the whole
/// working set is visible before switching. The native counterpart of the
/// Tauri client's header chip + `RepoDropdown`.
struct RepoSwitcher: View {
    let activePath: String
    let directory: RepoDirectoryStore

    /// Sweeps and switches hold off while a network operation runs.
    let isPaused: @MainActor () -> Bool

    let onSelect: (String) -> Void
    let onOpenOther: () -> Void
    let onClone: () -> Void

    @State private var isPresented = false

    var body: some View {
        Button {
            isPresented.toggle()
        } label: {
            Label(RepoDirectoryStore.displayName(of: activePath), systemImage: "folder")
        }
        // macOS toolbars render Labels icon-only by default; the active
        // repo's name is half the chip's value (and the toolbar title no
        // longer shows it).
        .labelStyle(.titleAndIcon)
        .help("Switch repository")
        .popover(isPresented: $isPresented, arrowEdge: .bottom) {
            RepoSwitcherList(
                activePath: activePath,
                directory: directory,
                isPaused: isPaused,
                onSelect: { path in
                    isPresented = false
                    onSelect(path)
                },
                onOpenOther: {
                    isPresented = false
                    onOpenOther()
                },
                onClone: {
                    isPresented = false
                    onClone()
                }
            )
        }
    }
}

/// The popover's content: filter field, repository rows, Open Other footer.
private struct RepoSwitcherList: View {
    let activePath: String
    let directory: RepoDirectoryStore
    let isPaused: @MainActor () -> Bool
    let onSelect: (String) -> Void
    let onOpenOther: () -> Void
    let onClone: () -> Void

    @State private var filter = ""
    @FocusState private var filterFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            TextField("Filter repositories", text: $filter)
                .textFieldStyle(.roundedBorder)
                .focused($filterFocused)
                .onSubmit {
                    if let first = filteredRepos.first {
                        onSelect(first)
                    }
                }
                .padding(10)

            Divider()

            if directory.repos.isEmpty {
                // Only a cold launch reaches the loading state: once a pass
                // has published rows, later refreshes replace them in place
                // rather than blinking the list through a spinner.
                if directory.isRefreshing {
                    loadingState
                } else {
                    emptyState
                }
            } else if filteredRepos.isEmpty {
                Text("No matches")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 20)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(filteredRepos, id: \.self) { path in
                            RepoRow(
                                path: path,
                                isActive: path == activePath,
                                sync: directory.syncByPath[path],
                                action: { onSelect(path) }
                            )
                        }
                    }
                    .padding(.vertical, 4)
                }
                .frame(maxHeight: 360)
            }

            Divider()

            HStack {
                Button("Open Other…", action: onOpenOther)
                Spacer()
                Button("Clone Repository…", action: onClone)
            }
            .buttonStyle(.borderless)
            .padding(8)
        }
        .frame(width: 320)
        .task {
            filterFocused = true
            // Fresh list and fresh badges each time the popover opens: full
            // discovery plus a fetch-less sweep of the rows (throttled inside
            // the store), exactly like the Tauri dropdown's open effect.
            await directory.refreshDirectory()
            await directory.sweepVisible(activePath: activePath, isPaused: isPaused)
        }
    }

    private var loadingState: some View {
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
            Text("Looking for repositories…")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 20)
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("No repositories found")
                .font(.callout.weight(.semibold))
            if !directory.scanFolders.isEmpty {
                Text("Searched:\n" + directory.scanFolders.joined(separator: "\n"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
    }

    /// Rows to show. Unfiltered: active repo first, then most-recently-opened
    /// order, then the rest by name — so the repos the user actually cycles
    /// between stay at the top. (The Tauri picker sorts by last-commit time
    /// instead; recency-of-use serves quick switching better and costs no
    /// extra git calls.) Filtered: `RepoSearch`'s match quality leads and that
    /// order only breaks ties, because Return opens the first row — pinning
    /// the open repository there would hide the repo the user just typed.
    private var filteredRepos: [String] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else {
            return directory.repos.sorted { rank(of: $0) < rank(of: $1) }
        }
        return directory.repos
            .compactMap { path in
                RepoSearch.match(query: query, for: path, scanFolders: directory.scanFolders)
                    .map { (path: path, match: $0) }
            }
            .sorted { lhs, rhs in
                lhs.match == rhs.match
                    ? rank(of: lhs.path) < rank(of: rhs.path)
                    : lhs.match < rhs.match
            }
            .map(\.path)
    }

    /// Sort key: 0 for the active repo, 1+index for MRU entries, then a
    /// name-ordered tail. Tuple-free so the comparator stays readable.
    private func rank(of path: String) -> (Int, String) {
        if path == activePath {
            return (0, "")
        }
        if let index = directory.recentRepos.firstIndex(of: path) {
            return (1 + index, "")
        }
        return (Int.max, RepoDirectoryStore.displayName(of: path).lowercased())
    }
}

/// One repository row: checkmark slot, name, and the three indicators.
private struct RepoRow: View {
    let path: String
    let isActive: Bool
    let sync: RepoSync?
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                    .opacity(isActive ? 1 : 0)
                    .frame(width: 14)

                Text(RepoDirectoryStore.displayName(of: path))
                    .lineLimit(1)
                    .truncationMode(.middle)

                Spacer(minLength: 12)

                if let sync {
                    indicators(for: sync)
                }
            }
            .contentShape(.rect)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
        }
        .buttonStyle(.plain)
        .background(isHovered ? AnyShapeStyle(.selection.opacity(0.15)) : AnyShapeStyle(.clear))
        .onHover { isHovered = $0 }
        .help(path)
    }

    /// Dirty dot, then ↓ pending pulls, then ↑ pending pushes — the same
    /// three badges as the Tauri dropdown, with its tooltip wording.
    @ViewBuilder
    private func indicators(for sync: RepoSync) -> some View {
        HStack(spacing: 6) {
            if sync.dirty {
                Circle()
                    .fill(.tint)
                    .frame(width: 6, height: 6)
                    .help("Uncommitted changes")
            }
            if sync.hasRemote, sync.behind > 0 {
                Text("↓\(sync.behind)")
                    .help("\(sync.behind) commit(s) to pull")
            }
            if sync.hasRemote, sync.ahead > 0 {
                Text("↑\(sync.ahead)")
                    .help("\(sync.ahead) commit(s) to push")
            }
        }
        .font(.caption.monospacedDigit())
        .foregroundStyle(.secondary)
    }
}
