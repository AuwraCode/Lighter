import { Channel, invoke } from "@tauri-apps/api/core";
import type { Batch } from "./generated/Batch";
import type { PermissionDecisionDto } from "./generated/PermissionDecisionDto";
import type { SessionConfig } from "./generated/SessionConfig";
import type { SessionInfo } from "./generated/SessionInfo";
import type { SessionSnapshot } from "./generated/SessionSnapshot";

export type BatchHandler = (batch: Batch) => void;

function makeChannel(onBatch: BatchHandler): Channel<Batch> {
  const channel = new Channel<Batch>();
  channel.onmessage = onBatch;
  return channel;
}

export function createSession(
  config: SessionConfig,
  onBatch: BatchHandler,
): Promise<SessionInfo> {
  return invoke("create_session", { config, channel: makeChannel(onBatch) });
}

export function attachSession(
  sessionId: string,
  onBatch: BatchHandler,
): Promise<SessionSnapshot> {
  return invoke("attach_session", {
    sessionId,
    channel: makeChannel(onBatch),
  });
}

export function sendUserMessage(sessionId: string, text: string): Promise<void> {
  return invoke("send_user_message", { sessionId, text });
}

export function respondPermission(
  sessionId: string,
  requestId: string,
  decision: PermissionDecisionDto,
): Promise<void> {
  return invoke("respond_permission", { sessionId, requestId, decision });
}

export function setPermissionMode(sessionId: string, mode: string): Promise<void> {
  return invoke("set_permission_mode", { sessionId, mode });
}

export function setModel(sessionId: string, model: string): Promise<void> {
  return invoke("set_model", { sessionId, model });
}

export function interruptSession(sessionId: string): Promise<void> {
  return invoke("interrupt_session", { sessionId });
}

export function stopSession(sessionId: string, graceful = true): Promise<void> {
  return invoke("stop_session", { sessionId, graceful });
}

export function removeSession(sessionId: string): Promise<void> {
  return invoke("remove_session", { sessionId });
}

export function listSessions(): Promise<SessionInfo[]> {
  return invoke("list_sessions");
}

/** Exactly one session (or none) is focused; only it receives text deltas. */
export function setFocus(sessionId: string | null): Promise<void> {
  return invoke("set_focus", { sessionId });
}
