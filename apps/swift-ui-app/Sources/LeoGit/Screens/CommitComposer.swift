import SwiftUI

/// The commit box under the changes list: summary, optional description, the
/// AI generate controls, and the Commit button. Pure presentation — all state
/// lives in `CommitStore`, and the actual commit and generation are the
/// owner's closures so it can gate them (e.g. behind the embedded-repo
/// confirmation) and pass the checked files.
///
/// Sized by the owner: the summary and control rows take their natural
/// height and the description editor absorbs the rest, so the box grows and
/// shrinks with the sidebar's resize handle the way the Tauri commit section
/// does — a taller box is more description, never more chrome.
struct CommitComposer: View {
    @Bindable var store: CommitStore

    /// How many files the next commit would contain, for the button label.
    let includedCount: Int

    /// Default message when exactly one file is checked ("Update foo.txt"),
    /// empty otherwise. Shown as the summary placeholder, and committed
    /// verbatim when the user types nothing.
    let autoSummary: String

    let onSubmit: () -> Void

    /// Generate a commit message with AI from the checked files' diff.
    let onGenerate: () -> Void

    /// Run a shell command in the app's own terminal. The composer knows the
    /// command that would fix an unready AI provider but nothing about where
    /// to run it — the terminal dock lives on the far side of the split.
    let onRunFixCommand: (String) -> Void

    /// What Commit would use: the typed summary, or the single-file
    /// auto-summary backing the placeholder.
    private var effectiveSummary: String {
        let typed = store.summary.trimmingCharacters(in: .whitespacesAndNewlines)
        return typed.isEmpty ? autoSummary : typed
    }

    /// Amending relaxes the file requirement: `git commit --amend` with
    /// nothing staged is the message-only edit, which is the whole point of
    /// amending a commit on a clean working tree.
    private var canCommit: Bool {
        !effectiveSummary.isEmpty
            && (includedCount > 0 || store.isAmending)
            && !isBusy
    }

    /// Committing and generating share one busy treatment: the fields lock
    /// (typing mid-generate would be overwritten by the result), and each
    /// button excludes the other — the store's guards enforce the same.
    private var isBusy: Bool {
        store.isCommitting || store.isGenerating
    }

    /// Generate is also held back while the provider is known to be unable to
    /// answer — a signed-out Claude CLI passes every "is it installed" check
    /// and fails every request. Not knowing is not blocking: an unanswered
    /// probe leaves the button live and lets the request report itself.
    private var canGenerate: Bool {
        !isBusy && includedCount > 0 && store.blockingProvider == nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if store.isAmending {
                amendNotice
            }

            WheelScrollableTextField(
                prompt: autoSummary.isEmpty ? "Summary (required)" : autoSummary,
                text: $store.summary
            )
            .disabled(isBusy)

            descriptionEditor

            if store.errorMessage != nil || store.blockingProvider != nil {
                statusStrip
            }

            HStack(alignment: .center, spacing: 8) {
                Picker("AI provider", selection: providerBinding) {
                    Text("Claude").tag("claude")
                    Text("Ollama").tag("ollama")
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .fixedSize()
                .disabled(isBusy)
                .help("AI provider used by Generate")

                Button(store.isGenerating ? "Generating…" : "Generate") {
                    onGenerate()
                }
                .keyboardShortcut("g", modifiers: .command)
                .disabled(!canGenerate)
                .help(store.blockingProvider?.reason
                    ?? "Generate a commit message from the checked files (⌘G)")

                Spacer(minLength: 0)

                if isBusy {
                    ProgressView()
                        .controlSize(.small)
                }

                Button(action: onSubmit) {
                    Text(commitLabel)
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(!canCommit)
                .help(
                    store.isAmending
                        ? "Rewrite the most recent commit (⌘↩)"
                        : "Commit the checked files (⌘↩)"
                )
            }
        }
        .padding(10)
    }

    /// Amend mode is a state the composer can sit in indefinitely, so it says
    /// so above the fields — with the way out right there, since the only
    /// other exit is committing. Deliberately doesn't name the commit: the
    /// message it seeded is already on screen, in the fields below.
    private var amendNotice: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text("Your changes will modify your \(Text("most recent commit").bold()).")

            Spacer(minLength: 0)

            Button("Stop Amending") { store.stopAmending() }
                .buttonStyle(.link)
                .disabled(isBusy)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .padding(.leading, 8)
        .padding(.vertical, 4)
        // A leading rule instead of a filled banner: the composer is a dense
        // stack of fields, and a tinted block here would read as another one.
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(.yellow)
                .frame(width: 2)
        }
    }

    /// One strip for everything that went wrong, so a failure and the standing
    /// state behind it read as a single message instead of as two unrelated
    /// lines. A leading rule rather than a filled banner, like the amend notice
    /// above: the composer is a dense stack of fields, and a tinted block here
    /// would read as another one.
    ///
    /// Both rows are independent — a commit failure has to stay visible while
    /// the AI provider is separately blocked. Only the *generate* failure that
    /// produced a remedy is folded away, and the store does that when it reads
    /// the remedy out, not here.
    private var statusStrip: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let errorMessage = store.errorMessage {
                Text(errorMessage)
                    .foregroundStyle(.red)
                    .lineLimit(3)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if let block = store.blockingProvider {
                // Why Generate is greyed out, stated rather than left to a
                // hover — with the action immediately after the sentence that
                // explains it. The button spells out the command it will run,
                // because the app is about to type into the user's shell.
                // One word space between the reason and the offer, not a
                // gutter: they are one sentence, and a gap wide enough to read
                // as a column break undoes that.
                HStack(alignment: .firstTextBaseline, spacing: 2) {
                    Text(block.reason)
                        .foregroundStyle(.secondary)

                    if !block.fixCommand.isEmpty {
                        // "Run" is prose at the strip's own size and colour —
                        // it is grammar, not a control. Only the command is
                        // clickable, so only the command is dressed as
                        // something to click.
                        HStack(alignment: .firstTextBaseline, spacing: 5) {
                            Text("Run").foregroundStyle(.secondary)
                            fixCommandChip(block.fixCommand)
                        }
                    }

                    Spacer(minLength: 0)
                }
                // The provider's own wording, when the remedy was read out of
                // a failed request — it replaces that line rather than
                // stacking under it, so this is where it stays reachable.
                .help(block.detail.isEmpty ? block.reason : block.detail)
            }
        }
        .font(.caption)
        .padding(.leading, 8)
        .padding(.vertical, 4)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(.red)
                .frame(width: 2)
        }
    }

    /// The command as its own tinted chip, in mono, in the accent — a thing
    /// you can tell apart from the sentence carrying it without leaving that
    /// sentence.
    ///
    /// Deliberately not a bordered `Button`: its chrome made the whole phrase
    /// read as one oversized control and buried which part was actually
    /// clickable. `.plain` hands the appearance back here; the link pointer is
    /// what still says it can be clicked.
    private func fixCommandChip(_ command: String) -> some View {
        Button {
            onRunFixCommand(command)
        } label: {
            Text(command)
                .font(Self.commandFont)
                .foregroundStyle(Color.accentColor)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(.quaternary, in: .rect(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .pointerStyle(.link)
        .disabled(isBusy)
        .help("Run this in the terminal below")
    }

    /// The strip's caption size, stepped down for monospace: mono renders
    /// visually larger than the UI face at the same point size, so matching
    /// values would not match on screen. macOS `.caption` is 10 pt.
    private static let commandFont = Font.system(size: 9.5, design: .monospaced)

    /// The native counterpart of the Tauri textarea: fills whatever height
    /// the owner leaves after the fixed rows, scrolling with a scrollbar once
    /// the text outgrows it. `TextEditor`
    /// rather than a vertical-axis `TextField` because only the editor is a
    /// real scroll view; it brings no bezel or placeholder of its own, so
    /// both are drawn here to match the summary field above.
    private var descriptionEditor: some View {
        ZStack(alignment: .topLeading) {
            TextEditor(text: $store.details)
                .font(.body)
                .scrollContentBackground(.hidden)
                .contentMargins(4, for: .scrollContent)
                .frame(maxHeight: .infinity)
                .disabled(isBusy)

            if store.details.isEmpty {
                Text("Description")
                    .foregroundStyle(Color(nsColor: .placeholderTextColor))
                    .padding(.top, 4)
                    // The editor's line fragment padding plus its content
                    // margin — keeps the prompt on the first character's spot.
                    .padding(.leading, 9)
                    .allowsHitTesting(false)
            }
        }
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.separator))
    }

    /// The picker writes through the store so a change persists to the
    /// shared config file (and reverts if that save fails).
    private var providerBinding: Binding<String> {
        Binding {
            store.aiProvider
        } set: { provider in
            Task { await store.setAIProvider(provider) }
        }
    }

    private var commitLabel: String {
        if store.isAmending {
            return store.isCommitting ? "Amending…" : "Amend Commit"
        }
        return switch includedCount {
        case 0: "Commit"
        case 1: "Commit 1 File"
        default: "Commit \(includedCount) Files"
        }
    }
}
