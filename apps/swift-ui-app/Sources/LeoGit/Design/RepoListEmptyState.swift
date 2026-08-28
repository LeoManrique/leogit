import SwiftUI

/// What a repo picker shows instead of rows — the native counterpart of the
/// Tauri client's `RepoListEmptyState.svelte`, string for string.
///
/// It answers *which* emptiness this is, because the three have different
/// fixes: still looking, nothing found anywhere, or nothing matched the query.
/// One line for all three is the failure this exists to prevent — the native
/// switcher used to distinguish them while the Tauri dropdown said "No
/// repositories" for every cause, and the shared component is what stops them
/// drifting apart again.
///
/// The searched folders belong only to the middle state: a user who typed a
/// query already knows the list isn't empty. The action belongs to both
/// non-looking states, because "none matched" is what you see when the
/// repository you want lives somewhere discovery was never pointed at — the
/// same problem, reached by typing a name.
struct RepoListEmptyState: View {
    let isDiscovering: Bool

    /// Whether the list has any rows at all — what separates "nothing matched
    /// your filter" from "nothing found anywhere".
    let hasRepos: Bool

    /// Tilde-expanded folders the walk covered. Paths are data, so they are
    /// rendered monospaced: seeing them is what turns "found nothing" into
    /// something the user can act on.
    let scannedPaths: [String]

    let onChooseFolders: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            if isDiscovering {
                ProgressView()
                    .controlSize(.small)
                Text("Looking for repositories…")
            } else {
                if hasRepos {
                    Text("No matching repositories")
                } else {
                    Text("No repositories found")
                    if !scannedPaths.isEmpty {
                        Text("Searched these folders:")
                            .font(.caption)
                        VStack(alignment: .leading, spacing: 2) {
                            ForEach(scannedPaths, id: \.self) { path in
                                Text(path)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                        }
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                    }
                }
                Button("Choose folders to search", action: onChooseFolders)
                    .buttonStyle(.link)
                    .font(.callout)
            }
        }
        .font(.callout)
        .foregroundStyle(.tertiary)
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
        .padding(.horizontal, 16)
    }
}
