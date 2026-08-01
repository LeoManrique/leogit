//! Parser for the progress lines git writes to stderr during network work.
//!
//! Modeled on GitHub Desktop's step-weighted approach: each operation
//! ([`GitOp`]) walks a fixed sequence of weighted steps ("Compressing
//! objects", "Writing objects", …). A stderr line that reports progress for
//! one of those steps maps onto an aggregate 0.0..=1.0 fraction for the whole
//! operation; every other line is "context" — it leaves the bar where it is
//! but still carries text worth showing. The fraction never decreases over
//! the parser's lifetime, so the bar can only move forward.
//!
//! The caller is expected to split git's stderr on `\r` and `\n` and feed the
//! resulting lines to [`GitProgressParser::parse_line`] one at a time.

/// Which git network operation is being observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOp {
    Push,
    Pull,
    Clone,
}

/// One parsed unit of progress: the aggregate fraction plus display text.
#[derive(Debug, Clone, PartialEq)]
pub struct GitProgress {
    /// Aggregate progress across all steps, 0.0..=1.0, monotonically non-decreasing.
    pub fraction: f32,
    /// The raw (trimmed) stderr line, e.g. `"Writing objects:  53% (531/1000), 1.2 MiB | 500 KiB/s"`.
    pub text: String,
}

/// One expected step of an operation, e.g. "Writing objects" during a push.
#[derive(Debug, Clone, Copy)]
struct Step {
    /// The line titles git prints for this step, matched exactly. More than
    /// one entry covers renames across git versions — checkout progress says
    /// "Updating files" since git 2.25 and "Checking out files" before.
    titles: &'static [&'static str],
    /// This step's share of the whole operation; all steps sum to 1.0.
    weight: f32,
}

/// The progress payload of a step line: either counts with a total, or a bare
/// running count. Later comma-parts ("1.2 MiB | 500 KiB/s", "done.") are
/// informational only — completion is already encoded in value/total.
#[derive(Debug, Clone, Copy)]
enum Counts {
    /// `"N% (value/total)"` — completed vs. total object counts.
    Ratio { value: u64, total: u64 },
    /// A bare integer — a running count with no known total.
    ValueOnly,
}

/// Stateful parser that folds a stream of git stderr lines into a
/// forward-only progress fraction for one [`GitOp`].
#[derive(Debug, Clone)]
pub struct GitProgressParser {
    /// The op's expected steps, weights normalized to sum 1.0.
    steps: Vec<Step>,
    /// Index of the furthest step reported so far; lines for earlier steps
    /// are treated as context so progress never rewinds.
    step_index: usize,
    /// Last fraction returned; `parse_line` never returns less than this.
    fraction: f32,
}

impl GitProgressParser {
    /// Create a parser for `op`, normalizing its step weights to sum 1.0.
    #[must_use]
    pub fn new(op: GitOp) -> Self {
        /// Checkout progress across git versions: ≥2.25 prints the first,
        /// older gits the second.
        const CHECKOUT: &[&str] = &["Updating files", "Checking out files"];
        let raw: &[(&'static [&'static str], f32)] = match op {
            GitOp::Push => &[
                (&["Compressing objects"], 0.2),
                (&["Writing objects"], 0.7),
                (&["remote: Resolving deltas"], 0.1),
            ],
            GitOp::Pull => &[
                (&["remote: Compressing objects"], 0.1),
                (&["Receiving objects"], 0.7),
                (&["Resolving deltas"], 0.15),
                (CHECKOUT, 0.15),
            ],
            GitOp::Clone => &[
                (&["remote: Compressing objects"], 0.1),
                (&["Receiving objects"], 0.6),
                (&["Resolving deltas"], 0.1),
                (CHECKOUT, 0.2),
            ],
        };
        let total: f32 = raw.iter().map(|&(_, weight)| weight).sum();
        let steps = raw
            .iter()
            .map(|&(titles, weight)| Step {
                titles,
                weight: weight / total,
            })
            .collect();
        Self {
            steps,
            step_index: 0,
            fraction: 0.0,
        }
    }

    /// Feed one stderr line (already split on `\r` or `\n` by the caller).
    /// Returns `None` for empty/whitespace-only lines.
    pub fn parse_line(&mut self, line: &str) -> Option<GitProgress> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if let Some((index, step_fraction)) = self.match_step(line) {
            self.step_index = index;
            let completed: f32 = self.steps[..index].iter().map(|s| s.weight).sum();
            let candidate = (completed + self.steps[index].weight * step_fraction).min(1.0);
            if candidate > self.fraction {
                self.fraction = candidate;
            }
        }
        Some(GitProgress {
            fraction: self.fraction,
            text: line.to_string(),
        })
    }

    /// Match `line` against the op's step table, returning the step index and
    /// that step's own completion (0.0..=1.0). `None` means a context line:
    /// no `": "` separator, an unparsable remainder, an unknown title, or a
    /// title belonging to an already-passed step.
    fn match_step(&self, line: &str) -> Option<(usize, f32)> {
        let split = line.rfind(": ")?;
        let title = &line[..split];
        let remainder = line[split + 2..].trim_start();
        let counts = parse_counts(remainder.split(", ").next()?)?;
        let index = self
            .steps
            .iter()
            .position(|step| step.titles.contains(&title))
            .filter(|&i| i >= self.step_index)?;
        let step_fraction = match counts {
            Counts::Ratio { value, total } => ratio(value, total),
            // No total to measure against: the step advances but its own
            // progress term contributes nothing yet.
            Counts::ValueOnly => 0.0,
        };
        Some((index, step_fraction))
    }
}

/// Parse the first comma-part of a step line's remainder: `"N% (value/total)"`
/// with a 1-3 digit percent, or a bare integer. Anything else is `None`.
fn parse_counts(part: &str) -> Option<Counts> {
    let (percent, rest) = take_digits(part)?;
    if rest.is_empty() {
        // A bare count, e.g. "remote: Counting objects: 167587".
        return Some(Counts::ValueOnly);
    }
    if percent.len() > 3 {
        return None;
    }
    let rest = rest.strip_prefix("% (")?;
    let (value, rest) = take_digits(rest)?;
    let rest = rest.strip_prefix('/')?;
    let (total, rest) = take_digits(rest)?;
    if rest != ")" {
        return None;
    }
    Some(Counts::Ratio {
        value: value.parse().ok()?,
        total: total.parse().ok()?,
    })
}

/// Split `s` into its leading run of ASCII digits and the rest; `None` when
/// it does not start with a digit.
fn take_digits(s: &str) -> Option<(&str, &str)> {
    let end = s
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(s.split_at(end))
    }
}

/// `value / total` as an f32 in 0.0..=1.0, treating a zero total as no
/// progress. Object counts stay far below f32's 24-bit mantissa in practice,
/// and a progress bar needs nowhere near that precision anyway.
#[allow(clippy::cast_precision_loss)]
fn ratio(value: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (value as f32 / total as f32).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerance for f32 fraction comparisons.
    const EPSILON: f32 = 1e-5;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < EPSILON,
            "expected fraction {expected}, got {actual}"
        );
    }

    /// Feed a line that must yield progress and return its fraction.
    fn feed(parser: &mut GitProgressParser, line: &str) -> f32 {
        parser
            .parse_line(line)
            .expect("non-empty line should yield progress")
            .fraction
    }

    #[test]
    fn full_push_sequence_reaches_one() {
        let mut p = GitProgressParser::new(GitOp::Push);

        let first = p
            .parse_line("Enumerating objects: 11939, done.")
            .expect("context line should yield progress");
        assert_close(first.fraction, 0.0);
        assert_eq!(first.text, "Enumerating objects: 11939, done.");

        // "Counting objects" is not a push step: context, the bar stays put.
        assert_close(
            feed(&mut p, "Counting objects: 100% (11939/11939), done."),
            0.0,
        );

        assert_close(
            feed(&mut p, "Compressing objects:  45% (5137/11415)"),
            0.2 * (5137.0 / 11415.0),
        );
        assert_close(
            feed(&mut p, "Writing objects:   1% (132/11939), 19.06 MiB | 1.38 MiB/s"),
            0.2 + 0.7 * (132.0 / 11939.0),
        );
        assert_close(
            feed(&mut p, "Writing objects: 100% (11939/11939), 272.10 MiB | 2.39 MiB/s, done."),
            0.9,
        );
        // The last ": " wins the title split, so the title here must resolve
        // to "remote: Resolving deltas".
        assert_close(
            feed(&mut p, "remote: Resolving deltas: 100% (500/500), completed with 100 local objects."),
            1.0,
        );
    }

    #[test]
    fn stray_earlier_step_never_lowers_the_fraction() {
        let mut p = GitProgressParser::new(GitOp::Push);
        let reached = feed(&mut p, "Writing objects:  50% (500/1000)");
        assert_close(reached, 0.2 + 0.7 * 0.5);
        // A late line for an already-passed step is context: no rewind.
        assert_close(feed(&mut p, "Compressing objects:  50% (100/200)"), reached);
    }

    #[test]
    fn skipped_step_counts_its_full_weight() {
        let mut p = GitProgressParser::new(GitOp::Push);
        // "Compressing objects" never appeared, but its weight is counted as
        // done the moment a later step reports.
        assert_close(feed(&mut p, "Writing objects:  10% (100/1000)"), 0.2 + 0.7 * 0.1);
    }

    #[test]
    fn full_pull_sequence_walks_all_four_steps() {
        // Pull's raw weights (0.1, 0.7, 0.15, 0.15) sum to 1.1, so every
        // expectation here is the raw prefix sum divided by 1.1.
        let mut p = GitProgressParser::new(GitOp::Pull);
        assert_close(
            feed(&mut p, "remote: Compressing objects: 100% (1000/1000)"),
            0.1 / 1.1,
        );
        assert_close(
            feed(&mut p, "Receiving objects:  50% (500/1000), 1.00 MiB | 2.00 MiB/s"),
            (0.1 + 0.7 * 0.5) / 1.1,
        );
        assert_close(
            feed(&mut p, "Receiving objects: 100% (1000/1000), 2.00 MiB | 2.00 MiB/s, done."),
            0.8 / 1.1,
        );
        assert_close(feed(&mut p, "Resolving deltas: 100% (400/400), done."), 0.95 / 1.1);
        assert_close(feed(&mut p, "Updating files: 100% (700/700), done."), 1.0);
    }

    // git ≥2.25 labels checkout progress "Updating files"; older gits said
    // "Checking out files". Both must finish the bar (verified against git
    // 2.50, which never prints the legacy label).
    #[test]
    fn modern_and_legacy_checkout_labels_both_finish_the_clone() {
        for label in ["Updating files", "Checking out files"] {
            let mut p = GitProgressParser::new(GitOp::Clone);
            let line = format!("{label}: 100% (300/300), done.");
            assert_close(feed(&mut p, &line), 1.0);
        }
    }

    // Git pads repaints with trailing spaces (to erase the previous, longer
    // line before the `\r`); the grammar must survive them.
    #[test]
    fn trailing_padding_from_git_repaints_still_parses() {
        let mut p = GitProgressParser::new(GitOp::Clone);
        assert_close(
            feed(&mut p, "Receiving objects:  16% (1/6)        "),
            (0.6 * (1.0 / 6.0) + 0.1) / 1.0,
        );
    }

    #[test]
    fn value_only_line_for_an_unknown_title_is_context() {
        let mut p = GitProgressParser::new(GitOp::Pull);
        let progress = p
            .parse_line("remote: Counting objects: 167587")
            .expect("context line should yield progress");
        assert_close(progress.fraction, 0.0);
        assert_eq!(progress.text, "remote: Counting objects: 167587");
        // Lines without any ": " at all are context too.
        assert_close(feed(&mut p, "Everything up-to-date"), 0.0);
    }

    #[test]
    fn value_only_line_for_a_real_step_advances_with_zero_contribution() {
        let mut p = GitProgressParser::new(GitOp::Pull);
        // A bare count for step 1 completes step 0's weight but adds nothing
        // for step 1 itself (there is no total to measure against).
        assert_close(feed(&mut p, "Receiving objects: 12345"), 0.1 / 1.1);
        // The step index advanced, so an earlier-step line is now context.
        assert_close(feed(&mut p, "remote: Compressing objects:  50% (10/20)"), 0.1 / 1.1);
    }

    #[test]
    fn empty_and_whitespace_lines_return_none() {
        let mut p = GitProgressParser::new(GitOp::Clone);
        assert!(p.parse_line("").is_none());
        assert!(p.parse_line("   ").is_none());
        assert!(p.parse_line("\t").is_none());
    }

    #[test]
    fn weights_normalize_so_completion_is_exactly_one() {
        // Union of all three ops' step lines, in step order for each op; the
        // ones an op doesn't know are context for it. Every op must land on
        // exactly 1.0, including Pull whose raw weights don't sum to 1.
        for op in [GitOp::Push, GitOp::Pull, GitOp::Clone] {
            let mut p = GitProgressParser::new(op);
            let mut last = 0.0_f32;
            for line in [
                "Compressing objects: 100% (10/10)",
                "remote: Compressing objects: 100% (10/10)",
                "Writing objects: 100% (10/10)",
                "Receiving objects: 100% (10/10)",
                "remote: Resolving deltas: 100% (10/10)",
                "Resolving deltas: 100% (10/10)",
                "Checking out files: 100% (10/10)",
            ] {
                last = feed(&mut p, line);
            }
            assert_close(last, 1.0);
        }
    }
}
