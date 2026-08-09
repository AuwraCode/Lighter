// Phase-2..4 development screen: drive ONE session end-to-end.
// The real dashboard/session views replace this in later phases.

import { useCallback, useEffect, useRef, useState } from "react";
import { useStore } from "zustand";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import {
  Check,
  CircleStop,
  Folder,
  Loader2,
  Play,
  Send,
  ShieldQuestion,
  Squircle,
  X,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { HandshakeInfo } from "@/lib/generated/HandshakeInfo";
import type { PendingPermission } from "@/lib/generated/PendingPermission";
import type { SessionStatus } from "@/lib/generated/SessionStatus";
import type { TranscriptItem } from "@/lib/generated/TranscriptItem";
import {
  applyBatch,
  getOrCreateSessionStore,
  makeAttachBuffer,
  type SessionStore,
} from "@/stores/session";

const MODELS = ["haiku", "sonnet", "opus[1m]", "default"];
// The set_permission_mode control request accepts "default" (not "manual",
// which is flag-only) — see PROTOCOL.md.
const MODES = ["default", "plan", "acceptEdits", "auto", "dontAsk", "bypassPermissions"];

function statusColor(status: SessionStatus): string {
  switch (status) {
    case "Working":
    case "Compacting":
      return "bg-accent";
    case "AwaitingApproval":
      return "bg-warning";
    case "Idle":
      return "bg-success";
    case "Failed":
      return "bg-danger";
    case "Exited":
      return "bg-fg-muted";
    default:
      return "bg-fg-muted";
  }
}

export function DevSession() {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [cwd, setCwd] = useState("");
  const [model, setModel] = useState("haiku");
  const [mode, setMode] = useState("default");
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Re-attach after a webview reload (dev HMR): pick up the first session.
  useEffect(() => {
    if (sessionId) return;
    ipc.listSessions().then((sessions) => {
      const first = sessions[0];
      if (!first) return;
      const store = getOrCreateSessionStore(first.id);
      const buffer = makeAttachBuffer(store);
      ipc
        .attachSession(first.id, buffer.onBatch)
        .then((snapshot) => {
          buffer.flush(snapshot);
          setSessionId(first.id);
          void ipc.setFocus(first.id);
        })
        .catch((e) => setError(String(e)));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const start = useCallback(async () => {
    if (!cwd) {
      setError("Pick a working directory first.");
      return;
    }
    setStarting(true);
    setError(null);
    try {
      // Create the store first so the channel handler has somewhere to write.
      const pendingBatches: Parameters<typeof applyBatch>[1][] = [];
      let store: SessionStore | null = null;
      const info = await ipc.createSession(
        { ...defaultConfig(), cwd, model, permission_mode: mode },
        (batch) => {
          if (store) {
            applyBatch(store, batch);
          } else {
            pendingBatches.push(batch);
          }
        },
      );
      store = getOrCreateSessionStore(info.id);
      for (const b of pendingBatches) applyBatch(store, b);
      setSessionId(info.id);
      void ipc.setFocus(info.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  }, [cwd, model, mode]);

  const pickFolder = useCallback(async () => {
    const dir = await openFolder({ directory: true });
    if (typeof dir === "string") setCwd(dir);
  }, []);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-2">
        <button
          onClick={pickFolder}
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-elevated px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
        >
          <Folder size={13} />
          {cwd ? cwd : "Pick folder"}
        </button>
        <select
          value={model}
          onChange={(e) => setModel(e.target.value)}
          className="rounded-md border border-border bg-elevated px-2 py-1.5 text-xs"
        >
          {MODELS.map((m) => (
            <option key={m}>{m}</option>
          ))}
        </select>
        <select
          value={mode}
          onChange={(e) => setMode(e.target.value)}
          className="rounded-md border border-border bg-elevated px-2 py-1.5 text-xs"
        >
          {MODES.map((m) => (
            <option key={m}>{m}</option>
          ))}
        </select>
        <button
          onClick={start}
          disabled={starting}
          className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
        >
          {starting ? <Loader2 size={13} className="animate-spin" /> : <Play size={13} />}
          New session
        </button>
        {error && <span className="truncate text-xs text-danger">{error}</span>}
      </div>

      {sessionId ? (
        <SessionPane key={sessionId} sessionId={sessionId} />
      ) : (
        <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
          Start a session to begin.
        </div>
      )}
    </div>
  );
}

function defaultConfig() {
  return {
    cwd: "",
    title: null,
    model: null,
    permission_mode: null,
    effort: null,
    allowed_tools: [],
    disallowed_tools: [],
    append_system_prompt: null,
    initial_prompt: null,
    resume_session_id: null,
  };
}

function SessionPane({ sessionId }: { sessionId: string }) {
  const store = getOrCreateSessionStore(sessionId);
  const status = useStore(store, (s) => s.status);
  const meta = useStore(store, (s) => s.meta);
  const stats = useStore(store, (s) => s.stats);
  const items = useStore(store, (s) => s.items);
  const pending = useStore(store, (s) => s.pending);
  const exited = useStore(store, (s) => s.exited);
  const handshake = useStore(store, (s) => s.handshake);

  const [controlError, setControlError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [items]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  }, []);

  // Esc: deny+interrupt the newest pending approval, else plain interrupt.
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
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
      setControlError(null);
      void ipc.setPermissionMode(sessionId, mode).catch((e) => setControlError(String(e)));
    },
    [sessionId],
  );

  const changeModel = useCallback(
    (model: string) => {
      setControlError(null);
      void ipc.setModel(sessionId, model).catch((e) => setControlError(String(e)));
    },
    [sessionId],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border-subtle px-4 py-1.5 text-xs text-fg-secondary">
        <span className={cn("h-2 w-2 shrink-0 rounded-full", statusColor(status))} />
        <span className="w-28 shrink-0">{status}</span>

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

        {controlError && (
          <span className="truncate text-danger" title={controlError}>
            {controlError}
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
          onClick={() => void ipc.stopSession(sessionId)}
          className="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-fg-muted hover:bg-hover hover:text-danger"
          title="Stop session"
        >
          <CircleStop size={13} />
        </button>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 select-text overflow-y-auto px-4 py-3"
      >
        <div className="mx-auto flex max-w-3xl flex-col gap-3">
          {items.map((item) => (
            <ItemView key={item.id} item={item} />
          ))}
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

function ModeSwitcher({
  current,
  disabled,
  onChange,
}: {
  current: string;
  disabled: boolean;
  onChange: (mode: string) => void;
}) {
  const options = MODES.includes(current) ? MODES : [current, ...MODES];
  return (
    <select
      value={current}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      title="Permission mode (live)"
      className="shrink-0 rounded border border-border bg-elevated px-1.5 py-0.5 text-xs disabled:opacity-50"
    >
      {options.map((m) => (
        <option key={m} value={m}>
          {m}
        </option>
      ))}
    </select>
  );
}

interface ModelOption {
  value: string;
  displayName: string;
  resolvedModel?: string;
}

function ModelSwitcher({
  current,
  handshake,
  disabled,
  onChange,
}: {
  current: string;
  handshake: HandshakeInfo | null;
  disabled: boolean;
  onChange: (model: string) => void;
}) {
  const models: ModelOption[] = Array.isArray(handshake?.models)
    ? (handshake!.models as unknown as ModelOption[])
    : [];
  // meta.model is the RESOLVED id (claude-haiku-4-5-…); options use aliases.
  const selected =
    models.find((m) => m.resolvedModel === current || m.value === current)?.value ??
    current;
  const hasSelected = models.some((m) => m.value === selected);
  return (
    <select
      value={selected}
      disabled={disabled || models.length === 0}
      onChange={(e) => onChange(e.target.value)}
      title="Model (live)"
      className="max-w-40 shrink-0 truncate rounded border border-border bg-elevated px-1.5 py-0.5 text-xs disabled:opacity-50"
    >
      {!hasSelected && <option value={selected}>{selected || "model"}</option>}
      {models.map((m) => (
        <option key={m.value} value={m.value}>
          {m.displayName}
        </option>
      ))}
    </select>
  );
}

function ItemView({ item }: { item: TranscriptItem }) {
  switch (item.kind) {
    case "UserText":
      return (
        <div
          className={cn(
            "rounded-lg border px-3 py-2 text-sm whitespace-pre-wrap",
            item.injected
              ? "border-warning/30 bg-warning/5 text-warning"
              : "border-accent-muted/60 bg-accent/5",
          )}
        >
          {item.text}
        </div>
      );
    case "AssistantText":
      return (
        <div className="whitespace-pre-wrap text-sm leading-relaxed">
          {item.text}
        </div>
      );
    case "Thinking":
      return (
        <div className="whitespace-pre-wrap border-l-2 border-border pl-3 text-xs italic leading-relaxed text-fg-muted">
          {item.text}
        </div>
      );
    case "ToolUse":
      return (
        <div className="overflow-hidden rounded-lg border border-border bg-surface font-mono text-xs">
          <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-1.5">
            <Squircle size={12} className="text-accent" />
            <span className="font-medium">{item.name}</span>
            {!item.output && (
              <Loader2 size={12} className="ml-auto animate-spin text-fg-muted" />
            )}
          </div>
          <pre className="max-h-40 overflow-auto px-3 py-2 text-fg-secondary">
            {JSON.stringify(item.input, null, 2)}
          </pre>
          {item.output && (
            <pre
              className={cn(
                "max-h-60 overflow-auto border-t border-border-subtle px-3 py-2",
                item.output.is_error ? "text-danger" : "text-fg-secondary",
              )}
            >
              {item.output.text || "(no output)"}
              {item.output.truncated ? "\n… (truncated)" : ""}
            </pre>
          )}
        </div>
      );
    case "CompactMarker":
      return (
        <div className="my-1 flex items-center gap-2 text-[11px] text-fg-muted">
          <div className="h-px flex-1 bg-border" />
          context compacted
          <div className="h-px flex-1 bg-border" />
        </div>
      );
  }
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
                  if (e.key === "Escape") setDenying(false);
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

  const send = useCallback(() => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText("");
    void ipc.sendUserMessage(sessionId, trimmed).catch(() => {});
  }, [sessionId, text]);

  return (
    <div className="border-t border-border-subtle p-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <textarea
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
          placeholder="Message Claude… (Ctrl+Enter to send, Esc to interrupt)"
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
