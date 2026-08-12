//! The Tauri host: a thin presentation shell over [`leogit_core`].
//!
//! Everything here is glue. [`shims`] re-exposes each core function as a
//! `#[tauri::command]`; [`event_sink`] maps [`leogit_core::events::CoreEvent`]s
//! onto window `emit`s; [`launch_glue`] handles the window-focusing half of the
//! `leogit <dir>` flow whose pure resolution lives in `leogit_core::launch`.

pub mod event_sink;
pub mod launch_glue;
pub mod shims;
