import Foundation
import Observation

/// Filter segments for the PR list — raw values are exactly gh's `--state`
/// filter values, passed straight through.
enum PullRequestFilter: String, CaseIterable, Identifiable {
    case open, closed, merged, all

    var id: Self { self }

    /// Segment label.
    var title: String { rawValue.capitalized }
}

/// Loading states for the PR list.
enum PullRequestListPhase: Equatable {
    case loading
    case failed(String)
    case loaded
}

/// One PR's lazily loaded CI checks.
enum PullRequestChecks {
    case loading
    /// gh produced no check data — usually its own "no checks reported…"
    /// line for a PR without CI, shown quietly rather than as a failure.
    case unavailable(String)
    case loaded([PrCheck])
}

extension PullRequest: Identifiable {
    public var id: UInt32 { number }
}

/// State for the Pull Requests tab: the filtered list, one selection, and a
/// per-PR CI-checks cache.
///
/// Lists load lazily — on tab visit, on filter change, on explicit reload —
/// and are never polled: PRs change on GitHub's timescale and every extra
/// `gh` call is a subprocess. Order is whatever `gh pr list` returned
/// (newest first), never re-sorted, like the retired TUI. Checks load per
/// PR on selection and are cached until the next list load.
///
/// Mutations return failure text (`nil` on success) instead of storing it —
/// the `BranchStore`/`SyncStore` contract; the view raises the alert.
@MainActor
@Observable
final class PullRequestStore {
    var filter: PullRequestFilter = .open
    private(set) var phase: PullRequestListPhase = .loading
    private(set) var pullRequests: [PullRequest] = []
    var selectedNumber: UInt32?
    private(set) var checks: [UInt32: PullRequestChecks] = [:]

    /// True while `gh pr checkout` runs; the tab disables its actions. Kept
    /// outside the sync slot like clone — it is not a push/pull, and the
    /// button's own busy state is the honest indicator.
    private(set) var isCheckingOut = false

    /// Bumped per load (and on reset) so a stale response can never
    /// overwrite a newer list or a switched repo's state.
    private var generation = 0

    var selected: PullRequest? {
        guard let selectedNumber else { return nil }
        return pullRequests.first { $0.number == selectedNumber }
    }

    /// Forget everything on repo switch.
    func reset() {
        filter = .open
        phase = .loading
        pullRequests = []
        selectedNumber = nil
        checks = [:]
        isCheckingOut = false
        generation += 1
    }

    /// Load the list for the current filter, dropping the checks cache —
    /// the one moment cached CI results can go stale on purpose.
    func load(repoPath: String) async {
        generation += 1
        let expected = generation
        phase = .loading
        checks = [:]
        do {
            let list = try await GitBridge.pullRequests(in: repoPath, state: filter.rawValue)
            guard generation == expected else { return }
            pullRequests = list
            phase = .loaded
            // Keep the selection if its PR is still listed; otherwise fall
            // to the first row so the detail pane is never empty.
            if !list.contains(where: { $0.number == selectedNumber }) {
                selectedNumber = list.first?.number
            }
            if let selectedNumber {
                await loadChecks(repoPath: repoPath, number: selectedNumber)
            }
        } catch {
            guard generation == expected else { return }
            pullRequests = []
            phase = .failed(error.displayMessage)
        }
    }

    /// Fetch one PR's checks unless already cached (or in flight), so
    /// moving through the list stays cheap — the retired TUI's behaviour.
    func loadChecks(repoPath: String, number: UInt32) async {
        guard checks[number] == nil else { return }
        checks[number] = .loading
        let expected = generation
        do {
            let rows = try await GitBridge.pullRequestChecks(in: repoPath, number: number)
            guard generation == expected else { return }
            checks[number] = .loaded(rows)
        } catch {
            guard generation == expected else { return }
            checks[number] = .unavailable(error.displayMessage)
        }
    }

    /// `gh pr checkout` — fetches the head ref and switches to a local
    /// tracking branch. The caller refreshes status and branches on
    /// success; a dirty working tree is git's refusal, surfaced verbatim.
    func checkout(repoPath: String, number: UInt32) async -> String? {
        guard !isCheckingOut else { return nil }
        isCheckingOut = true
        defer { isCheckingOut = false }
        do {
            try await GitBridge.checkoutPullRequest(in: repoPath, number: number)
            return nil
        } catch {
            return error.displayMessage
        }
    }
}
