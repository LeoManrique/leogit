import SwiftUI

/// The Clone sheet — the native counterpart of the Tauri `CloneOverlay`:
/// a GitHub tab listing the signed-in user's repositories via `gh`, a URL
/// tab for anything git can reach, a shared destination row, and the clone
/// itself with live progress on both routes — `gh repo clone` forwards
/// `--progress` to the `git clone` it runs, so it reports the same meter.
///
/// There is no cancel once a clone starts — dismissing the sheet wouldn't
/// stop the clone, just orphan its progress and eventual error — so every
/// exit is disabled while `isCloning`, exactly like the Tauri dialog.
///
/// The store is passed in rather than owned here: it caches the GitHub list
/// for the process, and a store created with the sheet would re-fetch that
/// list on every open. `reopen()` is what makes each presentation start clean.
struct CloneSheet: View {
    @Bindable var store: CloneStore

    /// Called with the fresh repository's path after a successful clone;
    /// the caller opens it.
    let onCloned: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var isChoosingDestination = false

    /// Which field takes the caret. Re-asserted on tab switch, so arriving on
    /// a tab means being able to type into it — the GitHub tab's filter is
    /// also where the list's arrow keys are read.
    @FocusState private var focusedField: Field?

    private enum Field: Hashable {
        case filter
        case url
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Clone Repository")
                .font(.title3.weight(.semibold))

            Picker("Source", selection: $store.source) {
                ForEach(CloneSource.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .disabled(store.isCloning)

            switch store.source {
            case .github: githubTab
            case .url: urlTab
            }

            Divider()

            destinationRow

            if store.isCloning {
                progressArea
            }

            if let message = store.errorMessage {
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(store.isCloning)
                Button(store.isCloning ? "Cloning…" : "Clone") {
                    Task { await performClone() }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!store.canClone)
            }
        }
        .padding(16)
        .frame(width: 480)
        .interactiveDismissDisabled(store.isCloning)
        .task { await store.reopen() }
        .onChange(of: store.source, initial: true) { _, source in
            focusedField = source == .github ? .filter : .url
        }
        .fileImporter(
            isPresented: $isChoosingDestination,
            allowedContentTypes: [.folder]
        ) { result in
            guard case let .success(url) = result else { return }
            store.destinationDir = url.path(percentEncoded: false)
        }
        .fileDialogDefaultDirectory(
            URL(
                fileURLWithPath: (store.destinationDir as NSString).expandingTildeInPath,
                isDirectory: true
            )
        )
        .fileDialogMessage("Choose the folder to clone into")
        .fileDialogConfirmationLabel("Choose")
    }

    private func performClone() async {
        guard let repoPath = await store.clone() else { return }
        onCloned(repoPath)
        dismiss()
    }

    // MARK: GitHub tab

    @ViewBuilder
    private var githubTab: some View {
        HStack(spacing: 8) {
            TextField("Filter repositories", text: $store.filter)
                .textFieldStyle(.roundedBorder)
                .focused($focusedField, equals: .filter)
                // The list is below the filter and the caret stays here, so
                // the arrows have to be read here too. Selecting as the cursor
                // moves is deliberate: the destination preview is derived from
                // the selection, so arrowing shows where each row would land
                // and Return can act on the row without a second press.
                .onKeyPress(keys: [.upArrow, .downArrow]) { press in
                    moveSelection(by: press.key == .downArrow ? 1 : -1)
                    return .handled
                }

            Button {
                store.toggleSortMode()
            } label: {
                Image(systemName: store.sortMode == .recent ? "clock" : "textformat.abc")
            }
            .help(
                store.sortMode == .recent
                    ? "Sorted by recently modified" : "Sorted alphabetically"
            )

            // The list is a once-per-run cache, so this is the only way to see
            // a repository created since launch — always offered, not just
            // after a failure.
            Button {
                Task { await store.loadGitHubList() }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .help("Refresh the list from GitHub")
            .disabled(store.listPhase == .loading)
        }
        .disabled(store.isCloning)

        Group {
            switch store.listPhase {
            case .loading:
                ProgressView("Loading your repositories…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .failed(let message):
                VStack(spacing: 8) {
                    Text(message)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    Button("Retry") {
                        Task { await store.loadGitHubList() }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .loaded where store.visibleRepos.isEmpty:
                // Two different problems: an account with nothing in it, and a
                // query that matched none of what is there.
                Text(
                    store.githubRepos.isEmpty
                        ? "No repositories found." : "No matching repositories."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .loaded:
                List(store.visibleRepos, selection: $store.selectedRepoID) { repo in
                    HStack(spacing: 8) {
                        Text(repo.nameWithOwner)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 12)
                        if repo.isPrivate {
                            Text("Private")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 1)
                                .background(.quaternary, in: .capsule)
                        }
                    }
                    .help(repo.description)
                }
                .listStyle(.bordered)
                .disabled(store.isCloning)
            }
        }
        .frame(height: 240)
    }

    /// Move the selection one row, wrapping — with nothing selected, Down
    /// starts at the top and Up at the bottom.
    private func moveSelection(by delta: Int) {
        let rows = store.visibleRepos
        let current = rows.firstIndex { $0.nameWithOwner == store.selectedRepoID } ?? -1
        let next = ListNavigation.nextIndex(after: current, count: rows.count, delta: delta)
        guard next >= 0 else { return }
        store.selectedRepoID = rows[next].nameWithOwner
    }

    // MARK: URL tab

    @ViewBuilder
    private var urlTab: some View {
        LabeledContent("Repository URL or owner/name") {
            TextField(
                "Repository URL",
                text: $store.url,
                prompt: Text("https://github.com/owner/repo.git")
            )
            .textFieldStyle(.roundedBorder)
            .labelsHidden()
            .autocorrectionDisabled()
            .focused($focusedField, equals: .url)
            .disabled(store.isCloning)
        }
        .labeledContentStyle(.vertical)
    }

    // MARK: Shared rows

    @ViewBuilder
    private var destinationRow: some View {
        LabeledContent("Clone into") {
            HStack(spacing: 8) {
                TextField(
                    "Destination",
                    text: $store.destinationDir,
                    prompt: Text("~/Dev")
                )
                .textFieldStyle(.roundedBorder)
                .labelsHidden()
                Button("Browse…") { isChoosingDestination = true }
            }
            .disabled(store.isCloning)
        }
        .labeledContentStyle(.vertical)

        if !store.targetPath.isEmpty {
            Text("Clones into \(store.targetPath)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .help(store.targetPath)
        }
    }

    /// Determinate once git's aggregate percent is known, indeterminate
    /// before the first tick. Both clone routes reach it — a bar frozen at
    /// zero reads as stuck, so "no number yet" is its own state.
    private var progressArea: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let percent = store.progressPercent {
                ProgressView(value: percent, total: 100)
            } else {
                ProgressView()
                    .progressViewStyle(.linear)
            }
            Text(store.progressText ?? "Cloning \(store.repoName)…")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .help(store.progressText ?? "")
        }
    }
}
