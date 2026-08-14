import SwiftUI

/// Shown until a repository is open.
struct WelcomeView: View {
    let coreVersion: String
    let onOpen: () -> Void
    let onClone: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "point.3.filled.connected.trianglepath.dotted")
                .font(.system(size: 48))
                .foregroundStyle(.tint)

            Text("LeoGit")
                .font(.largeTitle.weight(.semibold))

            Text("Open a Git repository to see its changes and history.")
                .font(.callout)
                .foregroundStyle(.secondary)

            HStack(spacing: 12) {
                Button("Open Repository…", action: onOpen)
                    .keyboardShortcut("o")
                Button("Clone Repository…", action: onClone)
            }
            .controlSize(.large)

            if !coreVersion.isEmpty {
                // Proves the Rust bridge answered before any repo was opened.
                Text("core \(coreVersion)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
