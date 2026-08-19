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

            if let errorMessage = store.errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .lineLimit(3)
                    .textSelection(.enabled)
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
                .disabled(isBusy || includedCount == 0)
                .help("Generate a commit message from the checked files (⌘G)")

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
