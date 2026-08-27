import Foundation

/// The single native owner of the shared `config.toml` contents — the Tauri
/// client's `$config` store. Everything outside the Settings window that
/// needs a setting observes this store instead of re-reading the file, so "a
/// setting changed" is one observation with one staleness window, not N
/// ad-hoc `GitBridge.appConfig()` calls with N of them.
///
/// Reload sites, and why they suffice:
/// - launch (`LeoGitApp`) — the first read every consumer starts from;
/// - every Settings save (`SettingsStore.save`) — an edit made *in this
///   process* is visible the moment it lands on disk, which is what re-keys
///   an open diff and re-arms the auto-fetch interval live;
/// - the activation resync (`ContentView.resyncOnActivate`) — edits made
///   from the *Tauri* client are picked up on return to the app, the same
///   moment everything else catches up.
///
/// The derived accessors carry the Tauri client's fallback defaults
/// (`$config?.x ?? default` in `MainLayout.svelte`), so the moments before
/// the launch read lands render identically in both clients.
@MainActor
@Observable
final class AppConfigStore {
    private(set) var config: Config?

    /// Diff: suppress whitespace-only changes (`git diff -w`).
    var hideWhitespace: Bool { config?.hideWhitespace ?? false }

    /// Diff: run the syntax-colour phase.
    var syntaxHighlighting: Bool { config?.syntaxHighlighting ?? true }

    /// Diff: tab stop width, in columns.
    var tabSize: Int { config.map { Int($0.tabSize) } ?? 4 }

    /// Re-read the file. Failures keep the last good config — a transient
    /// read error must not snap live views back to the defaults.
    func reload() async {
        if let fresh = try? await GitBridge.appConfig() {
            config = fresh
        }
    }
}
