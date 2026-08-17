import Foundation

/// Ranked fuzzy search over the repository list.
///
/// The Tauri picker's rule — the query is a subsequence of the name, the
/// `owner/name` label, *or the full path* — collapses under a real scan tree.
/// Every row shares the same ancestry, so a short query finds its letters
/// scattered through that shared prefix and matches everything: under
/// `/Users/leo/Dev/LeoManrique/Desktop`, `llm` matched all fourteen
/// repositories, reading as the `l` of `leo`, the `l` of `LeoManrique`, and
/// that word's `m`.
///
/// So the two halves of a path are searched by different rules. The name keeps
/// the scattered-subsequence match a fuzzy finder is expected to have, while
/// the path must contain the query *contiguously* and is first trimmed to what
/// lies below the scan folder holding it — the part the user chose, with the
/// ancestry every row carries alike removed.
enum RepoSearch {
    /// How the query matched, strongest first. Results sort on this before
    /// anything else: Return opens the first row, so the best match has to be
    /// there no matter which repository is open or most recently used.
    enum Match: Comparable {
        case exactName
        case namePrefix
        case nameSubstring
        case nameInitials
        case nameSubsequence
        case pathSubstring
    }

    /// The strongest way `query` matches the repository at `path`, or `nil`
    /// when it doesn't match at all. Case folding and trimming happen here, so
    /// callers pass the field's text and the store's folders unmodified.
    static func match(query: String, for path: String, scanFolders: [String]) -> Match? {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !needle.isEmpty else { return nil }

        let name = RepoDirectoryStore.displayName(of: path).lowercased()
        if name == needle { return .exactName }
        if name.hasPrefix(needle) { return .namePrefix }
        if name.contains(needle) { return .nameSubstring }
        if initials(of: name).hasPrefix(needle) { return .nameInitials }
        if isSubsequence(needle, of: name) { return .nameSubsequence }
        if searchablePath(for: path, scanFolders: scanFolders).contains(needle) {
            return .pathSubstring
        }
        return nil
    }

    /// First letter of each word: repository names are hyphenated or
    /// underscored, so `gpm` finds `git-projects-manager` without relying on
    /// the far weaker whole-name subsequence.
    private static func initials(of name: String) -> String {
        String(name.split { !$0.isLetter && !$0.isNumber }.compactMap(\.first))
    }

    /// The part of `path` worth searching: whatever lies below the deepest
    /// scan folder (or the home directory) containing it. Everything above
    /// that is common to every row and carries no signal — including it is
    /// what made the old rule match the whole list.
    private static func searchablePath(for path: String, scanFolders: [String]) -> String {
        let roots = (scanFolders + [NSHomeDirectory()])
            .map { $0.hasSuffix("/") ? $0 : $0 + "/" }
            .filter { path.hasPrefix($0) }
        guard let root = roots.max(by: { $0.count < $1.count }) else {
            return path.lowercased()
        }
        return String(path.dropFirst(root.count)).lowercased()
    }

    /// Every character of `needle` appears in `haystack`, in order, not
    /// necessarily adjacent. Both are expected already lowercased.
    private static func isSubsequence(_ needle: String, of haystack: String) -> Bool {
        var index = needle.startIndex
        for character in haystack {
            guard index < needle.endIndex else { break }
            if character == needle[index] {
                index = needle.index(after: index)
            }
        }
        return index == needle.endIndex
    }
}
