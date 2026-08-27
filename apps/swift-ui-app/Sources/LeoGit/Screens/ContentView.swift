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
/// `clearInterval` teardown. Whether each loop may run right now is not
/// decided here: every guard names a `BackgroundSchedulingPolicy` predicate.
struct ContentView: View {
    @Environment(RepoStore.self) private var store
    @Environment(AppConfigStore.self) private var appConfig
    @State private var branchStore = BranchStore()
    @State private var directoryStore = RepoDirectoryStore()
    @State private var terminalStore = TerminalStore()

    /// "May background work run right now?" — the policy every loop below
    /// consults by predicate name; its doc comment carries the table.
    /// Created alongside `syncStore` in `init` because the store publishes
    /// its network-op slot into the policy, so the pair must share identity.
    @State private var schedulingPolicy: BackgroundSchedulingPolicy
    @State private var syncStore: SyncStore
    /// Lives here, not in `ChangesSidebar`: switching tabs rebuilds that
    /// pane, which would drop an in-progress commit message — and amend mode
    /// is started from the *History* tab, so it has to survive the switch
    /// that puts the composer on screen.
    @State private var commitStore = CommitStore()
    @State private var isChoosingFolder = false
    @State private var isCloneSheetPresented = false
    @State private var tab: RepoTab = .changes

    /// Each tab's selection, keyed by path / sha so a reload that replaces
    /// every row value keeps it. Held here because each tab's list and its
    /// detail sit on opposite sides of the split — and so a round trip
    /// through the other tab comes back to the same file or commit.
    @State private var selectedPath: String?
    @State private var selectedSha: String?

    /// One attempt per launch: reopen the repo recorded in the shared state
    /// file. A flag rather than derived state so a failed restore leaves
    /// Welcome up instead of retrying forever.
    @State private var hasRestoredLastRepo = false

    /// Dedupes the activate resync — activation notifications can burst.
    @State private var isResyncing = false

    init() {
        let policy = BackgroundSchedulingPolicy()
        _schedulingPolicy = State(initialValue: policy)
        _syncStore = State(initialValue: SyncStore(schedulingPolicy: policy))
    }

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
        // The policy's two inputs this view owns: which window hosts the UI
        // (its occlusion gates everything) and whether a repo is open (the
        // App Nap assertion's other half). Attached to the root so they
        // survive the welcome ⇄ repository swap.
        .trackWindowVisibility(with: schedulingPolicy)
        .onChange(of: store.repoPath, initial: true) { _, path in
            schedulingPolicy.isRepoOpen = path != nil
        }
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

    /// The Tauri client's two-column screen: one permanent split whose left
    /// column is the sidebar (tab bar, then the tab's list, then — on Changes
    /// — the composer) and whose right column is the main content (the tab's
    /// detail, then the terminal dock). The split lives here, above the
    /// tabs, so its divider is one control that neither a tab switch nor an
    /// empty list can rebuild or hide: only what's *inside* each column
    /// swaps. Both columns hold the tab-switched content in place, which is
    /// what keeps the composer on a clean tree and the dock under the diff
    /// rather than under the whole window.
    private func repositoryScreen(repoPath: String) -> some View {
        VStack(spacing: 0) {
            if let errorMessage = store.errorMessage {
                ErrorBanner(message: errorMessage)
            }

            HSplitView {
                sidebar(repoPath: repoPath)
                    // The Tauri sidebar's range: 320 by default, never
                    // narrower than the composer's control row needs, and
                    // capped so the diff keeps most of the window.
                    .frame(minWidth: 280, idealWidth: 320, maxWidth: 640)
                mainContent(repoPath: repoPath)
                    .frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(store.repoName)
        // The repo name renders inside the switcher chip, so the toolbar
        // title would duplicate it; the title still names the window for
        // Mission Control and the Window menu.
        .toolbar(removing: .title)
        .task(id: repoPath) {
            branchStore.reset()
            syncStore.reset()
            // A different repository must not inherit the previous one's
            // draft message, checkbox opt-outs, or amend target — nor its
            // selections, which the sidebars re-seed from the new lists.
            commitStore.reset()
            selectedPath = nil
            selectedSha = nil
            // Sessions never survive a repo switch — a shell from the prior
            // repo would be a leak wearing the new repo's dock.
            terminalStore.closeSession()
            await directoryStore.noteOpened(repoPath)
            await branchStore.load(repoPath: repoPath)
            await warmUpFetch(repoPath: repoPath)
        }
        .task(id: repoPath) { await statusPollLoop() }
        .task(id: repoPath) { await autoFetchLoop(repoPath: repoPath) }
        .task(id: repoPath) {
            await directoryStore.runScheduler(activePath: repoPath, policy: schedulingPolicy)
        }
        .task {
            // The offline→online kick registers with the screen (not a
            // repo): the closure reads the open repo at fire time, and on
            // Welcome no registration exists — nothing to catch up there.
            directoryStore.networkObserver.onRecover = {
                Task { await resyncOnReconnect() }
            }
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
                    policy: schedulingPolicy,
                    onSelect: switchRepo,
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

    private func sidebar(repoPath: String) -> some View {
        VStack(spacing: 0) {
            RepoTabBar(
                selection: $tab,
                changesCount: store.status?.files.count ?? 0
            )

            Divider()

            switch tab {
            case .changes:
                ChangesSidebar(
                    repoPath: repoPath,
                    files: store.status?.files ?? [],
                    commitStore: commitStore,
                    selectedPath: $selectedPath,
                    onWorkingTreeChanged: { await store.refresh() },
                    onError: { store.errorMessage = $0 },
                    onRunInTerminal: terminalStore.run
                )
            case .history:
                HistorySidebar(
                    commits: store.commits,
                    status: store.status,
                    selectedSha: $selectedSha,
                    onReachEnd: { Task { await store.loadMoreHistory() } },
                    onAmend: startAmending,
                    onUndo: { undoCommit($0, in: repoPath) },
                    onCheckout: { checkoutCommit($0, in: repoPath) }
                )
            }
        }
    }

    private func mainContent(repoPath: String) -> some View {
        VStack(spacing: 0) {
            switch tab {
            case .changes:
                ChangesDetailPane(
                    repoPath: repoPath,
                    files: store.status?.files ?? [],
                    selectedPath: selectedPath,
                    workingTreeEpoch: store.workingTreeEpoch
                )
            case .history:
                HistoryDetailPane(
                    repoPath: repoPath,
                    commits: store.commits,
                    selectedSha: selectedSha
                )
            }

            // Outside the tab switch, so the shell survives a tab change.
            TerminalDock(repoPath: repoPath, store: terminalStore)
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

    /// The status poll, ported from the Tauri client: how changes made
    /// outside the app — the terminal, an editor — appear by themselves.
    /// Sequential by construction (each tick awaits the last), so the Tauri
    /// in-flight guard has no equivalent here. Deliberate divergence from
    /// the Tauri 2 s cadence (FRONTEND.md §8): the poll never stops — the
    /// policy's ladder slows it while unfocused and again while the window
    /// is hidden, so refocusing reveals a current screen instead of a
    /// catch-up. The interval is re-read per tick, so a focus or visibility
    /// change applies within one old interval; the activate resync covers
    /// the gap sooner anyway.
    @MainActor
    private func statusPollLoop() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: schedulingPolicy.statusPollInterval)
            if Task.isCancelled { return }
            guard schedulingPolicy.canPollStatus, !store.isLoading else { continue }
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

    /// One fetch on opening a repository, so ahead/behind reflect the remote
    /// within moments instead of waiting out an auto-fetch interval — the
    /// immediate startup fetch the Tauri client also runs, and the reason it
    /// happens even with auto-fetch off.
    ///
    /// Gated exactly like every other automatic fetch, which it was not
    /// before: skipped while offline or inside the breaker's backoff window,
    /// and skipped for a repository with no remote, whose fetch could only
    /// ever fail — `get_remote` answers `"origin"` for a remote-less repo, so
    /// the attempt is not merely useless but actively poisons the breaker for
    /// every other repo. Its outcome feeds the breaker, which is the contract
    /// for a real network attempt (the Tauri client's on-switch sync already
    /// reported its result; this one used to throw it away).
    ///
    /// Waits for the open to settle first: `.task(id: repoPath)` is started by
    /// `repoPath` being published, which happens before status exists, and a
    /// gate that reads `nil` is a gate that doesn't run.
    @MainActor
    private func warmUpFetch(repoPath: String) async {
        await store.awaitLoadSettled()
        guard store.repoPath == repoPath,
            store.status?.hasRemote == true,
            directoryStore.shouldAttemptBackground
        else { return }
        // `nil` = no fetch ran, which tells the breaker nothing (see
        // `silentFetch`). Status reloads only when one ran and reached the
        // remote — nothing can have moved otherwise.
        guard let reached = await syncStore.silentFetch(repoPath: repoPath) else { return }
        directoryStore.breaker.record(success: reached)
        if reached { await store.refresh() }
    }

    /// Auto-fetch for the open repository, driven by `auto_fetch` and
    /// `fetch_interval_ms` read from the shared `AppConfigStore` each tick —
    /// every Settings save reloads that store in-process, so a change in the
    /// Settings window still applies within one interval: the re-arm the
    /// Tauri client never got (it reads the pair once per repo switch).
    /// Edits made from the *Tauri* client arrive via the activation
    /// resync's reload instead of the per-tick file read this loop used to
    /// do. While disabled, the loop idles on a 30 s re-check instead of
    /// exiting, so flipping the toggle revives it.
    /// Fetches are held back while typing (a fetch can reorder the file
    /// list mid-keystroke), while the breaker is open, and — a deliberate
    /// improvement over Tauri — for repos with no remote, whose fetch could
    /// only ever fail and poison the breaker. Runs under `canAutoFetch`
    /// (FRONTEND.md §8): unlike the old `NSApp.isActive` gate, neither
    /// losing focus nor a hidden window stops it — hiding stretches the
    /// interval instead, so ahead/behind are already right on return.
    @MainActor
    private func autoFetchLoop(repoPath: String) async {
        while !Task.isCancelled {
            let config = appConfig.config
            let intervalMs = config?.autoFetch == true ? (config?.fetchIntervalMs ?? 0) : 0
            // The 30 s idle re-check while disabled is deliberately not
            // stretched: it fetches nothing, it only re-arms the toggle.
            let interval: Duration =
                intervalMs > 0
                ? schedulingPolicy.autoFetchInterval(configured: .milliseconds(Int64(intervalMs)))
                : .seconds(30)
            try? await Task.sleep(for: interval)
            if Task.isCancelled { return }
            guard intervalMs > 0,
                schedulingPolicy.canAutoFetch,
                !isTextInputFocused,
                store.status?.hasRemote == true,
                directoryStore.shouldAttemptBackground
            else { continue }
            if let reached = await syncStore.silentFetch(repoPath: repoPath) {
                directoryStore.breaker.record(success: reached)
            }
            await store.refreshQuietly()
        }
    }

    /// Coming back to the app: reload the shared config (Settings edits made
    /// from the Tauri client land here — the open diff re-keys itself if a
    /// diff setting changed), fetch, refresh status silently, force the open
    /// diff to reload (a file can change on disk without its status row
    /// changing), and give the most-recent repos' badges a throttled
    /// catch-up — the Tauri client's `resyncOnActive`.
    ///
    /// Re-asking the AI provider rides along, but ahead of the guards below:
    /// they exist to keep a network operation from being stomped, and a
    /// provider probe stomps nothing. Behind them, an activation that happened
    /// to land during a fetch would leave Generate dead until the user thought
    /// to leave and come back again.
    @MainActor
    private func resyncOnActivate() async {
        // Only while something is blocking, so a ready provider costs nothing
        // on every activation. This is what makes a *disabled* Generate safe to
        // ship: every way of fixing an unready provider leaves this app —
        // signing in opens a browser, installing the CLI or starting Ollama
        // happens in a terminal — so coming back is exactly when the answer can
        // have changed. Without it the button stays dead after the user has
        // already fixed the problem, which is worse than never disabling it.
        if commitStore.blockingProvider != nil {
            await commitStore.refreshProviderStatus()
        }

        guard store.repoPath != nil, !isResyncing, syncStore.activeOperation == nil else {
            return
        }
        isResyncing = true
        defer { isResyncing = false }

        await appConfig.reload()
        if let repoPath = store.repoPath,
            store.status?.hasRemote == true,
            directoryStore.shouldAttemptBackground
        {
            if let reached = await syncStore.silentFetch(repoPath: repoPath) {
                directoryStore.breaker.record(success: reached)
            }
        }
        await store.refreshQuietly(forceDiffReload: true)
        await directoryStore.refocusSweep(
            activePath: store.repoPath,
            policy: schedulingPolicy
        )
    }

    /// The offline→online kick, ported from the Tauri client's
    /// `initConnectivity` recovery: the OS says the network is back, so the
    /// breaker's backoff window no longer describes reality — close it,
    /// catch the active repo up quietly, and give the top tier's badges
    /// their throttled sweep. Each piece runs under the same policy
    /// predicate as its background twin, so Wi-Fi returning mid-transfer or
    /// while the app is inactive changes nothing those predicates protect.
    @MainActor
    private func resyncOnReconnect() async {
        directoryStore.breaker.reset()
        guard let repoPath = store.repoPath else { return }
        if schedulingPolicy.canAutoFetch, store.status?.hasRemote == true {
            if let reached = await syncStore.silentFetch(repoPath: repoPath) {
                directoryStore.breaker.record(success: reached)
            }
            await store.refreshQuietly()
        }
        await directoryStore.refocusSweep(activePath: repoPath, policy: schedulingPolicy)
    }

    // MARK: History row actions

    /// Seed the composer from the commit and show it — the rewrite itself
    /// happens when the user commits, so this only changes what's on screen.
    @MainActor
    private func startAmending(_ commit: CommitInfo) {
        commitStore.startAmending(commit)
        tab = .changes
    }

    /// Drop the last commit and hand its message to the composer, so the
    /// changes it left in the working tree can be re-committed without
    /// retyping. Runs immediately: nothing is lost that the composer and the
    /// working tree don't now hold.
    @MainActor
    private func undoCommit(_ commit: CommitInfo, in repoPath: String) {
        Task {
            do {
                try await GitBridge.undoCommit(in: repoPath)
                commitStore.restoreDraft(from: commit)
                tab = .changes
                await store.refresh()
                await branchStore.load(repoPath: repoPath)
            } catch {
                store.errorMessage = error.displayMessage
            }
        }
    }

    /// Check out a past commit. HEAD detaches, so the branch menu's checkmark
    /// and the sync ladder both change — hence the branch reload alongside
    /// the status refresh.
    @MainActor
    private func checkoutCommit(_ commit: CommitInfo, in repoPath: String) {
        Task {
            do {
                try await GitBridge.checkout(in: repoPath, commit: commit.sha)
                await store.refresh()
                await branchStore.load(repoPath: repoPath)
            } catch {
                store.errorMessage = error.displayMessage
            }
        }
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
