import AppKit
import SwiftTerm
import SwiftUI
import UniformTypeIdentifiers

/// Which list the main pane is showing.
enum RepoTab: String, CaseIterable, Identifiable {
    case changes = "Changes"
    case history = "History"
    case pullRequests = "Pull Requests"

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
    @State private var pullRequestStore = PullRequestStore()
    @State private var isChoosingFolder = false
    @State private var isCloneSheetPresented = false
    @State private var tab: RepoTab = .changes

    /// One attempt per launch: reopen the repo recorded in the shared state
    /// file. A flag rather than derived state so a failed restore leaves
    /// Welcome up instead of retrying forever.
    @State private var hasRestoredLastRepo = false

    /// The shared config's `show_pull_requests` — read on repo open and
    /// re-read when the Settings window saves, so the toggle applies live.
    @State private var isPullRequestsTabEnabled = true

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

            RepoTabBar(
                tabs: availableTabs,
                selection: $tab,
                changesCount: store.status?.files.count ?? 0
            )
            .onChange(of: availableTabs) { _, tabs in
                // The tab under the selection can disappear — the setting
                // turned off, or a switch to a remote-less repo.
                if !tabs.contains(tab) { tab = .changes }
            }

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
                HistoryView(
                    repoPath: repoPath,
                    commits: store.commits,
                    unpushedShas: Set(store.status?.unpushedShas ?? []),
                    onReachEnd: { Task { await store.loadMoreHistory() } }
                )
            case .pullRequests:
                PullRequestsView(
                    repoPath: repoPath,
                    store: pullRequestStore,
                    headCommitSummary: store.commits.first?.summary ?? "",
                    currentBranch: store.status?.branch ?? "",
                    onWorkingTreeChanged: {
                        // A checkout moved HEAD: status, log, and the
                        // branch menu all need to reflect the new branch.
                        await store.refresh()
                        await branchStore.load(repoPath: repoPath)
                    }
                )
            }

            TerminalDock(repoPath: repoPath, store: terminalStore)
        }
        .navigationTitle(store.repoName)
        // The repo name renders inside the switcher chip, so the toolbar
        // title would duplicate it; the title still names the window for
        // Mission Control and the Window menu.
        .toolbar(removing: .title)
        .task(id: repoPath) {
            branchStore.reset()
            syncStore.reset()
            pullRequestStore.reset()
            await refreshPullRequestsTabSetting()
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
        .task {
            // Discovery is a filesystem walk that takes a moment on a deep
            // scan tree, so it starts as soon as this screen exists rather
            // than when the switcher first opens — otherwise that first open
            // shows only the active repo and looks broken until reopened.
            // Deliberately not keyed on `repoPath`: the walk covers every
            // repo, so it belongs to the screen's lifetime, not a repo's, and
            // the popover re-runs it on open for freshness. Independent of
            // the `.task(id: repoPath)` chain above, so it runs alongside
            // opening the repo instead of delaying it.
            await directoryStore.refreshDirectory()
        }
        .onReceive(
            NotificationCenter.default.publisher(
                for: NSApplication.didBecomeActiveNotification
            )
        ) { _ in
            Task { await resyncOnActivate() }
        }
        .onReceive(NotificationCenter.default.publisher(for: .leogitConfigDidSave)) { _ in
            Task { await refreshPullRequestsTabSetting() }
        }
        .onReceive(NotificationCenter.default.publisher(for: .leogitRefreshRequested)) { _ in
            // ⌘R from the View menu — the keyboard-only successor of the
            // toolbar Refresh button: a full visible reload of status,
            // history, and branches. Held back during a network operation,
            // the way the Tauri client pauses its status poll: `git status`
            // racing a pull can trip over transient lock files.
            guard !store.isLoading, syncStore.activeOperation == nil else { return }
            Task {
                await store.refresh()
                await branchStore.load(repoPath: repoPath)
            }
        }
        .toolbar {
            // One "where am I" cluster at the leading edge. On the macOS 26
            // toolbar, capsule grouping follows `ToolbarSpacer` boundaries:
            // adjacent items with no spacer between them form one logical
            // grouping drawn with a shared glass background. (`ControlGroup`
            // and the `.navigation` placement both rendered separate
            // capsules instead.) Both controls stay stock.
            ToolbarItem {
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

            ToolbarItem {
                BranchMenu(
                    store: branchStore,
                    repoPath: repoPath,
                    status: store.status,
                    isMerging: store.isMerging,
                    onWorkingTreeChanged: { await store.refresh() }
                )
            }

            // With the toolbar title removed, no title area separates
            // leading from trailing, so the break is explicit — this pushes
            // the sync cluster to the trailing edge.
            ToolbarSpacer(.flexible)

            // Ahead/behind as standalone informative text beside the sync
            // button — a toolbar control can't host a count badge on macOS,
            // so the counts stand next to the action they feed. The hidden
            // shared background keeps it reading as status, not a button
            // (and out of the sync button's grouping).
            ToolbarItem {
                if !syncCountsText.isEmpty {
                    Text(syncCountsText)
                        .font(.body.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .help(syncCountsHelp)
                }
            }
            .sharedBackgroundVisibility(.hidden)

            ToolbarItem {
                SyncControls(
                    store: syncStore,
                    repoPath: repoPath,
                    status: store.status,
                    onWorkingTreeChanged: { await store.refresh() }
                )
            }
        }
        // The sync ladder's menu-bar face (Repository ▸ <action>, ⌘P),
        // published from the window content because a focused scene value
        // set inside `.toolbar` never reaches the scene — toolbar items
        // render in their own hosting hierarchy. The closure posts back to
        // `SyncControls`, whose sheet, alert, and busy guard live with the
        // button, so ⌘P runs the exact click path.
        .focusedSceneValue(\.syncCommand, syncMenuCommand)
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

    /// The ⌘P menu item's content: the ladder's proposal by title, enabled
    /// only when it's runnable and no operation holds the slot. The perform
    /// closure posts rather than acting — the sheet and alert the action may
    /// open belong to `SyncControls`.
    private var syncMenuCommand: SyncCommand {
        let proposal = SyncProposal(status: store.status)
        return SyncCommand(
            title: proposal.title,
            isEnabled: proposal.isActionable && syncStore.activeOperation == nil
        ) {
            NotificationCenter.default.post(name: .leogitSyncActionRequested, object: nil)
        }
    }

    /// The tabs the segmented picker offers. Pull Requests needs both the
    /// setting on and a repo with a remote — a remote-less repo can't have
    /// PRs, so the tab hides rather than sitting there empty (nil status,
    /// before the first load resolves, counts as remote-less: the tab pops
    /// in rather than flashing away).
    private var availableTabs: [RepoTab] {
        var tabs: [RepoTab] = [.changes, .history]
        if isPullRequestsTabEnabled, store.status?.hasRemote == true {
            tabs.append(.pullRequests)
        }
        return tabs
    }

    /// Re-read `show_pull_requests` from the shared config.
    @MainActor
    private func refreshPullRequestsTabSetting() async {
        isPullRequestsTabEnabled = (try? await GitBridge.appConfig())?.showPullRequests ?? true
    }

    /// `↑N ↓N` — pending pushes and pulls, empty when in sync (or detached,
    /// where neither direction exists).
    private var syncCountsText: String {
        guard let status = store.status, !status.detached else { return "" }
        var parts: [String] = []
        if status.ahead > 0 { parts.append("↑\(status.ahead)") }
        if status.behind > 0 { parts.append("↓\(status.behind)") }
        return parts.joined(separator: " ")
    }

    private var syncCountsHelp: String {
        guard let status = store.status else { return "" }
        var parts: [String] = []
        if status.ahead > 0 {
            parts.append("\(status.ahead) commit\(status.ahead == 1 ? "" : "s") to push")
        }
        if status.behind > 0 {
            parts.append("\(status.behind) commit\(status.behind == 1 ? "" : "s") to pull")
        }
        return parts.joined(separator: ", ")
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
            let previousHead = store.status?.headSha
            await store.refreshQuietly()
            if let repoPath = store.repoPath, let status = store.status {
                // Feeds the switcher's badge for the open repo for free —
                // the reason the scheduler excludes it from every tier.
                directoryStore.noteActiveStatus(repoPath, status)
                if status.headSha != previousHead {
                    // HEAD moved outside the app — a terminal checkout,
                    // commit, or merge: the branch menu's checkmark and each
                    // branch's metadata are stale, so the list reloads with
                    // the history the poll already refetched.
                    await branchStore.load(repoPath: repoPath)
                }
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
