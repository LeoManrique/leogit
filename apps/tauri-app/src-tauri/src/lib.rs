//! The Tauri host: a thin presentation shell over [`leogit_core`].
//!
//! Everything here is glue. [`shims`] re-exposes each core function as a
//! `#[tauri::command]`; [`event_sink`] maps [`leogit_core::events::CoreEvent`]s
//! onto window `emit`s; [`launch_glue`] handles the window-focusing half of the
//! `leogit <dir>` flow whose pure resolution lives in `leogit_core::launch`.

pub mod event_sink;
pub mod launch_glue;
pub mod shims;

#[cfg(test)]
mod tests {
    /// One version across both clients, and `scripts/_version.py` moves every
    /// file that states it together. This pins the pair that actually matters
    /// at runtime: `tauri.conf.json` is the declared source of truth, and this
    /// crate's manifest version is what [`shims::update::check_for_update`]
    /// hands to core to compare against the latest published tag. A bump that
    /// reaches one and not the other ships a build that announces an update to
    /// itself on every launch — silently, since both numbers look plausible.
    ///
    /// The FFI crate carries the mirror of this for `project.yml`'s
    /// `MARKETING_VERSION`, which is the macOS app's half of the same claim.
    #[test]
    fn crate_version_matches_the_declared_product_version() {
        // A line scan rather than a JSON parse: reading one string out of a
        // file whose shape is fixed is not worth a dependency.
        let declared = include_str!("../tauri.conf.json")
            .lines()
            .find_map(|line| line.trim().strip_prefix("\"version\": \""))
            .and_then(|rest| rest.split('"').next())
            .expect("tauri.conf.json declares a version");
        assert_eq!(
            declared,
            env!("CARGO_PKG_VERSION"),
            "tauri.conf.json and this crate's Cargo.toml disagree about the version"
        );
    }
}
