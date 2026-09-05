import SwiftUI

/// The toolbar's sync control — one adaptive split button running on
/// LeoGit's own workflow machinery.
///
/// A strict precedence ladder picks the single action that makes sense right
/// now: detached HEAD → publish repository (no remote) → publish branch (no
/// upstream) → pull (behind) → push (ahead) → fetch (in sync). Pull outranks
/// push, so a diverged branch proposes the step that must happen first — the
/// ahead count stays visible in the toolbar counts beside this control
/// meanwhile. Force push with lease is menu-only, offered only while
/// diverged, and behind a destructive confirmation. Fetch is the
/// primary action once nothing needs pulling or pushing — the manual "check
/// the remote" — and a menu item in every split state, which is what let the
/// separate toolbar Refresh button go (⌘R in the View menu still forces the
/// local reload). Underneath, nothing changed: single-slot operations,
/// `--ff`-only pull, gh-based publish, and the progress banner.
struct SyncControls: View {
    let store: SyncStore
    let repoPath: String

    /// The live status — `nil` until the open repository's first read lands.
    /// Everything this control *runs*, and everything that decides whether it
    /// may run at all, reads from here.
    let status: RepoStatus?

    /// What the control draws, which is the last read that landed rather than
    /// the live one — see `ToolbarStatus`. Display only: it picks the title,
    /// the symbol, whether the button wears a chevron, and the tooltip's
    /// counts, none of which can act.
    let shown: ToolbarStatus
    /// Called after any network operation: pull moves HEAD and the working
    /// tree, push/fetch move the ahead/behind counts.
    let onWorkingTreeChanged: () async -> Void

    /// A transfer the user was waiting on that failed — §6.13's first class,
    /// git's own message verbatim. Reported rather than presented, for the
    /// reason `ChangesSidebar` reports its own: the repository screen shows
    /// one at a time, and this control is a *toolbar item*, which is the worst
    /// place in the window to own a modal — it can already be behind the
    /// publish or force-push sheet when the answer arrives.
    let onFailure: (ActionFailure) -> Void

    @State private var isConfirmingForcePush = false
    @State private var isPublishSheetPresented = false

    /// The live reads the *actions* are built from: what gets pushed, whether
    /// that push sets an upstream, and whether the divergence that unlocks
    /// force push is real. All four fall to their empty values while the open
    /// repository has no status, which is what makes those actions unavailable
    /// rather than wrong.
    private var branch: String { status?.branch ?? "" }
    private var hasUpstream: Bool { status?.hasUpstream ?? false }
    private var ahead: Int32 { status?.ahead ?? 0 }
    private var behind: Int32 { status?.behind ?? 0 }

    /// Force push is offered only for a truly diverged branch: real upstream
    /// tracking plus commits on both sides. By the precedence ladder this can
    /// only be true in the `.pull` state, which is where the menu item
    /// appears.
    private var hasDiverged: Bool { hasUpstream && ahead > 0 && behind > 0 }

    private var isBusy: Bool { store.activeOperation != nil }

    /// The ladder's answer as the button *runs* it, decided in core and carried
    /// on the status itself. `nil` status means the first read hasn't landed —
    /// a fact about this view's own load, which is why it is the one case
    /// decided here — and `.loading` is not actionable, so nothing can fire
    /// against a repository nobody has read yet (F18).
    private var action: SyncProposal { status?.proposal ?? .loading }

    /// The ladder's answer as the button *draws* it: the same one while there
    /// is a status, the last that landed while there is not. Holding it is what
    /// keeps the control from collapsing to a narrower plain "Fetch" — chevron
    /// and all — on every switch, for the 100–500 ms the first read takes.
    private var face: SyncProposal { shown.proposal }

    /// Whether the control may act: not while a transfer holds the slot, and
    /// not before this repository's own status has landed. The second half is
    /// what today's `nil` → `.loading` → disabled did, said once so that
    /// holding the face above cannot loosen it.
    private var isEnabled: Bool { !isBusy && status != nil }

    var body: some View {
        Group {
            switch face {
            case .loading, .detached:
                plainButton(action: perform)
                    .disabled(true)
            case .publishRepository, .fetch:
                plainButton(action: perform)
                    .disabled(!isEnabled)
            case .publishBranch, .push, .pull:
                splitButton(primaryAction: perform)
            }
        }
        // macOS toolbars render Labels icon-only unless told otherwise, and
        // the state word is the whole point of an adaptive button.
        .labelStyle(.titleAndIcon)
        // The trailing action region emphasizes its label (bold text, heavier
        // symbol); the repo and branch chips at the leading edge render
        // regular. One chip family, one weight.
        .fontWeight(.regular)
        .help(helpText)
        // Repository ▸ <action> (⌘P) lands here. The menu item can't call
        // `perform()` directly — its focused value is published by the
        // window content, which doesn't own this view's sheet and alert
        // state — so it posts, and this view runs the exact click path.
        .onReceive(NotificationCenter.default.publisher(for: .leogitSyncActionRequested)) { _ in
            // The buttons express this as `.disabled`; the notification
            // path has to guard it itself — including the half that says the
            // face on screen may not be this repository's yet.
            guard isEnabled else { return }
            perform()
        }
        .sheet(isPresented: $isPublishSheetPresented) {
            PublishSheet(
                store: store,
                repoPath: repoPath,
                onPublished: onWorkingTreeChanged
            )
        }
        .sheet(isPresented: $isConfirmingForcePush) {
            ForcePushSheet(upstream: status?.upstream ?? "the remote branch") {
                await forcePush()
            }
        }
    }

    /// The states with no meaningful secondary action, so no dropdown.
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
    /// pending counts live where the platform can show them: their own toolbar
    /// item left of this one, and this button's tooltip.
    private func splitButton(primaryAction: @escaping () -> Void) -> some View {
        Menu {
            Button("Fetch", action: fetch)
                .disabled(!isEnabled)

            if hasDiverged {
                Divider()
                Button("Force Push (with Lease)…", role: .destructive) {
                    isConfirmingForcePush = true
                }
                .disabled(!isEnabled)
            }
        } label: {
            Label(title, systemImage: icon)
        } primaryAction: {
            primaryAction()
        }
        .disabled(!isEnabled)
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
        return face.title
    }

    private var icon: String {
        switch face {
        case .loading, .fetch: "arrow.triangle.2.circlepath"
        case .detached, .push: "arrow.up"
        case .publishRepository: "icloud.and.arrow.up"
        case .publishBranch: "arrow.up.circle"
        case .pull: "arrow.down"
        }
    }

    /// The tooltip explains the face, so its counts come from the same held
    /// read — a "Pull 0 commits" while the live status is still nil would be
    /// the one place the bar contradicted itself.
    private var helpText: String {
        switch face {
        case .loading:
            "Loading repository status"
        case .detached:
            "Detached HEAD — check out a branch to push"
        case .publishRepository:
            "Publish this repository to GitHub — creates the remote repo and pushes this branch"
        case .publishBranch:
            "Publish this branch to the remote and start tracking it"
        case .pull:
            "Pull \(shown.behind) commit\(shown.behind == 1 ? "" : "s") from the remote"
        case .push:
            "Push \(shown.ahead) commit\(shown.ahead == 1 ? "" : "s") to the remote"
        case .fetch:
            "Fetch from the remote — updates the ahead/behind counts without touching your files"
        }
    }

    /// Runs whatever the ladder proposes. The single entry point for the
    /// button face, the split button's primary action, and the menu command,
    /// so a state can never be reachable by one and not the others.
    ///
    /// Switches on `action`, the live proposal, and never on the face: a click
    /// cannot reach here while the two differ — `isEnabled` is false for
    /// exactly that window — and if one ever did, `.loading` does nothing
    /// rather than the previous repository's something.
    private func perform() {
        switch action {
        case .loading, .detached: break
        case .publishRepository: isPublishSheetPresented = true
        case .publishBranch, .push: push()
        case .pull: pull()
        case .fetch: fetch()
        }
    }

    private func pull() {
        Task { await settle(await store.pull(repoPath: repoPath)) }
    }

    private func fetch() {
        Task { await settle(await store.fetch(repoPath: repoPath)) }
    }

    private func push() {
        Task { await settle(await runPush(forceWithLease: false)) }
    }

    /// The force push's own caller: it re-reads like the others but *reports*
    /// nothing, because the sheet that asked is still on screen and keeps git's
    /// refusal inside itself (§6.13). It hands the outcome back so the sheet
    /// can tell "pushed" from "the slot was busy" — closing on the second would
    /// claim a push that never ran.
    private func forcePush() async -> OpOutcome {
        let outcome = await runPush(forceWithLease: true)
        await refresh(after: outcome)
        return outcome
    }

    /// Issue the push. Neither re-reads nor reports: its two callers differ on
    /// both.
    private func runPush(forceWithLease: Bool) async -> OpOutcome {
        guard !branch.isEmpty else { return .refusedBusy }
        // Derived at call time from real tracking configuration, never
        // synthesised: this is what makes a first push `--set-upstream`.
        let setUpstream = !hasUpstream
        return await store.push(
            repoPath: repoPath,
            branch: branch,
            setUpstream: setUpstream,
            forceWithLease: forceWithLease
        )
    }

    /// A finished transfer: re-read, then state any failure in the window's
    /// modal.
    private func settle(_ outcome: OpOutcome) async {
        await refresh(after: outcome)
        if case let .failed(message) = outcome {
            onFailure(ActionFailure(message))
        }
    }

    /// A refusal reaches here whenever the slot changed hands between the
    /// button enabling and the tap landing — a background auto-fetch is the
    /// usual one. It attempted nothing, so there is nothing to re-read.
    private func refresh(after outcome: OpOutcome) async {
        if case .refusedBusy = outcome { return }
        await onWorkingTreeChanged()
    }
}

/// The presentation half of core's `SyncProposal`, which is where the ladder
/// itself lives — one implementation for both clients, carried on
/// `RepoStatus.proposal` so the toolbar button, the ⌘P menu item and the Tauri
/// header can never disagree about what the repository needs next.
///
/// Only the words stay here: the two controls are shaped differently, and which
/// state earns a chevron is a macOS question rather than a policy one.
extension SyncProposal {
    /// The state word: the button face's title when idle, and the menu
    /// item's title always (a disabled "Pull" mid-pull reads better in a
    /// menu than "Pulling…").
    var title: String {
        switch self {
        case .loading, .fetch: "Fetch"
        case .detached: "Push"
        case .publishRepository: "Publish"
        case .publishBranch: "Publish Branch"
        case .pull: "Pull"
        case .push: "Push"
        }
    }

    /// Whether the proposal can be run at all — the two informational states
    /// have nothing to do, and both the button and the menu item say so by
    /// staying disabled.
    var isActionable: Bool {
        switch self {
        case .loading, .detached: false
        case .publishRepository, .publishBranch, .pull, .push, .fetch: true
        }
    }
}

/// The sync control's proposed action, published to the scene so the menu
/// bar can offer it under a keyboard shortcut. Carrying the title alongside
/// the closure is what lets the menu item rename itself — Publish, Pull,
/// Push, Fetch — instead of guessing at the state the toolbar is showing.
struct SyncCommand {
    let title: String
    let isEnabled: Bool
    let perform: () -> Void
}

extension FocusedValues {
    /// Set by the repository screen while a repository is open; `nil` on the
    /// welcome screen, which is what disables the menu item there.
    ///
    /// Deliberately published from the window content, not from
    /// `SyncControls` itself: a focused scene value set on a toolbar-hosted
    /// view never propagates to the scene — toolbar items render in their
    /// own hosting hierarchy — which left the menu item permanently
    /// disabled when this was first wired there.
    @Entry var syncCommand: SyncCommand?
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
