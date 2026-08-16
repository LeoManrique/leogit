import SwiftUI

/// The toolbar's sync control — one adaptive split button, the GitHub
/// Desktop model running on LeoGit's own workflow machinery.
///
/// A strict precedence ladder picks the single action that makes sense right
/// now (GitHub Desktop's order, adapted): detached HEAD → publish repository
/// (no remote) → publish branch (no upstream) → pull (behind) → push (ahead)
/// → fetch (in sync). Pull outranks push, so a diverged branch proposes the
/// step that must happen first — the ahead count stays visible in the window
/// subtitle meanwhile. Force push with lease is menu-only, offered only
/// while diverged, and behind a destructive confirmation. Fetch is the
/// primary action once nothing needs pulling or pushing — the manual "check
/// the remote" — and a menu item in every split state, which is what let the
/// separate toolbar Refresh button go (⌘R in the View menu still forces the
/// local reload). Underneath, nothing changed: single-slot operations,
/// `--ff`-only pull, gh-based publish, and the progress banner.
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
    @State private var isPublishSheetPresented = false

    /// What the button proposes. One state at a time, decided by `action`'s
    /// precedence ladder.
    private enum ProposedAction {
        /// Status not loaded yet: a neutral disabled Fetch, so the button
        /// never flashes "Publish" before the first load resolves.
        case loading
        /// Detached HEAD — nothing can be pushed or pulled; disabled.
        case detached
        /// No remote at all: create the GitHub repo and push in one shot.
        case publishRepository
        /// A remote exists but the branch is untracked — the first push must
        /// carry `--set-upstream`, and the button says what it will do.
        case publishBranch
        /// Behind the upstream: pulling comes first, whatever else is
        /// pending.
        case pull
        /// Ahead only: push.
        case push
        /// In sync: check the remote.
        case fetch
    }

    private var branch: String { status?.branch ?? "" }
    private var hasUpstream: Bool { status?.hasUpstream ?? false }
    private var ahead: Int32 { status?.ahead ?? 0 }
    private var behind: Int32 { status?.behind ?? 0 }

    /// Force push is offered only for a truly diverged branch: real upstream
    /// tracking plus commits on both sides. By the precedence ladder this can
    /// only be true in the `.pull` state, so the menu item appears exactly
    /// where GitHub Desktop puts it.
    private var hasDiverged: Bool { hasUpstream && ahead > 0 && behind > 0 }

    private var isBusy: Bool { store.activeOperation != nil }

    private var action: ProposedAction {
        // A detached HEAD has an empty branch name, so it passes its own way.
        guard let status, !status.branch.isEmpty || status.detached else { return .loading }
        // Detached before the remote checks — GitHub Desktop orders these the
        // other way round, but publishing would push a branch that does not
        // exist, so the honest state wins.
        if status.detached { return .detached }
        if !status.hasRemote { return .publishRepository }
        if !status.hasUpstream { return .publishBranch }
        if status.behind > 0 { return .pull }
        if status.ahead > 0 { return .push }
        return .fetch
    }

    var body: some View {
        Group {
            switch action {
            case .loading, .detached:
                plainButton {}
                    .disabled(true)
            case .publishRepository:
                plainButton { isPublishSheetPresented = true }
                    .disabled(isBusy)
            case .fetch:
                plainButton(action: fetch)
                    .disabled(isBusy)
            case .publishBranch, .push:
                splitButton { push() }
            case .pull:
                splitButton { pull() }
            }
        }
        // macOS toolbars render Labels icon-only unless told otherwise, and
        // the state word is the whole point of an adaptive button.
        .labelStyle(.titleAndIcon)
        .help(helpText)
        .sheet(isPresented: $isPublishSheetPresented) {
            PublishSheet(
                store: store,
                repoPath: repoPath,
                onPublished: onWorkingTreeChanged
            )
        }
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

    /// The states with no meaningful secondary action — GitHub Desktop's
    /// no-dropdown states.
    private func plainButton(action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
        }
    }

    /// The split states: the proposed action on the button face, plus a
    /// chevron menu that always offers Fetch and — only while diverged —
    /// force push with lease.
    ///
    /// Deliberately the stock `Menu(primaryAction:)` split button, nothing
    /// hand-built. macOS bridges a toolbar control's label to a system
    /// control that renders only its text and icon, so no custom view — a
    /// count pill included — can ride the face; and no system API badges a
    /// macOS toolbar item (the 26 SDKs' toolbar badges are iOS-only). The
    /// pending counts live where the platform can show them: the window
    /// subtitle, the Changes tab pill, and this button's tooltip.
    private func splitButton(primaryAction: @escaping () -> Void) -> some View {
        Menu {
            Button("Fetch", action: fetch)
                .disabled(isBusy)

            if hasDiverged {
                Divider()
                Button("Force Push (with Lease)…", role: .destructive) {
                    isConfirmingForcePush = true
                }
                .disabled(isBusy)
            }
        } label: {
            Label(title, systemImage: icon)
        } primaryAction: {
            primaryAction()
        }
        .disabled(isBusy)
    }

    private var title: String {
        if let operation = store.activeOperation {
            switch operation {
            case .fetch: return "Fetching…"
            case .pull: return "Pulling…"
            case .push: return "Pushing…"
            case .publish: return "Publishing…"
            }
        }
        switch action {
        case .loading, .fetch: return "Fetch"
        case .detached: return "Push"
        case .publishRepository: return "Publish"
        case .publishBranch: return "Publish Branch"
        case .pull: return "Pull"
        case .push: return "Push"
        }
    }

    private var icon: String {
        switch action {
        case .loading, .fetch: "arrow.triangle.2.circlepath"
        case .detached, .push: "arrow.up"
        case .publishRepository: "icloud.and.arrow.up"
        case .publishBranch: "arrow.up.circle"
        case .pull: "arrow.down"
        }
    }

    private var helpText: String {
        switch action {
        case .loading:
            "Loading repository status"
        case .detached:
            "Detached HEAD — check out a branch to push"
        case .publishRepository:
            "Publish this repository to GitHub — creates the remote repo and pushes this branch"
        case .publishBranch:
            "Publish this branch to the remote and start tracking it"
        case .pull:
            "Pull \(behind) commit\(behind == 1 ? "" : "s") from the remote"
        case .push:
            "Push \(ahead) commit\(ahead == 1 ? "" : "s") to the remote"
        case .fetch:
            "Fetch from the remote — updates the ahead/behind counts without touching your files"
        }
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
        case .publish: "Publishing to GitHub"
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
