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
    let status: RepoStatus?
    let isMerging: Bool
    /// Called after any operation that may move HEAD or touch the working
    /// tree (switch, create, merge, abort) — the owner reloads status/log.
    let onWorkingTreeChanged: () async -> Void

    @State private var isCreating = false
    @State private var mergeSource: MergeSource?
    @State private var pendingDelete: String?
    @State private var isConfirmingDelete = false
    @State private var isConfirmingAbort = false
    /// Failure text from menu-launched operations, shown as an alert.
    @State private var alertMessage: String?

    /// Current branch by name comparison against status, like the Tauri
    /// client — empty while detached, so no row gets the checkmark.
    private var currentBranch: String { status?.branch ?? "" }
    private var isDetached: Bool { status?.detached ?? false }

    /// Anything but the current branch can be merged into it, remotes included.
    private var mergeCandidates: [BranchInfo] {
        store.branches.filter { $0.name != currentBranch }
    }

    /// Only local, non-current branches are deletable, like the Tauri client.
    private var deletableBranches: [BranchInfo] {
        store.localBranches.filter { $0.name != currentBranch }
    }

    var body: some View {
        Menu {
            Picker("Local Branches", selection: switchSelection) {
                ForEach(store.localBranches) { branch in
                    Text(branch.name).tag(branch.name)
                }
            }
            .pickerStyle(.inline)
            // Menu content is built when the menu opens, so this reloads the
            // list at the moment of intent — how branches created or deleted
            // from an outside terminal appear without a manual refresh (the
            // status poll only catches the ones that move HEAD). `load`
            // replaces rows in place on success and never touches `isBusy`,
            // so the open menu doesn't flicker.
            .onAppear {
                Task { await store.load(repoPath: repoPath) }
            }

            if !store.remoteBranches.isEmpty {
                Section("Remote Branches") {
                    ForEach(store.remoteBranches) { branch in
                        Button(branch.name) { switchTo(branch.name) }
                    }
                }
            }

            Section {
                Button("New Branch…") { isCreating = true }

                if !isDetached && !mergeCandidates.isEmpty {
                    Menu("Merge into “\(currentBranch)”…") {
                        ForEach(mergeCandidates) { branch in
                            Button(branch.name) {
                                mergeSource = MergeSource(name: branch.name)
                            }
                        }
                    }
                }

                if isMerging {
                    Button("Abort Merge…", role: .destructive) {
                        isConfirmingAbort = true
                    }
                }

                if !deletableBranches.isEmpty {
                    Menu("Delete Branch") {
                        ForEach(deletableBranches) { branch in
                            Button(branch.name, role: .destructive) {
                                pendingDelete = branch.name
                                isConfirmingDelete = true
                            }
                        }
                    }
                }
            }
        } label: {
            Label(menuLabel, systemImage: "arrow.triangle.branch")
        }
        .disabled(store.isBusy)
        .help(isDetached ? "Detached HEAD — pick a branch to return to" : "Switch branch")
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
            ) { failure in
                await onWorkingTreeChanged()
                if let failure {
                    alertMessage = failure
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
            "Abort Merge?",
            isPresented: $isConfirmingAbort
        ) {
            Button("Abort Merge", role: .destructive) { abortMerge() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Conflict resolutions are discarded and the working tree returns to its pre-merge state.")
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

    private var menuLabel: String {
        if isDetached { return "Detached" }
        return currentBranch.isEmpty ? "Branches" : currentBranch
    }

    /// Selection drives the switch: reading reflects status, writing checks
    /// out the picked branch.
    private var switchSelection: Binding<String> {
        Binding(
            get: { currentBranch },
            set: { picked in
                guard picked != currentBranch else { return }
                switchTo(picked)
            }
        )
    }

    private func switchTo(_ branch: String) {
        Task {
            if let failure = await store.switchTo(branch, repoPath: repoPath) {
                alertMessage = failure
            } else {
                await onWorkingTreeChanged()
            }
        }
    }

    private func delete(_ name: String) {
        pendingDelete = nil
        Task {
            if let failure = await store.delete(name, repoPath: repoPath) {
                alertMessage = failure
            }
        }
    }

    private func abortMerge() {
        Task {
            let failure = await store.abortMerge(repoPath: repoPath)
            // Success or not, MERGE_HEAD and the working tree may have
            // changed — reload before surfacing any failure.
            await onWorkingTreeChanged()
            if let failure {
                alertMessage = failure
            }
        }
    }
}

/// Identifiable wrapper so `.sheet(item:)` can present the chosen source.
private struct MergeSource: Identifiable {
    let name: String
    var id: String { name }
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

            HStack {
                if store.isBusy {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Create", action: submit)
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(trimmedName.isEmpty || store.isBusy)
            }
        }
        .padding(20)
    }

    private func submit() {
        let branchName = trimmedName
        guard !branchName.isEmpty, !store.isBusy else { return }
        failureText = nil
        Task {
            if let failure = await store.createAndSwitch(named: branchName, repoPath: repoPath) {
                failureText = failure
            } else {
                dismiss()
                await onCreated()
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

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Merge Branch")
                .font(.headline)

            Text("Merge \(Text(source).fontWeight(.semibold)) into \(Text(target).fontWeight(.semibold)).")

            if let commitCount {
                Text(commitCount == 1 ? "Brings in 1 commit." : "Brings in \(commitCount) commits.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            HStack {
                if store.isBusy {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Squash & Merge") { run(squash: true) }
                    .disabled(store.isBusy)
                Button("Merge") { run(squash: false) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(store.isBusy)
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
            let failure = await store.merge(source, squash: squash, repoPath: repoPath)
            dismiss()
            await onFinished(failure)
        }
    }
}
