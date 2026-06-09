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
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::process::hide_console_async(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn claude CLI: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| format!("Failed to write prompt to claude stdin: {}", e))?;
        stdin
            .shutdown()
            .await
            .map_err(|e| format!("Failed to close claude stdin: {}", e))?;
        drop(stdin);
    }

    let out = tokio::time::timeout(
        Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "Claude CLI timed out".to_string())?
    .map_err(|e| format!("Claude CLI error: {}", e))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Claude CLI failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Parse the wrapper JSON: {"type":"result","result":"<inner json>"}
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(wrapper) => {
            if let Some(inner) = wrapper.get("result").and_then(|v| v.as_str()) {
                return parse_commit_message_text(inner);
            }
            // Wrapper parsed but no `result` field, fall back to raw parse
            parse_commit_message_text(&stdout)
        }
        Err(_) => parse_commit_message_text(&stdout),
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
