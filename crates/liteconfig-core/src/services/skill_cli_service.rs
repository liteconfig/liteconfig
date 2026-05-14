//! Shell out to the official `skills` CLI (https://www.npmjs.com/package/skills)
//! via `pnpx`/`npx` to install a skill.
//!
//! The CLI understands sources we can't handle with a git clone — notably
//! the `skills.volces.com/...` pseudo-registry and a few non-GitHub git
//! forges. When `pnpm` is missing but `node` is present, the TUI can offer
//! to activate pnpm through Corepack and stream the output.
//!
//! Every spawn returns a `CommandStream` whose internal fields are shared
//! across threads — a reader thread drains stdout/stderr into the shared
//! lines buffer and writes the final status when the process exits. The UI
//! polls the status each tick.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// What we found on PATH. Drives whether the TUI offers `p` as an install
/// shortcut or prompts for Corepack activation first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    /// `pnpm` is on PATH → prefer `pnpx skills add <owner/repo>`.
    Pnpm,
    /// `npm`+`npx` on PATH but no `pnpm`.
    Npm,
    /// `node` on PATH but no package manager — rare; treat as None.
    NodeOnly,
    /// Nothing runtime-related found. UI should fall back to git-clone.
    None,
}

/// Whether a long-running shell-out is in flight, done, or failed. Poll
/// from the UI's per-tick drain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Ok,
    Err(String),
}

/// Handle to a spawned child process. `lines` is populated line-by-line by
/// the reader thread; `status` flips off `Running` when the child exits.
#[derive(Clone)]
pub struct CommandStream {
    pub title: String,
    pub lines: Arc<Mutex<Vec<String>>>,
    pub status: Arc<Mutex<RunStatus>>,
}

impl CommandStream {
    pub fn snapshot_lines(&self, last: usize) -> Vec<String> {
        let guard = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        let len = guard.len();
        let start = len.saturating_sub(last);
        guard[start..].to_vec()
    }

    pub fn status(&self) -> RunStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(RunStatus::Err("poisoned".into()))
    }
}

/// Check PATH for `pnpm` / `npm` / `npx` / `node`. Pure, no side effects.
pub fn detect() -> InstallMethod {
    if which("pnpm").is_some() {
        return InstallMethod::Pnpm;
    }
    if which("npm").is_some() && which("npx").is_some() {
        return InstallMethod::Npm;
    }
    if which("node").is_some() {
        return InstallMethod::NodeOnly;
    }
    InstallMethod::None
}

/// The concrete argv we'd run for a given method. Also used by tests so we
/// can assert the exact shell invocation without spawning.
pub fn pnpx_argv(method: InstallMethod, owner_repo: &str) -> Option<(&'static str, Vec<String>)> {
    match method {
        InstallMethod::Pnpm => Some((
            "pnpx",
            vec!["skills".into(), "add".into(), owner_repo.into()],
        )),
        InstallMethod::Npm => Some((
            "npx",
            vec![
                "-y".into(),
                "skills".into(),
                "add".into(),
                owner_repo.into(),
            ],
        )),
        _ => None,
    }
}

/// Spawn `pnpx skills add <owner/repo>` in a reader thread. Returns the
/// stream handle immediately — the caller shows a popup and polls the
/// shared buffers.
pub fn install_via_pnpx(owner_repo: &str) -> CommandStream {
    let method = detect();
    let title = format!("skills add {owner_repo}");
    let stream = CommandStream {
        title: title.clone(),
        lines: Arc::new(Mutex::new(Vec::new())),
        status: Arc::new(Mutex::new(RunStatus::Running)),
    };
    let Some((program, args)) = pnpx_argv(method, owner_repo) else {
        *stream.status.lock().unwrap() = RunStatus::Err(
            "neither pnpm nor npm are on PATH — install Node.js or use git-clone fallback"
                .to_string(),
        );
        return stream;
    };
    spawn_and_stream(program, args, None, stream.clone());
    stream
}

/// Enable pnpm through Corepack. This avoids piping remote installer scripts
/// into a shell; Corepack is bundled with modern Node.js installations.
pub fn enable_pnpm_via_corepack() -> CommandStream {
    let stream = CommandStream {
        title: "enable pnpm".to_string(),
        lines: Arc::new(Mutex::new(Vec::new())),
        status: Arc::new(Mutex::new(RunStatus::Running)),
    };
    if which("corepack").is_none() {
        *stream.status.lock().unwrap() =
            RunStatus::Err("corepack not found on PATH; install pnpm manually".to_string());
        return stream;
    }
    spawn_and_stream(
        "corepack",
        vec!["enable".into(), "pnpm".into()],
        None,
        stream.clone(),
    );
    stream
}

// ---------- internals ----------

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
        // Windows: also check .cmd / .exe variants.
        if cfg!(windows) {
            for ext in ["cmd", "exe", "bat"] {
                let c = dir.join(format!("{cmd}.{ext}"));
                if c.is_file() {
                    return Some(c);
                }
            }
        }
    }
    None
}

fn spawn_and_stream(
    program: &str,
    args: Vec<String>,
    cwd: Option<std::path::PathBuf>,
    stream: CommandStream,
) {
    let mut cmd = Command::new(program);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            *stream.status.lock().unwrap() = RunStatus::Err(format!("spawn failed: {e}"));
            return;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let lines_handle = stream.lines.clone();
    let status_handle = stream.status.clone();

    thread::spawn(move || {
        // Drain both pipes concurrently.
        let t_out = stdout.map(|s| {
            let lines = lines_handle.clone();
            thread::spawn(move || stream_to_buffer(s, lines))
        });
        let t_err = stderr.map(|s| {
            let lines = lines_handle.clone();
            thread::spawn(move || stream_to_buffer(s, lines))
        });
        if let Some(t) = t_out {
            let _ = t.join();
        }
        if let Some(t) = t_err {
            let _ = t.join();
        }
        let final_status = match child.wait() {
            Ok(exit) if exit.success() => RunStatus::Ok,
            Ok(exit) => RunStatus::Err(format!("exit {}", exit.code().unwrap_or(-1))),
            Err(e) => RunStatus::Err(format!("wait failed: {e}")),
        };
        if let Ok(mut s) = status_handle.lock() {
            *s = final_status;
        }
    });
}

fn stream_to_buffer<R: std::io::Read>(reader: R, lines: Arc<Mutex<Vec<String>>>) {
    let buf = BufReader::new(reader);
    for line in buf.lines().map_while(Result::ok) {
        if let Ok(mut l) = lines.lock() {
            l.push(line);
            // Cap buffer so runaway installers don't balloon memory.
            let len = l.len();
            if len > 2000 {
                l.drain(0..(len - 2000));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_matches_expectations() {
        let (prog, args) = pnpx_argv(InstallMethod::Pnpm, "anthropic/cookbook").unwrap();
        assert_eq!(prog, "pnpx");
        assert_eq!(args, vec!["skills", "add", "anthropic/cookbook"]);
        let (prog, args) = pnpx_argv(InstallMethod::Npm, "anthropic/cookbook").unwrap();
        assert_eq!(prog, "npx");
        assert_eq!(args, vec!["-y", "skills", "add", "anthropic/cookbook"]);
        assert!(pnpx_argv(InstallMethod::None, "x/y").is_none());
    }

    #[test]
    fn detect_returns_a_variant() {
        // Just prove the function is callable on whatever the test host has.
        // No assertion on the exact variant (CI images vary).
        let _ = detect();
    }

    #[test]
    fn install_via_pnpx_with_no_node_surfaces_error_without_hanging() {
        // Simulate a PATH with nothing node-related on it and verify the
        // stream returns an `Err` status immediately.
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", "/definitely-not-a-real-dir");
        let stream = install_via_pnpx("foo/bar");
        // Give reader threads a sec if one did spawn.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let s = stream.status();
        assert!(matches!(s, RunStatus::Err(_)), "got: {s:?}");
        if let Some(p) = saved {
            std::env::set_var("PATH", p);
        } else {
            std::env::remove_var("PATH");
        }
    }
}
