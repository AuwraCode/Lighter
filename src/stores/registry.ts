// Global registry store: lightweight session summaries for the sidebar and
// dashboard tiles (fed at ~4 Hz), plus app-level view state. Heavy transcript
// state lives in per-session stores (see session.ts).

import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import type { RegistryBatch } from "@/lib/generated/RegistryBatch";
import type { SessionSummary } from "@/lib/generated/SessionSummary";
import * as ipc from "@/lib/ipc";

export interface RegistryState {
  ready: boolean;
  order: string[];
  sessions: Record<string, SessionSummary>;
  focusedId: string | null;
  newSessionOpen: boolean;
  paletteOpen: boolean;
  /** One-shot payload the focused composer picks up (slash command insert). */
  composerInsert: { sessionId: string; text: string; nonce: number } | null;
}

export const registryStore = createStore<RegistryState>(() => ({
  ready: false,
  order: [],
  sessions: {},
  focusedId: null,
  newSessionOpen: false,
  paletteOpen: false,
  composerInsert: null,
}));

export function useRegistry<T>(selector: (s: RegistryState) => T): T {
  return useStore(registryStore, selector);
}

function sortedOrder(sessions: Record<string, SessionSummary>): string[] {
  return Object.values(sessions)
    .sort((a, b) => Number(a.created_at_ms) - Number(b.created_at_ms))
    .map((s) => s.id);
}

function applyRegistryBatch(batch: RegistryBatch) {
  registryStore.setState((prev) => {
    const sessions = { ...prev.sessions };
    for (const s of batch.updates) sessions[s.id] = s;
    for (const id of batch.removed) delete sessions[id];
    return { sessions, order: sortedOrder(sessions) };
  });
}

/** Attach (or re-attach) the registry channel and load the current list. */
export async function initRegistry() {
  const list = await ipc.attachRegistry(applyRegistryBatch);
  const sessions: Record<string, SessionSummary> = {};
  for (const s of list) sessions[s.id] = s;
  registryStore.setState({
    ready: true,
    sessions,
    order: sortedOrder(sessions),
  });
}

export function focusSession(id: string | null) {
  registryStore.setState({ focusedId: id });
  void ipc.setFocus(id).catch(() => {});
}

export function openNewSession(open: boolean) {
  registryStore.setState({ newSessionOpen: open });
}

export function openPalette(open: boolean) {
  registryStore.setState({ paletteOpen: open });
}

export function insertIntoComposer(sessionId: string, text: string) {
  registryStore.setState({
    composerInsert: { sessionId, text, nonce: Date.now() },
  });
}
