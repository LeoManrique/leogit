import Foundation
import Observation

/// The folder a `leogit <dir>` invocation asked the app to open, held until
/// the root view acts on it.
///
/// Two sources feed one slot, which is the whole reason this type exists:
///
///   * **`application(_:open:)`** — LaunchServices handing over a folder:
///     `open -a LeoGit <dir>`, a drop on the Dock icon, Finder's *Open With*.
///     It activates the running instance rather than starting a second one, so
///     one call works cold and warm and macOS supplies the single-instance
///     behaviour the Tauri client needs a socket plugin for.
///   * **`CommandLine.arguments`** — the executable run directly with a path,
///     which is what a launcher that bypasses LaunchServices produces. The
///     installed `leogit` shell command is still one of those, and still points
///     at the Tauri bundle: giving it a native branch is an open packaging
///     question (see the parity plan's WS-M entry), not an app-side one.
///
/// The Tauri host keeps its cold-start target in a process global in core.
/// That deliberately has no native export: a global can hold the argv target,
/// but it cannot publish the *later* ones — `application(_:open:)` fires
/// whenever the user opens another folder — and a slot the UI has to poll is
/// not a slot the UI can observe. One observable store owns both routes
/// instead, so a warm hand-off and a cold one arrive the same way.
///
/// Resolution is a git call (a subdirectory has to walk up to its repository
/// root), so it is asynchronous and callers that must not race it —
/// launch, which decides between this and the remembered repository — wait
/// on `settle()` first.
@MainActor
@Observable
final class LaunchStore {
    /// The folder to act on, or `nil` once claimed. A target whose `isRepo`
    /// is false is still a target: the app offers to create a repository
    /// there rather than letting the invocation do nothing.
    private(set) var pending: LaunchTarget?

    /// The most recent target, kept after it has been claimed.
    ///
    /// The launch path reads this rather than `pending`, because the root
    /// view's own handler may have claimed the target already: the two run in
    /// the same turn and their order is not fixed. A launch that then fell
    /// through to the remembered repository would open it on top of the one
    /// the user actually asked for.
    private(set) var latest: LaunchTarget?

    /// The in-flight resolution, so `settle()` can await it and so a second
    /// hand-off queues behind the first rather than racing it.
    @ObservationIgnored private var resolution: Task<Void, Never>?

    /// argv is read once per process — every later target arrives through
    /// `application(_:open:)`.
    @ObservationIgnored private var hasReadArguments = false

    /// A folder handed over by LaunchServices: `open -a`, a drop on the Dock
    /// icon, or Finder's Open With. Only the first folder is taken — the app
    /// shows one repository at a time, and opening several would leave the
    /// user in the last one with no sign the others were discarded.
    func open(urls: [URL]) {
        guard let url = urls.first(where: \.isFileURL) else { return }
        resolve(arguments: ["leogit", url.path], workingDirectory: url.path)
    }

    /// This process's own arguments, for a launch that did not come through
    /// LaunchServices. Safe to call more than once; only the first reads.
    func readProcessArguments() {
        guard !hasReadArguments else { return }
        hasReadArguments = true
        resolve(
            arguments: CommandLine.arguments,
            workingDirectory: FileManager.default.currentDirectoryPath
        )
    }

    /// Wait for any resolution still running, so a caller reading `pending`
    /// sees the answer rather than the moment before it.
    func settle() async {
        await resolution?.value
    }

    /// Take the target, leaving the slot empty — the same one-shot semantics
    /// as core's `take_pending_launch_target`, and for the same reason: a view
    /// that re-runs its task must not re-open the folder.
    func claim() -> LaunchTarget? {
        defer { pending = nil }
        return pending
    }

    /// Resolution runs off the main actor and publishes back onto it. A
    /// hand-off that resolves to nothing leaves any earlier target standing:
    /// `open -a LeoGit <dir>` passes the folder as a document and *not* in
    /// argv, so the argv pass that follows must not clear what the open
    /// event just delivered.
    private func resolve(arguments: [String], workingDirectory: String) {
        let previous = resolution
        resolution = Task { @MainActor in
            await previous?.value
            guard
                let target = await GitBridge.launchTarget(
                    arguments: arguments,
                    workingDirectory: workingDirectory
                )
            else { return }
            print("[launch] target: \(target.path) (repo: \(target.isRepo))")
            latest = target
            pending = target
        }
    }
}
