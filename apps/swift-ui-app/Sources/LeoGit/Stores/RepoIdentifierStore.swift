import Foundation

/// The `owner/name` labels the repo pickers show, looked up from each
/// repository's remote and cached for the process's lifetime — the native
/// counterpart of the Tauri client's `stores/repoIdentifiers.ts`.
///
/// The cache is three-valued per path, deliberately: a missing key means "not
/// looked up yet", a stored `nil` means "looked up, and this repository has no
/// remote worth a label" — the row keeps its folder name and is never asked
/// again — and a value is the parsed pair. Collapsing the last two would
/// re-spawn a `git config` for every remote-less repository each time a list
/// rebuilt.
///
/// Lookups run `maxInFlight` at a time. Each one is a git subprocess, and a
/// picker over fifty repositories asking for fifty labels at once is the
/// unbounded fan-out that the same bound already removed from the badge
/// sweeps — spawned, here, at exactly the moment the user is waiting to pick.
///
/// The bound is the whole gate. Unlike the sweeps, this does **not** consult
/// `BackgroundSchedulingPolicy`: a sweep is deferrable work about repositories
/// nobody is looking at, while these are the labels of the rows on screen, and
/// pausing them would leave the list naming its rows by folder while a transfer
/// finished. `git config --get` touches no network and no working tree.
@MainActor
@Observable
final class RepoIdentifierStore {
    private static let maxInFlight = 4

    /// Looked-up paths only; see the three-valued rule above.
    private var identifiers: [String: RepoIdentifier?] = [:]

    /// How many answers have landed. A reader that derives something expensive
    /// from the labels watches this instead of the dictionary: comparing one
    /// `Int` is what makes "did the labels move?" cheap enough to ask on every
    /// pass.
    private(set) var revision = 0

    private var queue: [String] = []
    private var queued: Set<String> = []
    private var inFlight = 0

    /// The remote's `owner`/`name` for `path`, or `nil` while it is unknown
    /// *or* known not to exist — a caller that needs to tell those apart is
    /// asking the wrong question: both mean "label this row by its folder".
    func identifier(of path: String) -> RepoIdentifier? {
        identifiers[path] ?? nil
    }

    /// The row's own label: the remote's repository name where one is known,
    /// the folder's name otherwise.
    func label(of path: String) -> String {
        identifier(of: path)?.name ?? RepoDirectoryStore.displayName(of: path)
    }

    /// The row's owner-qualified label, for the tooltip and for the search
    /// index — the query has to find what the row displays, and a row shows
    /// its owner as soon as another row shares its name.
    func fullLabel(of path: String) -> String {
        guard let id = identifier(of: path) else {
            return RepoDirectoryStore.displayName(of: path)
        }
        return "\(id.owner)/\(id.name)"
    }

    /// Every label a user might reasonably type for this row, for
    /// `filter_repos`. One entry when the two coincide, so a repository with
    /// no remote isn't matched twice against the same string.
    func searchLabels(of path: String) -> [String] {
        let label = label(of: path)
        let full = fullLabel(of: path)
        return label == full ? [label] : [label, full]
    }

    /// Look up whatever in `paths` hasn't been looked up already.
    ///
    /// Asked for the whole list a picker is about to show rather than for the
    /// rows currently on screen: the labels are *searchable*, so a query has
    /// to reach a row the user has never scrolled to. The concurrency bound
    /// is what keeps that affordable.
    func ensure(_ paths: [String]) {
        for path in paths where identifiers.index(forKey: path) == nil && !queued.contains(path) {
            queued.insert(path)
            queue.append(path)
        }
        drain()
    }

    /// Start workers up to the bound. Each drains the shared queue until it
    /// runs dry, so a late arrival joins a running worker instead of waiting
    /// for a free slot.
    private func drain() {
        while inFlight < Self.maxInFlight, !queue.isEmpty {
            inFlight += 1
            Task { await work() }
        }
    }

    private func work() async {
        defer { inFlight -= 1 }
        while !queue.isEmpty {
            let path = queue.removeFirst()
            // `updateValue`, not a subscript assignment: assigning `nil`
            // through the subscript of a dictionary whose *value* is optional
            // removes the key, which would turn every "no remote" answer back
            // into "not looked up yet" and re-queue it forever.
            identifiers.updateValue(await GitBridge.identifier(of: path), forKey: path)
            revision += 1
            queued.remove(path)
        }
    }
}
