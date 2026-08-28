import Foundation

/// Identity for the GitHub tab's rows — `owner/name` is unique per account.
extension GhRepo: Identifiable {
    public var id: String { nameWithOwner }
}

/// Where the Clone sheet takes its source from — the Tauri dialog's two tabs.
enum CloneSource: String, CaseIterable, Identifiable {
    case github = "GitHub"
    case url = "URL"

    var id: Self { self }
}

/// The GitHub tab's list lifecycle. `gh repo list` can fail in dialog-worthy
/// ways (gh missing, unauthenticated, offline), so failure is a rendered
/// state with a Retry, not an alert.
enum GitHubListPhase: Equatable {
    case loading
    case failed(String)
    case loaded
}

/// Observable state for the Clone sheet: source selection, the gh-backed
/// repository list, destination handling, and the clone itself.
///
/// Ports the Tauri `CloneOverlay` semantics: the repo folder name is derived
/// client-side (from the selected GitHub repo or the URL), the destination
/// field holds the PARENT folder, and core receives the full target path.
/// There is no cancel — once started, a clone runs to completion or to
/// core's 600 s timeout, and the sheet must stay up to show the outcome.
///
/// The store **outlives the sheet** (`ContentView` owns it), which is what
/// makes the GitHub list a once-per-run cache rather than a ~20 s dead zone on
/// every open. `reopen()` is therefore where per-open state is cleared, and
/// what it deliberately does *not* clear is the list, the sort mode, and the
/// chosen tab: which tab you clone from is a preference, and re-fetching an
/// unchanged list is the cost the cache exists to avoid. The Refresh button is
/// how a repository created since launch is reached.
@MainActor
@Observable
final class CloneStore {
    var source: CloneSource = .github {
        didSet {
            guard source != oldValue else { return }
            // A URL-tab failure has nothing to say about the GitHub tab.
            // Leaving it up reported the last URL clone's error over a list
            // the user had just switched to.
            errorMessage = nil
        }
    }

    var url = ""

    /// Parent folder the clone lands in. Seeded from the shared
    /// `last_clone_dir` → first scan path → `~/Dev`, like the Tauri dialog.
    var destinationDir = ""

    var filter = "" {
        didSet {
            guard filter != oldValue else { return }
            recomputeVisibleRepos()
        }
    }

    var selectedRepoID: String?

    private(set) var githubRepos: [GhRepo] = []

    /// The rows on screen: `githubRepos` narrowed by `filter` and ordered by
    /// `sortMode`. Recomputed when one of those three changes rather than on
    /// every body evaluation — the list runs to 200 rows and the alternative
    /// re-sorted all of them per keystroke *per layout pass*.
    private(set) var visibleRepos: [GhRepo] = []

    private(set) var listPhase: GitHubListPhase = .loading

    /// Last-push order (newest first) or A-Z — shared with the Tauri dialog
    /// through `repos-state.json`.
    private(set) var sortMode: SortMode = .recent

    private(set) var isCloning = false

    /// Aggregate percent for a URL clone; `nil` before the first tick and
    /// always for a gh clone, which streams nothing — indeterminate bar.
    private(set) var progressPercent: Double?

    /// The raw git progress line, verbatim.
    private(set) var progressText: String?

    private(set) var errorMessage: String?

    /// Whether the shared state file has been read for this process — the
    /// destination seed and the sort mode.
    private var hasReadSharedState = false

    /// Whether the GitHub list has been fetched **successfully**. A failed
    /// load clears it, so the next open retries rather than reopening onto a
    /// stale error — including a Refresh that failed after a load that had
    /// worked, which would otherwise leave the cache flagged as loaded and the
    /// error standing until the app was restarted.
    private var hasLoadedList = false

    /// A load already in flight. `reopen()` is not cancellation-aware and the
    /// query it starts runs for seconds, so closing and reopening the sheet
    /// would otherwise start a second `gh repo list` — and each completion
    /// would race the others to assign the list.
    private var isLoadingList = false

    /// What the URL tab is about to clone, as core reads it — `nil` when the
    /// field holds nothing cloneable, which is also what disables the button.
    /// The rule lives in core because this pair was written twice and the two
    /// copies had already drifted on `.git` shorthand, on whitespace, and on
    /// a trailing slash, while sharing two shapes both of them enabled Clone
    /// for and then failed on.
    private var urlTarget: CloneTarget? {
        GitBridge.cloneTarget(rawURL: url, parent: destinationDir)
    }

    /// The folder name the clone will create — the whole validation rule,
    /// with a non-empty destination: no scheme checks, no reachability
    /// probes. Anything deeper is git's call, surfaced after the fact.
    var repoName: String {
        switch source {
        case .github: selectedRepo?.name ?? ""
        case .url: urlTarget?.repoName ?? ""
        }
    }

    var selectedRepo: GhRepo? {
        githubRepos.first { $0.nameWithOwner == selectedRepoID }
    }

    /// Live preview of the full clone target: `<destination>/<name>`.
    var targetPath: String {
        switch source {
        case .github:
            guard let name = selectedRepo?.name else { return "" }
            return GitBridge.clonePath(parent: destinationDir, repoName: name) ?? ""
        case .url:
            return urlTarget?.targetPath ?? ""
        }
    }

    var canClone: Bool {
        !isCloning && !targetPath.isEmpty
    }

    /// Everything a fresh open should start clean, run each time the sheet is
    /// presented. Carrying inputs across opens meant reopening onto a stale
    /// selection with Clone already lit — one Return away from cloning a
    /// repository the user had not looked at.
    ///
    /// Also the first load's trigger, because a first open has no cached list.
    func reopen() async {
        url = ""
        filter = ""
        selectedRepoID = nil
        errorMessage = nil

        if !hasReadSharedState {
            hasReadSharedState = true
            let state = try? await GitBridge.reposState()
            sortMode = SortMode(persisted: state?.cloneSortMode) ?? .recent
            destinationDir = await defaultDestination(lastCloneDir: state?.lastCloneDir)
        }

        if !hasLoadedList, !isLoadingList {
            await loadGitHubList()
        }
    }

    /// Where a clone lands unless the user says otherwise: wherever the last
    /// one went, then the first scan path, then `~/Dev`.
    private func defaultDestination(lastCloneDir: String?) async -> String {
        if let lastCloneDir, !lastCloneDir.isEmpty { return lastCloneDir }
        if let first = (try? await GitBridge.appConfig())?.scanPaths.first { return first }
        return "~/Dev"
    }

    /// (Re)load the gh repository list — the first open, the Retry button and
    /// the Refresh button all share this.
    func loadGitHubList() async {
        guard !isLoadingList else { return }
        isLoadingList = true
        defer { isLoadingList = false }

        listPhase = .loading
        do {
            githubRepos = try await GitBridge.githubRepositories(limit: 200)
            hasLoadedList = true
            listPhase = .loaded
            recomputeVisibleRepos()
        } catch {
            // The cached rows stay: they are still the last true answer, and
            // a failed refresh is a reason to say so, not to forget them.
            hasLoadedList = false
            listPhase = .failed(error.displayMessage)
        }
    }

    /// Flip recent ⇄ A-Z and persist the choice for both clients.
    /// Best-effort persistence: the toggle itself must never fail.
    func toggleSortMode() {
        sortMode = sortMode == .recent ? .name : .recent
        recomputeVisibleRepos()
        let mode = sortMode.rawValue
        Task { try? await GitBridge.setCloneSortMode(mode) }
    }

    /// Filter first, then sort: the query is what shrinks the list, so
    /// ordering the rows it is about to discard is work thrown away.
    private func recomputeVisibleRepos() {
        let query = filter.trimmingCharacters(in: .whitespaces)
        let matched =
            query.isEmpty
            ? githubRepos
            : githubRepos.filter { $0.nameWithOwner.localizedCaseInsensitiveContains(query) }

        visibleRepos = matched.sorted { lhs, rhs in
            // ISO-8601 timestamps order lexically, newest first.
            if sortMode == .recent, lhs.pushedAt != rhs.pushedAt {
                return lhs.pushedAt > rhs.pushedAt
            }
            switch NameCollation.compare(lhs.name, rhs.name) {
            case .orderedAscending: return true
            case .orderedDescending: return false
            // Swift's sort is not stable, so equal keys need a tiebreak of
            // their own or two same-second pushes swap places between passes.
            case .orderedSame: return lhs.nameWithOwner < rhs.nameWithOwner
            }
        }
    }

    /// Run the clone; returns the fresh repository's path on success, `nil`
    /// on failure (with `errorMessage` set for the sheet to render).
    func clone() async -> String? {
        guard canClone else { return nil }
        isCloning = true
        errorMessage = nil
        progressPercent = nil
        progressText = nil
        defer {
            isCloning = false
            progressPercent = nil
            progressText = nil
        }

        // The PARENT of the target, taken from the derived path so it can't
        // disagree with what the clone actually creates. A clone into the root
        // leaves nothing before the name — the parent is "/", not "", which
        // would come back as an empty destination next time.
        let target = targetPath
        let dropped = String(target.dropLast(repoName.count + 1))
        let parent = dropped.isEmpty ? "/" : dropped
        do {
            let repoPath: String
            if source == .github, let repo = selectedRepo {
                repoPath = try await GitBridge.githubClone(
                    nameWithOwner: repo.nameWithOwner,
                    into: target,
                    onProgress: progressHandler()
                )
            } else {
                guard let derived = urlTarget else { return nil }
                repoPath = try await GitBridge.cloneRepository(
                    url: derived.normalizedUrl,
                    into: target,
                    onProgress: progressHandler()
                )
            }
            // The PARENT folder, not the repo path — what the next Clone
            // sheet (in either client) pre-fills.
            try? await GitBridge.setLastCloneDir(parent)
            return repoPath
        } catch {
            errorMessage = error.displayMessage
            return nil
        }
    }

    /// Ticks arrive on a Rust background thread; hop to the main actor and
    /// drop stragglers once the clone ended.
    private func progressHandler() -> @Sendable (SyncProgress) -> Void {
        { [weak self] tick in
            Task { @MainActor [weak self] in
                guard let self, self.isCloning else { return }
                self.progressPercent = Double(tick.percent)
                self.progressText = tick.text
            }
        }
    }
}
