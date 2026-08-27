// Thin `#[tauri::command]` delegations to `leogit_core::ai`.
#![allow(clippy::needless_pass_by_value, clippy::missing_errors_doc)]

use leogit_core::ai::{self, AiProviderConfig, CommitMessage};

#[tauri::command]
pub fn load_ai_config() -> Result<AiProviderConfig, String> {
    ai::load_ai_config()
}

#[tauri::command]
pub async fn generate_commit_message(
    diff: String,
    provider: String,
    config: AiProviderConfig,
) -> Result<CommitMessage, String> {
    ai::generate_commit_message(diff, provider, config).await
}

#[tauri::command]
pub async fn check_provider_available(
    provider: String,
    config: AiProviderConfig,
) -> Result<bool, String> {
    ai::check_provider_available(provider, config).await
}
