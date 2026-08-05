use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessage {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

// Limits per Go reference
const CLAUDE_MAX_DIFF: usize = 20_971_520; // 20MB
const OLLAMA_MAX_DIFF: usize = 52_428_800; // 50MB
const DEFAULT_TIMEOUT_SECS: u64 = 120;
// Cap the claude CLI's internal request retries. By default a transient
// overload (HTTP 529) makes it retry with backoff for *minutes* — far past our
// timeout, so the user only ever sees "timed out". A small cap fails fast with
// the real error while still riding out a single blip.
const CLAUDE_MAX_RETRIES: u32 = 2;

fn build_prompt(diff: &str) -> String {
    format!(
        "You are a Git commit message generator. Analyze the provided git diff and generate a commit message.\n\n\
         Return ONLY valid JSON in this exact format:\n\
         {{\n\
           \"title\": \"A 50 character or less summary in imperative mood\",\n\
           \"description\": \"A detailed description of what changed and why\"\n\
         }}\n\n\
         Rules:\n\
         - The title MUST be 50 characters or less and use the imperative mood (e.g. \"Add\", \"Fix\", \"Update\")\n\
         - The description should explain what changed and why, but keep it concise and high level\n\
         - If multiple unrelated things have changed, divide them with bulletpoints\n\
         - Write the description in third person and omit articles (\"a\", \"an\", \"the\")\n\
         - Return ONLY the JSON object, no markdown fences, no extra text\n\n\
         Git diff:\n\
         ```diff\n{diff}\n```\n\n\
         Generate the commit message as JSON:"
    )
}

/// # Errors
/// Returns a human-readable message when the diff is empty or oversized, the
/// provider is unknown or unreachable, or the provider's response can't be
/// parsed as a commit message.
#[tauri::command]
pub async fn generate_commit_message(
    diff: String,
    provider: String,
    config: AiProviderConfig,
) -> Result<CommitMessage, String> {
    if diff.trim().is_empty() {
        return Err("no files selected".to_string());
    }
    match provider.as_str() {
        "claude" => generate_claude(&diff, &config).await,
        "ollama" => generate_ollama(&diff, &config).await,
        _ => Err(format!("Unknown AI provider: {provider}")),
    }
}

/// # Errors
/// Returns an error only for an unknown provider name; probe failures map to
/// `Ok(false)` so the UI can gate features without surfacing raw errors.
#[tauri::command]
pub async fn check_provider_available(
    provider: String,
    config: AiProviderConfig,
) -> Result<bool, String> {
    match provider.as_str() {
        "claude" => {
            let mut cmd = tokio::process::Command::new("claude");
            cmd.arg("--version");
            let out = super::process::hide_console_async(&mut cmd).output().await;
            match out {
                Ok(o) => Ok(o.status.success()),
                Err(_) => Ok(false),
            }
        }
        "ollama" => {
            let base_url = config
                .base_url
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let Ok(client) = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
            else {
                return Ok(false);
            };
            match client
                .get(format!("{}/api/tags", base_url.trim_end_matches('/')))
                .send()
                .await
            {
                Ok(resp) => Ok(resp.status().is_success()),
                Err(_) => Ok(false),
            }
        }
        _ => Err(format!("Unknown AI provider: {provider}")),
    }
}

async fn generate_claude(diff: &str, config: &AiProviderConfig) -> Result<CommitMessage, String> {
    if diff.len() > CLAUDE_MAX_DIFF {
        return Err(format!(
            "diff is {} bytes (max {})",
            diff.len(),
            CLAUDE_MAX_DIFF
        ));
    }

    let model = config.model.as_deref().unwrap_or("sonnet");
    let prompt = build_prompt(diff);

    let mut cmd = tokio::process::Command::new("claude");
    cmd.arg("--print")
        .arg("--output-format")
        .arg("json")
        .arg("--model")
        .arg(model)
        .env("CLAUDE_CODE_MAX_RETRIES", CLAUDE_MAX_RETRIES.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Kill the child if we stop awaiting it (e.g. on timeout) so a slow CLI
        // can't linger as an orphan after we've already given up on it.
        .kill_on_drop(true);
    super::process::hide_console_async(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn claude CLI: {e}"))?;

    // Stream the prompt on its own task so a large diff can't deadlock against
    // the child filling a stdout/stderr pipe before we begin draining it.
    let stdin = child.stdin.take();
    let prompt_bytes = prompt.into_bytes();
    let writer = tokio::spawn(async move {
        if let Some(mut sin) = stdin {
            let _ = sin.write_all(&prompt_bytes).await;
            let _ = sin.shutdown().await;
        }
    });

    let timed = tokio::time::timeout(
        Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await;
    let Ok(result) = timed else {
        writer.abort();
        return Err(format!("Claude CLI timed out after {DEFAULT_TIMEOUT_SECS}s"));
    };
    let out = result.map_err(|e| format!("Claude CLI error: {e}"))?;
    let _ = writer.await;

    if !out.status.success() {
        return Err(claude_failure_message(&out));
    }

    parse_claude_envelope(&String::from_utf8_lossy(&out.stdout))
}

/// Build the error for a non-zero claude CLI exit. The CLI is inconsistent
/// about where it reports failures: API and auth errors land in the stdout
/// JSON envelope (`is_error` + `result`), crashes write to stderr, and a
/// killed process may emit nothing at all — so try each in turn and fall back
/// to the exit status rather than ever surfacing a blank error.
fn claude_failure_message(out: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = envelope_error_message(&stdout)
        .or_else(|| non_empty_trimmed(&stderr))
        .or_else(|| non_empty_trimmed(&stdout));
    match detail {
        Some(msg) => format!("Claude CLI failed: {}", truncate_error(&msg)),
        None => format!("Claude CLI failed ({}) with no error output", out.status),
    }
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Cap error text shown in the UI; a Node stack trace can run to kilobytes
/// and the useful part (the message) is at the top.
fn truncate_error(msg: &str) -> String {
    const MAX_ERROR_CHARS: usize = 1000;
    match msg.char_indices().nth(MAX_ERROR_CHARS) {
        Some((cut, _)) => format!("{}…", &msg[..cut]),
        None => msg.to_string(),
    }
}

/// Interpret the claude CLI's `--output-format json` envelope:
/// `{"type":"result","subtype":"success","is_error":bool,"result":"<text>", …}`.
///
/// On `is_error` (e.g. a transient 529 Overloaded) the CLI puts a human-readable
/// message in `result`; we surface that verbatim as an `Err` instead of letting
/// the error text masquerade as a commit message. Otherwise the model's reply
/// lives in `result` and is parsed as the commit message. Falls back to parsing
/// the raw output when the text isn't the JSON envelope at all.
fn parse_claude_envelope(stdout: &str) -> Result<CommitMessage, String> {
    if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(stdout) {
        if is_error_envelope(&wrapper) {
            // Fall back to a placeholder rather than dropping through: on this
            // exit-0 path there is no stderr to consult, and letting the raw
            // envelope reach parse_commit_message_text would turn it into a
            // bogus commit title.
            let msg = envelope_message(&wrapper)
                .unwrap_or_else(|| "Claude CLI reported an error".to_string());
            return Err(truncate_error(&msg));
        }
        if let Some(inner) = wrapper.get("result").and_then(serde_json::Value::as_str) {
            return parse_commit_message_text(inner);
        }
    }
    parse_commit_message_text(stdout)
}

fn is_error_envelope(wrapper: &serde_json::Value) -> bool {
    matches!(
        wrapper.get("is_error").and_then(serde_json::Value::as_bool),
        Some(true)
    )
}

/// The envelope's `result` text, when it is a non-empty string.
fn envelope_message(wrapper: &serde_json::Value) -> Option<String> {
    wrapper
        .get("result")
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty_trimmed)
}

/// Extract the human-readable message from an `is_error` CLI envelope.
/// `None` when the text isn't an error envelope — or when the envelope carries
/// no usable message (empty/missing/non-string `result`), so the caller's
/// stderr/stdout/exit-status fallbacks get their chance instead of being
/// short-circuited by a blank or placeholder string.
fn envelope_error_message(stdout: &str) -> Option<String> {
    let wrapper = serde_json::from_str::<serde_json::Value>(stdout).ok()?;
    if is_error_envelope(&wrapper) {
        envelope_message(&wrapper)
    } else {
        None
    }
}

async fn generate_ollama(diff: &str, config: &AiProviderConfig) -> Result<CommitMessage, String> {
    if diff.len() > OLLAMA_MAX_DIFF {
        return Err(format!(
            "diff is {} bytes (max {})",
            diff.len(),
            OLLAMA_MAX_DIFF
        ));
    }

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:11434")
        .trim_end_matches('/');
    let model = config
        .model
        .as_deref()
        .unwrap_or("tavernari/git-commit-message:latest");
    let prompt = build_prompt(diff);

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "format": "json",
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .post(format!("{base_url}/api/generate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Ollama request timed out".to_string()
            } else {
                format!("Failed to reach Ollama: {e}")
            }
        })?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        if status.as_u16() == 404 {
            return Err(format!(
                "model {model:?} not found - run: ollama pull {model}"
            ));
        }
        return Err(format!("Ollama HTTP {status}: {text}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama JSON: {e}"))?;

    let text = json
        .get("response")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "empty response from Ollama".to_string())?;

    parse_commit_message_text(text)
}

fn parse_commit_message_text(text: &str) -> Result<CommitMessage, String> {
    let cleaned = text.trim();

    // Strip markdown code fences (```json ... ```)
    let cleaned = if cleaned.starts_with("```") {
        let lines: Vec<&str> = cleaned.lines().collect();
        if lines.len() >= 3 {
            lines[1..lines.len() - 1].join("\n")
        } else {
            cleaned.to_string()
        }
    } else {
        cleaned.to_string()
    };
    let cleaned = cleaned.trim();

    // Try to parse as JSON with field aliases
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(cleaned) {
        let title = ["title", "summary", "subject", "message"]
            .iter()
            .find_map(|k| {
                json.get(*k)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .map(ToString::to_string);
        let description = ["description", "body", "details"]
            .iter()
            .find_map(|k| {
                json.get(*k)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .map(ToString::to_string)
            .unwrap_or_default();

        if let Some(t) = title {
            return Ok(CommitMessage {
                title: t,
                description,
            });
        }
    }

    // Last resort: use first line as title, rest as description
    let mut lines = cleaned.lines();
    let title_raw = lines.next().unwrap_or("").trim().to_string();
    let description = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    if title_raw.is_empty() {
        return Err("Could not extract commit message from AI response".to_string());
    }

    Ok(CommitMessage {
        title: title_raw,
        description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The regression this fixes: a transient API error (here the real 529
    // Overloaded envelope the CLI emits) must surface as an Err, NOT become the
    // commit title. The exit code is 0 in this case, so only the envelope's
    // is_error flag distinguishes it.
    #[test]
    fn envelope_surfaces_api_error_instead_of_using_it_as_a_message() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":true,"api_error_status":529,"result":"API Error: 529 Overloaded. This is a server-side issue, usually temporary — try again in a moment."}"#;
        let err = parse_claude_envelope(stdout).expect_err("is_error must map to Err");
        assert!(err.contains("529"), "error should carry the CLI message: {err}");
    }

    #[test]
    fn envelope_extracts_commit_from_result_field() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"title\":\"Add widget\",\"description\":\"Introduce the widget module\"}"}"#;
        let msg = parse_claude_envelope(stdout).expect("valid envelope");
        assert_eq!(msg.title, "Add widget");
        assert_eq!(msg.description, "Introduce the widget module");
    }

    // Some responses aren't the CLI envelope at all (raw JSON, or fenced text);
    // those still parse directly rather than being misread as an error.
    #[test]
    fn envelope_falls_back_to_raw_commit_json() {
        let stdout = r#"{"title":"Fix parser","description":"Handle empty input"}"#;
        let msg = parse_claude_envelope(stdout).expect("raw commit json");
        assert_eq!(msg.title, "Fix parser");
        assert_eq!(msg.description, "Handle empty input");
    }

    #[cfg(unix)]
    fn output(code: i32, stdout: &str, stderr: &str) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    // The regression from the UI: the CLI exits non-zero with its error in the
    // stdout envelope and *nothing* on stderr, which used to surface as a bare
    // "Claude CLI failed: ".
    #[cfg(unix)]
    #[test]
    fn failure_message_reads_stdout_envelope_when_stderr_is_empty() {
        let stdout = r#"{"type":"result","is_error":true,"result":"Invalid API key. Please run /login"}"#;
        let msg = claude_failure_message(&output(1, stdout, ""));
        assert_eq!(msg, "Claude CLI failed: Invalid API key. Please run /login");
    }

    #[cfg(unix)]
    #[test]
    fn failure_message_prefers_envelope_over_noisy_stderr() {
        let stdout = r#"{"is_error":true,"result":"API Error: 401 Unauthorized"}"#;
        let msg = claude_failure_message(&output(1, stdout, "(node) DeprecationWarning: …\n"));
        assert_eq!(msg, "Claude CLI failed: API Error: 401 Unauthorized");
    }

    #[cfg(unix)]
    #[test]
    fn failure_message_uses_trimmed_stderr_then_raw_stdout() {
        let msg = claude_failure_message(&output(1, "", "Error: something broke\n"));
        assert_eq!(msg, "Claude CLI failed: Error: something broke");

        let msg = claude_failure_message(&output(1, "plain text error", "  \n"));
        assert_eq!(msg, "Claude CLI failed: plain text error");
    }

    // Whitespace-only output must not produce a blank error — fall back to the
    // exit status so the user always sees *something* actionable.
    #[cfg(unix)]
    #[test]
    fn failure_message_falls_back_to_exit_status() {
        let msg = claude_failure_message(&output(3, " \n", ""));
        assert!(
            msg.contains("exit status: 3") && msg.contains("no error output"),
            "should describe the exit status: {msg}"
        );
    }

    // An error envelope whose `result` is empty or non-string must not
    // short-circuit the fallback chain with a blank or placeholder message
    // when the real error sits on stderr.
    #[cfg(unix)]
    #[test]
    fn failure_message_skips_contentless_envelope_in_favor_of_stderr() {
        let empty = r#"{"type":"result","is_error":true,"result":""}"#;
        let msg = claude_failure_message(&output(1, empty, "Invalid API key\n"));
        assert_eq!(msg, "Claude CLI failed: Invalid API key");

        let non_string = r#"{"is_error":true,"result":null}"#;
        let msg = claude_failure_message(&output(1, non_string, "Invalid API key\n"));
        assert_eq!(msg, "Claude CLI failed: Invalid API key");
    }

    // On the exit-0 path the same contentless envelope has no stderr to fall
    // back to: surface the placeholder, never a blank Err and never the raw
    // envelope masquerading as a commit message.
    #[test]
    fn envelope_with_contentless_error_yields_placeholder_not_blank() {
        for stdout in [
            r#"{"type":"result","is_error":true,"result":""}"#,
            r#"{"type":"result","is_error":true,"result":"  "}"#,
            r#"{"type":"result","is_error":true}"#,
            r#"{"type":"result","is_error":true,"result":{"nested":"shape"}}"#,
        ] {
            let err = parse_claude_envelope(stdout).expect_err("is_error must map to Err");
            assert_eq!(err, "Claude CLI reported an error", "for envelope {stdout}");
        }
    }

    #[test]
    fn long_errors_are_truncated_for_the_ui() {
        let long = "x".repeat(5000);
        let msg = truncate_error(&long);
        assert_eq!(msg.chars().count(), 1001, "1000 chars plus ellipsis");
        assert!(msg.ends_with('…'));
        assert_eq!(truncate_error("short"), "short");
    }

    // The cap counts chars, not bytes: a multibyte message under the limit
    // must come back untouched (no spurious ellipsis), and a long one must be
    // cut at a char boundary without panicking.
    #[test]
    fn truncation_counts_chars_not_bytes() {
        let short_multibyte = "é".repeat(600); // 600 chars, 1200 bytes
        assert_eq!(truncate_error(&short_multibyte), short_multibyte);

        let long_multibyte = "日".repeat(1500);
        let msg = truncate_error(&long_multibyte);
        assert_eq!(msg.chars().count(), 1001, "1000 chars plus ellipsis");
        assert!(msg.ends_with('…'));
    }
}
