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
    /// `HEAD` against the working tree. `epoch` is
    /// `RepoStore.workingTreeEpoch`: "the working tree may have changed" —
    /// it is what re-keys the view and re-reads a diff whose file row looks
    /// unchanged; the equality skip below makes a bump for an *actually*
    /// unchanged file publish nothing.
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
///
/// Reloads are **seamless**, the Tauri/GitHub-Desktop contract
/// (`SeamlessDiffSwitcher`): the previous diff stays on screen while the
/// replacement loads, a spinner appears only when the load outlives
/// `slowLoadThreshold`, and a result equal to what's shown publishes nothing —
/// rows, scroll position, and tokens all survive untouched. That skip is the
/// other half of `RepoStore`'s epoch contract: the epoch signals *possibility*
/// (the working tree may have changed), and this store is where reality is
/// checked, so epoch bumps on refocus or status noise cost one subprocess and
/// zero repaints.
@MainActor
@Observable
final class DiffStore {
    /// What's on screen. Deliberately *not* cleared when a load starts —
    /// clearing is what blanked the pane on every status tick.
    private(set) var payload: DiffPayload?
    private(set) var rows: [DiffRow] = []
    /// One entry per row once tokenization lands; `nil` while in flight.
    private(set) var tokens: [[Token]]?
    /// Set when the diff parsed to nothing textual (e.g. a pure mode change).
    private(set) var isEmpty = false

    /// Where the current load stands. The view's rule: show content whenever
    /// `payload != nil`, fall back to the spinner only on `loading(slow: true)`
    /// — or on a fast first load, where there is nothing old to keep showing.
    enum Phase: Equatable {
        case idle
        case loading(slow: Bool)
        case failed(String)
    }

    private(set) var phase: Phase = .idle

    /// How long a load may run before the pane trades the old diff for a
    /// spinner — the Tauri client's `SLOW_DIFF_THRESHOLD_MS` (150), itself
    /// ported from GitHub Desktop. (One deliberate improvement: the Tauri
    /// client also *drops* the old diff at the threshold, so a slow load that
    /// lands unchanged repaints from scratch; here the payload is kept, so
    /// the equality skip still preserves scroll in that case.)
    static let slowLoadThreshold: Duration = .milliseconds(150)

    /// Guards against a superseded load writing over a newer one: the blocking
    /// FFI call cannot be interrupted, so a stale task may still resume here
    /// after `.task(id:)` has already started its replacement. The slow-load
    /// timer races against it under the same rule.
    private var generation = 0

    /// Load `file`'s diff from `target`. What's on screen stays until the
    /// result lands — and stays untouched entirely when the result is equal.
    func load(repoPath: String, file: FileEntry, target: DiffTarget) async {
        generation += 1
        let current = generation
        phase = .loading(slow: false)

        // The slow-load fallback: just another racer against `generation`.
        // Unstructured on purpose — `.task(id:)` cancelling the load must not
        // also cancel the escalation for a blocking FFI call that is still
        // running; the guards make a stale timer a no-op either way.
        Task {
            try? await Task.sleep(for: Self.slowLoadThreshold)
            guard current == generation, phase == .loading(slow: false) else { return }
            phase = .loading(slow: true)
        }

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
                payload = nil
                rows = []
                tokens = nil
                isEmpty = true
                phase = .idle
                return
            }
            guard current == generation else { return }
            isEmpty = false
            if parsed != payload {
                // Rows are `Identifiable` by flat index, so SwiftUI diffs the
                // list in place instead of rebuilding it; tokens reset to nil
                // and the plain-text phase shows immediately — the two-phase
                // paint is the contract (FRONTEND.md §7).
                payload = parsed
                rows = Self.flatten(parsed.fileDiff)
                tokens = nil
            }
            phase = .idle

            // Phase two: syntax colour. Runs even when the payload was equal —
            // context lines can change colour when surrounding blob content
            // changed without the diff text changing. Swapped in place, and
            // only when actually different, so an unchanged file recolours
            // (or does nothing) without a flash.
            let tokenLines = await GitBridge.diffTokens(
                for: parsed.fileDiff,
                source: blobSource(repoPath: repoPath, target: target)
            )
            guard current == generation else { return }
            if tokenLines != tokens {
                tokens = tokenLines
            }
        } catch {
            guard current == generation else { return }
            // A failed reload must not leave a stale diff posing as current.
            payload = nil
            rows = []
            tokens = nil
            isEmpty = false
            phase = .failed(error.displayMessage)
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
