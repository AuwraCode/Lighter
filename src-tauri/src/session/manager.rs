//! Registry of live sessions. Holds only cheap command senders — all real
//! state lives inside each session's router task.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::error::{Error, Result};

use super::events::{Batch, SessionConfig, SessionInfo, SessionSnapshot, SessionStatus};
use super::router::{self, SessionCommand};
use super::spawn;
use super::state::SessionState;

pub struct SessionHandle {
    pub tx: mpsc::UnboundedSender<SessionCommand>,
    pub title: String,
    pub cwd: String,
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<Uuid, SessionHandle>>,
}

impl SessionManager {
    pub fn create(
        &self,
        mut cfg: SessionConfig,
        channel: Channel<Batch>,
    ) -> Result<SessionInfo> {
        let session_id = match &cfg.resume_session_id {
            Some(id) => Uuid::parse_str(id)
                .map_err(|_| Error::InvalidInput(format!("invalid session id: {id}")))?,
            None => Uuid::new_v4(),
        };
        {
            let sessions = self.sessions.lock().unwrap();
            if sessions.contains_key(&session_id) {
                return Err(Error::InvalidInput(
                    "session with this id is already running".into(),
                ));
            }
        }

        let title = cfg
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| default_title(&cfg.cwd));
        cfg.title = Some(title.clone());

        let spawned = spawn::spawn_session(session_id, &cfg)?;
        let state = SessionState::new(session_id, title.clone(), cfg.cwd.clone());
        let initial_prompt = cfg.initial_prompt.clone().filter(|p| !p.trim().is_empty());
        let tx = router::start(session_id, state, spawned, initial_prompt, Some(channel));

        self.sessions.lock().unwrap().insert(
            session_id,
            SessionHandle {
                tx,
                title: title.clone(),
                cwd: cfg.cwd.clone(),
            },
        );

        Ok(SessionInfo {
            id: session_id,
            title,
            cwd: cfg.cwd,
            status: SessionStatus::Starting,
        })
    }

    pub fn command(&self, id: Uuid, cmd: SessionCommand) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let handle = sessions.get(&id).ok_or(Error::SessionNotFound)?;
        handle.tx.send(cmd).map_err(|_| Error::SessionGone)
    }

    pub async fn attach(&self, id: Uuid, channel: Channel<Batch>) -> Result<SessionSnapshot> {
        let (reply, rx) = oneshot::channel();
        self.command(id, SessionCommand::Attach { channel, reply })?;
        rx.await.map_err(|_| Error::SessionGone)
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .iter()
            .map(|(id, h)| SessionInfo {
                id: *id,
                title: h.title.clone(),
                cwd: h.cwd.clone(),
                // Live status flows through the event channel; this listing is
                // only used for enumeration.
                status: SessionStatus::Starting,
            })
            .collect()
    }

    /// Remove the session from the registry. Dropping the command sender ends
    /// the router; the router's Job handle kills any surviving process tree.
    pub fn remove(&self, id: Uuid) -> Result<()> {
        let handle = self
            .sessions
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or(Error::SessionNotFound)?;
        let _ = handle.tx.send(SessionCommand::Stop { graceful: true });
        Ok(())
    }

    pub fn stop_all(&self) {
        let sessions = self.sessions.lock().unwrap();
        for handle in sessions.values() {
            let _ = handle.tx.send(SessionCommand::Stop { graceful: true });
        }
    }

    /// Exactly one session (or none) is focused; only it receives deltas.
    pub fn set_focus(&self, focused: Option<Uuid>) {
        let sessions = self.sessions.lock().unwrap();
        for (id, handle) in sessions.iter() {
            let _ = handle
                .tx
                .send(SessionCommand::SetFocus(Some(*id) == focused));
        }
    }
}

fn default_title(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Session".to_string())
}
