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

/// One row of the split layout: which flat index each column shows, or none
/// where that side has no line and the cell is filler.
///
/// Core's `SbsPair` with an identity and Swift's own index width. It carries
/// indices rather than lines on purpose — both columns read the *same* row
/// model the unified layout renders, so the two arrangements can never
/// disagree about what a line says, and the token array (keyed by the same
/// flat index) is looked up per side with no bookkeeping at all.
struct DiffPairRow: Identifiable {
    /// Position in the pairing — stable for as long as the pairing is.
    let id: Int
    let left: Int?
    let right: Int?
    /// A `@@` header, which spans both columns instead of pairing two lines.
    let isHunkHeader: Bool
}

/// Where a file's diff is read from.
///
/// The two sources the Tauri client's `DiffViewer` serves: the working tree
/// (the Changes tab) and a commit against its first parent (the History
/// detail). Carried whole through `DiffView` into `DiffStore`, so the raw-diff
/// read and the blob source the tokenizer uses can never disagree.
enum DiffTarget: Equatable {
    /// `HEAD` against the working tree. `head` is the commit that comparison
    /// is made against — `RepoStatus.headSha`, `nil` until the first status
    /// lands — and it belongs to the target because moving `HEAD` changes what
    /// an untouched file is being compared *to*. A `--mixed` reset is the case
    /// that proves it: the bytes on disk and the status letters both stay
    /// exactly as they were while the diff grows by everything the reset
    /// commit contained.
    case workingTree(head: String?)
    /// One commit's changes, first-parent for merges.
    case commit(sha: String)
}

/// Which diff a payload is of: one file, from one source.
///
/// Deliberately coarser than `DiffTarget`, which also carries what makes a
/// diff *stale*. This is only the question "is this the same diff?", so it
/// drops the working tree's `HEAD` — a moved `HEAD` re-reads the file the
/// reader is already looking at, and neither the size-guard reveal nor their
/// scroll position should be taken away for it. It does keep the commit,
/// since one path in two commits is two different diffs.
struct DiffIdentity: Equatable {
    let source: Source
    let path: String

    enum Source: Equatable {
        case workingTree
        case commit(String)

        init(_ target: DiffTarget) {
            switch target {
            case .workingTree: self = .workingTree
            case .commit(let sha): self = .commit(sha)
            }
        }
    }

    init(_ file: FileEntry, from target: DiffTarget) {
        source = Source(target)
        path = file.path
    }
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
/// rows, scroll position, and tokens all survive untouched. The view asks for
/// a read only when something the *open file's* diff is a function of has
/// moved — its own bytes, which side of it is staged, or the commit it is
/// compared against — and this store is where that possibility is checked
/// against reality, so a read whose answer is unchanged costs one subprocess
/// and zero repaints.
@MainActor
@Observable
final class DiffStore {
    /// What's on screen. Deliberately *not* cleared when a load starts —
    /// clearing is what blanked the pane on every status tick.
    private(set) var payload: DiffPayload?
    private(set) var rows: [DiffRow] = []
    /// The split layout's rows over `rows`' own indices. Empty whenever the
    /// unified layout is the one on screen, because that is when core is not
    /// asked to build the pairing.
    private(set) var pairs: [DiffPairRow] = []
    /// One entry per row once tokenization lands; `nil` while in flight.
    private(set) var tokens: [[Token]]?

    /// The run of lines the reader has picked out in the gutter, as indices
    /// into `rows`.
    ///
    /// Owned here rather than by the view for one reason: it *is* a range into
    /// `rows`, so it is valid exactly as long as `rows` is. Clearing it lives
    /// in the one place the row model is rebuilt, which is what stops a
    /// selection made against one diff from addressing lines in the next.
    private(set) var lineSelection: DiffLineSelection?
    /// Set when there are no lines to show, and which of the three unrelated
    /// reasons it is — the pane names the actual cause instead of covering
    /// all of them with one caption.
    private(set) var emptyReason: EmptyDiffReason?

    /// Set when core withheld the diff for its size. The pane explains it and
    /// offers to render it anyway, rather than hanging on it.
    private(set) var sizeGuard: DiffSizeGuard?

    /// Which diff the payload on screen is of, or `nil` when nothing is
    /// rendered.
    ///
    /// What a *view* needs and cannot get from the payload: the payload
    /// describes a diff without naming it, and during a seamless switch the one
    /// it describes is still the previous file's. Two things read it — the
    /// header, which shows only the chrome that describes the file it is
    /// naming, and the scroll position, which resets when this moves and holds
    /// still when it does not, so every re-read of the same diff (a layout
    /// change, a whitespace toggle, an edit on disk) keeps the reader where
    /// they were.
    private(set) var rendered: DiffIdentity?

    /// The diff the user asked to see past the size guard, if any.
    ///
    /// Kept as an identity rather than as a bare flag, because the decision has
    /// to survive one thing and not the other. Every re-read of the **same**
    /// diff keeps it — a layout change, a whitespace toggle, an edit on disk —
    /// where a flag cleared on each load would silently take back what the
    /// reader asked for and drop them on the "Large Diff" prompt again, with
    /// the control that got them past it gone from the header. Moving to a
    /// **different** diff clears it, which is what makes the guard withhold
    /// rather than refuse: without that, a file with one long line (a minified
    /// bundle, a lock file, a long Markdown paragraph) would have no way to be
    /// read at all.
    private var revealed: DiffIdentity?

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
    /// `SLOW_DIFF_THRESHOLD_MS` (150), so both clients call the same load slow.
    /// Crossing it dims the diff on screen; the payload is kept either way, so
    /// a slow load that lands unchanged is absorbed by the equality skip and
    /// the user keeps their scroll position.
    static let slowLoadThreshold: Duration = .milliseconds(150)

    /// How long phase two waits before asking for syntax tokens — the Tauri
    /// client's `HIGHLIGHT_DEBOUNCE_MS` (80). Arrowing down a file list is a
    /// load per file passed, and tokenizing reads and parses whole blobs; the
    /// pause means only the file the reader stopped on pays for that, while
    /// phase one still paints on the frame its rows land.
    static let highlightDebounce: Duration = .milliseconds(80)

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
    /// the Tauri client's `syntaxHighlighting` guard; `sideBySide` asks core
    /// for the row pairing, which is built only for the layout that is about
    /// to render it. All three re-key `DiffView`'s load task, so changing any
    /// of them flows through this same seamless path — the Tauri client's
    /// `diffReadKey` effect, for free.
    func load(
        repoPath: String,
        file: FileEntry,
        target: DiffTarget,
        hideWhitespace: Bool,
        highlight: Bool,
        sideBySide: Bool,
        ignoringSizeGuard: Bool = false
    ) async {
        generation += 1
        let current = generation
        phase = .loading(slow: false)
        let asked = DiffIdentity(file, from: target)
        if ignoringSizeGuard {
            revealed = asked
        } else if revealed != asked {
            revealed = nil
        }

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
                repoPath: repoPath,
                file: file,
                target: target,
                hideWhitespace: hideWhitespace,
                sideBySide: sideBySide
            )
            guard current == generation else { return }

            // Nothing to render, and core says which of the three unrelated
            // reasons it is — including the whitespace-only case, which the
            // pane would otherwise report as "no changes" while the setting
            // was what hid them.
            if let reason = parsed.emptyReason {
                clearRenderedDiff()
                emptyReason = reason
                sizeGuard = nil
                phase = .idle
                return
            }
            // Withheld for its size rather than empty: the pane offers to
            // render it anyway instead of hanging on it.
            if let guardInfo = parsed.sizeGuard {
                clearRenderedDiff()
                emptyReason = nil
                sizeGuard = guardInfo
                phase = .idle
                return
            }
            emptyReason = nil
            sizeGuard = nil
            // Outside the equality skip below, deliberately: two *different*
            // files can parse to an equal payload (an identical one-line
            // change in two files), and a stale identity would then tell the
            // header it was describing the wrong file and leave the reader
            // scrolled into the middle of a diff they just opened.
            rendered = asked
            if parsed != payload {
                // The rows and their tokens are a function of the line model
                // alone; the pairing is a separate list over those same flat
                // indices. Comparing the two apart is what lets a *layout*
                // toggle — a re-read returning an identical model with a
                // pairing that appeared or vanished — keep the rows and the
                // syntax colour already on screen, so the arrangement changes
                // and nothing else does.
                let modelMoved = parsed.fileDiff != payload?.fileDiff
                let pairingMoved = parsed.sbsPairs != payload?.sbsPairs
                payload = parsed
                if modelMoved {
                    // Rows are `Identifiable` by flat index, so SwiftUI diffs
                    // the list in place instead of rebuilding it; tokens reset
                    // to nil and the plain-text phase shows immediately — the
                    // two-phase paint is the contract (FRONTEND.md §7).
                    rows = Self.flatten(parsed.fileDiff)
                    tokens = nil
                    // The one place a line selection can be silently wrong: it
                    // addresses rows, and these are different rows. Deliberately
                    // *not* on a layout change, which moves no line — the reader
                    // keeps their selection across it, as they keep their scroll.
                    lineSelection = nil
                }
                if pairingMoved {
                    pairs = Self.paired(parsed.sbsPairs)
                }
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
            //
            // Debounced first. A cancelled sleep resumes immediately rather
            // than throwing out of the function, so the guard after it is what
            // actually stops the work: `generation` catches the reader moving
            // to another file, and `isCancelled` catches the pane going away
            // entirely (a tab change), which moves no generation.
            try? await Task.sleep(for: Self.highlightDebounce)
            guard current == generation, !Task.isCancelled else { return }

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
            clearRenderedDiff()
            emptyReason = nil
            sizeGuard = nil
            phase = .failed(error.displayMessage)
        }
    }

    /// A gutter was clicked. `extending` is a shift-click, which grows the run
    /// from where it started instead of beginning a new one.
    ///
    /// Guarded against an index the rows no longer have: a click can land on a
    /// cell built from the row model of a diff that has just been replaced.
    func selectLine(_ index: Int, extending: Bool) {
        guard rows.indices.contains(index) else { return }
        if extending, var selection = lineSelection {
            selection.extend(to: index)
            lineSelection = selection
        } else {
            lineSelection = DiffLineSelection(index)
        }
    }

    /// The whole diff — the reader who wants all of it, without shift-clicking
    /// their way to the last line of a ten-thousand-row patch.
    func selectAllLines() {
        guard let last = rows.indices.last else { return }
        var selection = DiffLineSelection(0)
        selection.extend(to: last)
        lineSelection = selection
    }

    /// Drop the run. Escape, in the pane.
    func clearLineSelection() {
        lineSelection = nil
    }

    /// The file's own text for the selected run, rebuilt from the line model —
    /// the one place clipboard text comes from, whichever gesture asked for it.
    ///
    /// `nil` when nothing is selected, which is what lets the pane *decline* a
    /// Copy command and leave it to whatever character selection the reader
    /// made inside a line: the two never contend for ⌘C, because the pane only
    /// claims it while a run exists.
    var selectedLineText: String? {
        guard let lineSelection, let fileDiff = payload?.fileDiff else { return nil }
        return GitBridge.diffText(of: fileDiff, in: lineSelection.range)
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
        highlight: Bool,
        sideBySide: Bool
    ) async {
        await load(
            repoPath: repoPath,
            file: file,
            target: target,
            hideWhitespace: hideWhitespace,
            highlight: highlight,
            sideBySide: sideBySide,
            ignoringSizeGuard: true
        )
    }

    /// The one read, with the options this client asks for.
    private func read(
        repoPath: String,
        file: FileEntry,
        target: DiffTarget,
        hideWhitespace: Bool,
        sideBySide: Bool
    ) async throws -> DiffPayload {
        let options = GitBridge.diffOptions(
            sideBySide: sideBySide,
            showAnyway: revealed == DiffIdentity(file, from: target)
        )
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

    /// Core's pairing at Swift's index width, with a row identity.
    ///
    /// The zipping itself — delete runs against the add run that follows,
    /// context and `@@` headers spanning both columns, no-newline markers
    /// rowless — stays in `core::diff::build_sbs_pairs`, so the two clients
    /// cannot arrange the same diff differently.
    private static func paired(_ pairs: [SbsPair]) -> [DiffPairRow] {
        pairs.enumerated().map { index, pair in
            DiffPairRow(
                id: index,
                left: pair.left.map(Int.init),
                right: pair.right.map(Int.init),
                isHunkHeader: pair.isHunkHeader
            )
        }
    }

    /// Drop everything derived from a diff. The three results that are not one
    /// — an empty parse, a withheld one, a failed read — all reach it, so a
    /// future artifact cannot be forgotten at two of the three sites.
    private func clearRenderedDiff() {
        payload = nil
        rendered = nil
        rows = []
        pairs = []
        tokens = nil
        lineSelection = nil
    }
}
