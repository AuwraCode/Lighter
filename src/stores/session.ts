// Per-session vanilla zustand stores. One store per session, created outside
// React so an event in one session can never re-render another session's UI.
// applyBatch performs exactly ONE setState per IPC batch.

import { createStore, type StoreApi } from "zustand/vanilla";
import type { Batch } from "@/lib/generated/Batch";
import type { DeltaKind } from "@/lib/generated/DeltaKind";
import type { ExitInfo } from "@/lib/generated/ExitInfo";
import type { HandshakeInfo } from "@/lib/generated/HandshakeInfo";
import type { PendingPermission } from "@/lib/generated/PendingPermission";
import type { SessionMeta } from "@/lib/generated/SessionMeta";
import type { SessionSnapshot } from "@/lib/generated/SessionSnapshot";
import type { SessionStats } from "@/lib/generated/SessionStats";
import type { SessionStatus } from "@/lib/generated/SessionStatus";
import type { TranscriptItem } from "@/lib/generated/TranscriptItem";
import type { TurnStats } from "@/lib/generated/TurnStats";

export interface SessionUiState {
  meta: SessionMeta | null;
  status: SessionStatus;
  items: TranscriptItem[];
  itemIndex: Map<string, number>;
  /** item ids currently streaming text (styling + footer placement). */
  streaming: Map<string, DeltaKind>;
  pending: PendingPermission[];
  stats: SessionStats | null;
  handshake: HandshakeInfo | null;
  exited: ExitInfo | null;
  lastTurn: TurnStats | null;
  lastSeq: bigint;
  hydrated: boolean;
}

export type SessionStore = StoreApi<SessionUiState>;

function initialState(): SessionUiState {
  return {
    meta: null,
    status: "Starting",
    items: [],
    itemIndex: new Map(),
    streaming: new Map(),
    pending: [],
    stats: null,
    handshake: null,
    exited: null,
    lastTurn: null,
    lastSeq: 0n,
    hydrated: false,
  };
}

const stores = new Map<string, SessionStore>();

export function getOrCreateSessionStore(sessionId: string): SessionStore {
  let store = stores.get(sessionId);
  if (!store) {
    store = createStore<SessionUiState>(initialState);
    stores.set(sessionId, store);
  }
  return store;
}

export function dropSessionStore(sessionId: string) {
  stores.delete(sessionId);
}

function buildIndex(items: TranscriptItem[]): Map<string, number> {
  const index = new Map<string, number>();
  items.forEach((item, i) => index.set(item.id, i));
  return index;
}

export function hydrateFromSnapshot(store: SessionStore, snapshot: SessionSnapshot) {
  store.setState({
    meta: snapshot.meta,
    status: snapshot.status,
    items: snapshot.items,
    itemIndex: buildIndex(snapshot.items),
    streaming: new Map(snapshot.streaming.map((t) => [t.item_id, t.kind])),
    pending: snapshot.pending_permissions,
    stats: snapshot.stats,
    handshake: snapshot.handshake,
    exited: snapshot.exited,
    lastTurn: null,
    lastSeq: BigInt(snapshot.last_seq),
    hydrated: true,
  });
}

export function applyBatch(store: SessionStore, batch: Batch) {
  store.setState((prev) => {
    const next: SessionUiState = {
      ...prev,
      items: [...prev.items],
      itemIndex: new Map(prev.itemIndex),
      streaming: new Map(prev.streaming),
    };

    const upsert = (item: TranscriptItem) => {
      const ix = next.itemIndex.get(item.id);
      if (ix == null) {
        next.itemIndex.set(item.id, next.items.length);
        next.items.push(item);
      } else {
        next.items[ix] = item;
      }
    };

    for (const env of batch.events) {
      const seq = BigInt(env.seq);
      if (seq <= next.lastSeq) continue;
      next.lastSeq = seq;
      const e = env.event;
      switch (e.type) {
        case "Ready":
          next.meta = e.meta;
          break;
        case "Handshake":
          next.handshake = e.info;
          break;
        case "Status":
          next.status = e.status;
          break;
        case "ItemStarted":
          upsert(e.item);
          if (e.item.kind === "AssistantText") next.streaming.set(e.item.id, "Text");
          if (e.item.kind === "Thinking") next.streaming.set(e.item.id, "Thinking");
          break;
        case "ItemDelta": {
          const ix = next.itemIndex.get(e.item_id);
          if (ix != null) {
            const item = next.items[ix];
            if (item.kind === "AssistantText" || item.kind === "Thinking") {
              next.items[ix] = { ...item, text: item.text + e.delta };
            }
          }
          break;
        }
        case "ItemCompleted":
          upsert(e.item);
          next.streaming.delete(e.item.id);
          break;
        case "ItemUpdated":
          upsert(e.item);
          break;
        case "TurnCompleted":
          next.lastTurn = e.stats;
          next.streaming.clear();
          break;
        case "StatsUpdated":
          next.stats = e.stats;
          break;
        case "PermissionRequested":
          if (!next.pending.some((p) => p.request_id === e.request.request_id)) {
            next.pending = [...next.pending, e.request];
          }
          break;
        case "PermissionResolved":
          next.pending = next.pending.filter((p) => p.request_id !== e.request_id);
          break;
        case "CompactResult":
          break;
        case "RateLimit":
          break;
        case "Exited":
          next.exited = { code: e.code, stderr_tail: e.stderr_tail };
          break;
        case "ProtocolError":
          console.warn("protocol error:", e.message);
          break;
      }
    }
    return next;
  });
}

/**
 * Attach flow helper: batches can arrive on the channel before the snapshot
 * promise resolves; buffer them and replay after hydration (seq filtering
 * drops anything the snapshot already contains).
 */
export function makeAttachBuffer(store: SessionStore) {
  let buffered: Batch[] | null = [];
  return {
    onBatch(batch: Batch) {
      if (buffered) {
        buffered.push(batch);
      } else {
        applyBatch(store, batch);
      }
    },
    flush(snapshot: SessionSnapshot) {
      hydrateFromSnapshot(store, snapshot);
      const pending = buffered ?? [];
      buffered = null;
      for (const batch of pending) applyBatch(store, batch);
    },
  };
}
