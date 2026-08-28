import Foundation

/// Which way a picker's rows are ordered — the vocabulary behind both
/// clock ⇄ A-Z toggles in the app.
///
/// One type for both because both are persisted in the shared
/// `repos-state.json` under the same two strings (`repo_sort_mode` for the
/// repository picker, `clone_sort_mode` for the Clone sheet's GitHub list),
/// and both must survive a value the file doesn't name. What "recent" *means*
/// differs per list and is a matter for each toggle's tooltip, not for the
/// stored value.
enum SortMode: String {
    case recent
    case name

    /// The persisted string, or `nil` for anything the vocabulary doesn't
    /// cover — including a file written by a future version.
    init?(persisted: String?) {
        guard let persisted, let mode = SortMode(rawValue: persisted) else { return nil }
        self = mode
    }
}
