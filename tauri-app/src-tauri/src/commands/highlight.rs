use crate::commands::diff::{FileDiff, LineType};
use lazy_static::lazy_static;
use serde::Serialize;
use serde_repr::Serialize_repr;
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

/// Mirrors `MAX_INTRA_LINE_LEN` in diff.rs — skip tokenization for absurd lines
/// (minified blobs) so the parser never burns time on them.
const MAX_HIGHLIGHT_LINE_LEN: usize = 1024;

lazy_static! {
    /// two-face's extended set covers ~200 languages (Svelte, Astro, Zig, etc.),
    /// matching the breadth of `shiki/bundle/full` we replaced.
    static ref SYNTAX_SET: SyntaxSet = two_face::syntax::extra_newlines();
}

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

#[derive(Serialize, Debug)]
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
/// Hunk-header / NoNewline rows always return an empty `TokenLine`, preserving
/// 1:1 correspondence with the flattened `hunks[].lines[]` array on the JS side.
#[tauri::command]
pub fn highlight_diff(file_diff: FileDiff) -> Vec<TokenLine> {
    let path = if !file_diff.new_path.is_empty() {
        file_diff.new_path.as_str()
    } else {
        file_diff.old_path.as_str()
    };

    let syntax = resolve_language(path);
    let total_lines: usize = file_diff.hunks.iter().map(|h| h.lines.len()).sum();
    let mut out: Vec<TokenLine> = Vec::with_capacity(total_lines);

    // No syntax match (binary, unknown extension): return empty token lines so
    // the frontend falls back to plain escaped text + intra-line overlay only.
    let Some(syntax) = syntax else {
        out.resize_with(total_lines, Vec::new);
        return out;
    };

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

            let ops = match parse_state.parse_line(&input, &SYNTAX_SET) {
                Ok(ops) => ops,
                Err(_) => {
                    out.push(Vec::new());
                    continue;
                }
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
            let segment_char_len = segment.chars().count() as u32;
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
        let segment_char_len = segment.chars().count() as u32;
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

/// Maps the top scope on the stack to one of our compact `TokenClass` indices.
/// Order matters: more-specific prefixes must come before less-specific ones
/// (e.g. `entity.name.function` before `entity.name`).
fn scope_to_class(stack: &ScopeStack) -> TokenClass {
    let Some(scope) = stack.scopes.last() else {
        return TokenClass::Plain;
    };
    let s = format!("{}", scope);

    // Most-specific first.
    if s.starts_with("comment") {
        return TokenClass::Comment;
    }
    if s.starts_with("string") {
        return TokenClass::String;
    }
    if s.starts_with("constant.numeric") {
        return TokenClass::Number;
    }
    if s.starts_with("constant") {
        return TokenClass::Constant;
    }
    if s.starts_with("keyword.operator") || s.starts_with("punctuation.operator") {
        return TokenClass::Operator;
    }
    // VS Code / GitHub themes colour `storage.*` (storage.type, storage.modifier,
    // storage.type.function, storage.type.let, …) with the keyword colour. Bare
    // type NAMES live under `entity.name.type` / `support.type`, handled below.
    if s.starts_with("keyword") || s.starts_with("storage") {
        return TokenClass::Keyword;
    }
    if s.starts_with("entity.name.function") || s.starts_with("support.function") {
        return TokenClass::Function;
    }
    if s.starts_with("entity.name.type")
        || s.starts_with("entity.name.class")
        || s.starts_with("entity.name.struct")
        || s.starts_with("entity.name.enum")
        || s.starts_with("entity.name.interface")
        || s.starts_with("support.type")
        || s.starts_with("support.class")
    {
        return TokenClass::Type;
    }
    if s.starts_with("entity.name.tag") {
        return TokenClass::Tag;
    }
    if s.starts_with("entity.other.attribute-name") {
        return TokenClass::Attribute;
    }
    if s.starts_with("entity.name.decorator") || s.starts_with("meta.decorator") {
        return TokenClass::Decorator;
    }
    if s.starts_with("support.constant") || s.starts_with("support.variable") {
        return TokenClass::Builtin;
    }
    if s.starts_with("variable.parameter") || s.starts_with("variable.other") {
        return TokenClass::Variable;
    }
    if s.starts_with("variable") {
        return TokenClass::Variable;
    }
    if s.starts_with("punctuation") {
        return TokenClass::Punctuation;
    }
    TokenClass::Plain
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
        let lines = highlight_diff(diff);
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
        let lines = highlight_diff(diff);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.is_empty()));
    }
}
