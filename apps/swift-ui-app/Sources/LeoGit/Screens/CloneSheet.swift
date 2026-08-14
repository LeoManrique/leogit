import SwiftUI

/// The Clone sheet — the native counterpart of the Tauri `CloneOverlay`:
/// a GitHub tab listing the signed-in user's repositories via `gh`, a URL
/// tab for anything git can reach, a shared destination row, and the clone
/// itself with live progress (URL clones only; `gh` reports none).
///
/// There is no cancel once a clone starts — dismissing the sheet wouldn't
/// stop the clone, just orphan its progress and eventual error — so every
/// exit is disabled while `isCloning`, exactly like the Tauri dialog.
struct CloneSheet: View {
    /// Called with the fresh repository's path after a successful clone;
    /// the caller opens it.
    let onCloned: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var store = CloneStore()
    @State private var isChoosingDestination = false

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
                    Task {
                        if let repoPath = await store.clone() {
                            onCloned(repoPath)
                            dismiss()
                        }
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!store.canClone)
            }
        }
        .padding(16)
        .frame(width: 480)
        .interactiveDismissDisabled(store.isCloning)
        .task { await store.prepare() }
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

    // MARK: GitHub tab

    @ViewBuilder
    private var githubTab: some View {
        HStack(spacing: 8) {
            TextField("Filter repositories", text: $store.filter)
                .textFieldStyle(.roundedBorder)
            Button {
                store.toggleSortMode()
            } label: {
                Image(systemName: store.sortMode == "recent" ? "clock" : "textformat.abc")
            }
            .help(
                store.sortMode == "recent"
                    ? "Sorted by recently modified" : "Sorted alphabetically"
            )
        }
        .disabled(store.isCloning || store.listPhase != .loaded)

        Group {
            switch store.listPhase {
            case .loading:
                ProgressView("Loading repositories…")
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
                Text(store.githubRepos.isEmpty ? "No repositories found." : "No matches")
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

    /// Determinate once git's aggregate percent is known (URL clones);
    /// indeterminate before the first tick and always for `gh`, which
    /// streams nothing.
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
