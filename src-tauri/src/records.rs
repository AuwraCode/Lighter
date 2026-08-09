//! Durable session records (sessions.json): everything needed to offer
//! "resume after restart". Updated from registry summaries while sessions
//! run; kept after sessions end or are removed from the dashboard.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::Result;
use crate::persistence::Store;
use crate::session::events::{SessionStatus, SessionSummary};

const FILE: &str = "sessions.json";
const MAX_RECORDS: usize = 100;
const FLUSH_MIN_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionRecord {
    pub id: Uuid,
    pub title: String,
    pub cwd: String,
    pub model: String,
    pub permission_mode: String,
    pub total_cost_usd: f64,
    pub turns: u32,
    pub last_snippet: String,
    pub worktree_branch: Option<String>,
    pub created_at_ms: u64,
    pub last_active_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecordsFile {
    version: u32,
    records: Vec<SessionRecord>,
}

pub struct Records {
    store: Store,
    inner: Mutex<Inner>,
}

struct Inner {
    map: HashMap<Uuid, SessionRecord>,
    dirty: bool,
    last_flush: Instant,
}

impl Records {
    pub fn load(store: Store) -> Records {
        let file: RecordsFile = store.load_or_default(FILE);
        let map = file.records.into_iter().map(|r| (r.id, r)).collect();
        Records {
            store,
            inner: Mutex::new(Inner {
                map,
                dirty: false,
                last_flush: Instant::now(),
            }),
        }
    }

    pub fn update_from_summary(&self, summary: &SessionSummary) {
        // Nothing durable to say about a session that never got anywhere.
        if summary.status == SessionStatus::Starting {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        let record = SessionRecord {
            id: summary.id,
            title: summary.title.clone(),
            cwd: summary.cwd.clone(),
            model: summary.model.clone(),
            permission_mode: summary.permission_mode.clone(),
            total_cost_usd: summary.total_cost_usd,
            turns: summary.turns,
            last_snippet: summary.last_snippet.clone(),
            worktree_branch: summary.worktree_branch.clone(),
            created_at_ms: summary.created_at_ms,
            last_active_ms: now_ms(),
        };
        let changed = inner
            .map
            .get(&summary.id)
            .map(|prev| {
                prev.total_cost_usd != record.total_cost_usd
                    || prev.turns != record.turns
                    || prev.last_snippet != record.last_snippet
                    || prev.title != record.title
                    || prev.model != record.model
                    || prev.permission_mode != record.permission_mode
            })
            .unwrap_or(true);
        if changed {
            inner.map.insert(summary.id, record);
            inner.dirty = true;
        }
    }

    /// Called from the registry tick (~4 Hz); writes at most every 2s.
    pub fn flush_if_dirty(&self) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.dirty || inner.last_flush.elapsed() < FLUSH_MIN_INTERVAL {
            return;
        }
        inner.dirty = false;
        inner.last_flush = Instant::now();
        let _ = self.persist(&inner);
    }

    pub fn flush(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.dirty {
            inner.dirty = false;
            inner.last_flush = Instant::now();
            let _ = self.persist(&inner);
        }
    }

    pub fn list(&self) -> Vec<SessionRecord> {
        let inner = self.inner.lock().unwrap();
        let mut records: Vec<SessionRecord> = inner.map.values().cloned().collect();
        records.sort_by_key(|r| std::cmp::Reverse(r.last_active_ms));
        records
    }

    pub fn get(&self, id: Uuid) -> Option<SessionRecord> {
        self.inner.lock().unwrap().map.get(&id).cloned()
    }

    pub fn delete(&self, id: Uuid) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.map.remove(&id);
        inner.dirty = false;
        inner.last_flush = Instant::now();
        self.persist(&inner)
    }

    fn persist(&self, inner: &Inner) -> Result<()> {
        let mut records: Vec<SessionRecord> = inner.map.values().cloned().collect();
        records.sort_by_key(|r| std::cmp::Reverse(r.last_active_ms));
        records.truncate(MAX_RECORDS);
        self.store.save(
            FILE,
            &RecordsFile {
                version: 1,
                records,
            },
        )
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: Uuid, status: SessionStatus, cost: f64) -> SessionSummary {
        SessionSummary {
            id,
            title: "t".into(),
            cwd: "C:/tmp".into(),
            status,
            model: "m".into(),
            permission_mode: "default".into(),
            total_cost_usd: cost,
            turns: 1,
            pending_permissions: 0,
            last_snippet: "hi".into(),
            context_used_tokens: None,
            context_window: None,
            exited_code: None,
            created_at_ms: 1,
            worktree_branch: None,
        }
    }

    #[test]
    fn records_survive_reload_and_skip_starting() {
        let dir = std::env::temp_dir().join(format!("lighter-records-{}", Uuid::new_v4()));
        let records = Records::load(Store::new(dir.clone()));

        let id = Uuid::new_v4();
        records.update_from_summary(&summary(id, SessionStatus::Starting, 0.0));
        assert!(records.list().is_empty(), "Starting sessions are not recorded");

        records.update_from_summary(&summary(id, SessionStatus::Idle, 0.5));
        records.flush();
        assert_eq!(records.list().len(), 1);

        let reloaded = Records::load(Store::new(dir.clone()));
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.get(id).unwrap().total_cost_usd, 0.5);

        reloaded.delete(id).unwrap();
        assert!(Records::load(Store::new(dir.clone())).list().is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}
