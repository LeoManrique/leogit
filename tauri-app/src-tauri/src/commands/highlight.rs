use crate::commands::diff::{FileDiff, LineType};
use crate::commands::git;
use serde::{Deserialize, Serialize};
use serde_repr::Serialize_repr;
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// Mirrors `MAX_INTRA_LINE_LEN` in diff.rs — skip tokenization for absurd lines
/// (minified blobs) so the parser never burns time on them.
const MAX_HIGHLIGHT_LINE_LEN: usize = 1024;

/// Above this many lines, tokenizing a whole blob stops being worth it. Measured
/// release-mode cost of `highlight_diff` end to end (both blobs, including the
/// `git show`): ~19ms for a 150-line file, ~139ms for an 1875-line one. Bail to
/// the diff-only path rather than spend seconds on a generated/minified file.
const MAX_HIGHLIGHT_FILE_LINES: u32 = 20_000;

/// Where to read the diff's old/new sides from, so each can be tokenized from
/// line 1. Mirrors the two views that produce a diff: uncommitted changes
/// (`get_diff`, HEAD vs working tree) and a committed diff (`get_commit_diff`,
/// `<sha>^` vs `<sha>`). Kept as an enum so the frontend states *what it is
/// looking at* and this module owns the rev-spec knowledge.
#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlobSource {
    /// Uncommitted changes: old side is HEAD, new side is the file on disk.
    #[serde(rename_all = "camelCase")]
    WorkingTree { repo_path: String },
    /// A committed diff: old side is the commit's first parent, new side is the
    /// commit itself.
    #[serde(rename_all = "camelCase")]
    Commit { repo_path: String, sha: String },
}

/// two-face's extended set covers ~200 languages (Svelte, Astro, Zig, etc.),
/// matching the breadth of `shiki/bundle/full` we replaced.
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// Compact enum index emitted per token. Frontend resolves the actual color via
/// CSS variables on `[data-theme]`, so theme swap costs zero work here.
#[repr(u8)]
#[derive(Serialize_repr, Clone, Copy, Debug)]
pub enum TokenClass {
    Plain = 0,
    Keyword = 1,
    String = 2,
    Comment = 3,
    Function = 4,
    Type = 5,
    Variable = 6,
    Number = 7,
    Constant = 8,
    Operator = 9,
    Punctuation = 10,
    Tag = 11,
    Attribute = 12,
    Builtin = 13,
    Decorator = 14,
}

#[derive(Serialize, Debug, Clone)]
pub struct Token {
    /// Code-point index into the line content (matches `IntraLineRange`).
    pub start: u32,
    /// Code-point index (exclusive) into the line content.
    pub end: u32,
    pub class: TokenClass,
}

pub type TokenLine = Vec<Token>;

/// Tokenize every `Context | Add | Delete` line in `file_diff` so the frontend
/// can render syntax-coloured spans without re-tokenizing on theme swap.
///
/// Hunk-header / `NoNewline` rows always return an empty `TokenLine`, preserving
/// 1:1 correspondence with the flattened `hunks[].lines[]` array on the JS side.
///
/// When `source` is supplied, the old and new blobs are each tokenized from
/// line 1 and the tokens mapped onto diff rows by line number. That is the only
/// way to get correct results: syntect is a stateful, line-sequential parser, so
/// a line's classification depends on every line above it. Parsing the diff's
/// own lines instead leaves the parser in whatever context a fresh `ScopeStack`
/// starts in — for a `.svelte` file that is top-level *markup*, so TypeScript
/// inside `<script lang="ts">` gets tokenized as markup (a `listen<string>`
/// generic comes back as an HTML tag). A markup hunk looks perfect and a script
/// hunk looks broken, purely by where the hunk lands.
///
/// Falls back to the legacy diff-only parse when `source` is absent or a blob
/// can't be read, so highlighting degrades rather than disappears.
///
/// `(async)` is load-bearing: a plain `#[tauri::command]` runs on the **main
/// thread**, and tokenizing a whole blob is far too slow for that — the repo's
/// largest file (git.rs, 3286 lines) costs ~52ms release / ~284ms debug, which
/// beachballs the cursor on every file switch. Reading only the diff's lines
/// used to be ~2ms, cheap enough to hide the problem. Same reasoning as the
/// network commands in git.rs/gh.rs.
// Tauri deserializes command arguments into owned values, so `file_diff` has to
// arrive by value even though everything below only reads it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
pub fn highlight_diff(file_diff: FileDiff, source: Option<BlobSource>) -> Vec<TokenLine> {
    let path = if file_diff.new_path.is_empty() {
        file_diff.old_path.as_str()
    } else {
        file_diff.new_path.as_str()
    };

    let syntax = resolve_language(path);
    let total_lines: usize = file_diff.hunks.iter().map(|h| h.lines.len()).sum();

    // No syntax match (binary, unknown extension): return empty token lines so
    // the frontend falls back to plain escaped text + intra-line overlay only.
    let Some(syntax) = syntax else {
        let mut out: Vec<TokenLine> = Vec::with_capacity(total_lines);
        out.resize_with(total_lines, Vec::new);
        return out;
    };

    if let Some(source) = source {
        if let Some(out) = highlight_from_blobs(&file_diff, syntax, &source) {
            return out;
        }
    }

    highlight_from_diff_lines(&file_diff, syntax)
}

/// Tokenize the old and new blobs from line 1 and map their tokens onto diff
/// rows: `Delete` rows take the old side, `Add`/`Context` rows the new side.
/// Because the two sides never share a `ParseState`, a deleted line's text can
/// no longer poison the line that replaced it.
///
/// Returns `None` when a blob this diff needs can't be read (or the file is too
/// large), so the caller can fall back.
fn highlight_from_blobs(
    file_diff: &FileDiff,
    syntax: &SyntaxReference,
    source: &BlobSource,
) -> Option<Vec<TokenLine>> {
    let (repo_path, old_rev, new_rev) = match source {
        BlobSource::WorkingTree { repo_path } => (repo_path.as_str(), "HEAD".to_string(), None),
        BlobSource::Commit { repo_path, sha } => {
            (repo_path.as_str(), format!("{sha}^"), Some(sha.clone()))
        }
    };

    // Which lines each side actually needs. A rename gives the two sides
    // different paths, hence the separate lookups below.
    let mut want_old: BTreeSet<u32> = BTreeSet::new();
    let mut want_new: BTreeSet<u32> = BTreeSet::new();
    for hunk in &file_diff.hunks {
        for line in &hunk.lines {
            match line.line_type {
                LineType::Delete => {
                    if let Some(n) = line.old_line_no {
                        want_old.insert(u32::try_from(n).ok()?);
                    }
                }
                LineType::Add | LineType::Context => {
                    if let Some(n) = line.new_line_no {
                        want_new.insert(u32::try_from(n).ok()?);
                    }
                }
                LineType::Hunk | LineType::NoNewline => {}
            }
        }
    }

    // An added file has no old side and a deleted file no new side; skipping the
    // read keeps those cases on the fast path instead of failing into fallback.
    let old_tokens = if want_old.is_empty() {
        HashMap::new()
    } else {
        let text = git::read_blob(repo_path, &old_rev, &file_diff.old_path).ok()?;
        tokenize_file(syntax, &text, &want_old)?
    };
    let new_tokens = if want_new.is_empty() {
        HashMap::new()
    } else {
        let text = match &new_rev {
            Some(sha) => git::read_blob(repo_path, sha, &file_diff.new_path).ok()?,
            None => git::read_working_tree_file(repo_path, &file_diff.new_path).ok()?,
        };
        tokenize_file(syntax, &text, &want_new)?
    };

    let total_lines: usize = file_diff.hunks.iter().map(|h| h.lines.len()).sum();
    let mut out: Vec<TokenLine> = Vec::with_capacity(total_lines);
    for hunk in &file_diff.hunks {
        for line in &hunk.lines {
            let tokens = match line.line_type {
                LineType::Delete => line
                    .old_line_no
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(|n| old_tokens.get(&n)),
                LineType::Add | LineType::Context => line
                    .new_line_no
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(|n| new_tokens.get(&n)),
                LineType::Hunk | LineType::NoNewline => None,
            };
            out.push(tokens.cloned().unwrap_or_default());
        }
    }
    Some(out)
}

/// Parse `text` from line 1 so scope state is correct everywhere, recording
/// tokens only for the `wanted` (1-based) line numbers. Parsing is the cost;
/// recording is nearly free, so `wanted` bounds the returned payload rather
/// than the work.
///
/// Returns `None` for files past `MAX_HIGHLIGHT_FILE_LINES`.
fn tokenize_file(
    syntax: &SyntaxReference,
    text: &str,
    wanted: &BTreeSet<u32>,
) -> Option<HashMap<u32, TokenLine>> {
    let last_wanted = *wanted.iter().next_back()?;
    if last_wanted > MAX_HIGHLIGHT_FILE_LINES {
        return None;
    }

    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();
    let mut out: HashMap<u32, TokenLine> = HashMap::with_capacity(wanted.len());

    for (idx, line) in text.lines().enumerate() {
        let line_no = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        // Nothing below the last wanted line can affect it — stop early.
        if line_no > last_wanted {
            break;
        }

        // syntect needs a trailing newline to terminate its line scanner; the
        // byte offsets are clamped back to the real line length in
        // `tokens_for_line`.
        let mut input = line.to_string();
        input.push('\n');

        let Ok(ops) = parse_state.parse_line(&input, &SYNTAX_SET) else {
            // Malformed line: keep the scope stack as-is and carry on, so one
            // bad line can't desync everything below it.
            continue;
        };

        // Long lines are still parsed — the parser state below them depends on
        // it — but not recorded, matching `MAX_HIGHLIGHT_LINE_LEN`'s intent.
        if wanted.contains(&line_no) && line.len() <= MAX_HIGHLIGHT_LINE_LEN {
            out.insert(line_no, tokens_for_line(line, &ops, &mut scope_stack));
        } else {
            // Roll the scope stack forward without building tokens.
            for (_, op) in &ops {
                let _ = scope_stack.apply(op);
            }
        }
    }

    Some(out)
}

/// Legacy path: parse the diff's own lines with one shared `ParseState`.
/// Only correct when the first hunk happens to start in the same context a
/// fresh `ScopeStack` does. Kept as a fallback for when blobs are unavailable
/// (no `source`, unreadable blob, oversized file).
fn highlight_from_diff_lines(file_diff: &FileDiff, syntax: &SyntaxReference) -> Vec<TokenLine> {
    let total_lines: usize = file_diff.hunks.iter().map(|h| h.lines.len()).sum();
    let mut out: Vec<TokenLine> = Vec::with_capacity(total_lines);

    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();

    for hunk in &file_diff.hunks {
        for line in &hunk.lines {
            match line.line_type {
                LineType::Hunk | LineType::NoNewline => {
                    out.push(Vec::new());
                    continue;
                }
                _ => {}
            }

            if line.content.len() > MAX_HIGHLIGHT_LINE_LEN {
                out.push(Vec::new());
                continue;
            }

            // syntect needs a trailing newline to terminate its line scanner;
            // we strip it back out of the byte offsets by clamping to
            // `line.content.len()`.
            let mut input = line.content.clone();
            input.push('\n');

            let Ok(ops) = parse_state.parse_line(&input, &SYNTAX_SET) else {
                out.push(Vec::new());
                continue;
            };

            out.push(tokens_for_line(&line.content, &ops, &mut scope_stack));
        }
    }

    out
}

/// Walks the parse ops emitted by syntect for a single line and produces a
/// dense `Vec<Token>` with code-point indices. `scope_stack` is mutated in
/// place so multi-line constructs (block comments, multi-line strings) keep
/// their state across lines, matching how syntect's `HighlightLines` threads
/// scope through a file.
fn tokens_for_line(
    line: &str,
    ops: &[(usize, syntect::parsing::ScopeStackOp)],
    scope_stack: &mut ScopeStack,
) -> TokenLine {
    let mut tokens: TokenLine = Vec::new();
    let mut cursor_byte = 0usize;
    let mut cursor_char = 0u32;
    // `prev_class` is the class for the byte range `[cursor_byte, op_byte)`,
    // captured BEFORE applying each op so the token reflects the scope stack
    // that was active when those bytes were emitted.
    let mut prev_class = scope_to_class(scope_stack);
    let line_byte_len = line.len();

    for (op_byte, op) in ops {
        let segment_end_byte = (*op_byte).min(line_byte_len);
        if segment_end_byte > cursor_byte {
            let segment = &line[cursor_byte..segment_end_byte];
            let segment_char_len = u32::try_from(segment.chars().count()).unwrap_or(u32::MAX);
            push_token(
                &mut tokens,
                cursor_char,
                cursor_char + segment_char_len,
                prev_class,
            );
            cursor_byte = segment_end_byte;
            cursor_char += segment_char_len;
        }
        // Apply op to roll the scope stack forward for the next segment.
        if scope_stack.apply(op).is_err() {
            // Malformed scope op — keep going, just don't trust this op.
            continue;
        }
        prev_class = scope_to_class(scope_stack);
    }

    // Trailing segment up to end-of-line.
    if cursor_byte < line_byte_len {
        let segment = &line[cursor_byte..line_byte_len];
        let segment_char_len = u32::try_from(segment.chars().count()).unwrap_or(u32::MAX);
        push_token(
            &mut tokens,
            cursor_char,
            cursor_char + segment_char_len,
            prev_class,
        );
    }

    tokens
}

/// Coalesces consecutive same-class tokens to keep the payload small.
fn push_token(tokens: &mut TokenLine, start: u32, end: u32, class: TokenClass) {
    if end == start {
        return;
    }
    if let Some(last) = tokens.last_mut() {
        if last.end == start && last.class as u8 == class as u8 {
            last.end = end;
            return;
        }
    }
    tokens.push(Token { start, end, class });
}

/// Scope prefix -> class, MOST-SPECIFIC FIRST: `Scope::is_prefix_of` matches
/// atom-wise, so `constant` would swallow `constant.numeric` if it came first.
///
/// Matching on `Scope` rather than a formatted string keeps `scope_to_class`
/// allocation-free — it runs once per token per scope, which is the hottest
/// loop in this module.
static SCOPE_TABLE: LazyLock<Vec<(Scope, TokenClass)>> = LazyLock::new(|| {
    let s = |name: &str| Scope::new(name).expect("static scope selector must parse");
    vec![
        (s("comment"), TokenClass::Comment),
        (s("string"), TokenClass::String),
        (s("constant.numeric"), TokenClass::Number),
        (s("constant.language"), TokenClass::Constant),
        (s("constant.character"), TokenClass::Constant),
        (s("support.constant"), TokenClass::Builtin),
        (s("support.variable"), TokenClass::Builtin),
        (s("constant"), TokenClass::Constant),
        (s("keyword.operator"), TokenClass::Operator),
        (s("punctuation.operator"), TokenClass::Operator),
        // VS Code / GitHub themes colour `storage.*` (storage.type,
        // storage.modifier, storage.type.function, …) with the keyword colour.
        // Bare type NAMES live under `entity.name.type` / `support.type`.
        (s("keyword"), TokenClass::Keyword),
        (s("storage"), TokenClass::Keyword),
        (s("entity.name.function"), TokenClass::Function),
        (s("support.function"), TokenClass::Function),
        (s("entity.name.type"), TokenClass::Type),
        (s("entity.name.class"), TokenClass::Type),
        (s("entity.name.struct"), TokenClass::Type),
        (s("entity.name.enum"), TokenClass::Type),
        (s("entity.name.interface"), TokenClass::Type),
        (s("support.type"), TokenClass::Type),
        (s("support.class"), TokenClass::Type),
        (s("entity.name.tag"), TokenClass::Tag),
        (s("entity.other.attribute-name"), TokenClass::Attribute),
        (s("entity.name.decorator"), TokenClass::Decorator),
        (s("meta.decorator"), TokenClass::Decorator),
        (s("variable"), TokenClass::Variable),
        (s("punctuation"), TokenClass::Punctuation),
    ]
});

/// Classify a single scope, or `None` when nothing in the table matches (e.g.
/// `source.ts`, `meta.var.expr`) so the caller can keep looking down the stack.
fn classify_scope(scope: Scope) -> Option<TokenClass> {
    SCOPE_TABLE
        .iter()
        .find(|(prefix, _)| prefix.is_prefix_of(scope))
        .map(|(_, class)| *class)
}

/// Maps a scope stack to one of our compact `TokenClass` indices.
///
/// Walks from the leaf DOWN rather than reading only the top scope, because a
/// delimiter carries its container beneath it: `//` is
/// `[source.ts, comment.line.double-slash.ts, punctuation.definition.comment.ts]`.
/// Reading only the leaf classifies that as `Punctuation`, so every line comment
/// rendered two-tone — an uncoloured `//` followed by an italic body. Same for
/// string quotes.
///
/// Punctuation is therefore treated as *transparent*: it's remembered as a
/// fallback but we keep descending, so `//` picks up `Comment` from the scope
/// below it. Leaf-first order still means a genuinely nested token wins — the
/// `name` in `` `hi ${name}` `` has `variable.other` above `string.template`, so
/// it classifies as `Variable`, not `String`.
fn scope_to_class(stack: &ScopeStack) -> TokenClass {
    let mut delimiter: Option<TokenClass> = None;
    for scope in stack.scopes.iter().rev() {
        let Some(class) = classify_scope(*scope) else {
            continue;
        };
        if matches!(class, TokenClass::Punctuation) {
            delimiter.get_or_insert(class);
            continue;
        }
        return class;
    }
    // A bare separator (a comma, a brace) has no container to inherit from.
    delimiter.unwrap_or(TokenClass::Plain)
}

/// Resolves a file path to a syntect `SyntaxReference`. Replaces the frontend
/// `getLanguageFromPath` extension map — syntect's lookup is keyed on extension
/// AND first-line patterns (shebangs, XML declarations), so we get broader
/// detection for free.
fn resolve_language(path: &str) -> Option<&'static SyntaxReference> {
    if path.is_empty() {
        return None;
    }
    let ext = path.rsplit('.').next().unwrap_or("");
    if ext.is_empty() || ext == path {
        // No extension — fall through to filename-based lookup below.
    } else if let Some(syntax) = SYNTAX_SET.find_syntax_by_extension(ext) {
        return Some(syntax);
    }
    // Try by token (full filename without directories) — handles `Makefile`,
    // `Dockerfile`, etc.
    let filename = path.rsplit('/').next().unwrap_or(path);
    SYNTAX_SET.find_syntax_by_token(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::diff::{DiffLine, Hunk, HunkHeader};
    use std::fmt::Write as _;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn rust_diff() -> FileDiff {
        let hunk = Hunk {
            header: HunkHeader {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 2,
            },
            lines: vec![
                DiffLine {
                    text: "@@ -1 +1,2 @@".into(),
                    content: "@@ -1 +1,2 @@".into(),
                    line_type: LineType::Hunk,
                    old_line_no: None,
                    new_line_no: None,
                    intra_line_diff: None,
                },
                DiffLine {
                    text: "+fn main() {}".into(),
                    content: "fn main() {}".into(),
                    line_type: LineType::Add,
                    old_line_no: None,
                    new_line_no: Some(1),
                    intra_line_diff: None,
                },
                DiffLine {
                    text: "+let x = \"hi\";".into(),
                    content: "let x = \"hi\";".into(),
                    line_type: LineType::Add,
                    old_line_no: None,
                    new_line_no: Some(2),
                    intra_line_diff: None,
                },
            ],
        };
        FileDiff {
            old_path: "main.rs".into(),
            new_path: "main.rs".into(),
            file_header: String::new(),
            hunks: vec![hunk],
            is_binary: false,
        }
    }

    #[test]
    fn highlight_diff_emits_keyword_tokens_for_rust() {
        let diff = rust_diff();
        let lines = highlight_diff(diff, None);
        assert_eq!(lines.len(), 3, "one token line per diff row");
        assert!(lines[0].is_empty(), "hunk header has no tokens");
        let has_keyword = lines[1]
            .iter()
            .any(|t| matches!(t.class, TokenClass::Keyword));
        assert!(has_keyword, "expected at least one keyword in `fn main()`");
        let has_function = lines[1]
            .iter()
            .any(|t| matches!(t.class, TokenClass::Function));
        assert!(has_function, "expected the function-name token in `fn main()`");
        let has_string = lines[2]
            .iter()
            .any(|t| matches!(t.class, TokenClass::String));
        assert!(has_string, "expected at least one string token in `let x = \"hi\"`");
    }

    #[test]
    fn highlight_diff_unknown_extension_returns_empty_token_lines() {
        let mut diff = rust_diff();
        diff.new_path = "data.bin".into();
        diff.old_path = "data.bin".into();
        let lines = highlight_diff(diff, None);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(Vec::is_empty));
    }

    // -----------------------------------------------------------------------
    // Blob-sourced highlighting
    // -----------------------------------------------------------------------

    fn init_repo(dir: &Path) {
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .current_dir(dir)
                .args(args)
                .status()
                .expect("spawn git")
                .success();
            assert!(ok, "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test User"]);
        git(&["config", "commit.gpgsign", "false"]);
    }

    fn git_in(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .expect("spawn git")
            .success();
        assert!(ok, "git {args:?} failed");
    }

    /// A .svelte file whose script block opens on line 1. The lines the diff
    /// touches sit deep inside `<script lang="ts">`, which is exactly the shape
    /// that the diff-only parser gets wrong.
    fn svelte_file(comment: &str) -> String {
        let mut s = String::from("<script lang=\"ts\">\n");
        for i in 0..40 {
            writeln!(s, "  const filler{i} = {i}").expect("write to String");
        }
        writeln!(s, "  // {comment}").expect("write to String");
        s.push_str("  const value: string = 'hello'\n");
        s.push_str("</script>\n\n<div>markup</div>\n");
        s
    }

    fn real_diff(repo: &Path, path: &str) -> FileDiff {
        let out = Command::new("git")
            .current_dir(repo)
            .args(["diff", "--no-ext-diff", "--no-color", "HEAD", "--", path])
            .output()
            .expect("git diff");
        let raw = String::from_utf8_lossy(&out.stdout).to_string();
        crate::commands::diff::parse_diff(raw).expect("diff must parse")
    }

    /// The regression that motivated blob-sourced highlighting: a hunk landing
    /// inside `<script lang="ts">` must be tokenized as TypeScript. Parsing the
    /// diff's own lines starts the parser in top-level markup context (the state
    /// a fresh `ScopeStack` has), so the comment never gets `Comment`. Reading
    /// the blob from line 1 lets the parser enter the script context first.
    #[test]
    fn blob_source_highlights_typescript_inside_a_svelte_script_block() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        fs::write(repo.join("C.svelte"), svelte_file("before")).expect("write");
        git_in(repo, &["add", "C.svelte"]);
        git_in(repo, &["commit", "-qm", "add"]);

        // Touch only lines deep inside the script block.
        fs::write(repo.join("C.svelte"), svelte_file("the shell exited on its own")).expect("edit");

        let diff = real_diff(repo, "C.svelte");
        let repo_path = repo.to_str().expect("utf-8 path").to_string();

        let added_comment = |lines: &[TokenLine], diff: &FileDiff| -> TokenLine {
            let mut flat = diff.hunks.iter().flat_map(|h| &h.lines);
            let idx = flat
                .position(|l| {
                    matches!(l.line_type, LineType::Add) && l.content.contains("shell exited")
                })
                .expect("the added comment line must be in the diff");
            lines[idx].clone()
        };

        // Legacy diff-only path: the parser never sees `<script lang="ts">`.
        let legacy = highlight_diff(diff.clone(), None);
        let legacy_comment = added_comment(&legacy, &diff);
        assert!(
            !legacy_comment
                .iter()
                .any(|t| matches!(t.class, TokenClass::Comment)),
            "baseline: diff-only parsing must NOT classify the comment (this is the bug)"
        );

        // Blob-sourced path: parsed from line 1, so the script context is live.
        let fixed = highlight_diff(
            diff.clone(),
            Some(BlobSource::WorkingTree {
                repo_path: repo_path.clone(),
            }),
        );
        let fixed_comment = added_comment(&fixed, &diff);
        assert!(
            fixed_comment
                .iter()
                .any(|t| matches!(t.class, TokenClass::Comment)),
            "blob-sourced parsing must classify the TS comment inside <script>, got {fixed_comment:?}"
        );

        // The `const value: string` context line must read as TypeScript too.
        let flat: Vec<_> = diff.hunks.iter().flat_map(|h| &h.lines).collect();
        let typed_idx = flat
            .iter()
            .position(|l| l.content.contains("const value"))
            .expect("the typed line must be in the diff context");
        assert!(
            fixed[typed_idx]
                .iter()
                .any(|t| matches!(t.class, TokenClass::Keyword)),
            "`const` must be a Keyword under blob-sourced parsing, got {:?}",
            fixed[typed_idx]
        );
    }

    /// Delete and Add lines must be tokenized from their own side's blob, so an
    /// unterminated construct on a deleted line can't bleed into the lines that
    /// follow it (the diff-only parser turns the rest of the diff into String).
    #[test]
    fn blob_source_keeps_a_deleted_unterminated_string_from_bleeding() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);

        let before = "const s = `unterminated\ncontinues`;\nconst n = 42;\nfunction later() {}\n";
        fs::write(repo.join("a.ts"), before).expect("write");
        git_in(repo, &["add", "a.ts"]);
        git_in(repo, &["commit", "-qm", "add"]);

        let after = "const s = 'ok';\nconst m = 1;\nconst n = 42;\nfunction later() {}\n";
        fs::write(repo.join("a.ts"), after).expect("edit");

        let diff = real_diff(repo, "a.ts");
        let repo_path = repo.to_str().expect("utf-8 path").to_string();
        let lines = highlight_diff(
            diff.clone(),
            Some(BlobSource::WorkingTree { repo_path }),
        );

        let flat: Vec<_> = diff.hunks.iter().flat_map(|h| &h.lines).collect();
        let later_idx = flat
            .iter()
            .position(|l| l.content.contains("function later"))
            .expect("the trailing line must be in the diff");
        assert!(
            lines[later_idx]
                .iter()
                .any(|t| matches!(t.class, TokenClass::Keyword)),
            "`function later() {{}}` must stay real code, not bleed into String, got {:?}",
            lines[later_idx]
        );
    }

    /// An added file has no old blob. That must stay on the blob path (there are
    /// no Delete lines to serve) rather than failing into the legacy fallback.
    #[test]
    fn blob_source_handles_a_newly_added_file() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path();
        init_repo(repo);
        fs::write(repo.join("seed.txt"), "seed\n").expect("write seed");
        git_in(repo, &["add", "seed.txt"]);
        git_in(repo, &["commit", "-qm", "seed"]);

        fs::write(repo.join("b.ts"), "const x: number = 1;\n").expect("write");
        git_in(repo, &["add", "b.ts"]);

        let diff = real_diff(repo, "b.ts");
        let repo_path = repo.to_str().expect("utf-8 path").to_string();
        let lines = highlight_diff(diff, Some(BlobSource::WorkingTree { repo_path }));
        assert!(
            lines.iter().flatten().any(|t| matches!(t.class, TokenClass::Keyword)),
            "an added file must still highlight from its new-side blob"
        );
    }

    /// Tokenize a standalone snippet from line 1, mirroring what the blob path
    /// does, and return `(text, class)` pairs for one line.
    fn classes_for(path: &str, source: &str, line_no: u32) -> Vec<(String, TokenClass)> {
        let syntax = resolve_language(path).expect("syntax must resolve");
        let wanted: BTreeSet<u32> = std::iter::once(line_no).collect();
        let tokens = tokenize_file(syntax, source, &wanted).expect("tokenize");
        let line = source.lines().nth(line_no as usize - 1).expect("line exists");
        let chars: Vec<char> = line.chars().collect();
        tokens
            .get(&line_no)
            .expect("wanted line must be recorded")
            .iter()
            .map(|t| {
                (
                    chars[t.start as usize..t.end as usize].iter().collect(),
                    t.class,
                )
            })
            .collect()
    }

    /// A comment's `//` must colour as Comment, not Punctuation. The delimiter
    /// sits ABOVE `comment.line` on the scope stack, so reading only the leaf
    /// scope rendered every comment two-tone.
    #[test]
    fn comment_delimiter_takes_the_comment_class() {
        let src = "const a = 1;\n// hello world\n";
        let classes = classes_for("a.ts", src, 2);
        assert!(
            classes
                .iter()
                .all(|(_, c)| matches!(c, TokenClass::Comment)),
            "the whole comment line including `//` must be Comment, got {classes:?}"
        );
    }

    /// Likewise a string's quotes belong to the string.
    #[test]
    fn string_delimiters_take_the_string_class() {
        let src = "const s = \"hi\";\n";
        let classes = classes_for("a.ts", src, 1);
        let quote = classes
            .iter()
            .find(|(text, _)| text.contains('"'))
            .expect("a token covering a quote");
        assert!(
            matches!(quote.1, TokenClass::String),
            "string quotes must be String, got {quote:?}"
        );
    }

    /// Guards the subtle case a naive "scan the stack for `string`" fix breaks:
    /// an interpolated expression inside a template literal is real code and
    /// must NOT inherit String. Its own scope sits above `string.template`, so
    /// leaf-first descent keeps it correct.
    #[test]
    fn interpolated_expression_does_not_inherit_the_string_class() {
        let src = "const name = 'x';\nconst msg = `hi ${name} there`;\n";
        let classes = classes_for("a.ts", src, 2);
        let interpolated = classes
            .iter()
            .find(|(text, _)| text == "name")
            .expect("the interpolated `name` must be its own token");
        assert!(
            !matches!(interpolated.1, TokenClass::String),
            "`${{name}}` is code, not string content, got {interpolated:?}"
        );
        // The surrounding literal text is still a string.
        assert!(
            classes
                .iter()
                .any(|(text, c)| text.contains("hi") && matches!(c, TokenClass::String)),
            "literal template text must stay String, got {classes:?}"
        );
    }

    /// A bare separator has no container to inherit from and stays Punctuation.
    #[test]
    fn bare_separator_stays_punctuation() {
        let src = "const xs = [1, 2];\n";
        let classes = classes_for("a.ts", src, 1);
        assert!(
            classes
                .iter()
                .any(|(text, c)| text.contains(',') && matches!(c, TokenClass::Punctuation)),
            "a bare comma has no container and must stay Punctuation, got {classes:?}"
        );
    }

    /// A bad repo path must degrade to the legacy path, never panic or blank the
    /// diff entirely.
    #[test]
    fn unreadable_blob_falls_back_to_diff_only_highlighting() {
        let diff = rust_diff();
        let lines = highlight_diff(
            diff,
            Some(BlobSource::WorkingTree {
                repo_path: "/nonexistent/repo/path".to_string(),
            }),
        );
        assert_eq!(lines.len(), 3);
        assert!(
            lines[1].iter().any(|t| matches!(t.class, TokenClass::Keyword)),
            "fallback must still produce the legacy diff-only tokens"
        );
    }
}
