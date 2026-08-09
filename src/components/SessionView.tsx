// Focused session view: virtualized transcript, composer, live controls,
// permissions. Mounted with key={sessionId}; always (re)attaches on mount —
// the atomic snapshot makes that lossless.

import { useCallback, useEffect, useRef, useState } from "react";
import { useStore } from "zustand";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { toast } from "sonner";
import {
  ArrowDown,
  ArrowLeft,
  Check,
  CircleStop,
  Columns2,
  GitBranch,
  History as HistoryIcon,
  Loader2,
  Send,
  ShieldQuestion,
  X,
} from "lucide-react";
import { addSessionToWorkspace } from "@/stores/workspace";
import { openWorkspace } from "@/stores/registry";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { PendingPermission } from "@/lib/generated/PendingPermission";
import {
  getOrCreateSessionStore,
  makeAttachBuffer,
  prependHistory,
} from "@/stores/session";
import { focusSession, registryStore, useRegistry } from "@/stores/registry";
import { statusColor } from "@/lib/status";
import { MemoItemView } from "./TranscriptItems";
import { AccountChip, ModelSwitcher, ModeSwitcher } from "./SessionControls";

export function SessionView({
  sessionId,
  chrome = "full",
  active = true,
}: {
  sessionId: string;
  /** "none" = transcript+composer only (split-view panes bring their own header). */
  chrome?: "full" | "none";
  /** Only the active view responds to Esc (split view has many panes). */
  active?: boolean;
}) {
  const store = getOrCreateSessionStore(sessionId);
  const status = useStore(store, (s) => s.status);
  const meta = useStore(store, (s) => s.meta);
  const stats = useStore(store, (s) => s.stats);
  const items = useStore(store, (s) => s.items);
  const streaming = useStore(store, (s) => s.streaming);
  const pending = useStore(store, (s) => s.pending);
  const exited = useStore(store, (s) => s.exited);
  const handshake = useStore(store, (s) => s.handshake);

  const [attachError, setAttachError] = useState<string | null>(null);

  useEffect(() => {
    const buffer = makeAttachBuffer(store);
    ipc
      .attachSession(sessionId, buffer.onBatch)
      .then(buffer.flush)
      .catch((e) => setAttachError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const atBottomRef = useRef(true);
  const [showJump, setShowJump] = useState(false);

  const scrollToBottom = useCallback(() => {
    virtuosoRef.current?.scrollToIndex({ index: "LAST", align: "end" });
  }, []);

  // followOutput only reacts to count changes; streaming grows item heights,
  // so nudge the scroller on every batch while pinned to the bottom.
  useEffect(() => {
    if (atBottomRef.current) {
      requestAnimationFrame(scrollToBottom);
    }
  }, [items, scrollToBottom]);

  // Esc: deny+interrupt the newest pending approval, else plain interrupt.
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (!activeRef.current) return;
      const ui = registryStore.getState();
      if (ui.newSessionOpen || ui.paletteOpen) return;
      const newest = pendingRef.current[pendingRef.current.length - 1];
      if (newest) {
        void ipc
          .respondPermission(sessionId, newest.request_id, {
            allow: false,
            use_suggestions: false,
            message: "Interrupted by user",
            interrupt: true,
          })
          .catch(() => {});
      } else {
        void ipc.interruptSession(sessionId).catch(() => {});
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [sessionId]);

  const changeMode = useCallback(
    (mode: string) => {
      void ipc
        .setPermissionMode(sessionId, mode)
        .catch((e) => toast.error(`Mode change failed: ${e}`));
    },
    [sessionId],
  );

  const changeModel = useCallback(
    (model: string) => {
      void ipc
        .setModel(sessionId, model)
        .catch((e) => toast.error(`Model change failed: ${e}`));
    },
    [sessionId],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {chrome === "full" && (
      <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-1.5 text-xs text-fg-secondary">
        <button
          onClick={() => focusSession(null)}
          className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-fg-muted hover:bg-hover hover:text-fg"
          title="Back to dashboard (Ctrl+D)"
        >
          <ArrowLeft size={13} />
        </button>
        <span className={cn("h-2 w-2 shrink-0 rounded-full", statusColor(status))} />
        <span className="max-w-40 shrink-0 truncate font-medium text-fg">
          {meta?.title ?? "Session"}
        </span>
        {meta?.worktree && (
          <span
            className="inline-flex shrink-0 items-center gap-1 rounded bg-accent/10 px-1.5 py-0.5 font-mono text-[10px] text-accent"
            title={`Isolated worktree: ${meta.worktree.path}`}
          >
            <GitBranch size={10} />
            {meta.worktree.branch}
          </span>
        )}
        <span className="w-24 shrink-0">{status}</span>

        <ModelSwitcher
          current={meta?.model ?? ""}
          handshake={handshake}
          disabled={!!exited}
          onChange={changeModel}
        />
        <ModeSwitcher
          current={meta?.permission_mode ?? "default"}
          disabled={!!exited}
          onChange={changeMode}
        />
        <AccountChip handshake={handshake} />

        {attachError && (
          <span className="truncate text-danger" title={attachError}>
            {attachError}
          </span>
        )}
        {stats && (
          <span className="ml-auto shrink-0 font-mono text-fg-muted">
            ${stats.total_cost_usd.toFixed(4)} · {stats.turns} turns
            {stats.context_used_tokens != null && stats.context_window != null && (
              <>
                {" "}
                · ctx{" "}
                {Math.round(
                  (Number(stats.context_used_tokens) / Number(stats.context_window)) * 100,
                )}
                %
              </>
            )}
          </span>
        )}
        <button
          onClick={() => {
            addSessionToWorkspace(sessionId);
            openWorkspace();
          }}
          className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-fg-muted hover:bg-hover hover:text-fg"
          title="Open in split view (Ctrl+G)"
        >
          <Columns2 size={13} />
        </button>
        <button
          onClick={() => void ipc.stopSession(sessionId)}
          className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-fg-muted hover:bg-hover hover:text-danger"
          title="Stop session"
        >
          <CircleStop size={13} />
        </button>
      </div>
      )}

      <div className="relative min-h-0 flex-1 select-text">
        <Virtuoso
          ref={virtuosoRef}
          style={{ height: "100%" }}
          data={items}
          computeItemKey={(_, item) => item.id}
          increaseViewportBy={{ top: 600, bottom: 600 }}
          initialTopMostItemIndex={Math.max(0, items.length - 1)}
          followOutput={(isAtBottom) => (isAtBottom ? "auto" : false)}
          atBottomThreshold={80}
          atBottomStateChange={(isAtBottom) => {
            atBottomRef.current = isAtBottom;
            setShowJump(!isAtBottom);
          }}
          itemContent={(_, item) => (
            <div className="mx-auto w-full max-w-3xl px-4 pb-3">
              <MemoItemView item={item} streaming={streaming.has(item.id)} />
            </div>
          )}
          components={{
            Header: () => (
              <div className="mx-auto w-full max-w-3xl px-4 py-3">
                <LoadHistoryButton sessionId={sessionId} />
              </div>
            ),
            Footer: () => (
              <div className="mx-auto w-full max-w-3xl px-4 pb-3">
                {exited && (
                  <div className="rounded-lg border border-danger/40 bg-danger/10 p-3 text-xs">
                    <div className="font-medium text-danger">
                      Process exited (code {exited.code ?? "?"})
                    </div>
                    {exited.stderr_tail && (
                      <pre className="mt-2 overflow-x-auto whitespace-pre-wrap font-mono text-fg-secondary">
                        {exited.stderr_tail}
                      </pre>
                    )}
                  </div>
                )}
              </div>
            ),
          }}
        />
        {showJump && (
          <button
            onClick={scrollToBottom}
            className="absolute bottom-3 left-1/2 z-10 inline-flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-border bg-elevated px-3 py-1.5 text-[11px] text-fg-secondary shadow-lg hover:bg-hover hover:text-fg"
          >
            <ArrowDown size={11} /> Latest
          </button>
        )}
      </div>

      {pending.length > 0 && (
        <div className="flex flex-col gap-2 border-t border-border-subtle px-4 py-2">
          {pending.map((p) => (
            <PermissionCard key={p.request_id} sessionId={sessionId} pending={p} />
          ))}
        </div>
      )}

      <Composer sessionId={sessionId} disabled={!!exited} />
    </div>
  );
}

/** Backfills earlier turns from the CLI's transcript (best effort). */
function LoadHistoryButton({ sessionId }: { sessionId: string }) {
  const store = getOrCreateSessionStore(sessionId);
  const historyLoaded = useStore(store, (s) => s.historyLoaded);
  const cwd = useStore(store, (s) => s.meta?.cwd ?? "");
  const configDir = useStore(store, (s) => s.meta?.claude_config_dir ?? null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  if (historyLoaded || failed || !cwd) return null;

  const load = async () => {
    setBusy(true);
    try {
      const history = await ipc.loadHistory(sessionId, cwd, configDir);
      prependHistory(store, history);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <button
      onClick={load}
      disabled={busy}
      className="mx-auto flex items-center gap-1.5 rounded-full border border-border px-3 py-1 text-[11px] text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-50"
    >
      {busy ? (
        <Loader2 size={11} className="animate-spin" />
      ) : (
        <HistoryIcon size={11} />
      )}
      Load earlier conversation
    </button>
  );
}

/** Human-oriented one-liner for a tool call's input. */
function describeInput(pending: PendingPermission): string {
  if (pending.description) return pending.description;
  const input = pending.input as Record<string, unknown> | null;
  if (input && typeof input === "object") {
    if (typeof input.command === "string") return input.command;
    if (typeof input.file_path === "string") return input.file_path;
  }
  return JSON.stringify(pending.input).slice(0, 160);
}

function PermissionCard({
  sessionId,
  pending,
}: {
  sessionId: string;
  pending: PendingPermission;
}) {
  const [denying, setDenying] = useState(false);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  const respond = (allow: boolean, useSuggestions = false, denyMessage?: string) => {
    setBusy(true);
    void ipc
      .respondPermission(sessionId, pending.request_id, {
        allow,
        use_suggestions: useSuggestions,
        message: allow ? null : denyMessage || "Denied by user",
        interrupt: false,
      })
      .catch(() => {})
      .finally(() => setBusy(false));
  };

  const hasSuggestions =
    Array.isArray(pending.suggestions) && pending.suggestions.length > 0;

  return (
    <div className="rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
      <div className="flex items-center gap-2">
        <ShieldQuestion size={14} className="shrink-0 text-warning" />
        <span className="font-medium text-warning">
          {pending.display_name ?? pending.tool_name}
        </span>
        <span className="truncate font-mono text-fg-secondary" title={describeInput(pending)}>
          {describeInput(pending)}
        </span>
        <div className="ml-auto flex shrink-0 gap-1.5">
          {!denying ? (
            <>
              <button
                disabled={busy}
                onClick={() => respond(true)}
                className="inline-flex items-center gap-1 rounded bg-success/20 px-2 py-1 font-medium text-success hover:bg-success/30 disabled:opacity-50"
              >
                <Check size={12} /> Allow
              </button>
              {hasSuggestions && (
                <button
                  disabled={busy}
                  onClick={() => respond(true, true)}
                  title="Also apply the CLI's suggested permission rule for this session"
                  className="rounded bg-success/10 px-2 py-1 text-success hover:bg-success/20 disabled:opacity-50"
                >
                  Always
                </button>
              )}
              <button
                disabled={busy}
                onClick={() => setDenying(true)}
                className="inline-flex items-center gap-1 rounded bg-danger/20 px-2 py-1 font-medium text-danger hover:bg-danger/30 disabled:opacity-50"
              >
                <X size={12} /> Deny
              </button>
            </>
          ) : (
            <>
              <input
                autoFocus
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") respond(false, false, message);
                  if (e.key === "Escape") {
                    e.stopPropagation();
                    setDenying(false);
                  }
                }}
                placeholder="Reason (optional) — Enter to deny"
                className="w-64 rounded border border-border bg-elevated px-2 py-1 text-xs focus:border-danger focus:outline-none"
              />
              <button
                disabled={busy}
                onClick={() => respond(false, false, message)}
                className="rounded bg-danger/20 px-2 py-1 font-medium text-danger hover:bg-danger/30"
              >
                Deny
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function Composer({ sessionId, disabled }: { sessionId: string; disabled: boolean }) {
  const [text, setText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Palette "insert slash command" payloads.
  const insert = useRegistry((s) => s.composerInsert);
  useEffect(() => {
    if (insert && insert.sessionId === sessionId) {
      setText(insert.text);
      registryStore.setState({ composerInsert: null });
      requestAnimationFrame(() => textareaRef.current?.focus());
    }
  }, [insert, sessionId]);

  const send = useCallback(() => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText("");
    void ipc.sendUserMessage(sessionId, trimmed).catch((e) => toast.error(String(e)));
  }, [sessionId, text]);

  return (
    <div className="border-t border-border-subtle p-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              send();
            }
          }}
          disabled={disabled}
          rows={2}
          placeholder="Message Claude… (Ctrl+Enter to send, Esc to interrupt, Ctrl+K commands)"
          className="min-h-[3rem] flex-1 resize-none rounded-lg border border-border bg-elevated px-3 py-2 text-sm placeholder:text-fg-muted focus:border-accent focus:outline-none disabled:opacity-50"
        />
        <button
          onClick={send}
          disabled={disabled || !text.trim()}
          className="inline-flex h-9 w-9 items-center justify-center rounded-lg bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
        >
          <Send size={15} />
        </button>
      </div>
    </div>
  );
}
