//! Spawning the `claude` process: argument construction, hidden console,
//! kill-on-job-close Job Object so the whole child tree dies with us.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use uuid::Uuid;

use crate::error::{Error, Result};

use super::events::SessionConfig;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Spawned {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
    /// Kept alive for the session's lifetime; dropping it kills the tree.
    pub job: Option<win32job::Job>,
    pub pid: Option<u32>,
    pub args: Vec<String>,
}

pub fn resolve_claude_bin() -> Result<PathBuf> {
    which::which("claude")
        .map_err(|e| Error::Spawn(format!("claude binary not found on PATH: {e}")))
}

pub fn build_args(session_id: Uuid, cfg: &SessionConfig) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--replay-user-messages",
        "--forward-subagent-text",
        "--permission-prompt-tool",
        "stdio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    match &cfg.resume_session_id {
        Some(id) => {
            args.push("--resume".into());
            args.push(id.clone());
        }
        None => {
            args.push("--session-id".into());
            args.push(session_id.to_string());
        }
    }
    if let Some(model) = &cfg.model {
        args.push("--model".into());
        args.push(model.clone());
    }
    if let Some(mode) = &cfg.permission_mode {
        args.push("--permission-mode".into());
        args.push(mode.clone());
    }
    if let Some(effort) = &cfg.effort {
        args.push("--effort".into());
        args.push(effort.clone());
    }
    if let Some(title) = &cfg.title {
        if !title.is_empty() {
            args.push("--name".into());
            args.push(title.clone());
        }
    }
    if !cfg.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.extend(cfg.allowed_tools.iter().cloned());
    }
    if !cfg.disallowed_tools.is_empty() {
        args.push("--disallowedTools".into());
        args.extend(cfg.disallowed_tools.iter().cloned());
    }
    if let Some(prompt) = &cfg.append_system_prompt {
        if !prompt.is_empty() {
            args.push("--append-system-prompt".into());
            args.push(prompt.clone());
        }
    }
    args
}

pub fn spawn_session(session_id: Uuid, cfg: &SessionConfig) -> Result<Spawned> {
    let bin = resolve_claude_bin()?;
    let cwd = PathBuf::from(&cfg.cwd);
    if !cwd.is_dir() {
        return Err(Error::InvalidInput(format!(
            "working directory does not exist: {}",
            cfg.cwd
        )));
    }
    let args = build_args(session_id, cfg);

    let mut cmd = Command::new(&bin);
    cmd.args(&args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Backstop only — the Job Object is the real cleanup mechanism.
        .kill_on_drop(true);
    // Account selection: each Claude config dir carries its own credentials.
    if let Some(dir) = &cfg.claude_config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Spawn(format!("{}: {e}", bin.display())))?;

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let pid = child.id();

    let job = assign_job(&child);
    if job.is_none() {
        tracing::warn!(?pid, "failed to assign job object; falling back to kill_on_drop");
    }

    Ok(Spawned {
        child,
        stdin,
        stdout,
        stderr,
        job,
        pid,
        args,
    })
}

/// Put the child (and every descendant) into a Job Object configured to kill
/// the whole tree when the job handle closes — including when Lighter itself
/// crashes, since the OS closes the handle for us.
fn assign_job(child: &Child) -> Option<win32job::Job> {
    let job = win32job::Job::create().ok()?;
    let mut info = job.query_extended_limit_info().ok()?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info).ok()?;
    let handle = child.raw_handle()?;
    job.assign_process(handle as isize).ok()?;
    Some(job)
}
