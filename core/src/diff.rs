use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum LineType {
    Context,
    Add,
    Delete,
    Hunk,
    NoNewline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    /// The raw patch line, prefix included — but only where something actually
    /// reads it: a `Hunk` header (`@@ -1,3 +1,4 @@`) and a `NoNewline` marker,
    /// whose whole meaning *is* their text. For every other row it duplicated
    /// `content` byte for byte, once per line of every diff, in both clients'
    /// memory and across both wires; the prefix that made it differ is exactly
    /// what a viewer draws from `line_type` instead.
    pub text: Option<String>,
    pub content: String,
    pub line_type: LineType,
    pub old_line_no: Option<i32>,
    pub new_line_no: Option<i32>,
    /// Character range within `content` that differs from its paired add/delete
    /// counterpart. Populated after parsing for matched delete/add pairs within
    /// a hunk so the viewer can highlight just the changed substring (e.g.
    /// `Relay` → `Metrics` inside an otherwise identical line). `None` for
    /// context/hunk-header lines, unpaired changes, and lines longer than
    /// `MAX_INTRA_LINE_LEN`.
    pub intra_line_diff: Option<IntraLineRange>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IntraLineRange {
    /// Zero-based character (code point) index into the line's `content`.
    pub start: u32,
    /// Number of characters (code points) that differ starting at `start`.
    pub length: u32,
}

/// Match GitHub Desktop's safeguard — above this length, the prefix/suffix
/// match degenerates into noise, so we skip the annotation entirely.
const MAX_INTRA_LINE_LEN: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HunkHeader {
    pub old_start: i32,
    pub old_count: i32,
    pub new_start: i32,
    pub new_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub header: HunkHeader,
    /// All lines in this hunk, INCLUDING the `@@` header as the first entry.
    /// Frontend and backend rely on `lines.len()` reflecting this so that the
    /// flat/global line index stays consistent across both sides.
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    /// Raw metadata lines that appear before the first `@@` hunk header.
    /// This includes `diff --git`, `index ...`, `--- a/...`, and `+++ b/...`.
    /// Required by `git apply` for new/deleted/renamed files.
    pub file_header: String,
    pub hunks: Vec<Hunk>,
    /// `true` when the underlying file is binary. Git emits a single
    /// `Binary files a/X and b/Y differ` line instead of `@@` hunks, so the
    /// frontend has to render a stand-in message rather than a line-by-line
    /// diff.
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSelection {
    pub default_selected: bool,
    pub diverging_lines: HashMap<usize, bool>,
}

impl DiffSelection {
    fn is_selected(&self, idx: usize) -> bool {
        match self.diverging_lines.get(&idx) {
            Some(v) => *v,
            None => self.default_selected,
        }
    }
}

/// One row of the side-by-side layout: an old-side line paired with a
/// new-side line, referenced by flat/global line index into the hunks'
/// concatenated `lines` arrays — the same indexing the per-line HTML and the
/// selection map use. `None` renders an empty filler cell; a context or
/// hunk-header row carries the same index on both sides.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SbsPair {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub is_hunk_header: bool,
}

/// Why a diff has no lines to show.
///
/// "Nothing rendered" used to be a bare `None`, which left every viewer to
/// invent a caption covering three unrelated situations at once. Naming them
/// lets each one say what actually happened — and the whitespace case is common
/// enough to matter, since hide-whitespace is a setting people leave on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmptyDiffReason {
    /// Git reported no difference at all — the file matches its committed
    /// state. Reached by selecting a row whose change landed elsewhere.
    NoChanges,
    /// Every difference is whitespace, and whitespace is being ignored. The
    /// diff *is* there; the current setting is hiding it.
    WhitespaceOnly,
    /// A file header with no hunks: a mode change, or a pure rename. Something
    /// changed about the file, just not its contents.
    NoTextualChanges,
}

/// Which size limit withheld a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffSizeReason {
    /// The whole patch is larger than a viewer renders comfortably.
    TotalBytes,
    /// At least one line is longer than a viewer wraps gracefully — the
    /// minified-bundle and base64-blob case, which is slow at a size the byte
    /// total alone would wave through.
    LineLength,
}

/// A diff too large to render eagerly, and the measurements that say so.
///
/// Neither client had any guard, so a pathological diff was a hang with no
/// explanation. The thresholds live here rather than in either client so the
/// two can't disagree about what "too large" means, and the viewer keeps a
/// "show it anyway" escape — this withholds a diff, it never refuses one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DiffSizeGuard {
    pub reason: DiffSizeReason,
    /// Size of the raw patch in bytes — what the message quotes.
    pub bytes: u64,
    /// Longest single line, in bytes.
    pub longest_line: u64,
}

/// Patch size past which the viewer asks before rendering. GitHub Desktop's
/// "reasonable size" bar, which a diff of this size stops being.
const MAX_REASONABLE_DIFF_BYTES: u64 = 4_194_304;

/// Single-line length past which the viewer asks before rendering.
const MAX_REASONABLE_LINE_BYTES: u64 = 5_000;

/// What the caller wants built alongside the parse.
///
/// The parse is shared; the render artifacts are not. A `WebView` host paints
/// from `html` on the first frame and pairs rows from `sbs_pairs` in the split
/// layout; the native host renders straight from the line model and would only
/// pay to build, marshal and drop both. Asking makes that explicit instead of
/// leaving one host to throw the work away at the bridge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DiffOptions {
    /// Build the phase-1 HTML array.
    pub html: bool,
    /// Build the side-by-side row pairing.
    pub side_by_side: bool,
    /// Parse and render past the size guard — the viewer's "Show diff anyway".
    pub show_anyway: bool,
}

impl Default for DiffOptions {
    /// The line-model-only shape: parse, no render artifacts, guard active.
    fn default() -> Self {
        Self {
            html: false,
            side_by_side: false,
            show_anyway: false,
        }
    }
}

/// Everything the viewer needs from one round trip. `file_diff` stays a lean,
/// standalone struct because the frontend round-trips it back into
/// `highlight_diff` / `generate_patch` — the derived render artifacts (per-line
/// HTML, side-by-side pairing, +/- totals) ride alongside instead of on it so
/// they're never echoed back over IPC.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedDiff {
    pub file_diff: FileDiff,
    /// Phase-1 HTML per flattened line: plain escaped text + intra-line
    /// backplate, ready for `{@html}` the same frame the diff mounts.
    /// `highlight_diff` later replaces the whole array with tokenized spans.
    /// Empty unless [`DiffOptions::html`] asked for it.
    pub html: Vec<String>,
    /// Precomputed rows for the side-by-side layout. Empty unless
    /// [`DiffOptions::side_by_side`] asked for it.
    pub sbs_pairs: Vec<SbsPair>,
    /// Added-line total for the header badge (0 for binary diffs).
    pub additions: u32,
    /// Deleted-line total for the header badge (0 for binary diffs).
    pub deletions: u32,
    /// Set when there are no lines to show, and why. `file_diff` is then an
    /// empty shell — the viewer renders the reason, not the diff.
    pub empty_reason: Option<EmptyDiffReason>,
    /// Set when the diff was withheld rather than parsed. Re-request with
    /// [`DiffOptions::show_anyway`] to get it.
    pub size_guard: Option<DiffSizeGuard>,
}

impl ParsedDiff {
    /// The "nothing to show" shape: a reason and an empty diff.
    fn empty(reason: EmptyDiffReason) -> Self {
        Self {
            file_diff: FileDiff {
                old_path: String::new(),
                new_path: String::new(),
                file_header: String::new(),
                hunks: Vec::new(),
                is_binary: false,
            },
            html: Vec::new(),
            sbs_pairs: Vec::new(),
            additions: 0,
            deletions: 0,
            empty_reason: Some(reason),
            size_guard: None,
        }
    }
}

/// Measure a raw patch against the size guard. `None` means it's fine to
/// render.
fn size_guard_for(raw: &str) -> Option<DiffSizeGuard> {
    let bytes = raw.len() as u64;
    let longest_line = raw.split('\n').map(|l| l.len() as u64).max().unwrap_or(0);
    let reason = if bytes > MAX_REASONABLE_DIFF_BYTES {
        DiffSizeReason::TotalBytes
    } else if longest_line > MAX_REASONABLE_LINE_BYTES {
        DiffSizeReason::LineLength
    } else {
        return None;
    };
    Some(DiffSizeGuard {
        reason,
        bytes,
        longest_line,
    })
}

/// Parse a raw patch, building only the render artifacts `options` asks for.
///
/// `empty_reason` distinguishes "git said nothing changed" from "the file
/// changed but not its text"; the caller adds the whitespace-only case, since
/// only it knows whether whitespace was being ignored.
#[must_use]
pub fn parse_diff_with(raw: &str, options: DiffOptions) -> ParsedDiff {
    if raw.trim().is_empty() {
        return ParsedDiff::empty(EmptyDiffReason::NoChanges);
    }
    if !options.show_anyway
        && let Some(guard) = size_guard_for(raw)
    {
        let mut withheld = ParsedDiff::empty(EmptyDiffReason::NoChanges);
        withheld.empty_reason = None;
        withheld.size_guard = Some(guard);
        return withheld;
    }
    let Some(file_diff) = parse_file_diff(raw) else {
        // A header parsed but produced no hunks: a mode change or a pure
        // rename. Something happened to the file; none of it is text.
        return ParsedDiff::empty(EmptyDiffReason::NoTextualChanges);
    };
    let html = if options.html {
        super::render::plain_html(&file_diff)
    } else {
        Vec::new()
    };
    let sbs_pairs = if options.side_by_side {
        build_sbs_pairs(&file_diff.hunks)
    } else {
        Vec::new()
    };
    let (additions, deletions) = count_changes(&file_diff.hunks);
    ParsedDiff {
        file_diff,
        html,
        sbs_pairs,
        additions,
        deletions,
        empty_reason: None,
        size_guard: None,
    }
}

/// Read a working-tree file's diff and parse it in one call.
///
/// Fusing the read and the parse removes a full round trip per file selection
/// from each client, and gives the *whitespace-only* answer somewhere to be
/// computed: when hide-whitespace produced nothing, the unfiltered diff decides
/// whether the file is unchanged or merely re-indented — a second `git diff`,
/// but only on the path where the pane would otherwise be blank.
///
/// # Errors
/// When `git diff` can't run.
pub fn get_parsed_diff(
    repo_path: String,
    file: super::git::FileEntry,
    hide_whitespace: bool,
    options: DiffOptions,
) -> Result<ParsedDiff, String> {
    let raw = if hide_whitespace {
        super::git::get_diff_whitespace_ignored(repo_path.clone(), file.clone())?
    } else {
        super::git::get_diff(repo_path.clone(), file.clone())?
    };
    let parsed = parse_diff_with(&raw, options);
    if hide_whitespace && parsed.empty_reason.is_some() {
        // The question is not whether the unfiltered patch is non-empty — a
        // pure rename's header is non-empty and has no lines either. It is
        // whether the unfiltered patch has something to *render*: if it does
        // and the filtered one doesn't, the difference is the whitespace.
        // A patch big enough to trip the size guard plainly has content, and
        // leaves `empty_reason` unset, so it answers this without being parsed.
        let unfiltered = parse_diff_with(
            &super::git::get_diff(repo_path, file)?,
            DiffOptions::default(),
        );
        if unfiltered.empty_reason.is_none() {
            return Ok(ParsedDiff::empty(EmptyDiffReason::WhitespaceOnly));
        }
    }
    Ok(parsed)
}

/// Read one file's diff from a commit and parse it in one call.
///
/// An empty `file_path` yields the whole commit's patch, matching
/// [`super::git::get_commit_diff`].
///
/// # Errors
/// When `git log` can't run or the revision doesn't resolve.
pub fn get_parsed_commit_diff(
    repo_path: String,
    sha: String,
    file_path: String,
    options: DiffOptions,
) -> Result<ParsedDiff, String> {
    let raw = super::git::get_commit_diff(repo_path, sha, file_path)?;
    Ok(parse_diff_with(&raw, options))
}

/// Plain text of a flat line range, for the clipboard.
///
/// Rebuilt from the line model rather than scraped off the rendered view, so a
/// copy can't pick up gutters, `+`/`−` prefixes, side-by-side filler cells or a
/// viewer's tab expansion — `content` keeps the file's real tabs. `start` is
/// inclusive, `end` exclusive, indexed the same way as `html` and `sbs_pairs`:
/// flat across every hunk's `lines`, `@@` headers included. Out-of-range
/// indices clamp rather than panic; the viewer's selection and the model can
/// briefly disagree while a new diff loads.
///
/// `\ No newline at end of file` is dropped. It is git's annotation *about* a
/// line rather than a line, it belongs to one side of the diff and so has no
/// row at all in the side-by-side pairing — a reader selecting a block there
/// would paste a line they were never shown — and pasting it into source is
/// never what was meant. A `@@` header is kept, because it is a row the reader
/// can see and select in both arrangements.
#[must_use]
pub fn copy_text(file_diff: &FileDiff, start: usize, end: usize) -> String {
    file_diff
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .skip(start)
        .take(end.saturating_sub(start))
        .filter(|l| l.line_type != LineType::NoNewline)
        // A hunk header *is* its text; every other row's content is the file's
        // own line, prefix already stripped.
        .map(|l| l.text.as_ref().unwrap_or(&l.content).as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_file_diff(raw: &str) -> Option<FileDiff> {
    if raw.trim().is_empty() {
        return None;
    }

    // The patch's own trailing newline is a terminator, not a line. Splitting
    // without dropping it yields a final `""`, which the hunk body reads as an
    // empty context line — a blank, numbered row at the foot of every diff, and
    // one more line than the file has in anything copied from it. A *genuinely*
    // blank context line still arrives here as `""` when a tool has stripped
    // the trailing space git writes, which is why only the last one goes.
    let lines: Vec<&str> = raw.strip_suffix('\n').unwrap_or(raw).split('\n').collect();

    let mut old_path = String::new();
    let mut new_path = String::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut header_lines: Vec<String> = Vec::new();
    let mut is_binary = false;

    let mut current_hunk: Option<Hunk> = None;
    let mut old_line_no: i32 = 0;
    let mut new_line_no: i32 = 0;
    let mut in_header = true;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // -- Hunk header --
        if line.starts_with("@@") {
            in_header = false;

            match parse_hunk_header(line) {
                Some(header) => {
                    // Save the previous hunk if any.
                    if let Some(h) = current_hunk.take() {
                        hunks.push(h);
                    }

                    old_line_no = header.old_start;
                    new_line_no = header.new_start;

                    let header_diff_line = DiffLine {
                        text: Some(line.to_string()),
                        content: line.to_string(),
                        line_type: LineType::Hunk,
                        old_line_no: None,
                        new_line_no: None,
                        intra_line_diff: None,
                    };

                    current_hunk = Some(Hunk {
                        header,
                        lines: vec![header_diff_line],
                    });

                    i += 1;
                    continue;
                }
                None => {
                    // Malformed hunk header — skip it so we don't loop forever.
                    i += 1;
                    continue;
                }
            }
        }

        // -- File header (anything before the first @@) --
        if in_header {
            if let Some(rest) = line.strip_prefix("--- ") {
                old_path = strip_path_prefix(rest, "a/");
            } else if let Some(rest) = line.strip_prefix("+++ ") {
                new_path = strip_path_prefix(rest, "b/");
            } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
                // Git emits this single line for binary changes instead of `@@`
                // hunks. Capture both paths so the frontend can label the file,
                // and flag the diff so the viewer renders the binary stand-in.
                is_binary = true;
                if let Some((old, new)) = parse_binary_marker(line) {
                    if !old.is_empty() {
                        old_path = old;
                    }
                    if !new.is_empty() {
                        new_path = new;
                    }
                }
            }
            header_lines.push(line.to_string());
            i += 1;
            continue;
        }

        // -- Inside a hunk: classify each line --
        let hunk = match current_hunk.as_mut() {
            Some(h) => h,
            None => {
                i += 1;
                continue;
            }
        };

        if line.is_empty() {
            // Empty line in unified diff = context line with empty content.
            // `String::split('\n')` collapses a blank line into "", but in real
            // unified diff format it would be " ".
            hunk.lines.push(DiffLine {
                text: None,
                content: String::new(),
                line_type: LineType::Context,
                old_line_no: Some(old_line_no),
                new_line_no: Some(new_line_no),
                intra_line_diff: None,
            });
            old_line_no += 1;
            new_line_no += 1;
            i += 1;
            continue;
        }

        let first = line.as_bytes()[0];
        match first {
            b'+' => {
                let content = line[1..].to_string();
                hunk.lines.push(DiffLine {
                    text: None,
                    content,
                    line_type: LineType::Add,
                    old_line_no: None,
                    new_line_no: Some(new_line_no),
                    intra_line_diff: None,
                });
                new_line_no += 1;
            }
            b'-' => {
                let content = line[1..].to_string();
                hunk.lines.push(DiffLine {
                    text: None,
                    content,
                    line_type: LineType::Delete,
                    old_line_no: Some(old_line_no),
                    new_line_no: None,
                    intra_line_diff: None,
                });
                old_line_no += 1;
            }
            b'\\' => {
                // "\ No newline at end of file" — belongs inside the current hunk,
                // attached to whichever line it follows. Patch builder echoes it back.
                hunk.lines.push(DiffLine {
                    text: Some(line.to_string()),
                    content: line.to_string(),
                    line_type: LineType::NoNewline,
                    old_line_no: None,
                    new_line_no: None,
                    intra_line_diff: None,
                });
            }
            _ => {
                // Context line (typically prefixed with a space).
                let content = if first == b' ' {
                    line[1..].to_string()
                } else {
                    line.to_string()
                };
                hunk.lines.push(DiffLine {
                    text: None,
                    content,
                    line_type: LineType::Context,
                    old_line_no: Some(old_line_no),
                    new_line_no: Some(new_line_no),
                    intra_line_diff: None,
                });
                old_line_no += 1;
                new_line_no += 1;
            }
        }

        i += 1;
    }

    // Flush the trailing hunk.
    if let Some(h) = current_hunk.take() {
        hunks.push(h);
    }

    // Binary diffs legitimately have zero hunks — keep them; everything else
    // with no hunks is an empty/no-op diff we can discard.
    if hunks.is_empty() && !is_binary {
        return None;
    }

    annotate_intra_line_changes(&mut hunks);

    let file_header = header_lines.join("\n");

    Some(FileDiff {
        old_path,
        new_path,
        file_header,
        hunks,
        is_binary,
    })
}

/// Builds the side-by-side rows: context and hunk-header lines span both
/// columns; each run of deletes is zipped against the add run that follows it,
/// with `None` filling the shorter side. `NoNewline` markers get no row of
/// their own — the unified view shows them, side-by-side omits them.
fn build_sbs_pairs(hunks: &[Hunk]) -> Vec<SbsPair> {
    let mut pairs = Vec::new();
    // Flat index of the current hunk's first line; each hunk's `lines` array
    // includes its `@@` header, so the running total accounts for it.
    let mut base = 0usize;
    for hunk in hunks {
        let lines = &hunk.lines;
        let mut i = 0usize;
        while i < lines.len() {
            let g = base + i;
            match lines[i].line_type {
                LineType::Hunk => {
                    pairs.push(SbsPair {
                        left: Some(g),
                        right: Some(g),
                        is_hunk_header: true,
                    });
                    i += 1;
                }
                LineType::Context => {
                    pairs.push(SbsPair {
                        left: Some(g),
                        right: Some(g),
                        is_hunk_header: false,
                    });
                    i += 1;
                }
                LineType::NoNewline => {
                    i += 1;
                }
                LineType::Delete | LineType::Add => {
                    let del_start = i;
                    while i < lines.len() && lines[i].line_type == LineType::Delete {
                        i += 1;
                    }
                    let del_end = i;
                    while i < lines.len() && lines[i].line_type == LineType::Add {
                        i += 1;
                    }
                    let deletes = del_end - del_start;
                    let adds = i - del_end;
                    for k in 0..deletes.max(adds) {
                        pairs.push(SbsPair {
                            left: (k < deletes).then(|| base + del_start + k),
                            right: (k < adds).then(|| base + del_end + k),
                            is_hunk_header: false,
                        });
                    }
                }
            }
        }
        base += lines.len();
    }
    pairs
}

/// Added/deleted line totals for the viewer's header badge.
fn count_changes(hunks: &[Hunk]) -> (u32, u32) {
    let mut additions = 0u32;
    let mut deletions = 0u32;
    for hunk in hunks {
        for line in &hunk.lines {
            match line.line_type {
                LineType::Add => additions += 1,
                LineType::Delete => deletions += 1,
                _ => {}
            }
        }
    }
    (additions, deletions)
}

/// Pairs up consecutive delete/add runs within each hunk and tags each pair
/// with the character range that actually changed, so the viewer can highlight
/// `Relay` → `Metrics` inside an otherwise identical line. Matches GitHub
/// Desktop's approach (`relativeChanges` in `app/src/ui/diff/changed-range.ts`):
/// only annotate when the delete count equals the add count, then pair by
/// index. Mismatched counts are left untouched (full-line diff still shows).
fn annotate_intra_line_changes(hunks: &mut [Hunk]) {
    for hunk in hunks.iter_mut() {
        let mut i = 0;
        while i < hunk.lines.len() {
            // Walk to the next Delete run.
            if hunk.lines[i].line_type != LineType::Delete {
                i += 1;
                continue;
            }
            let delete_start = i;
            while i < hunk.lines.len() && hunk.lines[i].line_type == LineType::Delete {
                i += 1;
            }
            let delete_end = i;

            // The matching Add run must follow immediately — no context or
            // other line types in between — for these to count as a paired
            // edit.
            let add_start = i;
            while i < hunk.lines.len() && hunk.lines[i].line_type == LineType::Add {
                i += 1;
            }
            let add_end = i;

            let delete_count = delete_end - delete_start;
            let add_count = add_end - add_start;
            if delete_count == 0 || add_count == 0 || delete_count != add_count {
                continue;
            }

            for j in 0..delete_count {
                let del_idx = delete_start + j;
                let add_idx = add_start + j;
                let del_content = hunk.lines[del_idx].content.clone();
                let add_content = hunk.lines[add_idx].content.clone();
                if del_content.len() > MAX_INTRA_LINE_LEN || add_content.len() > MAX_INTRA_LINE_LEN
                {
                    continue;
                }
                let (del_range, add_range) = compute_intra_line_ranges(&del_content, &add_content);
                hunk.lines[del_idx].intra_line_diff = del_range;
                hunk.lines[add_idx].intra_line_diff = add_range;
            }
        }
    }
}

/// Returns the changed character range on each side after stripping the longest
/// common prefix and suffix. Character indices are code points, not bytes, so
/// they line up with `Array.from(str).slice(...)` on the JS side. Returns
/// `None` for a side when no characters differ.
fn compute_intra_line_ranges(a: &str, b: &str) -> (Option<IntraLineRange>, Option<IntraLineRange>) {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut prefix = 0;
    let prefix_cap = a_chars.len().min(b_chars.len());
    while prefix < prefix_cap && a_chars[prefix] == b_chars[prefix] {
        prefix += 1;
    }

    let mut suffix = 0;
    let suffix_cap = (a_chars.len() - prefix).min(b_chars.len() - prefix);
    while suffix < suffix_cap
        && a_chars[a_chars.len() - 1 - suffix] == b_chars[b_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let a_len = a_chars.len() - prefix - suffix;
    let b_len = b_chars.len() - prefix - suffix;

    // The call site skips lines longer than `MAX_INTRA_LINE_LEN`, so every
    // index fits the `u32` the range shares with `highlight::Token`.
    let index =
        |n: usize| u32::try_from(n).expect("intra-line indices bounded by MAX_INTRA_LINE_LEN");

    let a_range = (a_len > 0).then_some(IntraLineRange {
        start: index(prefix),
        length: index(a_len),
    });
    let b_range = (b_len > 0).then_some(IntraLineRange {
        start: index(prefix),
        length: index(b_len),
    });
    (a_range, b_range)
}

/// Parses a `Binary files a/X and b/Y differ` line into `(old_path, new_path)`.
/// `/dev/null` on either side (add or delete) is returned as an empty string so
/// callers can distinguish "no prior path" from "real path".
fn parse_binary_marker(line: &str) -> Option<(String, String)> {
    let inner = line
        .strip_prefix("Binary files ")?
        .strip_suffix(" differ")?;
    let mid = inner.find(" and ")?;
    let lhs = &inner[..mid];
    let rhs = &inner[mid + 5..];
    let old = if lhs == "/dev/null" {
        String::new()
    } else {
        lhs.strip_prefix("a/").unwrap_or(lhs).to_string()
    };
    let new = if rhs == "/dev/null" {
        String::new()
    } else {
        rhs.strip_prefix("b/").unwrap_or(rhs).to_string()
    };
    Some((old, new))
}

/// One `---`/`+++` header line: the prefixed path, or `/dev/null` for the side
/// a creation or a deletion does not have.
fn format_patch_path(marker: &str, prefix: &str, path: &str) -> String {
    if path.is_empty() {
        format!("{marker} /dev/null\n")
    } else {
        format!("{marker} {prefix}{path}\n")
    }
}

/// Strips the `--- `/`+++ ` argument down to a repo-relative path.
/// Removes the conventional `a/` or `b/` prefix git uses, and trims an
/// optional trailing timestamp (tab-separated) found in some diff outputs.
fn strip_path_prefix(rest: &str, prefix: &str) -> String {
    // Drop trailing timestamp if present (git's --raw format separates with TAB).
    let mut path = rest.split('\t').next().unwrap_or("").to_string();
    // `/dev/null` is git's way of saying *this side does not exist* — an added
    // file has no old path and a deleted one has no new path. It is not a path,
    // so it is answered as absence, the same as `parse_binary_marker` already
    // does: a viewer comparing the two sides to spot a rename would otherwise
    // read every add as `/dev/null → <file>`.
    if path == "/dev/null" {
        return String::new();
    }
    if let Some(stripped) = path.strip_prefix(prefix) {
        path = stripped.to_string();
    }
    path
}

/// Parses a unified-diff hunk header of the form:
///   `@@ -oldStart[,oldCount] +newStart[,newCount] @@[ optional heading]`
fn parse_hunk_header(header: &str) -> Option<HunkHeader> {
    // Manual parser (regex crate not in Cargo.toml).
    let s = header;
    let rest = s.strip_prefix("@@ -")?;

    // Find " +" separating old/new ranges.
    let plus_idx = rest.find(" +")?;
    let old_part = &rest[..plus_idx];
    let after_plus = &rest[plus_idx + 2..];

    // Find " @@" closing marker.
    let end_idx = after_plus.find(" @@")?;
    let new_part = &after_plus[..end_idx];

    let (old_start, old_count) = parse_range(old_part)?;
    let (new_start, new_count) = parse_range(new_part)?;

    Some(HunkHeader {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

/// Parses "start[,count]" into `(start, count)`. Missing count defaults to 1.
fn parse_range(range: &str) -> Option<(i32, i32)> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    let mut parts = range.split(',');
    let start = parts.next()?.parse::<i32>().ok()?;
    let count = match parts.next() {
        Some(c) => c.parse::<i32>().ok()?,
        None => 1,
    };
    Some((start, count))
}

pub fn generate_patch(
    repo_path: String,
    file_diff: FileDiff,
    selection: DiffSelection,
) -> Result<(), String> {
    let patch_content = build_patch(&file_diff, &selection, false)?;
    if patch_content.is_empty() {
        return Ok(());
    }
    apply_patch(&repo_path, &patch_content, false, true)?;
    Ok(())
}

pub fn generate_inverse_patch(
    repo_path: String,
    file_diff: FileDiff,
    selection: DiffSelection,
) -> Result<(), String> {
    // Inverse patch is applied to the working tree (no --cached) via
    // `git apply --reverse`. Building the patch in "forward" form and letting
    // git reverse it keeps the math simple and matches the Go reference's
    // semantic of "discard from working tree".
    let patch_content = build_patch(&file_diff, &selection, false)?;
    if patch_content.is_empty() {
        return Ok(());
    }
    apply_patch(&repo_path, &patch_content, true, false)?;
    Ok(())
}

/// Builds a unified diff patch from a `FileDiff` plus a per-line selection.
///
/// Semantics (matches Go's `GeneratePatch`):
/// - Unselected ADDs are dropped entirely (they don't exist in old file).
/// - Unselected DELETEs become context lines (prefix ' ' + content), so the
///   hunk still describes a contiguous slice of the old file.
/// - Context lines are always kept and counted in both old/new.
/// - The hunk's `@@` header line is regenerated with recalculated counts.
/// - `NoNewline` markers are echoed back next to the line they attach to.
///
/// The `inverse` flag swaps add/delete semantics for "discard" workflows.
fn build_patch(
    file_diff: &FileDiff,
    selection: &DiffSelection,
    inverse: bool,
) -> Result<String, String> {
    let mut patch = String::new();

    // Emit the captured preamble (diff --git, index, ---, +++) first.
    // `git apply` REQUIRES the `diff --git` header for new/deleted/renamed files.
    if !file_diff.file_header.is_empty() {
        patch.push_str(&file_diff.file_header);
        if !file_diff.file_header.ends_with('\n') {
            patch.push('\n');
        }
    } else {
        // Fallback: synthesise minimal headers so plain edits still apply. An
        // absent side is written back as `/dev/null`, which is what it was
        // parsed from and what `git apply` needs to recognise a creation or a
        // deletion — an omitted `---` line makes the patch unparseable.
        patch.push_str(&format_patch_path("---", "a/", &file_diff.old_path));
        patch.push_str(&format_patch_path("+++", "b/", &file_diff.new_path));
    }

    let mut flat_idx: usize = 0;
    let mut any_hunk_emitted = false;

    for hunk in &file_diff.hunks {
        let mut out_lines: Vec<String> = Vec::new();
        let mut new_old_count: i32 = 0;
        let mut new_new_count: i32 = 0;
        let mut has_changes = false;

        for line in &hunk.lines {
            let idx = flat_idx;
            flat_idx += 1;

            match line.line_type {
                LineType::Hunk => {
                    // Skip the original header — we regenerate it below.
                    continue;
                }
                LineType::Context => {
                    out_lines.push(format!(" {}", line.content));
                    new_old_count += 1;
                    new_new_count += 1;
                }
                LineType::Add => {
                    let selected = selection.is_selected(idx);
                    if !inverse {
                        if selected {
                            out_lines.push(format!("+{}", line.content));
                            new_new_count += 1;
                            has_changes = true;
                        }
                        // Unselected adds are dropped (don't exist in old file).
                    } else {
                        // Inverse: selected add becomes a delete; unselected add
                        // becomes context (it survives in the new working tree).
                        if selected {
                            out_lines.push(format!("-{}", line.content));
                            new_old_count += 1;
                            has_changes = true;
                        } else {
                            out_lines.push(format!(" {}", line.content));
                            new_old_count += 1;
                            new_new_count += 1;
                        }
                    }
                }
                LineType::Delete => {
                    let selected = selection.is_selected(idx);
                    if !inverse {
                        if selected {
                            out_lines.push(format!("-{}", line.content));
                            new_old_count += 1;
                            has_changes = true;
                        } else {
                            // Unselected delete -> convert to context so the
                            // patch still covers a contiguous old-file slice.
                            out_lines.push(format!(" {}", line.content));
                            new_old_count += 1;
                            new_new_count += 1;
                        }
                    } else {
                        // Inverse: selected delete becomes an add; unselected
                        // delete becomes context.
                        if selected {
                            out_lines.push(format!("+{}", line.content));
                            new_new_count += 1;
                            has_changes = true;
                        } else {
                            out_lines.push(format!(" {}", line.content));
                            new_old_count += 1;
                            new_new_count += 1;
                        }
                    }
                }
                LineType::NoNewline => {
                    // Always echo the no-newline marker.
                    out_lines.push(line.text.clone().unwrap_or_else(|| line.content.clone()));
                }
            }
        }

        if !has_changes {
            continue;
        }

        // Inverse patches swap old/new starts so the header matches git's
        // expectation when applied with --reverse... but since we apply with
        // `--reverse` for inverse mode (not inverse-built), keep the natural
        // orientation here. Only swap if we were producing an already-inverted
        // patch for an apply WITHOUT --reverse — which we don't do today.
        let (old_start, new_start) = (hunk.header.old_start, hunk.header.new_start);

        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start, new_old_count, new_start, new_new_count
        ));

        for l in &out_lines {
            patch.push_str(l);
            patch.push('\n');
        }

        any_hunk_emitted = true;
    }

    if !any_hunk_emitted {
        return Ok(String::new());
    }

    Ok(patch)
}

/// Calculates the flat/global line index of `line_idx_in_current` within
/// `hunks[hunks.len() - 1]`. Each hunk's `lines` array INCLUDES its `@@` header
/// line, so the running total naturally accounts for it.
///
/// Contract (must match the frontend):
///   `global_idx = sum(prev_hunk.lines.len()) + line_idx_in_current`
///
/// Kept available for tests/debug; the patch builder uses a running counter.
#[allow(dead_code)]
fn calculate_global_line_index(hunks: &[Hunk], line_idx_in_current: usize) -> usize {
    if hunks.is_empty() {
        return line_idx_in_current;
    }
    let mut index = 0usize;
    // Sum lengths of every hunk except the current (last) one.
    for h in &hunks[..hunks.len().saturating_sub(1)] {
        index += h.lines.len();
    }
    index + line_idx_in_current
}

/// Pipes a patch to `git apply`. Mirrors the Go reference's flag set:
/// `--unidiff-zero` (allow zero-context hunks) and `--whitespace=nowarn`.
///
/// - `reverse=true` applies the patch in reverse (used for discard).
/// - `cached=true` stages via the index (used for partial commit); otherwise
///   the patch is applied to the working tree.
///
/// `--reject` is intentionally NOT passed; it silently writes `.rej` files
/// and masks failures.
fn apply_patch(
    repo_path: &str,
    patch_content: &str,
    reverse: bool,
    cached: bool,
) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path);
    super::process::hide_console(&mut cmd);
    cmd.arg("apply");

    if cached {
        cmd.arg("--cached");
    }
    if reverse {
        cmd.arg("--reverse");
    }

    cmd.arg("--unidiff-zero");
    cmd.arg("--whitespace=nowarn");

    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        stdin
            .write_all(patch_content.as_bytes())
            .map_err(|e| format!("Failed to write to git apply stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git apply failed: {}", stderr));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the way a `WebView` host does — every render artifact built —
    /// since these tests assert on `html` and `sbs_pairs`.
    fn parse(raw: &str) -> ParsedDiff {
        parse_diff_with(
            raw,
            DiffOptions {
                html: true,
                side_by_side: true,
                show_anyway: false,
            },
        )
    }

    const HEADER: &str = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n";

    /// Context/header rows span both columns; a delete run is zipped against
    /// the add run that follows it, `None` filling the shorter side.
    #[test]
    fn sbs_pairs_zip_delete_and_add_runs() {
        let raw = format!("{HEADER}@@ -1,4 +1,3 @@\n ctx\n-old1\n-old2\n+new1\n tail\n");
        let parsed = parse(&raw);
        let rows: Vec<(Option<usize>, Option<usize>, bool)> = parsed
            .sbs_pairs
            .iter()
            .map(|p| (p.left, p.right, p.is_hunk_header))
            .collect();
        // Flat lines: 0=@@ 1=ctx 2=-old1 3=-old2 4=+new1 5=tail. The patch's
        // own trailing newline is a terminator, not a sixth row.
        assert_eq!(
            rows,
            [
                (Some(0), Some(0), true),
                (Some(1), Some(1), false),
                (Some(2), Some(4), false),
                (Some(3), None, false),
                (Some(5), Some(5), false),
            ]
        );
        assert_eq!((parsed.additions, parsed.deletions), (1, 2));
    }

    /// The `\ No newline at end of file` marker keeps its flat line (and html
    /// slot) but gets no side-by-side row of its own.
    #[test]
    fn no_newline_marker_gets_no_side_by_side_row() {
        // Flat lines: 0=@@ 1=-old 2=+new 3=NoNewline.
        let raw = format!("{HEADER}@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n");
        let parsed = parse(&raw);
        assert_eq!(
            parsed.sbs_pairs.len(),
            2,
            "header row + one zipped pair; no NoNewline row"
        );
        assert_eq!(parsed.html.len(), 4, "html stays 1:1 with flattened lines");
    }

    /// The phase-1 HTML is escaped and carries the intra-line backplate the
    /// annotator computed for a paired single-line edit.
    #[test]
    fn phase1_html_carries_escaping_and_intra_line_backplate() {
        let raw = format!("{HEADER}@@ -1 +1 @@\n-if a < 1 {{}}\n+if b < 1 {{}}\n");
        let parsed = parse(&raw);
        assert_eq!(
            parsed.html[1],
            "if <span class=\"diff-intra-remove\">a</span> &lt; 1 {}"
        );
        assert_eq!(
            parsed.html[2],
            "if <span class=\"diff-intra-add\">b</span> &lt; 1 {}"
        );
    }

    // -----------------------------------------------------------------------
    // Render options (H-8)
    // -----------------------------------------------------------------------

    /// The parse is shared; the render artifacts are not. A host that renders
    /// from the line model must not pay to build HTML and pairings it drops.
    #[test]
    fn render_artifacts_are_built_only_when_asked_for() {
        let raw = format!("{HEADER}@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n");

        let lean = parse_diff_with(&raw, DiffOptions::default());
        assert!(lean.html.is_empty(), "no HTML unless asked");
        assert!(lean.sbs_pairs.is_empty(), "no pairs unless asked");
        assert_eq!(lean.additions, 1, "the counts are never optional");
        assert_eq!(lean.deletions, 1);
        assert!(!lean.file_diff.hunks.is_empty(), "the parse still happened");

        let full = parse(&raw);
        assert!(!full.html.is_empty());
        assert!(!full.sbs_pairs.is_empty());
        // Same parse either way — options change what is *derived* from it.
        assert_eq!(full.additions, lean.additions);
        assert_eq!(
            full.file_diff.hunks[0].lines.len(),
            lean.file_diff.hunks[0].lines.len()
        );
    }

    /// `text` exists only where something reads it. Every other row carries its
    /// content once instead of twice.
    #[test]
    fn line_text_is_kept_only_for_hunk_headers_and_no_newline_markers() {
        let raw =
            format!("{HEADER}@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n\\ No newline at end of file\n");
        let parsed = parse_diff_with(&raw, DiffOptions::default());
        let lines: Vec<&DiffLine> = parsed
            .file_diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .collect();

        for line in &lines {
            match line.line_type {
                LineType::Hunk | LineType::NoNewline => assert!(
                    line.text.is_some(),
                    "{:?} row keeps its raw text",
                    line.line_type
                ),
                _ => assert!(
                    line.text.is_none(),
                    "{:?} row drops the duplicate of `content`",
                    line.line_type
                ),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Empty-parse reasons (H-9)
    // -----------------------------------------------------------------------

    /// An empty patch and a header-only patch are different events and get
    /// different answers; a real diff gets neither.
    #[test]
    fn empty_reason_separates_no_changes_from_no_textual_changes() {
        assert_eq!(
            parse_diff_with("", DiffOptions::default()).empty_reason,
            Some(EmptyDiffReason::NoChanges)
        );
        assert_eq!(
            parse_diff_with("   \n \n", DiffOptions::default()).empty_reason,
            Some(EmptyDiffReason::NoChanges)
        );

        // A pure rename: a full header, no hunks. The file changed; its text
        // did not — which is precisely what the viewer could not say before.
        let rename = "diff --git a/old.txt b/new.txt\nsimilarity index 100%\n\
                      rename from old.txt\nrename to new.txt\n";
        assert_eq!(
            parse_diff_with(rename, DiffOptions::default()).empty_reason,
            Some(EmptyDiffReason::NoTextualChanges)
        );

        let real = format!("{HEADER}@@ -1 +1 @@\n-a\n+b\n");
        assert_eq!(
            parse_diff_with(&real, DiffOptions::default()).empty_reason,
            None
        );
    }

    /// A binary diff has no hunks either, but it is not empty — the viewer has
    /// a stand-in to render, so claiming "no textual changes" would hide it.
    #[test]
    fn a_binary_diff_is_not_reported_as_empty() {
        let raw = "diff --git a/x.png b/x.png\nBinary files a/x.png and b/x.png differ\n";
        let parsed = parse_diff_with(raw, DiffOptions::default());
        assert_eq!(parsed.empty_reason, None);
        assert!(parsed.file_diff.is_binary);
    }

    // -----------------------------------------------------------------------
    // Size guard (H-15)
    // -----------------------------------------------------------------------

    /// One very long line is enough to withhold a diff even when the patch as
    /// a whole is small — the minified-bundle case, which the byte total alone
    /// waves through and which is exactly what makes a viewer crawl.
    #[test]
    fn size_guard_withholds_a_long_line_and_show_anyway_overrides_it() {
        let long = "x".repeat(MAX_REASONABLE_LINE_BYTES as usize + 1);
        let raw = format!("{HEADER}@@ -1 +1 @@\n-a\n+{long}\n");

        let withheld = parse_diff_with(&raw, DiffOptions::default());
        let guard = withheld.size_guard.expect("guard trips on the long line");
        assert_eq!(guard.reason, DiffSizeReason::LineLength);
        assert!(guard.longest_line > MAX_REASONABLE_LINE_BYTES);
        assert!(withheld.file_diff.hunks.is_empty(), "nothing was parsed");
        assert_eq!(
            withheld.empty_reason, None,
            "withheld is not the same as empty — the viewer offers to show it"
        );

        let shown = parse_diff_with(
            &raw,
            DiffOptions {
                show_anyway: true,
                ..DiffOptions::default()
            },
        );
        assert!(shown.size_guard.is_none(), "the escape clears the guard");
        assert!(!shown.file_diff.hunks.is_empty(), "and the diff parses");
    }

    /// A total over the byte bar trips the guard for a different reason, so the
    /// viewer can say which limit it hit.
    #[test]
    fn size_guard_reports_the_byte_total_separately() {
        let filler = "+line of some length here\n".repeat(200_000);
        let raw = format!("{HEADER}@@ -1 +1,200000 @@\n{filler}");
        let guard = parse_diff_with(&raw, DiffOptions::default())
            .size_guard
            .expect("guard trips on the byte total");
        assert_eq!(guard.reason, DiffSizeReason::TotalBytes);
        assert!(guard.bytes > MAX_REASONABLE_DIFF_BYTES);
    }

    /// An ordinary diff is never withheld.
    #[test]
    fn size_guard_leaves_an_ordinary_diff_alone() {
        let raw = format!("{HEADER}@@ -1 +1 @@\n-a\n+b\n");
        assert!(
            parse_diff_with(&raw, DiffOptions::default())
                .size_guard
                .is_none()
        );
    }

    // -----------------------------------------------------------------------
    // Clipboard text (H-16)
    // -----------------------------------------------------------------------

    /// Copy is rebuilt from the model, so it carries the file's own lines —
    /// no `+`/`-` prefixes, no gutters, and real tabs rather than a viewer's
    /// expansion of them.
    #[test]
    fn copy_text_rebuilds_source_lines_without_prefixes() {
        let raw = format!("{HEADER}@@ -1,3 +1,3 @@\n ctx\n-\told\n+\tnew\n");
        let parsed = parse_diff_with(&raw, DiffOptions::default());
        let flat = parsed.file_diff.hunks[0].lines.len();

        // 0 is the `@@` header; 1..4 are ctx, -old, +new.
        assert_eq!(
            copy_text(&parsed.file_diff, 1, 4),
            "ctx\n\told\n\tnew",
            "prefixes gone, tabs intact"
        );
        // A hunk header is its text — that is what the viewer shows.
        assert!(copy_text(&parsed.file_diff, 0, 1).starts_with("@@"));
        // Out-of-range indices clamp instead of panicking: the viewer's
        // selection and the model can disagree while a new diff loads.
        assert_eq!(
            copy_text(&parsed.file_diff, 1, flat + 50),
            copy_text(&parsed.file_diff, 1, flat)
        );
        assert!(copy_text(&parsed.file_diff, flat + 1, flat + 2).is_empty());
    }

    /// The no-newline marker is git's annotation about a line, not a line: it
    /// has no row at all in the side-by-side pairing, so a reader selecting a
    /// block there would paste something they were never shown.
    #[test]
    fn copy_text_drops_the_no_newline_marker() {
        let raw = format!(
            "{HEADER}@@ -1,3 +1,3 @@\n ctx\n-old\n\\ No newline at end of file\n+new\n tail\n"
        );
        let parsed = parse_diff_with(&raw, DiffOptions::default());
        let flat = parsed.file_diff.hunks[0].lines.len();
        assert_eq!(
            parsed.file_diff.hunks[0].lines[3].line_type,
            LineType::NoNewline,
            "the marker sits mid-hunk, not at the end"
        );
        assert_eq!(flat, 6, "@@, ctx, -old, marker, +new, tail");
        assert_eq!(copy_text(&parsed.file_diff, 1, flat), "ctx\nold\nnew\ntail");
    }

    /// `/dev/null` is git saying *this side does not exist*, not a path. A
    /// viewer comparing the two sides to spot a rename read every added file as
    /// `/dev/null → <file>` while it survived the parse.
    #[test]
    fn an_absent_side_parses_as_no_path_at_all() {
        let added = concat!(
            "diff --git a/new.txt b/new.txt\n",
            "new file mode 100644\n",
            "index 0000000..e69de29\n",
            "--- /dev/null\n",
            "+++ b/new.txt\n",
            "@@ -0,0 +1 @@\n",
            "+hello\n"
        );
        let parsed = parse_diff_with(added, DiffOptions::default());
        assert_eq!(parsed.file_diff.old_path, "");
        assert_eq!(parsed.file_diff.new_path, "new.txt");

        let deleted = concat!(
            "diff --git a/gone.txt b/gone.txt\n",
            "deleted file mode 100644\n",
            "index e69de29..0000000\n",
            "--- a/gone.txt\n",
            "+++ /dev/null\n",
            "@@ -1 +0,0 @@\n",
            "-bye\n"
        );
        let parsed = parse_diff_with(deleted, DiffOptions::default());
        assert_eq!(parsed.file_diff.old_path, "gone.txt");
        assert_eq!(parsed.file_diff.new_path, "");
    }

    /// A synthesised header has to write the absent side back as `/dev/null`,
    /// which is what `git apply` reads as a creation or a deletion. Omitting
    /// the line makes the patch unparseable.
    #[test]
    fn a_synthesised_patch_header_names_the_absent_side_dev_null() {
        let added = concat!(
            "diff --git a/new.txt b/new.txt\n",
            "--- /dev/null\n",
            "+++ b/new.txt\n",
            "@@ -0,0 +1 @@\n",
            "+hello\n"
        );
        let mut parsed = parse_diff_with(added, DiffOptions::default());
        // Drop the captured header so the fallback path is the one under test.
        parsed.file_diff.file_header = String::new();
        let selection = DiffSelection {
            default_selected: true,
            diverging_lines: HashMap::new(),
        };
        let patch = build_patch(&parsed.file_diff, &selection, false).expect("a patch");
        assert!(
            patch.starts_with("--- /dev/null\n+++ b/new.txt\n"),
            "{patch}"
        );
    }

    // -----------------------------------------------------------------------
    // Fused read + parse (H-8), and the whitespace-only answer it enables
    // -----------------------------------------------------------------------

    /// A re-indented file with hide-whitespace on renders an empty pane. The
    /// caption has to say *why*, and only the fused call can find out: the
    /// unfiltered diff is what separates "unchanged" from "whitespace only".
    #[test]
    fn hide_whitespace_reports_a_whitespace_only_change_as_such() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        for args in [
            ["init", "-q", "."].as_slice(),
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "Test User"],
            &["config", "commit.gpgsign", "false"],
        ] {
            let ok = Command::new("git")
                .current_dir(repo)
                .args(args)
                .status()
                .expect("spawn git")
                .success();
            assert!(ok, "git {args:?} failed");
        }
        std::fs::write(repo.join("a.txt"), "fn main() {}\n").expect("write");
        for args in [["add", "-A"].as_slice(), &["commit", "-q", "-m", "base"]] {
            Command::new("git")
                .current_dir(repo)
                .args(args)
                .status()
                .expect("spawn git");
        }
        let repo_path = repo.to_str().expect("utf-8 path").to_string();
        let status = super::super::git::get_status(repo_path.clone()).expect("status");
        assert!(status.files.is_empty(), "clean after the commit");

        // Re-indent only.
        std::fs::write(repo.join("a.txt"), "    fn main() {}\n").expect("rewrite");
        let file = super::super::git::get_status(repo_path.clone())
            .expect("status")
            .files
            .pop()
            .expect("the edited file is listed");

        let shown = get_parsed_diff(
            repo_path.clone(),
            file.clone(),
            false,
            DiffOptions::default(),
        )
        .expect("diff with whitespace shown");
        assert_eq!(shown.empty_reason, None, "the change is visible normally");

        let hidden = get_parsed_diff(
            repo_path.clone(),
            file.clone(),
            true,
            DiffOptions::default(),
        )
        .expect("diff with whitespace hidden");
        assert_eq!(
            hidden.empty_reason,
            Some(EmptyDiffReason::WhitespaceOnly),
            "an empty pane that names the setting hiding the change"
        );

        // A genuinely unchanged file must not borrow that caption.
        std::fs::write(repo.join("a.txt"), "fn main() {}\n").expect("restore");
        let restored =
            get_parsed_diff(repo_path, file, true, DiffOptions::default()).expect("diff");
        assert_eq!(restored.empty_reason, Some(EmptyDiffReason::NoChanges));
    }
}
