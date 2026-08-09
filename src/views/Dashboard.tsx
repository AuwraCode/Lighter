import { Plus, Trash2, Zap } from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { SessionSummary } from "@/lib/generated/SessionSummary";
import { statusColor, statusLabel } from "@/lib/status";
import { focusSession, openNewSession, useRegistry } from "@/stores/registry";
import { dropSessionStore } from "@/stores/session";

export function Dashboard() {
  const order = useRegistry((s) => s.order);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-5">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-sm font-semibold tracking-tight">Sessions</h1>
        <button
          onClick={() => openNewSession(true)}
          className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover"
        >
          <Plus size={13} /> New session
        </button>
      </div>

      {order.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
          {order.map((id) => (
            <SessionTile key={id} id={id} />
          ))}
        </div>
      )}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center gap-4 py-24">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-border bg-elevated">
        <Zap size={26} className="text-accent" />
      </div>
      <div className="text-center">
        <div className="text-sm font-medium">No sessions yet</div>
        <p className="mt-1 max-w-72 text-xs text-fg-secondary">
          Start a Claude Code session in any folder. Run several side by side —
          each in its own process.
        </p>
      </div>
      <button
        onClick={() => openNewSession(true)}
        className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover"
      >
        <Plus size={13} /> New session
      </button>
    </div>
  );
}

function SessionTile({ id }: { id: string }) {
  const summary = useRegistry((s) => s.sessions[id]);
  if (!summary) return null;
  return <SessionTileInner summary={summary} />;
}

function SessionTileInner({ summary }: { summary: SessionSummary }) {
  const gone = summary.status === "Exited" || summary.status === "Failed";
  const ctxPct =
    summary.context_used_tokens != null && summary.context_window != null
      ? Math.round(
          (Number(summary.context_used_tokens) / Number(summary.context_window)) * 100,
        )
      : null;

  return (
    <button
      onClick={() => focusSession(summary.id)}
      className={cn(
        "group flex flex-col gap-2 rounded-xl border border-border bg-surface p-3 text-left transition-colors hover:border-fg-muted/40 hover:bg-elevated",
        summary.pending_permissions > 0 && "border-warning/50",
      )}
    >
      <div className="flex items-center gap-2">
        <span className={cn("h-2 w-2 shrink-0 rounded-full", statusColor(summary.status))} />
        <span className="truncate text-[13px] font-medium">{summary.title}</span>
        {summary.pending_permissions > 0 && (
          <span className="shrink-0 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning">
            {summary.pending_permissions} approval
            {summary.pending_permissions > 1 ? "s" : ""}
          </span>
        )}
        <span
          onClick={(e) => {
            e.stopPropagation();
            if (gone) {
              void ipc.removeSession(summary.id).catch(() => {});
              dropSessionStore(summary.id);
            } else {
              void ipc.stopSession(summary.id).catch(() => {});
            }
          }}
          title={gone ? "Remove" : "Stop"}
          className="ml-auto hidden shrink-0 cursor-pointer rounded p-1 text-fg-muted hover:bg-hover hover:text-danger group-hover:block"
        >
          <Trash2 size={12} />
        </span>
      </div>

      <div className="line-clamp-2 min-h-8 text-xs text-fg-secondary">
        {summary.last_snippet || (
          <span className="text-fg-muted">{statusLabel(summary.status)}</span>
        )}
      </div>

      <div className="flex items-center gap-2 font-mono text-[10px] text-fg-muted">
        <span>{shortModel(summary.model)}</span>
        <span>·</span>
        <span>${summary.total_cost_usd.toFixed(3)}</span>
        {ctxPct != null && (
          <>
            <span>·</span>
            <span>ctx {ctxPct}%</span>
          </>
        )}
        <span className="ml-auto truncate" title={summary.cwd}>
          {lastSegment(summary.cwd)}
        </span>
      </div>
    </button>
  );
}

function shortModel(model: string): string {
  if (!model) return "–";
  const m = model.match(/claude-([a-z]+)-?(\d+)?/);
  return m ? `${m[1]}${m[2] ? ` ${m[2]}` : ""}` : model;
}

function lastSegment(path: string): string {
  const parts = path.replace(/\\+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
