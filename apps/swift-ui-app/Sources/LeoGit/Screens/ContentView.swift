import SwiftUI
import UniformTypeIdentifiers

/// Which list the main pane is showing.
enum RepoTab: String, CaseIterable, Identifiable {
    case changes = "Changes"
    case history = "History"

    var id: Self { self }
}

/// Root view: the welcome screen until a repository is open, the repository
/// screen afterwards.
struct ContentView: View {
    @Environment(RepoStore.self) private var store
    @State private var branchStore = BranchStore()
    @State private var isChoosingFolder = false
    @State private var tab: RepoTab = .changes

    var body: some View {
        Group {
            if let repoPath = store.repoPath {
                repositoryScreen(repoPath: repoPath)
            } else {
                WelcomeView(coreVersion: store.coreVersionText) { isChoosingFolder = true }
            }
        }
        .frame(minWidth: 720, minHeight: 460)
        .fileImporter(
            isPresented: $isChoosingFolder,
            allowedContentTypes: [.folder]
        ) { result in
            guard case let .success(url) = result else { return }
            Task { await store.open(at: url) }
        }
        .fileDialogMessage("Choose a folder inside a Git repository")
        .fileDialogConfirmationLabel("Open Repository")
    }

    private func repositoryScreen(repoPath: String) -> some View {
        VStack(spacing: 0) {
            if let errorMessage = store.errorMessage {
                ErrorBanner(message: errorMessage)
            }

            Picker("View", selection: $tab) {
                ForEach(RepoTab.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider()

            switch tab {
            case .changes:
                ChangesView(
                    repoPath: repoPath,
                    files: store.status?.files ?? [],
                    statusEpoch: store.statusEpoch,
                    onCommitted: { await store.refresh() }
                )
            case .history:
                HistoryView(commits: store.commits)
            }
        }
        .navigationTitle(store.repoName)
        .navigationSubtitle(branchSubtitle)
        .task(id: repoPath) {
            branchStore.reset()
            await branchStore.load(repoPath: repoPath)
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button {
                    store.close()
                } label: {
                    Label("Close Repository", systemImage: "chevron.left")
                }
                .help("Close this repository")
            }

            ToolbarItem(placement: .principal) {
                BranchMenu(
                    store: branchStore,
                    repoPath: repoPath,
                    status: store.status,
                    isMerging: store.isMerging,
                    onWorkingTreeChanged: { await store.refresh() }
                )
            }

            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task {
                        await store.refresh()
                        await branchStore.load(repoPath: repoPath)
                    }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .disabled(store.isLoading)
                .help("Reload status, history, and branches")
            }
        }
        .overlay(alignment: .top) {
            if store.isLoading {
                ProgressView()
                    .progressViewStyle(.linear)
                    .frame(maxWidth: .infinity)
            }
        }
    }

    /// Branch, plus ahead/behind counts when the branch tracks a remote, plus
    /// a merging marker while `MERGE_HEAD` exists.
    private var branchSubtitle: String {
        guard let status = store.status else { return "" }
        var parts: [String] = []
        if status.detached {
            parts.append("Detached at \(String(status.headSha.prefix(7)))")
        } else {
            parts.append(status.branch)
            if status.ahead > 0 { parts.append("↑\(status.ahead)") }
            if status.behind > 0 { parts.append("↓\(status.behind)") }
        }
        if store.isMerging { parts.append("· merging") }
        return parts.joined(separator: " ")
    }
}

/// Non-blocking failure banner; the last good data stays on screen behind it.
struct ErrorBanner: View {
    let message: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
            Text(message)
                .font(.callout)
                .textSelection(.enabled)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.orange.opacity(0.12))
    }
}
