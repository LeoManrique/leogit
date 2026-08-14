import AppKit
import SwiftTerm
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
///
/// Also the home of the background refresh machinery, sequenced here because
/// it spans every store: the 2 s status poll, the config-driven auto-fetch
/// loop, the tiered badge scheduler for the other repos, and the
/// refresh-on-activate resync. Each is a `.task(id: repoPath)` loop, so a
/// repo switch or close cancels and restarts them structurally — no timer
/// bookkeeping, which is where the Tauri client needs explicit
/// `clearInterval` teardown.
struct ContentView: View {
    @Environment(RepoStore.self) private var store
    @State private var branchStore = BranchStore()
    @State private var syncStore = SyncStore()
    @State private var directoryStore = RepoDirectoryStore()
    @State private var terminalStore = TerminalStore()
    @State private var isChoosingFolder = false
    @State private var isCloneSheetPresented = false
    @State private var tab: RepoTab = .changes

    /// One attempt per launch: reopen the repo recorded in the shared state
    /// file. A flag rather than derived state so a failed restore leaves
    /// Welcome up instead of retrying forever.
    @State private var hasRestoredLastRepo = false

    /// Dedupes the activate resync — activation notifications can burst.
    @State private var isResyncing = false

    var body: some View {
        Group {
            if let repoPath = store.repoPath {
                repositoryScreen(repoPath: repoPath)
            } else {
                WelcomeView(
                    coreVersion: store.coreVersionText,
                    onOpen: { isChoosingFolder = true },
                    onClone: { isCloneSheetPresented = true }
                )
                .task { await restoreLastRepo() }
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
        .sheet(isPresented: $isCloneSheetPresented) {
            // A successful clone opens the fresh repo directly — the
            // `.task(id: repoPath)` chain then records it as recent and
            // runs the warm-up fetch like any other open.
            CloneSheet { repoPath in
                Task { await store.open(at: URL(fileURLWithPath: repoPath, isDirectory: true)) }
            }
        }
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

            TerminalDock(repoPath: repoPath, store: terminalStore)
        }
        .navigationTitle(store.repoName)
        .navigationSubtitle(branchSubtitle)
        .task(id: repoPath) {
            branchStore.reset()
            syncStore.reset()
            // Sessions never survive a repo switch — a shell from the prior
            // repo would be a leak wearing the new repo's dock.
            terminalStore.closeSession()
            await directoryStore.noteOpened(repoPath)
            await branchStore.load(repoPath: repoPath)
            // One warm-up fetch so ahead/behind reflect the remote shortly
            // after opening — the Tauri client runs the same immediate fetch
            // at startup even with auto-fetch off. Silent: failures (offline,
            // no remote) show nothing, and status reloads only if it worked.
            if await syncStore.silentFetch(repoPath: repoPath) {
                await store.refresh()
            }
        }
        .task(id: repoPath) { await statusPollLoop() }
        .task(id: repoPath) { await autoFetchLoop(repoPath: repoPath) }
        .task(id: repoPath) {
            await directoryStore.runScheduler(activePath: repoPath, isPaused: backgroundPaused)
        }
        .onReceive(
            NotificationCenter.default.publisher(
                for: NSApplication.didBecomeActiveNotification
            )
        ) { _ in
            Task { await resyncOnActivate() }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                RepoSwitcher(
                    activePath: repoPath,
                    directory: directoryStore,
                    isPaused: backgroundPaused,
                    onSelect: switchRepo,
                    onOpenOther: { isChoosingFolder = true },
                    onClone: { isCloneSheetPresented = true }
                )
                // Switching mid-transfer would reset the sync UI out from
                // under the running operation — held back like Refresh.
                .disabled(syncStore.activeOperation != nil)
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

            ToolbarItemGroup(placement: .primaryAction) {
                SyncControls(
                    store: syncStore,
                    repoPath: repoPath,
                    status: store.status,
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
                // Also held back during a network operation, the way the
                // Tauri client pauses its status poll: `git status` racing a
                // pull can trip over transient lock files.
                .disabled(store.isLoading || syncStore.activeOperation != nil)
                .help("Reload status, history, and branches")
            }
        }
        .overlay(alignment: .top) {
            if let operation = syncStore.activeOperation {
                SyncProgressBanner(
                    operation: operation,
                    percent: syncStore.progressPercent,
                    text: syncStore.progressText
                )
            } else if store.isLoading {
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

    // MARK: Background refresh

    /// Background work holds off while a network operation runs (the Tauri
    /// guard) and while the app is inactive — a native improvement the
    /// activate resync makes safe, since it catches up immediately on return.
    @MainActor
    private func backgroundPaused() -> Bool {
        syncStore.activeOperation != nil || !NSApp.isActive
    }

    /// Whether a text field (the commit composer, a sheet's name field, the
    /// switcher's filter) or the embedded terminal has keyboard focus —
    /// SwiftUI text editing runs through AppKit's field editor, an
    /// `NSTextView` first responder, and the terminal is SwiftTerm's own
    /// view. The native analogue of the Tauri client's document-wide
    /// `userTyping` flag, which xterm's hidden textarea also sets.
    @MainActor
    private var isTextInputFocused: Bool {
        let firstResponder = NSApp.keyWindow?.firstResponder
        return firstResponder is NSTextView || firstResponder is TerminalView
    }

    /// The 2 s status poll, ported from the Tauri client: how changes made
    /// outside the app — the terminal, an editor — appear by themselves.
    /// Sequential by construction (each tick awaits the last), so the Tauri
    /// in-flight guard has no equivalent here.
    @MainActor
    private func statusPollLoop() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(2))
            if Task.isCancelled { return }
            guard !backgroundPaused(), !store.isLoading else { continue }
            await store.refreshQuietly()
            if let repoPath = store.repoPath, let status = store.status {
                // Feeds the switcher's badge for the open repo for free —
                // the reason the scheduler excludes it from every tier.
                directoryStore.noteActiveStatus(repoPath, status)
            }
        }
    }

    /// Auto-fetch for the open repository, driven by `auto_fetch` and
    /// `fetch_interval_ms` from the shared config — re-read every tick, a
    /// cheap TOML load, so a change in the Settings window applies within
    /// one interval: the re-arm the Tauri client never got (it reads the
    /// pair once per repo switch). While disabled, the loop idles on a 30 s
    /// re-check instead of exiting, so flipping the toggle revives it.
    /// Fetches are held back while typing (a fetch can reorder the file
    /// list mid-keystroke), while the breaker is open, and — a deliberate
    /// improvement over Tauri — for repos with no remote, whose fetch could
    /// only ever fail and poison the breaker.
    @MainActor
    private func autoFetchLoop(repoPath: String) async {
        while !Task.isCancelled {
            let config = try? await GitBridge.appConfig()
            let intervalMs = config?.autoFetch == true ? (config?.fetchIntervalMs ?? 0) : 0
            let interval: Duration =
                intervalMs > 0 ? .milliseconds(Int64(intervalMs)) : .seconds(30)
            try? await Task.sleep(for: interval)
            if Task.isCancelled { return }
            guard intervalMs > 0,
                !backgroundPaused(),
                !isTextInputFocused,
                store.status?.hasRemote == true,
                directoryStore.breaker.shouldAttempt
            else { continue }
            let reached = await syncStore.silentFetch(repoPath: repoPath)
            directoryStore.breaker.record(success: reached)
            await store.refreshQuietly()
        }
    }

    /// Coming back to the app: fetch, refresh status silently, force the
    /// open diff to reload (a file can change on disk without its status row
    /// changing), and give the most-recent repos' badges a throttled
    /// catch-up — the Tauri client's `resyncOnActive`.
    @MainActor
    private func resyncOnActivate() async {
        guard store.repoPath != nil, !isResyncing, syncStore.activeOperation == nil else {
            return
        }
        isResyncing = true
        defer { isResyncing = false }

        if let repoPath = store.repoPath,
            store.status?.hasRemote == true,
            directoryStore.breaker.shouldAttempt
        {
            let reached = await syncStore.silentFetch(repoPath: repoPath)
            directoryStore.breaker.record(success: reached)
        }
        await store.refreshQuietly(forceDiffReload: true)
        await directoryStore.refocusSweep(
            activePath: store.repoPath,
            isPaused: backgroundPaused
        )
    }

    // MARK: Repo switching

    /// Reopen the repo recorded in the shared state file, once per launch —
    /// the restore both clients perform, so they hand the working repo to
    /// each other. Best-effort: a moved or deleted path just leaves Welcome.
    @MainActor
    private func restoreLastRepo() async {
        guard !hasRestoredLastRepo else { return }
        hasRestoredLastRepo = true
        guard let last = (try? await GitBridge.reposState())?.lastOpenedRepo else { return }
        await store.open(at: URL(fileURLWithPath: last, isDirectory: true))
        if store.repoPath == nil {
            // A failed restore is not the user's error; don't greet them
            // with a banner about a repo they may have deleted on purpose.
            store.errorMessage = nil
        }
    }

    /// Switch straight to another repository — no detour through Welcome.
    /// The `.task(id: repoPath)` modifiers do the rest: stores reset, warm-up
    /// fetch, background loops restart against the new path.
    @MainActor
    private func switchRepo(_ path: String) {
        guard path != store.repoPath else { return }
        Task { await store.open(at: URL(fileURLWithPath: path, isDirectory: true)) }
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
