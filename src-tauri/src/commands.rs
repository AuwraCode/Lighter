//! Thin IPC layer: every command validates input, forwards to the session
//! manager and returns quickly. Long-lived data flows through channels.

use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::presets::{Preset, Presets};
use crate::profiles::{Profile, Profiles, ProfilesInfo};
use crate::records::{Records, SessionRecord};
use crate::session::events::{
    Batch, PermissionDecisionDto, RegistryBatch, SessionConfig, SessionInfo, SessionSnapshot,
    SessionSummary, TranscriptItem,
};
use crate::session::manager::SessionManager;
use crate::session::router::SessionCommand;

// NOTE: commands that spawn processes or shell out to git are async on
// purpose. Sync commands execute on the window's event-loop thread, where
// blocking work freezes the UI and a panic aborts the whole app (observed:
// tokio::spawn without a runtime context → STATUS_STACK_BUFFER_OVERRUN).
#[tauri::command]
pub async fn create_session(
    manager: State<'_, SessionManager>,
    config: SessionConfig,
    channel: Channel<Batch>,
) -> Result<SessionInfo> {
    manager.create(config, channel)
}

#[tauri::command]
pub async fn attach_session(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    channel: Channel<Batch>,
) -> Result<SessionSnapshot> {
    manager.attach(session_id, channel).await
}

#[tauri::command]
pub fn send_user_message(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    text: String,
) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::InvalidInput("empty message".into()));
    }
    manager.command(session_id, SessionCommand::SendUser { text })
}

#[tauri::command]
pub async fn respond_permission(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    request_id: String,
    decision: PermissionDecisionDto,
) -> Result<()> {
    let (reply, rx) = oneshot::channel();
    manager.command(
        session_id,
        SessionCommand::RespondPermission {
            request_id,
            decision,
            reply,
        },
    )?;
    rx.await
        .map_err(|_| Error::SessionGone)?
        .map_err(Error::Control)
}

#[tauri::command]
pub async fn set_permission_mode(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    mode: String,
) -> Result<()> {
    let (reply, rx) = oneshot::channel();
    manager.command(session_id, SessionCommand::SetMode { mode, reply })?;
    rx.await
        .map_err(|_| Error::SessionGone)?
        .map_err(Error::Control)
}

#[tauri::command]
pub async fn set_model(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    model: String,
) -> Result<()> {
    let (reply, rx) = oneshot::channel();
    manager.command(session_id, SessionCommand::SetModel { model, reply })?;
    rx.await
        .map_err(|_| Error::SessionGone)?
        .map_err(Error::Control)
}

#[tauri::command]
pub async fn interrupt_session(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
) -> Result<()> {
    let (reply, rx) = oneshot::channel();
    manager.command(session_id, SessionCommand::Interrupt { reply })?;
    rx.await
        .map_err(|_| Error::SessionGone)?
        .map_err(Error::Control)
}

#[tauri::command]
pub fn stop_session(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    graceful: bool,
) -> Result<()> {
    manager.command(session_id, SessionCommand::Stop { graceful })
}

/// Returns a human-readable warning when worktree cleanup was refused
/// (uncommitted changes) — the session itself is removed regardless.
#[tauri::command]
pub async fn remove_session(
    manager: State<'_, SessionManager>,
    session_id: Uuid,
    cleanup_worktree: bool,
) -> Result<Option<String>> {
    manager.remove(session_id, cleanup_worktree)
}

#[tauri::command]
pub fn list_sessions(manager: State<'_, SessionManager>) -> Vec<SessionInfo> {
    manager.list()
}

/// Single view passes one id; the split view passes every visible pane.
#[tauri::command]
pub fn set_visible_sessions(manager: State<'_, SessionManager>, session_ids: Vec<Uuid>) {
    manager.set_visible(&session_ids)
}

#[tauri::command]
pub async fn attach_registry(
    manager: State<'_, SessionManager>,
    channel: Channel<RegistryBatch>,
) -> Result<Vec<SessionSummary>> {
    manager.attach_registry(channel).await
}

#[tauri::command]
pub fn list_presets(presets: State<'_, Presets>) -> Vec<Preset> {
    presets.list()
}

#[tauri::command]
pub fn save_preset(presets: State<'_, Presets>, preset: Preset) -> Result<Preset> {
    if preset.name.trim().is_empty() {
        return Err(Error::InvalidInput("preset name is required".into()));
    }
    if preset.cwd.trim().is_empty() {
        return Err(Error::InvalidInput("preset directory is required".into()));
    }
    presets.save(preset)
}

#[tauri::command]
pub fn delete_preset(presets: State<'_, Presets>, preset_id: Uuid) -> Result<()> {
    presets.delete(preset_id)
}

#[tauri::command]
pub fn list_session_records(records: State<'_, Arc<Records>>) -> Vec<SessionRecord> {
    records.list()
}

#[tauri::command]
pub fn delete_session_record(records: State<'_, Arc<Records>>, record_id: Uuid) -> Result<()> {
    records.delete(record_id)
}

/// Respawn a recorded session with `--resume`; the CLI restores the model's
/// context from its own transcript, we restore cost from the record.
#[tauri::command]
pub async fn resume_session(
    manager: State<'_, SessionManager>,
    records: State<'_, Arc<Records>>,
    record_id: Uuid,
    channel: Channel<Batch>,
) -> Result<SessionInfo> {
    let record = records
        .get(record_id)
        .ok_or_else(|| Error::InvalidInput("unknown session record".into()))?;
    if !std::path::Path::new(&record.cwd).is_dir() {
        return Err(Error::InvalidInput(format!(
            "the working directory no longer exists: {}",
            record.cwd
        )));
    }
    let cfg = SessionConfig {
        cwd: record.cwd.clone(),
        title: Some(record.title.clone()),
        // The CLI resolves aliases in init; feed the resolved id back in.
        model: (!record.model.is_empty()).then(|| record.model.clone()),
        resume_session_id: Some(record.id.to_string()),
        worktree_policy: Some("never".into()),
        // Same account as the original run — its config dir holds the transcript.
        claude_config_dir: record.claude_config_dir.clone(),
        ..Default::default()
    };
    manager.create_with_base_cost(cfg, channel, record.total_cost_usd)
}

/// Best-effort transcript backfill from the CLI's own JSONL files.
#[tauri::command]
pub async fn load_history(
    session_id: Uuid,
    cwd: String,
    claude_config_dir: Option<String>,
) -> Result<Vec<TranscriptItem>> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::history::load_history(&cwd, session_id, claude_config_dir)
    })
    .await
    .map_err(|e| Error::Control(e.to_string()))?
}

// ---------------------------------------------------------------------------
// account profiles

#[tauri::command]
pub fn list_profiles(profiles: State<'_, Profiles>) -> ProfilesInfo {
    profiles.info()
}

#[tauri::command]
pub fn save_profile(profiles: State<'_, Profiles>, profile: Profile) -> Result<Profile> {
    profiles.save(profile)
}

#[tauri::command]
pub fn delete_profile(profiles: State<'_, Profiles>, profile_id: Uuid) -> Result<()> {
    profiles.delete(profile_id)
}

#[tauri::command]
pub fn set_default_profile(profiles: State<'_, Profiles>, profile_id: Uuid) -> Result<()> {
    profiles.set_default(profile_id)
}

/// Home-dir scan for `.claude*` dirs with credentials not yet registered.
#[tauri::command]
pub fn discover_profiles(profiles: State<'_, Profiles>) -> Vec<Profile> {
    profiles.discover()
}

/// `claude auth status --json` for a config dir (None = system default).
#[tauri::command]
pub async fn profile_auth_status(config_dir: Option<String>) -> Result<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || crate::profiles::auth_status(config_dir))
        .await
        .map_err(|e| Error::Control(e.to_string()))?
}

/// Opens a console running `claude auth login` for the profile's config dir.
#[tauri::command]
pub fn open_login_terminal(config_dir: Option<String>) -> Result<()> {
    crate::profiles::open_login_terminal(config_dir)
}

/// The CLI version the protocol fixtures were captured against.
pub const TESTED_CLI_VERSION: &str = "2.1.226";

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AppInfo {
    pub claude_path: Option<String>,
    pub claude_version: Option<String>,
    pub tested_cli_version: String,
    pub app_version: String,
}

#[tauri::command]
pub async fn get_app_info(app: tauri::AppHandle) -> AppInfo {
    let app_version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let claude_path = crate::session::spawn::resolve_claude_bin()
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let claude_version = claude_path.as_ref().and_then(|path| {
            let mut cmd = std::process::Command::new(path);
            cmd.arg("--version");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            let output = cmd.output().ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            text.split_whitespace().next().map(String::from)
        });
        AppInfo {
            claude_path,
            claude_version,
            tested_cli_version: TESTED_CLI_VERSION.to_string(),
            app_version,
        }
    })
    .await
    .unwrap_or(AppInfo {
        claude_path: None,
        claude_version: None,
        tested_cli_version: TESTED_CLI_VERSION.to_string(),
        app_version: String::new(),
    })
}
