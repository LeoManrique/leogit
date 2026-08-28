import SwiftUI

/// "Update v0.1.32" — the one place the app mentions a newer release.
///
/// Deliberately quiet: a control that appears only when there is something to
/// say, never a banner, modal, or toast. An update is never urgent enough to
/// take space from the repository, and the app does not install one itself —
/// it hands over the one-liner that does, or the release page.
///
/// Stock chrome rather than the Tauri client's tinted pill, per STYLE.md's
/// standing rule that the native app takes what a standard control gives it:
/// this sits in the toolbar beside two other stock chips, and a hand-tinted
/// third would be the only non-native control in the window.
struct UpdateChip: View {
    let info: UpdateInfo
    let onDismiss: () -> Void

    /// The copy confirmation swaps the *glyph* and leaves the label alone —
    /// swapping the text would resize the control and shove its neighbours
    /// sideways for the duration.
    @State private var hasCopied = false
    @State private var copyResetTask: Task<Void, Never>?
    @State private var failureText: String?

    private static let copiedDuration: Duration = .seconds(2.5)

    var body: some View {
        Menu {
            if let command = info.installCommand {
                Button("Copy update command") { copy(command) }
                // With a command the release page is still worth a link —
                // notes, assets. Without one it *is* the download, below.
                Button("View release on GitHub") { openReleasePage() }
            } else {
                Button("Download from GitHub") { openReleasePage() }
            }

            Divider()

            Button("Dismiss for this session", action: onDismiss)
        } label: {
            Label(
                "Update v\(info.version)",
                systemImage: hasCopied ? "checkmark.circle" : "arrow.up.circle"
            )
        }
        // macOS toolbars render Labels icon-only by default, and the version
        // is what this control is for.
        .labelStyle(.titleAndIcon)
        .menuIndicator(.hidden)
        .help(helpText)
        .alert(
            "Could not open the release page",
            isPresented: Binding(
                get: { failureText != nil },
                set: { if !$0 { failureText = nil } }
            )
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failureText ?? "")
        }
        .onDisappear { copyResetTask?.cancel() }
    }

    private var helpText: String {
        if hasCopied { return "Command copied" }
        return info.installCommand == nil
            ? "leogit v\(info.version) is available — download the installer"
            : "leogit v\(info.version) is available — copy the update command"
    }

    private func copy(_ command: String) {
        Clipboard.copy(command)
        hasCopied = true
        copyResetTask?.cancel()
        copyResetTask = Task { @MainActor in
            try? await Task.sleep(for: Self.copiedDuration)
            guard !Task.isCancelled else { return }
            hasCopied = false
        }
    }

    /// Handing off to the browser can fail, and the user is waiting on it
    /// having just chosen the item — which FRONTEND §6.13 puts in a modal
    /// rather than a background strip. It also has to work on the Welcome
    /// screen, which has no strip to put it in.
    private func openReleasePage() {
        Task {
            do {
                try await GitBridge.openInBrowser(info.url)
            } catch {
                failureText = error.displayMessage
            }
        }
    }
}
