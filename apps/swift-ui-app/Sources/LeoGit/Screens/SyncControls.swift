import SwiftUI

/// Toolbar pull/push controls — the native port of the Tauri header's sync
/// buttons.
///
/// Same semantics, native shapes: Pull renders only when the branch has real
/// upstream tracking; Push is a split button whose primary action re-labels
/// to "Publish Branch" when a remote exists but the branch is untracked
/// (routing through the same push with `--set-upstream`); force push with
/// lease is offered only in the menu, only while the branch has diverged, and
/// only behind a destructive confirmation. Fetch — invisible auto-fetch in
/// the Tauri client — is an explicit menu item here, since the native client
/// has no background polling yet.
struct SyncControls: View {
    let store: SyncStore
    let repoPath: String
    let status: RepoStatus?
    /// Called after any network operation: pull moves HEAD and the working
    /// tree, push/fetch move the ahead/behind counts.
    let onWorkingTreeChanged: () async -> Void

    /// Failure text from a finished operation, shown as an alert — git's own
    /// message, verbatim, like the Tauri error modal.
    @State private var alertMessage: String?
    @State private var isConfirmingForcePush = false

    private var branch: String { status?.branch ?? "" }
    private var hasUpstream: Bool { status?.hasUpstream ?? false }
    private var ahead: Int32 { status?.ahead ?? 0 }
    private var behind: Int32 { status?.behind ?? 0 }
    private var isDetached: Bool { status?.detached ?? false }

    /// No remote at all: the Tauri client publishes the repo to GitHub via
    /// `gh` here — not ported, so the button explains itself instead.
    private var hasNoRemote: Bool {
        guard let status, !status.branch.isEmpty else { return false }
        return !status.hasRemote
    }

    /// A remote exists but the branch is untracked — the first push must
    /// carry `--set-upstream`, and the button says what it will do.
    private var isPublishingBranch: Bool {
        guard let status, !status.branch.isEmpty else { return false }
        return status.hasRemote && !status.hasUpstream
    }

    /// Force push is offered only for a truly diverged branch: real upstream
    /// tracking plus commits on both sides.
    private var hasDiverged: Bool { hasUpstream && ahead > 0 && behind > 0 }

    private var isBusy: Bool { store.activeOperation != nil }

    var body: some View {
        if hasUpstream {
            Button(action: pull) {
                Label(pullTitle, systemImage: "arrow.down")
            }
            .disabled(isBusy)
            .help(behind > 0 ? "Pull \(behind) commit\(behind == 1 ? "" : "s") from the remote" : "Pull from the remote")
        }

        Menu {
            Button(pushActionTitle) { push() }
                .disabled(isBusy)

            if hasDiverged {
                Button("Force Push (with Lease)…", role: .destructive) {
                    isConfirmingForcePush = true
                }
                .disabled(isBusy)
            }

            Divider()

            Button("Fetch", action: fetch)
                .disabled(isBusy)
        } label: {
            Label(pushTitle, systemImage: pushIcon)
        } primaryAction: {
            push()
        }
        .disabled(isBusy || isDetached || hasNoRemote)
        .help(pushHelp)
        .confirmationDialog(
            "Force Push with Lease?",
            isPresented: $isConfirmingForcePush
        ) {
            Button("Force Push", role: .destructive) { push(forceWithLease: true) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                """
                This will overwrite “\(status?.upstream ?? "the remote branch")” with your local branch. \
                With-lease refuses the push if someone else has pushed since your last fetch — \
                safer than plain force, but it cannot be undone once it succeeds.
                """
            )
        }
        .alert(
            "Error",
            isPresented: Binding(
                get: { alertMessage != nil },
                set: { if !$0 { alertMessage = nil } }
            )
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(alertMessage ?? "")
        }
    }

    private var pullTitle: String {
        if store.activeOperation == .pull { return "Pulling…" }
        return behind > 0 ? "Pull (\(behind))" : "Pull"
    }

    private var pushTitle: String {
        if store.activeOperation == .push { return "Pushing…" }
        if isPublishingBranch { return "Publish Branch" }
        return ahead > 0 ? "Push (\(ahead))" : "Push"
    }

    private var pushIcon: String {
        if hasNoRemote { return "icloud.and.arrow.up" }
        if isPublishingBranch { return "arrow.up.circle" }
        return "arrow.up"
    }

    private var pushActionTitle: String {
        isPublishingBranch ? "Publish Branch" : "Push"
    }

    private var pushHelp: String {
        if isDetached { return "Detached HEAD — check out a branch to push" }
        if hasNoRemote {
            return "This repository has no remote. Publish to GitHub from the Tauri client, or add a remote in a terminal."
        }
        if isPublishingBranch { return "Publish this branch to the remote and start tracking it" }
        if ahead > 0 { return "Push \(ahead) commit\(ahead == 1 ? "" : "s") to the remote" }
        return "Push to the remote"
    }

    private func pull() {
        Task {
            let failure = await store.pull(repoPath: repoPath)
            await onWorkingTreeChanged()
            if let failure { alertMessage = failure }
        }
    }

    private func push(forceWithLease: Bool = false) {
        guard !branch.isEmpty else { return }
        // Derived at click time from real tracking configuration, never
        // synthesised: this is what makes a first push `--set-upstream`.
        let setUpstream = !hasUpstream
        Task {
            let failure = await store.push(
                repoPath: repoPath,
                branch: branch,
                setUpstream: setUpstream,
                forceWithLease: forceWithLease
            )
            await onWorkingTreeChanged()
            if let failure { alertMessage = failure }
        }
    }

    private func fetch() {
        Task {
            let failure = await store.fetch(repoPath: repoPath)
            await onWorkingTreeChanged()
            if let failure { alertMessage = failure }
        }
    }
}

/// Thin strip surfacing the in-flight network operation at the top of the
/// window: determinate once git reports step percentages, indeterminate
/// before the first tick (and throughout fetch, which never reports), with
/// git's own progress line shown verbatim — the native counterpart of the
/// Tauri header's in-button fill plus raw text.
struct SyncProgressBanner: View {
    let operation: NetworkOperation
    /// Aggregate 0–100, or `nil` while no percentage is known.
    let percent: Double?
    /// Raw git progress line, e.g. "Writing objects:  53% (531/1000), …".
    let text: String?

    private var title: String {
        switch operation {
        case .fetch: "Fetching"
        case .pull: "Pulling"
        case .push: "Pushing"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(title)
                    .font(.callout)
                    .fontWeight(.medium)
                if let text {
                    Text(text)
                        .font(.callout.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                Spacer(minLength: 0)
            }
            if let percent {
                ProgressView(value: percent, total: 100)
            } else {
                ProgressView()
                    .progressViewStyle(.linear)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.regularMaterial)
    }
}
