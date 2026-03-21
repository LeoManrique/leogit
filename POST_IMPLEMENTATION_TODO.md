1. When providing a relative path in cli such as "." it should be converted to absolute path.
I think it already works with "~" but I haven't tested it yet.
2. Diff viewer should be updated when the file is selected / highlighted, no need to press enter.
Let's check how not to use too much memory for this.
3. Word wrapper in diff viewer should not affect line number side column.
4. Mouse support for selecting files.
5. Line selector is too dim
6. AI commit message generation should respect line-level selection.
Currently, it sends full file diffs for selected files. 
It should use `GeneratePatch()` with the stored `DiffSelection` per file
so the AI only sees the lines the user actually selected.
Requires storing per-file selections (e.g. `map[string]diff.DiffSelection` in FileListModel)
and propagating selection changes from diffview → app → filelist.
7. When focusing the commit message section maybe we can increase its size a little bit
so the message and description are visible.
we can also do manual resizing with ctrl+shift+arrow, maybe.
8. When we generate the messages, we only the last part of it if its long, we should see the beginning.
Maybe a subtle scroll bar would be nice or "...".
9. There should also be a Create GitHub Project as action.
10. When the terminal is not expanded, there should at least
be a thing section to suggest the user to expand it.
11. Fix when refocusing the terminal, the borders go blue but I cant type anything.
12. Fix terminal resizing doesnt work
13. Cant press space on terminal nor control X or some shortcuts in general
14. 