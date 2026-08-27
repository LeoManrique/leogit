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
@MainActor
@Observable
final class CloneStore {
    var source: CloneSource = .github
    var url = ""

    /// Parent folder the clone lands in. Seeded from the shared
    /// `last_clone_dir` → first scan path → `~/Dev`, like the Tauri dialog.
    var destinationDir = ""

    var filter = ""
    var selectedRepoID: String?

    private(set) var githubRepos: [GhRepo] = []
    private(set) var listPhase: GitHubListPhase = .loading

    /// `"recent"` (last push, newest first) or `"name"` — shared with the
    /// Tauri dialog through `repos-state.json`.
    private(set) var sortMode = "recent"

    private(set) var isCloning = false

    /// Aggregate percent for a URL clone; `nil` before the first tick and
    /// always for a gh clone, which streams nothing — indeterminate bar.
    private(set) var progressPercent: Double?

    /// The raw git progress line, verbatim.
    private(set) var progressText: String?

    private(set) var errorMessage: String?

    private var hasPrepared = false

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

    /// The GitHub rows after the sort toggle and the filter — filtering on
    /// `owner/name`, like the Tauri dialog.
    var visibleRepos: [GhRepo] {
        let sorted =
            sortMode == "name"
            ? githubRepos.sorted {
                $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
            }
            // ISO-8601 timestamps order lexically, newest first.
            : githubRepos.sorted { $0.pushedAt > $1.pushedAt }
        let query = filter.trimmingCharacters(in: .whitespaces).lowercased()
        guard !query.isEmpty else { return sorted }
        return sorted.filter { $0.nameWithOwner.lowercased().contains(query) }
    }

    /// Seed the destination and sort mode from shared state, then load the
    /// GitHub list. Once per sheet.
    func prepare() async {
        guard !hasPrepared else { return }
        hasPrepared = true

        let state = try? await GitBridge.reposState()
        if let mode = state?.cloneSortMode, mode == "recent" || mode == "name" {
            sortMode = mode
        }
        if let dir = state?.lastCloneDir, !dir.isEmpty {
            destinationDir = dir
        } else if let first = (try? await GitBridge.appConfig())?.scanPaths.first {
            destinationDir = first
        } else {
            destinationDir = "~/Dev"
        }

        await loadGitHubList()
    }

    /// (Re)load the gh repository list — the initial load and the Retry
    /// button share this.
    func loadGitHubList() async {
        listPhase = .loading
        do {
            githubRepos = try await GitBridge.githubRepositories(limit: 200)
            listPhase = .loaded
        } catch {
            listPhase = .failed(error.displayMessage)
        }
    }

    /// Flip recent ↔ name and persist the choice for both clients.
    /// Best-effort persistence: the toggle itself must never fail.
    func toggleSortMode() {
        sortMode = sortMode == "recent" ? "name" : "recent"
        let mode = sortMode
        Task { try? await GitBridge.setCloneSortMode(mode) }
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
