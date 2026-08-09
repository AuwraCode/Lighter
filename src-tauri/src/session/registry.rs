//! App-wide registry stream: coalesces per-session summaries (last write wins)
//! and flushes them to the dashboard on a 250ms tick over one global channel.
//! This is what keeps background sessions cheap for the UI — tiles re-render
//! at most 4×/s regardless of how fast sessions stream.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::events::{RegistryBatch, SessionSummary};

const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

pub enum RegistryMsg {
    Update(SessionSummary),
    Removed(Uuid),
    Attach {
        channel: Channel<RegistryBatch>,
        reply: oneshot::Sender<Vec<SessionSummary>>,
    },
}

pub fn start() -> mpsc::UnboundedSender<RegistryMsg> {
    let (tx, mut rx) = mpsc::unbounded_channel::<RegistryMsg>();
    tauri::async_runtime::spawn(async move {
        let mut latest: HashMap<Uuid, SessionSummary> = HashMap::new();
        let mut dirty: HashSet<Uuid> = HashSet::new();
        let mut removed: Vec<Uuid> = Vec::new();
        let mut sink: Option<Channel<RegistryBatch>> = None;
        let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(RegistryMsg::Update(summary)) => {
                        let changed = latest
                            .get(&summary.id)
                            .map(|prev| prev != &summary)
                            .unwrap_or(true);
                        if changed {
                            dirty.insert(summary.id);
                            latest.insert(summary.id, summary);
                        }
                    }
                    Some(RegistryMsg::Removed(id)) => {
                        latest.remove(&id);
                        dirty.remove(&id);
                        removed.push(id);
                    }
                    Some(RegistryMsg::Attach { channel, reply }) => {
                        sink = Some(channel);
                        let mut all: Vec<SessionSummary> = latest.values().cloned().collect();
                        all.sort_by_key(|s| s.created_at_ms);
                        dirty.clear();
                        removed.clear();
                        let _ = reply.send(all);
                    }
                    None => break,
                },
                _ = ticker.tick() => {
                    if sink.is_none() || (dirty.is_empty() && removed.is_empty()) {
                        continue;
                    }
                    let mut updates: Vec<SessionSummary> = dirty
                        .drain()
                        .filter_map(|id| latest.get(&id).cloned())
                        .collect();
                    updates.sort_by_key(|s| s.created_at_ms);
                    let batch = RegistryBatch {
                        updates,
                        removed: std::mem::take(&mut removed),
                    };
                    if let Some(channel) = &sink {
                        if channel.send(batch).is_err() {
                            sink = None; // webview reloading; next attach resyncs
                        }
                    }
                }
            }
        }
    });
    tx
}
