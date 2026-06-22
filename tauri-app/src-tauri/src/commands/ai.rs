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
         - The description should explain what changed and why, not how\n\
         - Write the description in third person and omit articles (\"a\", \"an\", \"the\")\n\
         - Return ONLY the JSON object, no markdown fences, no extra text\n\n\
         Git diff:\n\
         ```diff\n{}\n```\n\n\
         Generate the commit message as JSON:",
        diff
    )
}

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
        _ => Err(format!("Unknown AI provider: {}", provider)),
    }
}

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
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
            {
                Ok(c) => c,
                Err(_) => return Ok(false),
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
        _ => Err(format!("Unknown AI provider: {}", provider)),
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
        return Err("Claude CLI timed out".to_string());
    };
    let out = result.map_err(|e| format!("Claude CLI error: {e}"))?;
    let _ = writer.await;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Claude CLI failed: {stderr}"));
    }

    parse_claude_envelope(&String::from_utf8_lossy(&out.stdout))
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
        if matches!(
            wrapper.get("is_error").and_then(serde_json::Value::as_bool),
            Some(true)
        ) {
            let msg = wrapper
                .get("result")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Claude CLI reported an error");
            return Err(msg.to_string());
        }
        if let Some(inner) = wrapper.get("result").and_then(serde_json::Value::as_str) {
            return parse_commit_message_text(inner);
        }
    }
    parse_commit_message_text(stdout)
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
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let resp = client
        .post(format!("{}/api/generate", base_url))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Ollama request timed out".to_string()
            } else {
                format!("Failed to reach Ollama: {}", e)
            }
        })?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        if status.as_u16() == 404 {
            return Err(format!(
                "model {:?} not found - run: ollama pull {}",
                model, model
            ));
        }
        return Err(format!("Ollama HTTP {}: {}", status, text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama JSON: {}", e))?;

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
            .map(|s| s.to_string());
        let description = ["description", "body", "details"]
            .iter()
            .find_map(|k| {
                json.get(*k)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .map(|s| s.to_string())
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
}
