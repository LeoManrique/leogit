use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessage {
    pub title: String,
    pub description: String,
}

/// Everything a generate request needs, resolved for one provider.
///
/// Built by [`provider_config`] from the user's settings — one implementation,
/// in core, rather than the two that had drifted (a Rust copy in the native
/// bridge and a TypeScript copy in the composer) over which config read the
/// provider came from and whether a base URL was set at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// How long the request may run before it is abandoned. Carried here
    /// rather than hardcoded, because a settings control that persists a
    /// timeout nothing reads is worse than no control at all.
    pub timeout_secs: u32,
}

/// Resolve the settings into the knobs the selected provider actually uses.
#[must_use]
pub fn provider_config(cfg: &super::config::Config) -> AiProviderConfig {
    // `normalized` already folded any unrecognized name onto claude and turned
    // emptied fields into absent ones, so this is a straight read.
    if cfg.ai_provider == "ollama" {
        AiProviderConfig {
            provider: "ollama".to_string(),
            model: cfg.ollama.model.clone(),
            base_url: Some(cfg.ollama.server_url.clone()),
            timeout_secs: cfg.ollama.timeout_secs,
        }
    } else {
        AiProviderConfig {
            provider: "claude".to_string(),
            model: cfg.claude.model.clone(),
            base_url: None,
            timeout_secs: cfg.claude.timeout_secs,
        }
    }
}

/// Read the settings and resolve them for the selected provider.
///
/// # Errors
/// When the config file can't be read or parsed.
pub fn load_ai_config() -> Result<AiProviderConfig, String> {
    Ok(provider_config(&super::config::load_config()?))
}

// Limits per Go reference
const CLAUDE_MAX_DIFF: usize = 20_971_520; // 20MB
const OLLAMA_MAX_DIFF: usize = 52_428_800; // 50MB
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

/// Whether a provider can actually serve a request — and when it can't, what to
/// tell the user and what would fix it.
///
/// Deliberately richer than a boolean. "The binary is installed" and "the binary
/// will answer" are different questions, and a gate that asks the first while
/// the user is waiting on the second lets a doomed request through, then reports
/// a failure the probe could have named before it started: an installed Claude
/// CLI with an expired session passes `--version` and fails every generate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub ready: bool,
    /// Why not, as a sentence a client renders as-is. Empty when ready.
    pub reason: String,
    /// A shell command that would fix it, for a client with a terminal to offer
    /// it in. Empty when there is none, or when we can't know it would help —
    /// a remote Ollama is not fixed by starting a local one.
    pub fix_command: String,
}

impl ProviderStatus {
    fn ready() -> Self {
        Self {
            ready: true,
            reason: String::new(),
            fix_command: String::new(),
        }
    }

    fn blocked(reason: impl Into<String>, fix_command: &str) -> Self {
        Self {
            ready: false,
            reason: reason.into(),
            fix_command: fix_command.to_string(),
        }
    }
}

const CLAUDE_MISSING: &str = "Claude CLI not found. Install it, or switch the provider to Ollama.";
const CLAUDE_SIGNED_OUT: &str = "Claude is installed but not signed in.";
const CLAUDE_AUTH_FAILED: &str = "Claude couldn't authenticate. Sign in again.";
const CLAUDE_LOGIN_COMMAND: &str = "claude auth login";
const OLLAMA_SERVE_COMMAND: &str = "ollama serve";

/// Read a *failed request* for a provider state the user can fix.
///
/// This is not a fallback for [`check_provider_status`] — for the expired
/// session it is the only thing that works. Signing out deletes the
/// credentials, so a probe sees it; a session that expired leaves them on disk,
/// so `claude auth status` still reports a signed-in CLI and only a real
/// request discovers the refresh failed. Any gate built on the probe alone
/// waves that case straight through, which is exactly what happened.
///
/// Matching on the CLI's wording is unavoidable here — it is the only place
/// that state is ever reported — so it is done in one place, against the shapes
/// actually observed, and only ever to *offer* a remedy. The message the user
/// sees is still the CLI's own.
#[must_use]
pub fn provider_status_from_failure(provider: &str, error: &str) -> ProviderStatus {
    if provider != "claude" {
        return ProviderStatus::ready();
    }
    let lowered = error.to_ascii_lowercase();
    let needs_sign_in = [
        "not logged in",
        "failed to authenticate",
        "oauth",
        "run /login",
        "invalid api key",
        "unauthorized",
    ]
    .iter()
    .any(|marker| lowered.contains(marker));
    if needs_sign_in {
        ProviderStatus::blocked(CLAUDE_AUTH_FAILED, CLAUDE_LOGIN_COMMAND)
    } else {
        ProviderStatus::ready()
    }
}

/// Probe whether `provider` is ready to take a request.
///
/// Every probe failure is an answer, never an error: the only `Err` is a
/// provider name nothing here knows. And an answer we can't interpret opens the
/// gate rather than closing it — locking a user out of Generate because a CLI
/// changed its output format is worse than letting a request report itself.
///
/// # Errors
/// Returns an error only for an unknown provider name.
pub async fn check_provider_status(
    provider: String,
    config: AiProviderConfig,
) -> Result<ProviderStatus, String> {
    match provider.as_str() {
        "claude" => Ok(check_claude().await),
        "ollama" => Ok(check_ollama(&config).await),
        _ => Err(format!("Unknown AI provider: {provider}")),
    }
}

/// Run `claude` with `args`, or `None` if it could not be spawned at all.
async fn run_claude(args: &[&str]) -> Option<std::process::Output> {
    let mut cmd = tokio::process::Command::new("claude");
    cmd.args(args);
    super::process::hide_console_async(&mut cmd)
        .output()
        .await
        .ok()
}

/// Installed *and* signed in — two separate questions, asked in that order so
/// the reason names the one that actually blocks.
async fn check_claude() -> ProviderStatus {
    let installed = run_claude(&["--version"])
        .await
        .is_some_and(|out| out.status.success());
    if !installed {
        return ProviderStatus::blocked(CLAUDE_MISSING, "");
    }
    // Spawning worked a moment ago, so a failure here is a mystery rather than
    // evidence — say ready and let the request speak for itself.
    let Some(auth) = run_claude(&["auth", "status"]).await else {
        return ProviderStatus::ready();
    };
    if !auth.status.success() || claude_signed_out(&String::from_utf8_lossy(&auth.stdout)) {
        return ProviderStatus::blocked(CLAUDE_SIGNED_OUT, CLAUDE_LOGIN_COMMAND);
    }
    ProviderStatus::ready()
}

/// `claude auth status` reports through its JSON payload (`loggedIn`), not only
/// through its exit code — a field that would be pointless if the status were in
/// the exit alone. Anything unparseable counts as signed in, per the
/// open-the-gate rule on [`check_provider_status`].
fn claude_signed_out(stdout: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()
        .and_then(|payload| payload.get("loggedIn").and_then(serde_json::Value::as_bool))
        .is_some_and(|logged_in| !logged_in)
}

async fn check_ollama(config: &AiProviderConfig) -> ProviderStatus {
    let base_url = config
        .base_url
        .clone()
        .unwrap_or_else(super::config::default_ollama_url);
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    else {
        return ProviderStatus::ready();
    };
    let reachable = matches!(
        client
            .get(format!("{}/api/tags", base_url.trim_end_matches('/')))
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    );
    if reachable {
        return ProviderStatus::ready();
    }
    ProviderStatus::blocked(
        format!(
            "Ollama isn't answering at {base_url}. Start it, or change the address in Settings."
        ),
        if is_loopback_url(&base_url) {
            OLLAMA_SERVE_COMMAND
        } else {
            ""
        },
    )
}

/// Whether a URL points at this machine — the only case where "start Ollama" is
/// advice the user can act on here. Pointed at a server, the fix is on that
/// server, and offering to run `ollama serve` locally would start a second,
/// empty instance that still isn't the one they configured.
fn is_loopback_url(url: &str) -> bool {
    let Some(host) = reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    if host == "localhost" {
        return true;
    }
    // `host_str` brackets an IPv6 literal, which `IpAddr` won't parse. Going
    // through `IpAddr` rather than matching strings is what makes the whole
    // 127.0.0.0/8 block count, not just 127.0.0.1.
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .is_ok_and(|addr| addr.is_loopback())
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

    let timeout_secs = u64::from(config.timeout_secs);
    let timed =
        tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await;
    let Ok(result) = timed else {
        writer.abort();
        return Err(format!("Claude CLI timed out after {timeout_secs}s"));
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

    let fallback_url = super::config::default_ollama_url();
    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or(&fallback_url)
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
        .timeout(Duration::from_secs(u64::from(config.timeout_secs)))
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

    // The whole point of reading the payload rather than the exit code: the
    // signed-out answer is a successful run that says `loggedIn: false`.
    #[test]
    fn claude_auth_status_is_read_from_its_payload() {
        assert!(claude_signed_out(r#"{"loggedIn":false}"#));
        assert!(!claude_signed_out(
            r#"{"loggedIn":true,"authMethod":"claude.ai","subscriptionType":"max"}"#
        ));
    }

    // Both real failures, copied verbatim from the two states that produce
    // them. They are different states — signing out deletes the credentials, an
    // expired session leaves them on disk — and only the second one is
    // invisible to `claude auth status`, which is why this path exists at all.
    #[test]
    fn a_failed_request_names_the_sign_in_states() {
        for error in [
            "Claude CLI failed: Failed to authenticate: OAuth session expired and could not be refreshed",
            "Claude CLI failed: Not logged in · Please run /login",
            "Claude CLI failed: Invalid API key. Please run /login",
        ] {
            let status = provider_status_from_failure("claude", error);
            assert!(!status.ready, "should offer a remedy for: {error}");
            assert_eq!(status.fix_command, CLAUDE_LOGIN_COMMAND);
        }
    }

    // Everything else is the provider working and the request failing — a rate
    // limit, a crash, a timeout. Offering "sign in again" there is noise, and
    // would disable Generate over something signing in cannot fix.
    #[test]
    fn an_unrelated_failure_offers_no_remedy() {
        for error in [
            "Claude CLI failed: API Error: 529 Overloaded. This is a server-side issue",
            "Claude CLI failed (exit status: 1) with no error output",
            "request timed out",
        ] {
            assert!(provider_status_from_failure("claude", error).ready);
        }
        // Ollama's failures are never a sign-in problem.
        assert!(provider_status_from_failure("ollama", "Not logged in").ready);
    }

    // A probe that cannot understand the answer must open the gate, not close
    // it — otherwise a CLI output change locks the user out of Generate with no
    // way to disagree.
    #[test]
    fn unreadable_auth_status_counts_as_signed_in() {
        assert!(!claude_signed_out(""));
        assert!(!claude_signed_out("not json at all"));
        assert!(!claude_signed_out(r#"{"someOtherShape":true}"#));
    }

    // "Start Ollama" is only advice we can act on when the address is this
    // machine; against a server it would start a second, empty local instance.
    #[test]
    fn ollama_fix_command_is_offered_only_for_a_local_address() {
        assert!(is_loopback_url("http://localhost:11434"));
        assert!(is_loopback_url("http://127.0.0.1:11434"));
        assert!(is_loopback_url("http://[::1]:11434"));
        assert!(is_loopback_url("http://127.0.0.2:11434"));
        assert!(!is_loopback_url("http://ollama.example.com:11434"));
        assert!(!is_loopback_url("http://192.168.1.40:11434"));
        assert!(!is_loopback_url("not a url"));
    }

    // The regression this fixes: a transient API error (here the real 529
    // Overloaded envelope the CLI emits) must surface as an Err, NOT become the
    // commit title. The exit code is 0 in this case, so only the envelope's
    // is_error flag distinguishes it.
    #[test]
    fn envelope_surfaces_api_error_instead_of_using_it_as_a_message() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":true,"api_error_status":529,"result":"API Error: 529 Overloaded. This is a server-side issue, usually temporary — try again in a moment."}"#;
        let err = parse_claude_envelope(stdout).expect_err("is_error must map to Err");
        assert!(
            err.contains("529"),
            "error should carry the CLI message: {err}"
        );
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
        let stdout =
            r#"{"type":"result","is_error":true,"result":"Invalid API key. Please run /login"}"#;
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
