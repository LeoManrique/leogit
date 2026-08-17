/**
 * Ranked fuzzy search over the repository list, shared by the startup picker
 * and the header dropdown — and identical, tier for tier, to the native
 * client's `RepoSearch.swift`, so both frontends answer a query the same way.
 *
 * The rule this replaces accepted the query as a subsequence of the *full
 * path*, which collapses once every row shares an ancestry: under
 * `/Users/leo/Dev/LeoManrique/Desktop`, `llm` matched every repository — the
 * `l` of `leo`, the `l` of `LeoManrique`, and that word's `m` — so the filter
 * appeared not to filter at all.
 *
 * The two halves of a path carry different amounts of signal, so they are
 * searched by different rules: a name keeps the scattered-subsequence match a
 * fuzzy finder is expected to have, while the path must contain the query
 * *contiguously* and is first trimmed to what lies below the scan folder that
 * found it — the part the user chose, with the ancestry every row carries
 * alike removed.
 */

/** How a query matched, strongest first — callers sort ascending. */
export const RepoMatch = {
  ExactName: 0,
  NamePrefix: 1,
  NameSubstring: 2,
  NameInitials: 3,
  NameSubsequence: 4,
  PathSubstring: 5,
} as const

export type RepoMatch = (typeof RepoMatch)[keyof typeof RepoMatch]

/**
 * The strongest way `query` matches a repository, or `null` when it doesn't.
 *
 * @param names Labels to match loosely, best first — a repo's GitHub name and
 *   its `owner/name` where known, otherwise just the folder's basename.
 * @param scanFolders Discovery's `~`-expanded folders, whose prefixes are
 *   stripped from the path before it is searched.
 */
export function matchRepo(
  query: string,
  path: string,
  names: string[],
  scanFolders: string[]
): RepoMatch | null {
  const needle = query.trim().toLowerCase()
  if (!needle) return null

  let best: RepoMatch | null = null
  for (const name of names) {
    const tier = matchName(needle, name.toLowerCase())
    if (tier !== null && (best === null || tier < best)) best = tier
  }
  if (best !== null) return best

  return searchablePath(path, scanFolders).includes(needle) ? RepoMatch.PathSubstring : null
}

function matchName(needle: string, name: string): RepoMatch | null {
  if (name === needle) return RepoMatch.ExactName
  if (name.startsWith(needle)) return RepoMatch.NamePrefix
  if (name.includes(needle)) return RepoMatch.NameSubstring
  if (initials(name).startsWith(needle)) return RepoMatch.NameInitials
  if (isSubsequence(needle, name)) return RepoMatch.NameSubsequence
  return null
}

/**
 * First letter of each word: repository names are hyphenated or underscored,
 * so `gpm` finds `git-projects-manager` without relying on the far weaker
 * whole-name subsequence.
 */
function initials(name: string): string {
  return name
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .map((word) => word[0])
    .join('')
}

/**
 * The part of `path` worth searching: whatever lies below the deepest scan
 * folder containing it. Everything above that is common to every row and
 * carries no signal — including it is what made the old rule match the whole
 * list. Both separators fold to `/` for the comparison, since discovery
 * returns `C:\Users\…` on Windows.
 */
function searchablePath(path: string, scanFolders: string[]): string {
  const target = normalize(path)
  let cut = 0
  for (const folder of scanFolders) {
    const prefix = normalize(folder).replace(/\/?$/, '/')
    if (prefix.length > cut && target.startsWith(prefix)) cut = prefix.length
  }
  return target.slice(cut)
}

function normalize(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase()
}

/**
 * Every character of `needle` appears in `haystack`, in order, not
 * necessarily adjacent. Both are expected already lowercased.
 */
function isSubsequence(needle: string, haystack: string): boolean {
  let i = 0
  for (let j = 0; j < haystack.length && i < needle.length; j++) {
    if (haystack[j] === needle[i]) i++
  }
  return i === needle.length
}
