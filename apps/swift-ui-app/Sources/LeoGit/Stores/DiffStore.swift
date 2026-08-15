import Foundation

/// One line of a rendered diff, addressed by its flat index.
///
/// The flat index — hunks concatenated, each hunk's `@@` header first — is the
/// contract every parallel array in core keys on: the token lines returned by
/// `tokenize_diff` line up with these rows one-to-one.
struct DiffRow: Identifiable {
    let id: Int
    let line: DiffLine
}

/// Where a file's diff is read from.
///
/// The two sources the Tauri client's `DiffViewer` serves: the working tree
/// (the Changes tab) and a commit against its first parent (the History
/// detail). Carried whole through `DiffView` into `DiffStore`, so the raw-diff
/// read and the blob source the tokenizer uses can never disagree.
enum DiffTarget: Equatable {
    /// `HEAD` against the working tree. `epoch` is `RepoStore.statusEpoch`:
    /// it changes on every status reload, which is what re-keys the view and
    /// re-reads a diff whose file row looks unchanged.
    case workingTree(epoch: Int)
    /// One commit's changes, first-parent for merges.
    case commit(sha: String)
}

/// Observable state for the diff of one selected file.
///
/// Loads in two phases, same as the Tauri client: the parsed structure renders
/// immediately as plain text, then syntax tokens — which may read and parse
/// whole blobs — arrive and recolour in place. All mutation happens on the
/// main actor; the blocking git work hops off it inside `GitBridge`.
@MainActor
@Observable
final class DiffStore {
    private(set) var payload: DiffPayload?
    private(set) var rows: [DiffRow] = []
    /// One entry per row once tokenization lands; `nil` while in flight.
    private(set) var tokens: [[Token]]?
    private(set) var isLoading = false
    /// Set when the diff parsed to nothing textual (e.g. a pure mode change).
    private(set) var isEmpty = false
    private(set) var errorMessage: String?

    /// Guards against a superseded load writing over a newer one: the blocking
    /// FFI call cannot be interrupted, so a stale task may still resume here
    /// after `.task(id:)` has already started its replacement.
    private var generation = 0

    /// Load `file`'s diff from `target`, replacing whatever was shown.
    func load(repoPath: String, file: FileEntry, target: DiffTarget) async {
        generation += 1
        let current = generation
        isLoading = true
        payload = nil
        rows = []
        tokens = nil
        isEmpty = false
        errorMessage = nil

        do {
            let raw: String
            switch target {
            case .workingTree:
                raw = try await GitBridge.rawDiff(of: repoPath, for: file)
            case .commit(let sha):
                raw = try await GitBridge.commitDiff(in: repoPath, sha: sha, filePath: file.path)
            }
            guard current == generation else { return }

            guard let parsed = await GitBridge.parsedDiff(from: raw) else {
                guard current == generation else { return }
                isEmpty = true
                isLoading = false
                return
            }
            guard current == generation else { return }
            payload = parsed
            rows = Self.flatten(parsed.fileDiff)
            isLoading = false

            // Phase two: syntax colour. The structure above is already on
            // screen; failure here just leaves the diff plain.
            let tokenLines = await GitBridge.diffTokens(
                for: parsed.fileDiff,
                source: blobSource(repoPath: repoPath, target: target)
            )
            guard current == generation else { return }
            tokens = tokenLines
        } catch {
            guard current == generation else { return }
            errorMessage = error.displayMessage
            isLoading = false
        }
    }

    /// Where the tokenizer reads whole blobs from — disk for the working
    /// tree, the commit's own trees for history, so a file later rewritten
    /// still colours as it was at that commit.
    private func blobSource(repoPath: String, target: DiffTarget) -> BlobSource {
        switch target {
        case .workingTree: .workingTree(repoPath: repoPath)
        case .commit(let sha): .commit(repoPath: repoPath, sha: sha)
        }
    }

    /// The flat row list every parallel array is keyed against.
    private static func flatten(_ fileDiff: FileDiff) -> [DiffRow] {
        var rows: [DiffRow] = []
        for hunk in fileDiff.hunks {
            for line in hunk.lines {
                rows.append(DiffRow(id: rows.count, line: line))
            }
        }
        return rows
    }
}
