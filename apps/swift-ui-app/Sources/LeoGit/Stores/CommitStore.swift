import Foundation

/// State and behaviour for composing a commit from the changes list.
///
/// Inclusion is *derived*, not stored: every committable file is included
/// unless its path is in `excludedPaths`. Tracking exclusions instead of
/// inclusions is what the Tauri client does too (its `userDeselected` set),
/// and for the same reason — a status reload rebuilds the file list from
/// scratch, and files that appear after the user last touched a checkbox must
/// default to included without any re-seeding bookkeeping.
@MainActor
@Observable
final class CommitStore {
    /// First line of the commit message. Required to commit.
    var summary = ""

    /// Optional message body, joined below the summary with a blank line.
    var details = ""

    /// Paths the user explicitly unchecked.
    ///
    /// An opt-out outlives its path leaving the file list, so a file that
    /// briefly disappears — a formatter rewriting it between two status reads —
    /// keeps it and cannot be silently re-included by the next commit. It does
    /// not outlive it *forever*: `pruneExpiredExclusions(against:)` drops one
    /// whose path has been gone longer than core's grace window. Both clients
    /// run that one rule.
    private(set) var excludedPaths: Set<String> = []

    /// How long, and over how many status reads, each excluded path has been
    /// missing from the file list. The clock behind `excludedPaths`, kept beside
    /// it rather than inside it because every reader of that set asks one
    /// question — "is this path excluded?" — and a dictionary of clocks would
    /// make each of them carry the answer to a different one. Written only where
    /// the set is.
    private var clocks: [String: Exclusion] = [:]

    /// When the ages above were last advanced.
    private var lastReconcile = Date.now

    private(set) var isCommitting = false

    private(set) var isGenerating = false

    /// The app-wide config owner — this store's only route to `ai_provider`.
    /// A dependency rather than composer state, hence unobserved.
    @ObservationIgnored private let configStore: AppConfigStore

    init(config: AppConfigStore) {
        configStore = config
    }

    /// AI provider driving Generate.
    ///
    /// Read from the shared owner rather than mirrored here: the Settings
    /// window has a picker for the same setting, and two independent copies
    /// meant the two could disagree while both were on screen — and that a
    /// Settings write of any unrelated field reverted a change made here.
    var aiProvider: String { configStore.aiProvider }

    /// Why the AI provider can't serve a request, when it can't — see
    /// `providerBlock`.
    struct ProviderBlock: Equatable {
        /// The provider this describes. An answer is stale only when the
        /// picker has moved on, which is what `blockingProvider` checks.
        let provider: String
        /// Core's sentence, shown as-is.
        let reason: String
        /// A shell command that would fix it, for the terminal dock to run.
        /// Empty when there is none worth offering.
        let fixCommand: String
        /// The failed request this was read out of, when it came from one —
        /// the provider's own wording, kept as the row's tooltip.
        let detail: String
    }

    /// The standing reason Generate is unavailable, or `nil`.
    ///
    /// Only the blocked case is stored, so `nil` covers both "ready" and "not
    /// asked yet" — the same thing to the gate. Refusing on "not known" is
    /// worse than letting a doomed request report itself.
    private(set) var providerBlock: ProviderBlock?

    /// The block while it still describes the selected provider. Tagging is
    /// what makes switching provider drop its predecessor's block on the spot,
    /// with no clearing step to forget.
    var blockingProvider: ProviderBlock? {
        providerBlock?.provider == aiProvider ? providerBlock : nil
    }

    /// Failure text from the last commit, generate, or provider-save
    /// attempt — one shared slot, like the Tauri composer's inline error.
    /// Cleared when the next attempt starts, on success, and on `reset()`.
    private(set) var errorMessage: String?

    /// The commit being rewritten, when the History row menu put the composer
    /// in amend mode. The whole `CommitInfo` rather than a sha: its message
    /// seeds the draft, and its co-author trailers have to be re-applied to
    /// the amended commit.
    private(set) var amendTarget: CommitInfo?

    /// `Co-authored-by` values carried over from an amended or undone commit.
    /// Invisible state — the composer has no co-author field, so these are
    /// simply re-attached to the next message rather than silently dropped
    /// (core pre-parses them off the commit's trailers, so nothing is parsed
    /// here). Other trailers are the user's to re-add, matching Tauri.
    private var coAuthors: [String] = []

    var isAmending: Bool { amendTarget != nil }

    /// Whether the parent repository can stage this entry at all. A dirty
    /// submodule can't be committed from here — its changes live inside the
    /// submodule and only a pointer move would be recorded.
    static func isCommittable(_ file: FileEntry) -> Bool {
        !file.submoduleDirty
    }

    /// GitHub-Desktop-style default message when the commit would contain
    /// exactly one file — "Create/Delete/Update <name>" — so the most common
    /// commit needs zero typing (the Tauri composer's `autoSummary` rule).
    /// Empty for zero or several files: those still require a real summary.
    static func autoSummary(for files: [FileEntry]) -> String {
        guard files.count == 1, let file = files.first else { return "" }
        let verb = switch file.status {
        case .new: "Create"
        case .deleted: "Delete"
        default: "Update"
        }
        return "\(verb) \(file.displayName)"
    }

    func isIncluded(_ file: FileEntry) -> Bool {
        Self.isCommittable(file) && !excludedPaths.contains(file.path)
    }

    /// The subset of `files` the next commit would contain, in list order.
    func includedFiles(from files: [FileEntry]) -> [FileEntry] {
        files.filter { isIncluded($0) }
    }

    /// A freshly unchecked path: zero on both terms, which is also what core
    /// answers for a path it can still see.
    private static func newClock(for path: String) -> Exclusion {
        Exclusion(path: path, absentMs: 0, absentReads: 0)
    }

    func setIncluded(_ file: FileEntry, _ include: Bool) {
        guard Self.isCommittable(file) else { return }
        if include {
            excludedPaths.remove(file.path)
            clocks.removeValue(forKey: file.path)
        } else {
            excludedPaths.insert(file.path)
            clocks[file.path] = Self.newClock(for: file.path)
        }
    }

    func setAllIncluded(_ include: Bool, in files: [FileEntry]) {
        let committable = files.filter(Self.isCommittable).map(\.path)
        if include {
            excludedPaths.subtract(committable)
            for path in committable { clocks.removeValue(forKey: path) }
        } else {
            excludedPaths.formUnion(committable)
            for path in committable { clocks[path] = Self.newClock(for: path) }
        }
    }

    /// Age the opt-outs against the file list a status read just produced, and
    /// drop the ones that have now been gone long enough, and over enough reads,
    /// to be gone rather than mid-rewrite.
    ///
    /// Called from the status poll rather than from wherever the file list
    /// changes: a path is pruned for having been *absent* long enough, which is
    /// exactly what an unchanged file list keeps being true of. Elapsed time is
    /// measured here, so an irregular caller — a tick the poll skipped while a
    /// transfer held the slot — costs an opt-out nothing and buys it nothing.
    ///
    /// The crossing is skipped entirely while nothing is excluded, which is the
    /// usual state of the app.
    func pruneExpiredExclusions(against files: [FileEntry]) {
        let now = Date.now
        let elapsed = max(now.timeIntervalSince(lastReconcile), 0) * 1000
        lastReconcile = now
        guard !excludedPaths.isEmpty else {
            clocks = [:]
            return
        }
        let excluded = excludedPaths.map { clocks[$0] ?? Self.newClock(for: $0) }
        // Total by construction: `UInt32(someDouble)` traps on anything it can't
        // represent, and a clock that jumped is not worth a crash. Anything past
        // the range is the same answer as the maximum — every window expired.
        let elapsedMs = UInt32(exactly: elapsed.rounded(.down)) ?? .max
        let kept = GitBridge.survivingExclusions(
            excluded,
            present: files.map(\.path),
            elapsedMs: elapsedMs
        )
        excludedPaths = Set(kept.map(\.path))
        clocks = Dictionary(uniqueKeysWithValues: kept.map { ($0.path, $0) })
    }

    /// Forget everything typed and unchecked, for a draft that has been spent:
    /// the commit it described landed. A repository switch does *not* come
    /// through here — it parks the draft instead, see `activate(repoPath:)`.
    func reset() {
        summary = ""
        details = ""
        excludedPaths = []
        clocks = [:]
        lastReconcile = .now
        errorMessage = nil
        amendTarget = nil
        coAuthors = []
    }

    // MARK: Per-repository drafts

    /// Everything typed or unchecked for one repository, as a value that can be
    /// set down.
    ///
    /// `errorMessage`, `isCommitting` and the provider block are deliberately
    /// not part of it: each describes an attempt or a machine state rather than
    /// a repository, and carrying one back would re-raise something the user
    /// has already dealt with.
    struct Draft {
        var summary = ""
        var details = ""
        var excludedPaths: Set<String> = []
        var clocks: [String: Exclusion] = [:]
        var amendTarget: CommitInfo?
        var coAuthors: [String] = []

        /// Nothing to come back to. A repository the user only looked at holds
        /// one of these, and parking it would grow the cache with entries that
        /// restore an empty composer.
        var isEmpty: Bool {
            summary.isEmpty && details.isEmpty && excludedPaths.isEmpty
                && amendTarget == nil && coAuthors.isEmpty
        }
    }

    /// Drafts belonging to repositories that are not on screen, keyed by path.
    ///
    /// In memory only, and for one run of the app: a draft is worth a trip to
    /// another repository and back, which is the trip that used to lose it. The
    /// dictionary is unobserved because no view reads it — only the live
    /// properties it swaps in and out are on screen.
    @ObservationIgnored private var parked: [String: Draft] = [:]

    /// Which repository the live properties currently describe.
    @ObservationIgnored private var activeRepo: String?

    /// The live draft as a value, for parking and restoring.
    private var draft: Draft {
        get {
            Draft(
                summary: summary,
                details: details,
                excludedPaths: excludedPaths,
                clocks: clocks,
                amendTarget: amendTarget,
                coAuthors: coAuthors
            )
        }
        set {
            summary = newValue.summary
            details = newValue.details
            excludedPaths = newValue.excludedPaths
            clocks = newValue.clocks
            amendTarget = newValue.amendTarget
            coAuthors = newValue.coAuthors
        }
    }

    /// Point the composer at `repoPath`: park the outgoing repository's draft
    /// and take back whatever was left in this one.
    ///
    /// Idempotent, and that is the point. The screen tells the composer which
    /// repository it is on from a `.task(id:)`, which restarts whenever the
    /// window re-appears as well as when the id changes — and a `leogit <dir>`
    /// naming the repository already open re-appears the window without
    /// changing anything. Being asked again for the same repository has to mean
    /// nothing happens, or a message half-typed at the moment the command ran
    /// is the thing that gets thrown away.
    func activate(repoPath: String) {
        guard repoPath != activeRepo else { return }
        if let previous = activeRepo {
            let outgoing = draft
            if outgoing.isEmpty {
                parked.removeValue(forKey: previous)
            } else {
                parked[previous] = outgoing
            }
        }
        // Removed rather than copied: the live properties are now the draft,
        // and a second copy under the old key would be the one restored after
        // the next switch, silently undoing everything typed in between.
        draft = parked.removeValue(forKey: repoPath) ?? Draft()
        activeRepo = repoPath
        // The exclusion clocks came back from a repository that has not been
        // polled since it was parked; ageing them against the gap would expire
        // opt-outs for time the file list was never read.
        lastReconcile = .now
        errorMessage = nil
    }

    // MARK: Amend

    /// Rewrite `commit` instead of creating a new one: its message seeds the
    /// draft, overwriting whatever was there, and Commit becomes `--amend`.
    ///
    /// Re-entering on the same commit is a no-op, so right-clicking Amend
    /// again while already amending doesn't wipe the edits in progress — the
    /// sha guard the Tauri composer uses for the same reason. The checkbox
    /// state is deliberately left alone: an amend commits HEAD's message plus
    /// whatever the working tree currently has checked, which is what lets it
    /// fold new changes into the last commit.
    func startAmending(_ commit: CommitInfo) {
        guard amendTarget?.sha != commit.sha else { return }
        amendTarget = commit
        summary = commit.summary
        details = commit.bodyWithoutCoauthors
        coAuthors = commit.coAuthors
        errorMessage = nil
    }

    /// Leave amend mode and clear the draft it seeded, so the amended message
    /// can't be re-submitted as a brand-new commit by accident.
    func stopAmending() {
        guard isAmending else { return }
        amendTarget = nil
        summary = ""
        details = ""
        coAuthors = []
    }

    /// Seed the composer from a commit that was just undone, so its message
    /// isn't lost with it. Not amend mode — the commit is gone, and what
    /// follows is an ordinary commit of the changes it left behind.
    func restoreDraft(from commit: CommitInfo) {
        amendTarget = nil
        summary = commit.summary
        details = commit.bodyWithoutCoauthors
        coAuthors = commit.coAuthors
        errorMessage = nil
    }

    /// Format the message and commit `files`, falling back to `autoSummary`
    /// when nothing was typed. On success the composer is cleared and the
    /// caller should reload status + history; on failure the draft is kept
    /// for another attempt and `errorMessage` carries core's own text.
    /// Returns whether the commit landed.
    ///
    /// While amending, an empty file list is legal — `git commit --amend`
    /// with nothing staged rewrites just the message — so only the summary is
    /// required there.
    func commit(repoPath: String, files: [FileEntry], autoSummary: String = "") async -> Bool {
        let typed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedSummary = typed.isEmpty ? autoSummary : typed
        // `!isGenerating`: committing mid-generation would clear both drafts
        // and then have the late AI result overwrite the empty composer. The
        // Tauri client leaves that race open; the busy-guard closes it, like
        // BranchStore's double-fire guard.
        guard !trimmedSummary.isEmpty, !isCommitting, !isGenerating else { return false }
        guard !files.isEmpty || isAmending else { return false }
        isCommitting = true
        errorMessage = nil
        let amending = isAmending
        defer { isCommitting = false }

        let message = await GitBridge.commitMessage(
            summary: trimmedSummary,
            description: details.trimmingCharacters(in: .whitespacesAndNewlines),
            coAuthors: coAuthors
        )
        do {
            try await GitBridge.commitChanges(
                in: repoPath,
                message: message,
                files: files,
                amend: amending
            )
            reset()
            return true
        } catch {
            errorMessage = "\(amending ? "Amend" : "Commit") failed: \(error.displayMessage)"
            return false
        }
    }

    // MARK: AI generation

    /// Ask whether the selected provider can serve a request, so Generate can
    /// say *why* it is greyed out instead of letting a doomed request report
    /// it. Called when the composer appears, after a provider change, and
    /// whenever the app is re-activated while something is blocking — every
    /// way of fixing an unready provider leaves this app, so coming back is
    /// exactly when the answer can have changed.
    ///
    /// The result is assigned only once it is in hand: clearing on the way in
    /// would make the remedy blink out and back on every re-ask, and asking
    /// Claude costs two process spawns.
    func refreshProviderStatus() async {
        // The picker's write may still be in flight, and the config read below
        // reads the file it is writing — the same wait `generate` does.
        await configStore.settle()
        let target = aiProvider
        do {
            let config = try await GitBridge.aiConfig()
            // The file still names the provider being replaced, or the picker
            // moved on while we asked: either way this answer is about the
            // wrong provider.
            guard config.provider == target, target == aiProvider else { return }
            let status = try await GitBridge.providerStatus(config: config)
            guard target == aiProvider else { return }
            providerBlock = status.ready
                ? nil
                : ProviderBlock(
                    provider: target,
                    reason: status.reason,
                    fixCommand: status.fixCommand,
                    detail: ""
                )
        } catch {
            // Core throws only for a provider name it doesn't know; anything
            // else here is a wiring failure, not an answer, so nothing
            // changes. In particular it must not clear a block a real failed
            // request proved, which would put Generate back in front of a
            // provider already known to be dead. An unasked question leaves
            // the gate open, so a broken probe can't lock anyone out.
            print("[ai] provider probe failed; leaving Generate enabled: \(error.displayMessage)")
        }
    }

    /// Read a failed generate for a provider state the user can fix, and raise
    /// the remedy if there is one.
    ///
    /// This is not a fallback for `refreshProviderStatus` — for an expired
    /// session it is the only thing that works. Signing out deletes the
    /// credentials, so the probe sees it; an expired session leaves them on
    /// disk, so `claude auth status` still reports a signed-in CLI and only a
    /// real request discovers the refresh failed.
    ///
    /// Only ever *raises* a block: a failure core doesn't recognize says
    /// nothing about the provider.
    private func classifyFailure(_ message: String) {
        let target = aiProvider
        let status = GitBridge.providerStatus(fromFailure: message, provider: target)
        guard !status.ready else { return }
        providerBlock = ProviderBlock(
            provider: target,
            reason: status.reason,
            fixCommand: status.fixCommand,
            detail: message
        )
        // The remedy replaces the raw failure rather than stacking under it:
        // both describe one state, and the remedy is the half the user can act
        // on. The provider's own wording survives as the row's tooltip.
        errorMessage = nil
    }

    /// Persist a provider change. On failure the picker reverts — the owner
    /// drops the value it was showing optimistically — and the error shows
    /// inline, which keeps the control truthful about what Generate will
    /// actually use.
    ///
    /// Synchronous, so the picker shows the choice in the same layout pass it
    /// was made in; only the reporting waits.
    func setAIProvider(_ provider: String) {
        guard let write = configStore.setAIProvider(provider) else { return }
        Task {
            do {
                try await write.value
            } catch {
                errorMessage = "Failed to save provider: \(error.displayMessage)"
            }
        }
    }

    /// Generate a commit message from the checked files' combined diff and
    /// fill both drafts with the result — overwriting whatever was typed,
    /// exactly like the Tauri composer. Failures land in the shared
    /// `errorMessage` slot with the same wording the Tauri client uses.
    func generate(repoPath: String, files: [FileEntry]) async {
        guard !isGenerating, !isCommitting else { return }
        guard !files.isEmpty else {
            errorMessage = "No files selected"
            return
        }
        isGenerating = true
        errorMessage = nil
        defer { isGenerating = false }

        do {
            let diff = try await GitBridge.selectedDiff(in: repoPath, files: files)
            // A fresh config read per generate, resolved for the selected
            // provider by core — so the model and server URL always belong to
            // the provider actually about to run, which splicing a picker
            // value over a separately-loaded config could not guarantee.
            // `setAIProvider` persists the picker's choice; waiting for that
            // write is what makes this read reflect it.
            await configStore.settle()
            let config = try await GitBridge.aiConfig()
            let message = try await GitBridge.generateMessage(diff: diff, config: config)
            summary = message.title
            details = message.description
        } catch {
            let message = error.displayMessage
            errorMessage = "Generate failed: \(message)"
            classifyFailure(message)
        }
    }
}
