1. When providing a relative path in cli such as "." it should be converted to abosolue path. I think it already works with "~" but I haven't tested it yet.
2. Diff viewer should be updated when the file is selected / highlighted, no need to press enter. Lets check how not to use too much memory for this.
3. Word wrapper in diff viewer should not affect line number side column.
4. Mouse support for selecting files.
5. Line selector is too dimm
6. AI commit message generation should respect line-level selection. Currently it sends full file diffs for selected files. It should use `GeneratePatch()` with the stored `DiffSelection` per file so the AI only sees the lines the user actually selected. Requires storing per-file selections (e.g. `map[string]diff.DiffSelection` in FileListModel) and propagating selection changes from diffview → app → filelist.
7. When focusing the commit message section maybe we can increse its size a little bit so the message and description are visible.
8. When we generate the messages, we only the the last part of it if its long, we should see the beginning. Maybe a subtle scroll bar would be nice or "...".