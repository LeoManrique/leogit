import SwiftUI

/// The Pull Requests tab — a native re-derivation of the PR overview (the
/// Tauri client never had one; the retired TUI's overlay is the prior art):
/// filter segments, a list pane with state dots, and a detail pane whose CI
/// checks load lazily per PR. Everything flows through `gh`, so when it is
/// missing or unauthenticated the error state explains in gh's own words.
struct PullRequestsView: View {
    let repoPath: String
    @Bindable var store: PullRequestStore
    /// Seed for the create sheet's title — the head commit's summary, the
    /// closest native equivalent of `gh pr create --fill`.
    let headCommitSummary: String
    let currentBranch: String
    /// Called after a checkout moved HEAD: status and branches reload.
    let onWorkingTreeChanged: () async -> Void

    @State private var isCreateSheetPresented = false
    /// Failure text from a checkout, shown as an alert — the toolbar
    /// controls' error surface.
    @State private var alertMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            headerBar
            Divider()
            content
        }
        // Reloads on first visit and whenever the filter changes; leaving
        // the tab unmounts the view, so returning is also a fresh load.
        .task(id: "\(repoPath)#\(store.filter.rawValue)") {
            await store.load(repoPath: repoPath)
        }
        .onChange(of: store.selectedNumber) { _, number in
            guard let number else { return }
            Task { await store.loadChecks(repoPath: repoPath, number: number) }
        }
        .sheet(isPresented: $isCreateSheetPresented) {
            CreatePullRequestSheet(
                repoPath: repoPath,
                currentBranch: currentBranch,
                seedTitle: headCommitSummary,
                onCreated: {
                    store.filter = .open
                    await store.load(repoPath: repoPath)
                }
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

    private var headerBar: some View {
        HStack(spacing: 8) {
            Picker("Filter", selection: $store.filter) {
                ForEach(PullRequestFilter.allCases) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(maxWidth: 320)

            Spacer(minLength: 8)

            Button {
                isCreateSheetPresented = true
            } label: {
                Label("New Pull Request", systemImage: "plus")
            }
            .help("Open a pull request for “\(currentBranch)”")

            Button {
                Task { await store.load(repoPath: repoPath) }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .disabled(store.phase == .loading)
            .help("Reload pull requests and CI status")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var content: some View {
        switch store.phase {
        case .loading:
            ProgressView("Loading pull requests…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let message):
            ContentUnavailableView {
                Label("Pull Requests Unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(message)
            } actions: {
                Button("Retry") {
                    Task { await store.load(repoPath: repoPath) }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded where store.pullRequests.isEmpty:
            ContentUnavailableView {
                Label("No Pull Requests", systemImage: "arrow.triangle.branch")
            } description: {
                Text("No \(store.filter == .all ? "" : "\(store.filter.rawValue) ")pull requests in this repository.")
            } actions: {
                Button("New Pull Request…") { isCreateSheetPresented = true }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded:
            HSplitView {
                listPane
                    .frame(minWidth: 260, idealWidth: 320, maxWidth: 480)
                detailPane
                    .frame(minWidth: 320, maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    // MARK: List pane

    private var listPane: some View {
        List(store.pullRequests, selection: $store.selectedNumber) { pr in
            PullRequestRow(pullRequest: pr)
        }
        .listStyle(.inset)
    }

    // MARK: Detail pane

    @ViewBuilder
    private var detailPane: some View {
        if let pr = store.selected {
            VStack(spacing: 0) {
                ScrollView {
                    PullRequestDetail(
                        pullRequest: pr,
                        checks: store.checks[pr.number]
                    )
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }

                Divider()

                HStack(spacing: 8) {
                    if let url = URL(string: pr.url) {
                        Link("Open on GitHub", destination: url)
                    }
                    Spacer(minLength: 8)
                    Button(store.isCheckingOut ? "Checking Out…" : "Checkout") {
                        checkout(number: pr.number)
                    }
                    .disabled(store.isCheckingOut)
                    .help("Fetch this PR's branch and switch to it")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
            }
        } else {
            ContentUnavailableView(
                "No Selection",
                systemImage: "sidebar.left",
                description: Text("Select a pull request to see its details.")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func checkout(number: UInt32) {
        Task {
            if let failure = await store.checkout(repoPath: repoPath, number: number) {
                alertMessage = failure
            } else {
                await onWorkingTreeChanged()
            }
        }
    }
}

/// One list row: state dot, number + title, author · branch, draft badge.
private struct PullRequestRow: View {
    let pullRequest: PullRequest

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle()
                .fill(pullRequest.stateColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                Text("#\(String(pullRequest.number))  \(pullRequest.title)")
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text("\(pullRequest.author) · \(pullRequest.headRefName)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Spacer(minLength: 4)

            if pullRequest.isDraft {
                Text("Draft")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(.quaternary, in: .capsule)
            }
        }
        .padding(.vertical, 2)
    }
}

/// The scrollable detail body: title, metadata, branches, churn, CI checks,
/// and the PR description.
private struct PullRequestDetail: View {
    let pullRequest: PullRequest
    let checks: PullRequestChecks?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(pullRequest.title)
                    .font(.title3.weight(.semibold))
                    .textSelection(.enabled)
                HStack(spacing: 6) {
                    Text("#\(String(pullRequest.number))")
                    Text("·")
                    Text(pullRequest.author)
                    Text("·")
                    Text(CommitDate.relative(pullRequest.createdAt))
                }
                .font(.callout)
                .foregroundStyle(.secondary)
            }

            HStack(spacing: 8) {
                stateBadge
                if pullRequest.isDraft { badge("Draft", color: .secondary) }
                if let decision = pullRequest.reviewDecision {
                    badge(reviewLabel(decision), color: reviewColor(decision))
                }
            }

            Text("\(pullRequest.headRefName) → \(pullRequest.baseRefName)")
                .font(.callout.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)

            HStack(spacing: 8) {
                Text("+\(String(pullRequest.additions))")
                    .foregroundStyle(.green)
                Text("−\(String(pullRequest.deletions))")
                    .foregroundStyle(.red)
                Text("· \(String(pullRequest.changedFiles)) file\(pullRequest.changedFiles == 1 ? "" : "s") changed")
                    .foregroundStyle(.secondary)
            }
            .font(.callout.monospaced())

            Divider()

            checksSection

            if !pullRequest.body.isEmpty {
                Divider()
                Text(pullRequest.body)
                    .font(.callout)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var stateBadge: some View {
        badge(pullRequest.state.capitalized, color: pullRequest.stateColor)
    }

    private func badge(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.caption.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(color.opacity(0.14), in: .capsule)
    }

    private func reviewLabel(_ decision: String) -> String {
        switch decision {
        case "APPROVED": "Approved"
        case "CHANGES_REQUESTED": "Changes Requested"
        case "REVIEW_REQUIRED": "Review Required"
        default: decision.capitalized
        }
    }

    private func reviewColor(_ decision: String) -> Color {
        switch decision {
        case "APPROVED": .green
        case "CHANGES_REQUESTED": .red
        default: .orange
        }
    }

    // MARK: Checks

    @ViewBuilder
    private var checksSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Checks")
                .font(.callout.weight(.medium))

            switch checks {
            case nil, .loading:
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading checks…")
                        .foregroundStyle(.secondary)
                }
                .font(.callout)
            case .unavailable(let message):
                // Usually gh's own "no checks reported…" — information,
                // not a failure.
                Text(message)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            case .loaded(let rows) where rows.isEmpty:
                Text("No checks")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            case .loaded(let rows):
                Text(summary(of: rows))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                ForEach(Array(rows.enumerated()), id: \.offset) { _, check in
                    checkRow(check)
                }
            }
        }
    }

    /// "3 passed · 1 failed · 2 pending" — everything outside pass/fail
    /// counts as pending, the retired TUI's bucketing.
    private func summary(of rows: [PrCheck]) -> String {
        let passed = rows.count { $0.bucket == "pass" }
        let failed = rows.count { $0.bucket == "fail" }
        let pending = rows.count - passed - failed
        var parts: [String] = []
        if passed > 0 { parts.append("\(passed) passed") }
        if failed > 0 { parts.append("\(failed) failed") }
        if pending > 0 { parts.append("\(pending) pending") }
        return parts.joined(separator: " · ")
    }

    private func checkRow(_ check: PrCheck) -> some View {
        HStack(spacing: 6) {
            Image(systemName: bucketIcon(check.bucket))
                .foregroundStyle(bucketColor(check.bucket))
            Text(check.name)
                .lineLimit(1)
                .truncationMode(.tail)
            if let workflow = check.workflow, !workflow.isEmpty {
                Text(workflow)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 4)
            if let link = check.link, let url = URL(string: link) {
                Link(destination: url) {
                    Image(systemName: "arrow.up.right.square")
                }
                .help("Open this check's page")
            }
        }
        .font(.callout)
    }

    private func bucketIcon(_ bucket: String) -> String {
        switch bucket {
        case "pass": "checkmark.circle.fill"
        case "fail": "xmark.circle.fill"
        case "skipping": "minus.circle"
        case "cancel": "slash.circle"
        default: "clock"
        }
    }

    private func bucketColor(_ bucket: String) -> Color {
        switch bucket {
        case "pass": .green
        case "fail": .red
        case "skipping", "cancel": .secondary
        default: .orange
        }
    }
}

extension PullRequest {
    /// Open = green, merged = purple, closed = red — GitHub's own colours;
    /// a draft is grey wherever it stands.
    fileprivate var stateColor: Color {
        if isDraft { return .secondary }
        switch state {
        case "OPEN": return .green
        case "MERGED": return .purple
        case "CLOSED": return .red
        default: return .secondary
        }
    }
}
