//! Tauri-free core for `LeoGit`: all git / diff / highlight / terminal / config
//! / shell / update logic, written once and embedded in-process by every client.
//!
//! Today the Tauri host (`apps/tauri-app`) links this crate and exposes each
//! function through a thin `#[tauri::command]` shim; the SwiftUI/UniFFI host
//! links the same crate next. The only place the UI framework leaks in is
//! [`events::EventSink`] — the host-provided channel that streaming producers
//! (git `--progress`, PTY output) deliver [`events::CoreEvent`]s through.
//!
//! Modules keep their original `super::`-relative wiring: since each is a
//! crate-root sibling, `super::render`, `super::diff`, … still resolve exactly
//! as they did under the former `commands::` parent.

pub mod events;

pub mod ai;
pub mod appimage;
pub mod config;
pub mod diff;
pub mod exclusions;
pub mod gh;
pub mod git;
pub mod highlight;
pub mod launch;
pub mod os;
pub mod paths;
pub mod process;
pub mod progress;
pub mod render;
pub mod repos;
pub mod shell;
pub mod terminal;
pub mod update;
