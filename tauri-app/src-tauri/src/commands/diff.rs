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
    pub text: String,
    pub content: String,
    pub line_type: LineType,
    pub old_line_no: Option<i32>,
    pub new_line_no: Option<i32>,
}

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

#[tauri::command]
pub fn parse_diff(raw: String) -> Option<FileDiff> {
    if raw.trim().is_empty() {
        return None;
    }

    let lines: Vec<&str> = raw.split('\n').collect();

    let mut old_path = String::new();
    let mut new_path = String::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut header_lines: Vec<String> = Vec::new();

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
                        text: line.to_string(),
                        content: line.to_string(),
                        line_type: LineType::Hunk,
                        old_line_no: None,
                        new_line_no: None,
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
                text: " ".to_string(),
                content: String::new(),
                line_type: LineType::Context,
                old_line_no: Some(old_line_no),
                new_line_no: Some(new_line_no),
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
                    text: line.to_string(),
                    content,
                    line_type: LineType::Add,
                    old_line_no: None,
                    new_line_no: Some(new_line_no),
                });
                new_line_no += 1;
            }
            b'-' => {
                let content = line[1..].to_string();
                hunk.lines.push(DiffLine {
                    text: line.to_string(),
                    content,
                    line_type: LineType::Delete,
                    old_line_no: Some(old_line_no),
                    new_line_no: None,
                });
                old_line_no += 1;
            }
            b'\\' => {
                // "\ No newline at end of file" — belongs inside the current hunk,
                // attached to whichever line it follows. Patch builder echoes it back.
                hunk.lines.push(DiffLine {
                    text: line.to_string(),
                    content: line.to_string(),
                    line_type: LineType::NoNewline,
                    old_line_no: None,
                    new_line_no: None,
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
                    text: line.to_string(),
                    content,
                    line_type: LineType::Context,
                    old_line_no: Some(old_line_no),
                    new_line_no: Some(new_line_no),
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

    if hunks.is_empty() {
        return None;
    }

    let file_header = header_lines.join("\n");

    Some(FileDiff {
        old_path,
        new_path,
        file_header,
        hunks,
    })
}

/// Strips the `--- `/`+++ ` argument down to a repo-relative path.
/// Removes the conventional `a/` or `b/` prefix git uses, and trims an
/// optional trailing timestamp (tab-separated) found in some diff outputs.
fn strip_path_prefix(rest: &str, prefix: &str) -> String {
    // Drop trailing timestamp if present (git's --raw format separates with TAB).
    let mut path = rest.split('\t').next().unwrap_or("").to_string();
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

#[tauri::command]
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

#[tauri::command]
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
        // Fallback: synthesise minimal headers so plain edits still apply.
        if !file_diff.old_path.is_empty() {
            patch.push_str(&format!("--- a/{}\n", file_diff.old_path));
        }
        if !file_diff.new_path.is_empty() {
            patch.push_str(&format!("+++ b/{}\n", file_diff.new_path));
        }
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
                    out_lines.push(line.text.clone());
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
