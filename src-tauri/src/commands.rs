//! Thin IPC layer: every command validates input, forwards to the session
//! manager and returns quickly. Long-lived data flows through channels.

use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::presets::{Preset, Presets};
use crate::session::events::{
    Batch, PermissionDecisionDto, RegistryBatch, SessionConfig, SessionInfo, SessionSnapshot,
    SessionSummary,
};
use crate::session::manager::SessionManager;
use crate::session::router::SessionCommand;

#[tauri::command]
pub fn create_session(
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

#[tauri::command]
pub fn remove_session(manager: State<'_, SessionManager>, session_id: Uuid) -> Result<()> {
    manager.remove(session_id)
}

#[tauri::command]
pub fn list_sessions(manager: State<'_, SessionManager>) -> Vec<SessionInfo> {
    manager.list()
}

#[tauri::command]
pub fn set_focus(manager: State<'_, SessionManager>, session_id: Option<Uuid>) {
    manager.set_focus(session_id)
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
