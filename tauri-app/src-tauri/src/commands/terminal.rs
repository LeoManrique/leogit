use lazy_static::lazy_static;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};

/// Holds the master PTY (must stay alive to keep the PTY open),
/// its writer (for stdin), and the child process handle.
struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

lazy_static! {
    static ref SESSIONS: Mutex<HashMap<u32, Arc<Mutex<PtySession>>>> = Mutex::new(HashMap::new());
    static ref NEXT_PID: AtomicU32 = AtomicU32::new(1);
}

/// Start a new PTY shell with cwd=repo_path.
/// Returns a synthetic session id used by subsequent commands.
#[tauri::command]
pub fn start_terminal(app: AppHandle, repo_path: String) -> Result<u32, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "cmd.exe".into()
        } else {
            "/bin/zsh".into()
        }
    });

    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(&repo_path);
    // Forward parent env (TERM, PATH, HOME, etc.) so the shell behaves like a normal login.
    for (key, val) in std::env::vars() {
        cmd.env(key, val);
    }
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| e.to_string())?;

    // Close slave fd in this process; child has its own copy.
    drop(pair.slave);

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    let session = Arc::new(Mutex::new(PtySession {
        master: pair.master,
        writer,
        child,
    }));
    SESSIONS.lock().unwrap().insert(pid, session.clone());

    // Reader thread: pump PTY output into a Tauri event scoped by pid.
    let app_handle = app.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_handle.emit(&format!("terminal-output-{}", pid), data);
                }
                Err(_) => break,
            }
        }
        // Clean up session on EOF/error and notify frontend.
        SESSIONS.lock().unwrap().remove(&pid);
        let _ = app_handle.emit(&format!("terminal-closed-{}", pid), ());
    });

    Ok(pid)
}

/// Write data to PTY stdin.
#[tauri::command]
pub fn write_terminal(pid: u32, data: String) -> Result<(), String> {
    let sessions = SESSIONS.lock().map_err(|_| "sessions lock poisoned")?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| format!("no pty for pid {}", pid))?
        .clone();
    drop(sessions);

    let mut s = session.lock().map_err(|_| "session lock poisoned")?;
    s.writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    s.writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Resize the PTY window.
#[tauri::command]
pub fn resize_terminal(pid: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = SESSIONS.lock().map_err(|_| "sessions lock poisoned")?;
    let session = sessions
        .get(&pid)
        .ok_or_else(|| format!("no pty for pid {}", pid))?
        .clone();
    drop(sessions);

    let s = session.lock().map_err(|_| "session lock poisoned")?;
    s.master
        .resize(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Kill the child process and remove the session.
#[tauri::command]
pub fn close_terminal(pid: u32) -> Result<(), String> {
    let mut sessions = SESSIONS.lock().map_err(|_| "sessions lock poisoned")?;
    if let Some(session) = sessions.remove(&pid) {
        if let Ok(mut s) = session.lock() {
            let _ = s.child.kill();
        }
    }
    Ok(())
}
