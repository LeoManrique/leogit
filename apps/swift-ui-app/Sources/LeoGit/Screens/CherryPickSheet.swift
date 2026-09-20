import SwiftUI

/// "Cherry-pick N Commits" — which local branch the selected commits are
/// copied onto.
///
/// A sheet with its own list rather than a submenu of branch names, which is
/// what Merge uses: a History context menu is already one level deep in a
/// selection, and a repository with sixty branches needs the filter. The
/// filter-and-cursor idiom is `RepoPickerList`'s — the arrows move the cursor
/// while the *field* keeps focus, which a `List(selection:)` cannot do.
///
/// It stays up while the pick runs, as `CheckoutCommitSheet` does and for the
/// same reasons: replaying commits is not instant, and a second write must not
/// be issued to race the first on `index.lock`. A refusal that choosing another
/// branch can fix — the target is checked out in another worktree — is stated
/// here, under the list (FRONTEND §6.13's refinement). A conflict is the one
/// outcome that leaves: it has already moved the repository onto the target,
/// and the work continues in the Changes tab.
struct CherryPickSheet: View {
    /// The commits being picked, newest first, snapshotted when the sheet
    /// opened — the menu's targets, not whatever the selection has become.
    let commits: [CommitInfo]

    /// The branch they are on, which is where an abort would return to.
    let source: String

    /// Every local branch but `source`, snapshotted with the commits.
    let candidates: [String]

    let store: HistoryActionStore
    let repoPath: String

    /// Called once git has been asked, whatever it answered — the owner
    /// re-reads the repository, and reports a conflict.
    let onFinished: (HistoryActionOutcome) async -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var filter = ""
    /// The keyboard cursor, as a branch name rather than an index so it
    /// survives the rows being filtered under it.
    @State private var cursor: String?
    @State private var errorMessage: String?
    @FocusState private var filterFocused: Bool

    /// A plain case-insensitive substring, like the Tauri branch popover: a
    /// branch name is short enough that a contiguous match is the predictable
    /// answer.
    private var rows: [String] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return candidates }
        return candidates.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    private var title: String {
        commits.count == 1 ? "Cherry-pick Commit" : "Cherry-pick \(commits.count) Commits"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.title3.weight(.semibold))

            Text("Copy from “\(source)” onto which branch? You will end up on it.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            TextField("Filter branches", text: $filter)
                .textFieldStyle(.roundedBorder)
                .focused($filterFocused)
                .onSubmit(pick)
                .onKeyPress(keys: [.upArrow, .downArrow]) { press in
                    moveCursor(by: press.key == .downArrow ? 1 : -1)
                    return .handled
                }
                .disabled(store.isRunning)

            branchList

            if let errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if store.isBlocked, !store.isRunning { WriteBlockedNote() }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(store.isRunning)
                Button(store.isRunning ? "Cherry-picking…" : "Cherry-pick", action: pick)
                    .buttonStyle(.borderedProminent)
                    .disabled(store.isBlocked || cursor == nil)
            }
        }
        .padding(16)
        .frame(width: 420)
        .interactiveDismissDisabled(store.isRunning)
        .onAppear {
            filterFocused = true
            cursor = rows.first
        }
        // Return always acts on the row a query just put first — the rule every
        // filtered list in the app follows.
        .onChange(of: filter) {
            cursor = rows.first
        }
    }

    private var branchList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 0) {
                    if rows.isEmpty {
                        Text(
                            candidates.isEmpty
                                ? "There is no other local branch to cherry-pick onto."
                                : "No branch matches “\(filter)”."
                        )
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                    }
                    ForEach(rows, id: \.self) { branch in
                        BranchPickRow(name: branch, isCursor: branch == cursor) {
                            cursor = branch
                        }
                        .id(branch)
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(height: 180)
            .background(.background.secondary, in: .rect(cornerRadius: 6))
            .disabled(store.isRunning)
            // `anchor: nil` scrolls the least amount that reveals the row, so a
            // click on a visible row never makes the list jump.
            .onChange(of: cursor) { _, branch in
                guard let branch else { return }
                proxy.scrollTo(branch, anchor: nil)
            }
        }
    }

    private func moveCursor(by delta: Int) {
        guard !rows.isEmpty else { return }
        let current = cursor.flatMap { rows.firstIndex(of: $0) } ?? -1
        cursor = rows[min(max(current + delta, 0), rows.count - 1)]
    }

    private func pick() {
        // Return in the filter field arrives here too, past the button's
        // `.disabled`, so the same gate is asked again.
        guard !store.isBlocked, let target = cursor else { return }
        errorMessage = nil
        Task {
            let outcome = await store.cherryPick(
                commits.map(\.sha),
                onto: target,
                repoPath: repoPath
            )
            switch outcome {
            case .landed, .stoppedOnConflict:
                dismiss()
                await onFinished(outcome)
            case let .failed(message):
                errorMessage = message
                await onFinished(outcome)
            // Nothing was attempted, so the sheet stays as it is, still
            // offering the pick — and says why, since the slot was taken
            // between the guard above and the claim.
            case .refusedBusy:
                errorMessage = RepositoryWriteGate.busyMessage
            }
        }
    }
}

/// One branch in the cherry-pick target list.
private struct BranchPickRow: View {
    let name: String
    let isCursor: Bool
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Label(name, systemImage: "arrow.triangle.branch")
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
        }
        .buttonStyle(.plain)
        .background(rowFill)
        .onHover { isHovered = $0 }
    }

    /// The cursor reads as a selection and hover as the lighter wash — the
    /// repository picker's two fills.
    private var rowFill: AnyShapeStyle {
        if isCursor { return AnyShapeStyle(.selection.opacity(0.35)) }
        if isHovered { return AnyShapeStyle(.selection.opacity(0.15)) }
        return AnyShapeStyle(.clear)
    }
}
