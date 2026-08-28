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
    /// Set when there are no lines to show, and which of the three unrelated
    /// reasons it is — the pane names the actual cause instead of covering
    /// all of them with one caption.
    private(set) var emptyReason: EmptyDiffReason?

    /// Set when core withheld the diff for its size. The pane explains it and
    /// offers to render it anyway, rather than hanging on it.
    private(set) var sizeGuard: DiffSizeGuard?

    /// Whether the user asked for the withheld diff anyway.
    ///
    /// Deliberately not sticky: `load` clears it, so the decision applies to
    /// the file it was made about and moving to another one gets the guard
    /// back. Without this the guard *refuses* rather than withholds — a file
    /// with one long line (a minified bundle, a lock file, a long Markdown
    /// paragraph) would simply have no way to be read.
    private(set) var showAnyway = false

    /// What the in-flight `read` should ask for. Separate from `showAnyway`,
    /// which describes what is *on screen*: the flag has to be set before the
    /// load and published only once its result lands.
    private var pendingShowAnyway = false

    /// Where the current load stands. The view's rule: show content whenever
    /// `payload != nil`, and on `loading(slow: true)` dim it and lay a spinner
    /// over it rather than replacing it. A fast first load, with nothing old to
    /// keep showing, stays blank.
    enum Phase: Equatable {
        case idle
        case loading(slow: Bool)
        case failed(String)
    }

    private(set) var phase: Phase = .idle

    /// How long a load may run before the pane says so — the Tauri client's
    /// `SLOW_DIFF_THRESHOLD_MS` (150), itself ported from GitHub Desktop.
    /// Crossing it dims the diff on screen; the payload is kept either way, so
    /// a slow load that lands unchanged is absorbed by the equality skip and
    /// the user keeps their scroll position.
    static let slowLoadThreshold: Duration = .milliseconds(150)

    /// Guards against a superseded load writing over a newer one: the blocking
    /// FFI call cannot be interrupted, so a stale task may still resume here
    /// after `.task(id:)` has already started its replacement. The slow-load
    /// timer races against it under the same rule.
    private var generation = 0

    /// Load `file`'s diff from `target`. What's on screen stays until the
    /// result lands — and stays untouched entirely when the result is equal.
    ///
    /// `hideWhitespace` picks the whitespace-ignored raw read for working-tree
    /// targets (commit diffs have no such variant — the caller passes `false`
    /// there); `highlight` off skips phase two and drops any tokens on screen,
    /// the Tauri client's `syntaxHighlighting` guard. Both settings re-key
    /// `DiffView`'s load task, so a toggle flows through this same seamless
    /// path — the Tauri client's `lastHideWhitespace` effect, for free.
    func load(
        repoPath: String,
        file: FileEntry,
        target: DiffTarget,
        hideWhitespace: Bool,
        highlight: Bool,
        ignoringSizeGuard: Bool = false
    ) async {
        generation += 1
        let current = generation
        phase = .loading(slow: false)
        showAnyway = false
        pendingShowAnyway = ignoringSizeGuard

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
            let parsed = try await read(
                repoPath: repoPath, file: file, target: target, hideWhitespace: hideWhitespace)
            guard current == generation else { return }

            // Nothing to render, and core says which of the three unrelated
            // reasons it is — including the whitespace-only case, which the
            // pane would otherwise report as "no changes" while the setting
            // was what hid them.
            if let reason = parsed.emptyReason {
                payload = nil
                rows = []
                tokens = nil
                emptyReason = reason
                sizeGuard = nil
                phase = .idle
                return
            }
            // Withheld for its size rather than empty: the pane offers to
            // render it anyway instead of hanging on it.
            if let guardInfo = parsed.sizeGuard {
                payload = nil
                rows = []
                tokens = nil
                emptyReason = nil
                sizeGuard = guardInfo
                phase = .idle
                return
            }
            emptyReason = nil
            sizeGuard = nil
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

            // Highlighting off skips phase two entirely and drops whatever
            // colour is on screen — plain text stays, exactly the Tauri
            // client's `if (!sh) return` after publishing the plain render.
            guard highlight else {
                if tokens != nil { tokens = nil }
                return
            }

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
            emptyReason = nil
            sizeGuard = nil
            phase = .failed(error.displayMessage)
        }
    }

    /// Re-request the diff the size guard withheld, this time rendering it.
    ///
    /// A second load rather than a flag on the first: the guard's whole point
    /// is that core did not parse the patch, so there is nothing held back to
    /// reveal.
    func loadIgnoringSizeGuard(
        repoPath: String,
        file: FileEntry,
        target: DiffTarget,
        hideWhitespace: Bool,
        highlight: Bool
    ) async {
        await load(
            repoPath: repoPath,
            file: file,
            target: target,
            hideWhitespace: hideWhitespace,
            highlight: highlight,
            ignoringSizeGuard: true
        )
        showAnyway = true
    }

    /// The one read, with the options this client asks for.
    private func read(
        repoPath: String,
        file: FileEntry,
        target: DiffTarget,
        hideWhitespace: Bool
    ) async throws -> DiffPayload {
        let options = DiffOptions(
            html: false, sideBySide: false, showAnyway: pendingShowAnyway)
        switch target {
        case .workingTree:
            return try await GitBridge.parsedDiff(
                of: repoPath, for: file, hideWhitespace: hideWhitespace, options: options)
        case .commit(let sha):
            return try await GitBridge.parsedCommitDiff(
                in: repoPath, sha: sha, filePath: file.path, options: options)
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
