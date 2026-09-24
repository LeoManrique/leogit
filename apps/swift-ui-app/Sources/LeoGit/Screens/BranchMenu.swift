import SwiftUI

/// Toolbar branch picker and branch actions.
///
/// Switching is an inline `Picker` (the checkmark marks the current branch,
/// exactly the macOS menu convention); remote branches switch via plain
/// buttons since selecting one *creates* a local tracking branch rather than
/// marking a selection. Create runs in a sheet, delete and abort behind
/// destructive confirmation dialogs, merge in a sheet with a commit-count
/// preview — the flow the Tauri client wired up but never exposed.
struct BranchMenu: View {
    let store: BranchStore
    let repoPath: String

    /// The live status — `nil` until the open repository's first read lands.
    /// Everything this control *does* reads from here, so a menu opened in that
    /// window offers nothing about a repository nobody has read yet.
    let status: RepoStatus?

    /// What the label draws, which is the last read that landed rather than the
    /// live one — see `ToolbarStatus`. Display only: the branch name on a
    /// toolbar chip is not a control, while losing it for 100–500 ms on every
    /// switch resizes the leading cluster twice.
    let shown: ToolbarStatus

    /// The branch a cherry-pick begun in History came from, while that pick is
    /// stopped on a conflict — where aborting it ends. `nil` for every other
    /// operation, including a cherry-pick begun in a terminal.
    let cherryPickSource: String?

    /// Called after any operation that may move HEAD or touch the working
    /// tree (switch, create, merge, abort) — the owner reloads status/log.
    let onWorkingTreeChanged: () async -> Void

    /// Something worth reading that is not a failure — §6.13's second class,
    /// the screen's dismissible strip. An abort that worked but could not
    /// rewind all the way says so here, in git's words.
    let onNotice: (String) -> Void

    /// A branch operation the user was waiting on that failed — §6.13's first
    /// class. Reported rather than presented, for the reason `ChangesSidebar`
    /// reports its own: the repository screen shows one at a time, and this
    /// control is a *toolbar item*, which is the worst place in the window to
    /// own a modal — it can already be behind one of its own sheets when the
    /// answer arrives.
    let onFailure: (ActionFailure) -> Void

    @State private var isCreating = false
    @State private var mergeSource: MergeSource?
    @State private var pendingDelete: String?
    @State private var isConfirmingDelete = false
    /// The operation the abort confirmation is asking about. Held apart from
    /// the live `operation` so the question keeps its wording if a status poll
    /// lands while it is up; core aborts whatever is open when it is answered.
    @State private var pendingAbort: OperationInProgress?
    @State private var isConfirmingAbort = false

    /// Current branch by name comparison against status, like the Tauri
    /// client — empty while detached, so no row gets the checkmark.
    ///
    /// The *live* status, never the held one: this decides which row is ticked
    /// and which branch a merge merges into, and a menu opened before the new
    /// repository's first read must tick nothing rather than the branch the
    /// previous one was on.
    private var currentBranch: String { status?.branch ?? "" }
    private var isDetached: Bool { status?.detached ?? false }
    /// The operation the repository is stopped in the middle of — live, like
    /// everything else here that an item acts on.
    private var operation: OperationInProgress? { status?.operation }

    /// This control's items as data, with every closure wired straight to the
    /// state beside it. The identical value is published to the menu bar from
    /// the window content (a focused scene value set inside `.toolbar` never
    /// reaches the scene), where the closures post instead.
    private var command: BranchCommand {
        BranchCommand(
            localBranches: store.localBranches,
            remoteBranches: store.remoteBranches,
            current: currentBranch,
            isDetached: isDetached,
            operation: operation,
            isBlocked: store.isBlocked,
            perform: perform
        )
    }

    var body: some View {
        Menu {
            // Menu content is built when the menu opens, so `onOpen` reloads
            // the list at the moment of intent — how branches created or
            // deleted from an outside terminal appear without a manual
            // refresh (the status poll only catches the ones that move HEAD).
            // `load` replaces rows in place on success and never touches
            // `isRunning`, so the open menu doesn't flicker.
            BranchMenuContent(command: command) {
                Task { await store.load(repoPath: repoPath) }
            }
        } label: {
            Label {
                Text(menuLabel)
            } icon: {
                Image(systemName: "arrow.triangle.branch")
            }
        }
        // macOS toolbars render Labels icon-only by default; the branch name
        // is this control's whole value.
        .labelStyle(.titleAndIcon)
        // The repo chip beside this one opens a popover with no indicator,
        // so a chevron on only half the pair reads as an inconsistency.
        .menuIndicator(.hidden)
        // Dimmed for this menu's own operation only. Under someone else's
        // write — a commit — the chip still opens, so the branch can be read;
        // it is the items inside that refuse to start (`BranchCommand.isBlocked`).
        .disabled(store.isRunning)
        .help(isDetached ? "Detached HEAD — pick a branch to return to" : "Switch branch")
        // Right-click acts on the branch you are on; left-click browses the
        // others. The live name, like every item here that acts on something:
        // empty while detached or before the first read, and the item stays
        // in place greyed out rather than copying a label that is not a name.
        .toolbarContextMenu {
            Button("Copy Branch Name") { Clipboard.copy(currentBranch) }
                .disabled(currentBranch.isEmpty)
        }
        .sheet(isPresented: $isCreating) {
            CreateBranchSheet(store: store, repoPath: repoPath) {
                await onWorkingTreeChanged()
            }
        }
        .sheet(item: $mergeSource) { source in
            MergeSheet(
                source: source.name,
                target: currentBranch,
                store: store,
                repoPath: repoPath
            ) { message in
                await onWorkingTreeChanged()
                if let message {
                    onFailure(ActionFailure(message))
                }
            }
        }
        .confirmationDialog(
            "Delete Branch?",
            isPresented: $isConfirmingDelete,
            presenting: pendingDelete
        ) { name in
            Button("Delete", role: .destructive) { delete(name) }
            Button("Cancel", role: .cancel) { pendingDelete = nil }
        } message: { name in
            Text("Are you sure you want to delete “\(name)”? Unmerged commits are lost.")
        }
        .confirmationDialog(
            "Abort \(pendingAbort?.words.title ?? "Operation")?",
            isPresented: $isConfirmingAbort,
            presenting: pendingAbort
        ) { operation in
            Button("Abort \(operation.words.title)", role: .destructive) { abortOperation() }
            Button("Cancel", role: .cancel) { pendingAbort = nil }
        } message: { operation in
            Text(abortConsequence(of: operation))
        }
        // The same items chosen from the menu bar. They arrive as requests
        // rather than as calls because `BranchCommands` lives on the scene,
        // while every sheet and confirmation an action opens lives here.
        .onReceive(NotificationCenter.default.publisher(for: .leogitBranchActionRequested)) {
            notification in
            guard let action = notification.object as? BranchAction else { return }
            perform(action)
        }
    }

    /// The one place a branch action turns into state. Both surfaces route
    /// here, so a menu-bar Merge opens the same sheet the toolbar's does —
    /// and a busy store refuses both alike, rather than one of them.
    @MainActor
    private func perform(_ action: BranchAction) {
        guard !store.isBlocked else { return }
        switch action {
        case let .switchTo(branch):
            switchTo(branch)
        case .create:
            isCreating = true
        case let .merge(branch):
            mergeSource = MergeSource(name: branch)
        case .abortOperation:
            // Nothing to ask about if the operation ended since the menu drew.
            guard let operation else { return }
            pendingAbort = operation
            isConfirmingAbort = true
        case let .delete(branch):
            pendingDelete = branch
            isConfirmingDelete = true
        }
    }

    /// The exceptional states ride the label too — the window subtitle that
    /// used to carry them is gone, since the toolbar title area no longer
    /// renders (the repo name lives on the switcher chip).
    ///
    /// An `AttributedString` rather than a `String` for one reason: the
    /// operation suffix (`· merging`, `· rebasing`, …) carries the conflict
    /// colour, so the one state that changes what half this menu's items *do*
    /// stops reading as more of the branch name. It is still the first thing
    /// macOS truncates — a toolbar label has the width it has — but a coloured
    /// run survives being clipped far better than a grey one, and the
    /// conflicted files in the Changes tab wear the same colour.
    ///
    /// The suffix rides the detached label as well as a branch name: a rebase
    /// detaches HEAD for as long as it runs, so that is where it shows most.
    ///
    /// Every state here — the detached label, the operation suffix, the name
    /// itself — is drawn from `shown`, the held read, so none of them can drop
    /// to the bare "Branches" for the window in which the newly opened
    /// repository has no status.
    private var menuLabel: AttributedString {
        var label = AttributedString(placeLabel)
        guard let operation = shown.operation else { return label }
        var suffix = AttributedString(" · \(operation.words.gerund)")
        suffix.foregroundColor = .conflict
        label.append(suffix)
        return label
    }

    /// Where HEAD is, in words: the branch, the detached commit, or the
    /// control's own name before any status has landed.
    private var placeLabel: String {
        if shown.isDetached {
            return shown.headSha.isEmpty
                ? "Detached"
                : "Detached at \(String(shown.headSha.prefix(7)))"
        }
        return shown.branch.isEmpty ? "Branches" : shown.branch
    }

    private func switchTo(_ branch: String) {
        Task {
            switch await store.switchTo(branch, repoPath: repoPath) {
            case .succeeded:
                await onWorkingTreeChanged()
            case let .failed(message):
                onFailure(ActionFailure(message))
            // Nothing was attempted, so there is nothing to report and
            // nothing to re-read — the branch the user asked for is simply
            // still not checked out, and the menu is still there to ask again.
            case .refusedBusy:
                break
            }
        }
    }

    private func delete(_ name: String) {
        Task {
            switch await store.delete(name, repoPath: repoPath) {
            case .succeeded:
                pendingDelete = nil
            case let .failed(message):
                pendingDelete = nil
                onFailure(ActionFailure(message))
            // The branch is still there, so the confirmation it was asked
            // about stays too: clearing it here would take the question away
            // and leave the answer unchanged, with nothing on screen saying so.
            case .refusedBusy:
                isConfirmingDelete = true
            }
        }
    }

    /// What the abort confirmation promises. A cherry-pick begun in History
    /// also leaves the branch it was picking onto, and says so.
    private func abortConsequence(of operation: OperationInProgress) -> String {
        let rewind =
            "Conflict resolutions are discarded, and the branch and working tree return to where they were before the \(operation.words.noun)."
        guard let cherryPickSource else { return rewind }
        return "\(rewind) You will be back on “\(cherryPickSource)”, where the commits came from."
    }

    private func abortOperation() {
        // Read before the abort: the reload that follows ends the cherry-pick,
        // and with it the record of where it came from.
        let source = cherryPickSource
        Task {
            let (outcome, said, stranded) = await store.abortOperation(
                returningTo: source,
                repoPath: repoPath
            )
            // A refused abort touched nothing, so there is nothing to re-read
            // and the question stays up, as it does for a refused delete.
            if case .refusedBusy = outcome {
                isConfirmingAbort = true
                return
            }
            pendingAbort = nil
            // Success or not, git's state files and the working tree may have
            // changed — reload before surfacing anything.
            await onWorkingTreeChanged()
            if case let .failed(message) = outcome {
                onFailure(ActionFailure(message))
                return
            }
            // Two separate things to say, and either can come without the
            // other: where the user has been left, and what git said about
            // how far the abort rewound.
            if let stranded { onFailure(ActionFailure(stranded)) }
            if let said { onNotice(said) }
        }
    }
}

/// Identifiable wrapper so `.sheet(item:)` can present the chosen source.
private struct MergeSource: Identifiable {
    let name: String
    var id: String { name }
}

/// What a branch item asks for. Carried as the object of
/// `.leogitBranchActionRequested` when the request comes from the menu bar.
enum BranchAction: Sendable {
    case switchTo(String)
    case create
    case merge(String)
    case abortOperation
    case delete(String)
}

/// Everything the branch items are a function of, plus the one closure that
/// performs them — published as a focused scene value so the menu bar can
/// render the same list the toolbar control does.
struct BranchCommand {
    var localBranches: [BranchInfo]
    var remoteBranches: [BranchInfo]
    var current: String
    var isDetached: Bool
    /// The operation in progress, if any — what Abort is named for, and why
    /// Merge is withheld.
    var operation: OperationInProgress?
    /// One repository write runs at a time; a second would contend on
    /// `index.lock`. The items dim rather than silently refusing, in the
    /// toolbar menu and the menu bar alike.
    var isBlocked: Bool
    var perform: (BranchAction) -> Void
}

extension FocusedValues {
    @Entry var branchCommand: BranchCommand?
}

/// The branch items themselves, defined once and rendered by both the toolbar
/// menu and the menu-bar Branch menu.
///
/// Written as a view rather than duplicated into each host because the two had
/// no way to stay in step otherwise, and a branch action reachable from one
/// menu and not the other is precisely the drift this plan keeps finding. The
/// derived sets — what can be merged, what can be deleted — live here for the
/// same reason: they are rules about the list, not about either host.
struct BranchMenuContent: View {
    let command: BranchCommand

    /// Whether this copy owns the key equivalents. Only the menu bar does:
    /// the two hosts render the same items, and declaring a shortcut in both
    /// registers the same chord twice, which SwiftUI resolves arbitrarily.
    /// The menu bar is also the copy that is always present, so a chord bound
    /// there does not come and go with a popover.
    var bindsShortcuts = false

    /// Run when this copy is built, which for a pull-down menu is the moment
    /// it opens. Carried by the toolbar copy alone: the menu bar's is rebuilt
    /// on every republish, not on opening, so hanging a `for-each-ref` off it
    /// would spend one per state change and answer nobody's intent.
    /// Attached to a single child rather than the whole body — a `Group`
    /// propagates a modifier to each of its children, which would mean one
    /// reload per section.
    var onOpen: (() -> Void)?

    /// Anything but the current branch can be merged into it, remotes
    /// included.
    private var mergeCandidates: [BranchInfo] {
        (command.localBranches + command.remoteBranches).filter { $0.name != command.current }
    }

    /// Only local, non-current branches are deletable, like the Tauri client.
    private var deletableBranches: [BranchInfo] {
        command.localBranches.filter { $0.name != command.current }
    }

    /// Selection drives the switch: reading reflects status, writing checks
    /// out the picked branch.
    private var switchSelection: Binding<String> {
        Binding(
            get: { command.current },
            set: { picked in
                guard picked != command.current else { return }
                command.perform(.switchTo(picked))
            }
        )
    }

    var body: some View {
        Group {
            Picker("Local Branches", selection: switchSelection) {
                ForEach(command.localBranches) { branch in
                    Text(branch.name).tag(branch.name)
                }
            }
            .pickerStyle(.inline)
            .onAppear { onOpen?() }

            if !command.remoteBranches.isEmpty {
                Section("Remote Branches") {
                    ForEach(command.remoteBranches) { branch in
                        Button(branch.name) { command.perform(.switchTo(branch.name)) }
                    }
                }
            }

            Section {
                Button("New Branch…") { command.perform(.create) }
                    .modifier(OptionalShortcut(key: "n", isActive: bindsShortcuts))

                // Hidden while an operation is open: git refuses a merge over
                // an unfinished merge, rebase or pick, so offering the submenu
                // there is an invitation to a refusal. Abort takes its place
                // below, named for whatever is running.
                if !command.isDetached && command.operation == nil && !mergeCandidates.isEmpty {
                    Menu("Merge into “\(command.current)”…") {
                        ForEach(mergeCandidates) { branch in
                            Button(branch.name) { command.perform(.merge(branch.name)) }
                        }
                    }
                }

                if let operation = command.operation {
                    Button("Abort \(operation.words.title)…", role: .destructive) {
                        command.perform(.abortOperation)
                    }
                }

                if !deletableBranches.isEmpty {
                    Menu("Delete Branch") {
                        ForEach(deletableBranches) { branch in
                            Button(branch.name, role: .destructive) {
                                command.perform(.delete(branch.name))
                            }
                        }
                    }
                }
            }
        }
        .disabled(command.isBlocked)
    }
}

/// ⇧⌘N, but only on the copy of the menu that owns it.
///
/// `.keyboardShortcut` has no "sometimes" form, and a plain `if` around the
/// button would give SwiftUI two structurally different views for the same
/// item. A modifier keeps one view and turns the binding on or off.
private struct OptionalShortcut: ViewModifier {
    let key: KeyEquivalent
    let isActive: Bool

    func body(content: Content) -> some View {
        content.keyboardShortcut(
            isActive ? KeyboardShortcut(key, modifiers: [.shift, .command]) : nil
        )
    }
}

/// Name a branch, create it off HEAD, and land on it. Failures show inline
/// and keep the sheet open so the name can be corrected.
private struct CreateBranchSheet: View {
    let store: BranchStore
    let repoPath: String
    let onCreated: () async -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var failureText: String?

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Create New Branch")
                .font(.headline)

            TextField("Branch name", text: $name)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 300)
                .onSubmit(submit)

            if let failureText {
                Text(failureText)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
                    .lineLimit(3)
            }

            if store.isBlocked, !store.isRunning { WriteBlockedNote() }

            HStack {
                if store.isRunning {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Create", action: submit)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(trimmedName.isEmpty || store.isBlocked)
            }
        }
        .padding(20)
    }

    private func submit() {
        let branchName = trimmedName
        guard !branchName.isEmpty, !store.isBlocked else { return }
        failureText = nil
        Task {
            switch await store.createAndSwitch(named: branchName, repoPath: repoPath) {
            case .succeeded:
                dismiss()
                await onCreated()
            case let .failed(message):
                failureText = message
            // No branch was created, so the sheet stays open with the typed
            // name intact — it used to dismiss here and report success.
            case .refusedBusy:
                break
            }
        }
    }
}

/// Pick a strategy and merge `source` into `target`, previewing how many
/// commits it brings in. The sheet always closes once the merge ran; a
/// failure (most commonly conflicts) is reported to the owner, and the
/// conflicted files show up in the changes list.
private struct MergeSheet: View {
    let source: String
    let target: String
    let store: BranchStore
    let repoPath: String
    /// Receives core's failure text, or `nil` when the merge succeeded.
    let onFinished: (String?) async -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var commitCount: Int32?

    /// Nothing to bring in. The sheet says so and takes the primary away
    /// rather than offering a merge that would be a no-op — a live Merge
    /// button beside "Brings in 0 commits." invites a click that does nothing
    /// and then reports success.
    private var isUpToDate: Bool { commitCount == 0 }

    /// The count line, or nil while the count is still being read — a separate
    /// property because a nested ternary inside a `Text` interpolation is what
    /// sends SwiftUI's type-checker into a crawl.
    private var previewText: String? {
        guard let commitCount else { return nil }
        if isUpToDate { return "“\(target)” is already up to date with “\(source)”." }
        return commitCount == 1 ? "Brings in 1 commit." : "Brings in \(commitCount) commits."
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Merge Branch")
                .font(.headline)

            Text("Merge \(Text(source).fontWeight(.semibold)) into \(Text(target).fontWeight(.semibold)).")

            if let previewText {
                Text(previewText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            if store.isBlocked, !store.isRunning { WriteBlockedNote() }

            HStack {
                if store.isRunning {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Squash & Merge") { run(squash: true) }
                    .disabled(store.isBlocked || isUpToDate)
                Button("Merge") { run(squash: false) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(store.isBlocked || isUpToDate)
            }
        }
        .padding(20)
        .frame(minWidth: 360)
        .task {
            commitCount = try? await GitBridge.commitsToMerge(in: repoPath, from: source)
        }
    }

    private func run(squash: Bool) {
        Task {
            switch await store.merge(source, squash: squash, repoPath: repoPath) {
            case .succeeded:
                dismiss()
                await onFinished(nil)
            case let .failed(message):
                dismiss()
                await onFinished(message)
            // Nothing was merged, so the sheet stays up still offering the
            // merge — it used to dismiss and report the merge as done.
            case .refusedBusy:
                break
            }
        }
    }
}
