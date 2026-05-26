use std::process::Command;

// `check_auth` stays so future gh-backed features (e.g. `gh project create`)
// can gate themselves on the user having `gh` authenticated. The PR-list /
// PR-checks / PR-create commands were removed when the PR view was retired
// from the UI — re-add them if/when the PR overview ships again.
#[tauri::command]
pub fn check_auth() -> bool {
    let output = Command::new("gh").arg("auth").arg("status").output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}
