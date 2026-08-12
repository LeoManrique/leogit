//! Renders parsed diff lines into the pre-escaped HTML the viewer drops into
//! `{@html}`. Two producers share this module so the two render phases can
//! never drift apart:
//!
//! - `parse_diff` emits the phase-1 HTML (plain escaped text + intra-line
//!   backplate) in the same payload as the parse, so the viewer can paint it
//!   in the same frame it receives the diff.
//! - `highlight_diff` re-emits every line with syntect token spans (`.syn-*`
//!   classes) layered over the same backplate.
//!
//! Output entries correspond 1:1 with the flattened `hunks[].lines[]` array —
//! the same global indexing the side-by-side pairs and the selection map use.

use super::diff::{FileDiff, IntraLineRange, LineType};
use super::highlight::{Token, TokenClass};

/// Phase-1 HTML: every line as plain escaped text, with only the intra-line
/// add/remove backplate applied.
pub(crate) fn plain_html(file_diff: &FileDiff) -> Vec<String> {
    render_lines(file_diff, |_| None)
}

/// Phase-2 HTML: `tokens[i]` (from the syntect tokenizer) laid over line `i`'s
/// backplate. An empty token line falls back to one plain span, so lines the
/// tokenizer skipped (hunk headers, `NoNewline`, unmatched languages) still
/// render exactly like phase 1.
pub(crate) fn highlighted_html(file_diff: &FileDiff, tokens: &[Vec<Token>]) -> Vec<String> {
    render_lines(file_diff, |i| tokens.get(i).map(Vec::as_slice))
}

fn render_lines<'a>(
    file_diff: &FileDiff,
    tokens_for: impl Fn(usize) -> Option<&'a [Token]>,
) -> Vec<String> {
    let total_lines: usize = file_diff.hunks.iter().map(|h| h.lines.len()).sum();
    let mut out = Vec::with_capacity(total_lines);
    let mut i = 0;
    for hunk in &file_diff.hunks {
        for line in &hunk.lines {
            // The backplate class only exists for changed lines; context and
            // header lines never carry an intra-line range in the first place.
            let intra_class = match line.line_type {
                LineType::Add => "diff-intra-add",
                LineType::Delete => "diff-intra-remove",
                _ => "",
            };
            let overlay = line
                .intra_line_diff
                .filter(|r| r.length > 0 && !intra_class.is_empty())
                .map(|r| (r, intra_class));
            out.push(render_line(&line.content, tokens_for(i), overlay));
            i += 1;
        }
    }
    out
}

/// Walks `content`'s code points and emits class-tagged spans for each token,
/// splitting spans around the intra-line overlay range so the overlap gets the
/// backplate class layered on top of the syntax class. Token indices are code
/// points, matching the tokenizer and `IntraLineRange`.
fn render_line(
    content: &str,
    tokens: Option<&[Token]>,
    overlay: Option<(IntraLineRange, &str)>,
) -> String {
    let chars: Vec<char> = content.chars().collect();

    // No tokens (no language match, hunk header, or phase 1) — treat the whole
    // line as one plain "token" so the intra-line overlay still applies.
    let plain_line;
    let tokens = match tokens {
        Some(t) if !t.is_empty() => t,
        _ => {
            plain_line = [Token {
                start: 0,
                end: u32::try_from(chars.len()).unwrap_or(u32::MAX),
                class: TokenClass::Plain,
            }];
            &plain_line
        }
    };

    let mut out = String::new();
    let mut cursor = 0usize;
    for tok in tokens {
        // Defensive clamping: a malformed token must never leak past the line
        // bounds or run backwards.
        let tok_start = (tok.start as usize).clamp(cursor, chars.len());
        let tok_end = (tok.end as usize).clamp(tok_start, chars.len());
        if tok_start > cursor {
            // Gap between tokens — render as plain (rarely happens; insurance).
            render_slice(&chars, cursor, tok_start, "", overlay, &mut out);
        }
        render_slice(
            &chars,
            tok_start,
            tok_end,
            css_class(tok.class),
            overlay,
            &mut out,
        );
        cursor = tok_end;
    }
    if cursor < chars.len() {
        render_slice(&chars, cursor, chars.len(), "", overlay, &mut out);
    }
    out
}

/// Emits `chars[start..end]` split around the overlay range so the overlap
/// gets the backplate class layered on top of `base_class`.
fn render_slice(
    chars: &[char],
    start: usize,
    end: usize,
    base_class: &str,
    overlay: Option<(IntraLineRange, &str)>,
    out: &mut String,
) {
    if end <= start {
        return;
    }
    let Some((intra, intra_class)) = overlay else {
        emit_span(&chars[start..end], base_class, out);
        return;
    };
    let overlap_start = start.max(intra.start);
    let overlap_end = end.min(intra.start + intra.length);
    if overlap_start >= overlap_end {
        emit_span(&chars[start..end], base_class, out);
        return;
    }
    let merged = if base_class.is_empty() {
        intra_class.to_string()
    } else {
        format!("{base_class} {intra_class}")
    };
    if overlap_start > start {
        emit_span(&chars[start..overlap_start], base_class, out);
    }
    emit_span(&chars[overlap_start..overlap_end], &merged, out);
    if end > overlap_end {
        emit_span(&chars[overlap_end..end], base_class, out);
    }
}

fn emit_span(chars: &[char], classes: &str, out: &mut String) {
    if chars.is_empty() {
        return;
    }
    if classes.is_empty() {
        escape_into(chars, out);
        return;
    }
    out.push_str("<span class=\"");
    out.push_str(classes);
    out.push_str("\">");
    escape_into(chars, out);
    out.push_str("</span>");
}

/// HTML-escapes text destined for element content (`&`, `<`, `>` — quotes need
/// no escaping outside attribute values, and class names are our own).
fn escape_into(chars: &[char], out: &mut String) {
    for &c in chars {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// CSS class per token class. The frontend resolves the actual colours via CSS
/// variables on `[data-theme]`, so a theme swap re-renders nothing here.
/// `Plain`, `Variable`, and `Punctuation` intentionally map to no class — they
/// take the line's default foreground.
fn css_class(class: TokenClass) -> &'static str {
    match class {
        TokenClass::Plain | TokenClass::Variable | TokenClass::Punctuation => "",
        TokenClass::Keyword => "syn-keyword",
        TokenClass::String => "syn-string",
        TokenClass::Comment => "syn-comment",
        TokenClass::Function => "syn-function",
        TokenClass::Type => "syn-type",
        TokenClass::Number => "syn-number",
        TokenClass::Constant => "syn-constant",
        TokenClass::Operator => "syn-operator",
        TokenClass::Tag => "syn-tag",
        TokenClass::Attribute => "syn-attribute",
        TokenClass::Builtin => "syn-builtin",
        TokenClass::Decorator => "syn-decorator",
        TokenClass::Heading => "syn-heading",
        TokenClass::Strong => "syn-strong",
        TokenClass::Emphasis => "syn-emphasis",
        TokenClass::Strikethrough => "syn-strike",
        TokenClass::Link => "syn-link",
        TokenClass::Raw => "syn-raw",
        TokenClass::Quote => "syn-quote",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(start: u32, end: u32, class: TokenClass) -> Token {
        Token { start, end, class }
    }

    #[test]
    fn plain_line_is_escaped_text_only() {
        assert_eq!(
            render_line("a < b && c > d", None, None),
            "a &lt; b &amp;&amp; c &gt; d"
        );
    }

    #[test]
    fn tokens_emit_class_tagged_spans_and_plain_gaps() {
        let tokens = [
            token(0, 2, TokenClass::Keyword),
            token(3, 4, TokenClass::Plain),
        ];
        assert_eq!(
            render_line("if x", Some(&tokens), None),
            "<span class=\"syn-keyword\">if</span> x"
        );
    }

    #[test]
    fn overlay_splits_spans_and_merges_classes() {
        let overlay = Some((
            IntraLineRange {
                start: 2,
                length: 3,
            },
            "diff-intra-add",
        ));
        // Whole line is one string token; the middle 3 chars get the backplate
        // layered on top of the syntax class.
        let tokens = [token(0, 7, TokenClass::String)];
        assert_eq!(
            render_line("abXYZcd", Some(&tokens), overlay),
            "<span class=\"syn-string\">ab</span>\
             <span class=\"syn-string diff-intra-add\">XYZ</span>\
             <span class=\"syn-string\">cd</span>"
        );
        // Without tokens the overlap still gets the backplate, unclassed
        // outside it.
        assert_eq!(
            render_line("abXYZcd", None, overlay),
            "ab<span class=\"diff-intra-add\">XYZ</span>cd"
        );
    }

    #[test]
    fn indices_are_code_points_not_bytes() {
        // "héllo" — the range must count the accented char as one unit.
        let overlay = Some((
            IntraLineRange {
                start: 1,
                length: 2,
            },
            "diff-intra-remove",
        ));
        assert_eq!(
            render_line("héllo", None, overlay),
            "h<span class=\"diff-intra-remove\">él</span>lo"
        );
    }

    #[test]
    fn malformed_token_bounds_are_clamped() {
        // End past the line, start before the cursor — must not panic and must
        // still cover the whole line exactly once.
        let tokens = [
            token(0, 3, TokenClass::Keyword),
            token(1, 99, TokenClass::String),
        ];
        assert_eq!(
            render_line("abcdef", Some(&tokens), None),
            "<span class=\"syn-keyword\">abc</span><span class=\"syn-string\">def</span>"
        );
    }
}
