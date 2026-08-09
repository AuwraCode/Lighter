//! Registry of live sessions. Holds only cheap command senders — all real
//! state lives inside each session's router task.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::records::Records;
use crate::worktree::{self, WorktreeMeta};

use super::events::{
    Batch, RegistryBatch, SessionConfig, SessionInfo, SessionSnapshot, SessionStatus,
    SessionSummary,
};
use super::registry::{self, RegistryMsg};
use super::router::{self, SessionCommand};
use super::spawn;
use super::state::SessionState;

pub struct SessionHandle {
    pub tx: mpsc::UnboundedSender<SessionCommand>,
    pub title: String,
    pub cwd: String,
    /// Case-insensitive repo-root key when the cwd is inside a git repo.
    pub repo_key: Option<String>,
    pub worktree: Option<WorktreeMeta>,
}

pub struct SessionManager {
    sessions: Mutex<HashMap<Uuid, SessionHandle>>,
    registry_tx: OnceLock<mpsc::UnboundedSender<RegistryMsg>>,
    /// Serializes "inspect repo → maybe create worktree → register" so two
    /// simultaneous launches into one repo can't both skip isolation.
    worktree_lock: Mutex<()>,
    records: Arc<Records>,
    /// Config dirs whose skill plugins were provisioned this app-run (keyed by
    /// config dir, "" for the default). Guards against re-running per session.
    skills_ensured: AsyncMutex<HashSet<String>>,
}

/// Test-only convenience: records land in a temp directory.
impl Default for SessionManager {
    fn default() -> Self {
        let dir = std::env::temp_dir().join("lighter-test-records");
        SessionManager::new(Arc::new(Records::load(crate::persistence::Store::new(dir))))
    }
}

impl SessionManager {
    pub fn new(records: Arc<Records>) -> SessionManager {
        SessionManager {
            sessions: Mutex::new(HashMap::new()),
            registry_tx: OnceLock::new(),
            worktree_lock: Mutex::new(()),
            records,
            skills_ensured: AsyncMutex::new(HashSet::new()),
        }
    }

    /// Ensure the given skill plugins are installed for this account, once per
    /// config dir per app-run. Runs the (blocking) `claude plugin` commands off
    /// the async runtime; failures are logged, never fatal to session launch.
    pub async fn ensure_skills(&self, config_dir: Option<String>, plugins: Vec<String>) {
        if plugins.is_empty() {
            return;
        }
        let key = config_dir.clone().unwrap_or_default();
        let mut ensured = self.skills_ensured.lock().await;
        if ensured.contains(&key) {
            return;
        }
        let dir = config_dir.clone();
        let done = tauri::async_runtime::spawn_blocking(move || {
            crate::skills::ensure(dir.as_deref(), &plugins)
        })
        .await;
        match done {
            Ok(Ok(())) => {
                ensured.insert(key);
            }
            Ok(Err(e)) => tracing::warn!(%e, "skill provisioning failed"),
            Err(e) => tracing::warn!(%e, "skill provisioning task panicked"),
        }
    }

    /// The registry task is spawned lazily on first use (commands run inside
    /// the tauri async runtime; construction happens before it exists).
    fn registry(&self) -> &mpsc::UnboundedSender<RegistryMsg> {
        self.registry_tx
            .get_or_init(|| registry::start(self.records.clone()))
    }

    pub async fn attach_registry(
        &self,
        channel: Channel<RegistryBatch>,
    ) -> Result<Vec<SessionSummary>> {
        let (reply, rx) = oneshot::channel();
        self.registry()
            .send(RegistryMsg::Attach { channel, reply })
            .map_err(|_| Error::SessionGone)?;
        rx.await.map_err(|_| Error::SessionGone)
    }
    pub fn create(&self, cfg: SessionConfig, channel: Channel<Batch>) -> Result<SessionInfo> {
        self.create_with_base_cost(cfg, channel, 0.0)
    }

    /// `base_cost`: cumulative spend from previous processes of this session
    /// (resume) so the UI keeps counting from where it left off.
    pub fn create_with_base_cost(
        &self,
        mut cfg: SessionConfig,
        channel: Channel<Batch>,
        base_cost: f64,
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

        // Worktree isolation. The lock spans decision→creation→registration
        // so two parallel launches into one repo serialize correctly.
        let mut worktree_meta: Option<WorktreeMeta> = None;
        let mut repo_key: Option<String> = None;
        let policy = cfg.worktree_policy.clone().unwrap_or_else(|| "auto".into());
        if policy != "never" && cfg.resume_session_id.is_none() {
            let _guard = self.worktree_lock.lock().unwrap();
            if let Some(root) = worktree::repo_root(&cfg.cwd) {
                let key = worktree::repo_key(&root);
                let active_on_repo = {
                    let sessions = self.sessions.lock().unwrap();
                    sessions
                        .values()
                        .filter(|h| h.repo_key.as_deref() == Some(key.as_str()))
                        .count()
                };
                if worktree::should_isolate(&policy, active_on_repo) {
                    let slug = worktree::slugify(&title);
                    let suffix = &session_id.simple().to_string()[..4];
                    match worktree::create(&root, &slug, suffix) {
                        Ok(meta) => {
                            if worktree::is_dirty(&root) {
                                tracing::warn!(
                                    repo = %root.display(),
                                    "parent checkout has uncommitted changes; they are NOT carried into the worktree"
                                );
                            }
                            cfg.cwd = meta.path.clone();
                            worktree_meta = Some(meta);
                        }
                        Err(e) => {
                            // Never block a launch on isolation problems
                            // (e.g. repo without any commit yet).
                            tracing::warn!(%e, "worktree isolation failed; using the repo directly");
                        }
                    }
                }
                repo_key = Some(key);
            }
        }

        let spawned = spawn::spawn_session(session_id, &cfg)?;
        let mut state = SessionState::new(
            session_id,
            title.clone(),
            cfg.cwd.clone(),
            worktree_meta.clone(),
            cfg.claude_config_dir.clone(),
        );
        if base_cost > 0.0 {
            state.set_base_cost(base_cost);
        }
        let initial_prompt = cfg.initial_prompt.clone().filter(|p| !p.trim().is_empty());
        let tx = router::start(
            session_id,
            state,
            spawned,
            initial_prompt,
            Some(channel),
            self.registry().clone(),
        );

        self.sessions.lock().unwrap().insert(
            session_id,
            SessionHandle {
                tx,
                title: title.clone(),
                cwd: cfg.cwd.clone(),
                repo_key,
                worktree: worktree_meta,
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
    /// Returns a warning when worktree cleanup was requested but refused.
    pub fn remove(&self, id: Uuid, cleanup_worktree: bool) -> Result<Option<String>> {
        let handle = self
            .sessions
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or(Error::SessionNotFound)?;
        let _ = handle.tx.send(SessionCommand::Stop { graceful: true });
        let _ = self.registry().send(RegistryMsg::Removed(id));
        if !cleanup_worktree {
            return Ok(None);
        }
        let Some(meta) = handle.worktree else {
            return Ok(None);
        };
        match worktree::remove(&meta, false) {
            Ok(()) => Ok(None),
            Err(e) => Ok(Some(format!(
                "session removed, but the worktree was kept: {e}"
            ))),
        }
    }

    pub fn stop_all(&self) {
        let sessions = self.sessions.lock().unwrap();
        for handle in sessions.values() {
            let _ = handle.tx.send(SessionCommand::Stop { graceful: true });
        }
    }

    /// Sessions currently visible in the UI (single view: one id; split
    /// view: every pane). Only visible sessions receive text deltas.
    pub fn set_visible(&self, visible: &[Uuid]) {
        let sessions = self.sessions.lock().unwrap();
        for (id, handle) in sessions.iter() {
            let _ = handle
                .tx
                .send(SessionCommand::SetFocus(visible.contains(id)));
        }
    }
}

fn default_title(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Session".to_string())
}
