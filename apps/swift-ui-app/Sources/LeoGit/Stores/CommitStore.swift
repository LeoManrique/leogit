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

    /// Paths the user explicitly unchecked. Deliberately not pruned when a
    /// path leaves the file list — matching the Tauri client — so a file that
    /// briefly disappears (e.g. touched by a formatter) keeps its opt-out.
    private(set) var excludedPaths: Set<String> = []

    private(set) var isCommitting = false

    private(set) var isGenerating = false

    /// AI provider driving Generate — mirrors `ai_provider` in the shared
    /// config file. `"claude"` until `loadAIProvider` reads the real value.
    private(set) var aiProvider = "claude"

    /// Failure text from the last commit, generate, or provider-save
    /// attempt — one shared slot, like the Tauri composer's inline error.
    /// Cleared when the next attempt starts, on success, and on `reset()`.
    private(set) var errorMessage: String?

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

    func setIncluded(_ file: FileEntry, _ include: Bool) {
        guard Self.isCommittable(file) else { return }
        if include {
            excludedPaths.remove(file.path)
        } else {
            excludedPaths.insert(file.path)
        }
    }

    func setAllIncluded(_ include: Bool, in files: [FileEntry]) {
        let committable = files.filter(Self.isCommittable).map(\.path)
        if include {
            excludedPaths.subtract(committable)
        } else {
            excludedPaths.formUnion(committable)
        }
    }

    /// Forget everything typed and unchecked — for switching repositories.
    func reset() {
        summary = ""
        details = ""
        excludedPaths = []
        errorMessage = nil
    }

    /// Format the message and commit `files`, falling back to `autoSummary`
    /// when nothing was typed. On success the composer is cleared and the
    /// caller should reload status + history; on failure the draft is kept
    /// for another attempt and `errorMessage` carries core's own text.
    /// Returns whether the commit landed.
    func commit(repoPath: String, files: [FileEntry], autoSummary: String = "") async -> Bool {
        let typed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedSummary = typed.isEmpty ? autoSummary : typed
        // `!isGenerating`: committing mid-generation would clear both drafts
        // and then have the late AI result overwrite the empty composer. The
        // Tauri client leaves that race open; the busy-guard closes it, like
        // BranchStore's double-fire guard.
        guard !trimmedSummary.isEmpty, !files.isEmpty, !isCommitting, !isGenerating else {
            return false
        }
        isCommitting = true
        errorMessage = nil
        defer { isCommitting = false }

        let message = await GitBridge.commitMessage(
            summary: trimmedSummary,
            description: details.trimmingCharacters(in: .whitespacesAndNewlines),
            coAuthors: []
        )
        do {
            try await GitBridge.commitChanges(in: repoPath, message: message, files: files)
            reset()
            return true
        } catch {
            errorMessage = error.displayMessage
            return false
        }
    }

    // MARK: AI generation

    /// Read the persisted provider so the picker shows the real value.
    /// Failure keeps the default — the picker still works, and generating
    /// surfaces any real config problem itself.
    func loadAIProvider() async {
        if let config = try? await GitBridge.aiConfig() {
            aiProvider = config.provider
        }
    }

    /// Persist a provider change. On failure the picker reverts and the
    /// error shows inline — unlike the Tauri client, which leaves the
    /// optimistic value on screen; reverting keeps the picker truthful
    /// about what Generate will actually use.
    func setAIProvider(_ provider: String) async {
        guard provider != aiProvider else { return }
        let previous = aiProvider
        aiProvider = provider
        do {
            try await GitBridge.setAIProvider(provider)
        } catch {
            aiProvider = previous
            errorMessage = "Failed to save provider: \(error.displayMessage)"
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
            // A fresh config read per generate, degrading to bare defaults
            // when it fails — the Tauri client wraps its load in the same
            // swallow-everything catch. The picker's value is the provider
            // either way, so what the user sees is what runs.
            let loaded = try? await GitBridge.aiConfig()
            let config = AiProviderConfig(
                provider: aiProvider,
                model: loaded?.model,
                apiKey: loaded?.apiKey,
                baseUrl: loaded?.baseUrl
            )
            let message = try await GitBridge.generateMessage(diff: diff, config: config)
            summary = message.title
            details = message.description
        } catch {
            errorMessage = "Generate failed: \(error.displayMessage)"
        }
    }
}
