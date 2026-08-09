import { useEffect, useState } from "react";
import { History, Loader2, Pencil, Play, Plus, Trash2, X, Zap } from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { Preset } from "@/lib/generated/Preset";
import type { SessionRecord } from "@/lib/generated/SessionRecord";
import type { SessionSummary } from "@/lib/generated/SessionSummary";
import { statusColor, statusLabel } from "@/lib/status";
import { focusSession, openNewSession, useRegistry } from "@/stores/registry";
import { dropSessionStore } from "@/stores/session";
import { editPreset, launchPreset, usePresets } from "@/stores/presets";

export function Dashboard() {
  const order = useRegistry((s) => s.order);
  const active = useRegistry((s) => s.sessions);
  const presets = usePresets((s) => s.presets);
  const [records, setRecords] = useState<SessionRecord[]>([]);

  useEffect(() => {
    void ipc.listSessionRecords().then(setRecords).catch(() => {});
  }, [order.length]);

  const resumable = records
    .filter((r) => !(r.id in active))
    .slice(0, 12);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-5">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="text-sm font-semibold tracking-tight">Presets</h1>
        <button
          onClick={() => editPreset("new")}
          className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
        >
          <Plus size={12} /> New preset
        </button>
      </div>
      {presets.length === 0 ? (
        <div className="mb-6 rounded-lg border border-dashed border-border px-4 py-3 text-xs text-fg-muted">
          Save a named launch configuration (folder, model, permission mode,
          prompts) and start it with one click.
        </div>
      ) : (
        <div className="mb-6 grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2.5">
          {presets.map((p) => (
            <PresetCard key={p.id} preset={p} />
          ))}
        </div>
      )}

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

      {resumable.length > 0 && (
        <>
          <h1 className="mb-3 mt-6 text-sm font-semibold tracking-tight">
            Resumable
          </h1>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2.5">
            {resumable.map((r) => (
              <ResumableCard
                key={r.id}
                record={r}
                onChanged={() =>
                  void ipc.listSessionRecords().then(setRecords).catch(() => {})
                }
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function ResumableCard({
  record,
  onChanged,
}: {
  record: SessionRecord;
  onChanged: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const resume = async () => {
    setBusy(true);
    setError(null);
    try {
      const info = await ipc.resumeSession(record.id);
      focusSession(info.id);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="group flex items-center gap-2 rounded-lg border border-border-subtle bg-surface/60 px-3 py-2.5">
      <History size={14} className="shrink-0 text-fg-muted" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-xs font-medium">{record.title}</span>
          <span className="shrink-0 font-mono text-[10px] text-fg-muted">
            {relativeTime(Number(record.last_active_ms))}
          </span>
        </div>
        <div
          className="truncate text-[10px] text-fg-muted"
          title={error ?? record.last_snippet}
        >
          {error ? <span className="text-danger">{error}</span> : record.last_snippet || record.cwd}
        </div>
      </div>
      <button
        onClick={() => {
          void ipc.deleteSessionRecord(record.id).then(onChanged).catch(() => {});
        }}
        title="Forget"
        className="hidden shrink-0 rounded p-1 text-fg-muted hover:bg-hover hover:text-danger group-hover:block"
      >
        <X size={12} />
      </button>
      <button
        onClick={resume}
        disabled={busy}
        title="Resume session"
        className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-[11px] text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
      >
        {busy ? <Loader2 size={11} className="animate-spin" /> : <Play size={11} />}
        Resume
      </button>
    </div>
  );
}

function relativeTime(ms: number): string {
  const diff = Date.now() - ms;
  const min = Math.floor(diff / 60_000);
  if (min < 1) return "now";
  if (min < 60) return `${min}m ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function PresetCard({ preset }: { preset: Preset }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const launch = async () => {
    setBusy(true);
    setError(null);
    try {
      await launchPreset(preset);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="group flex items-center gap-2 rounded-lg border border-border bg-surface px-3 py-2.5 transition-colors hover:border-fg-muted/40">
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium">{preset.name}</div>
        <div
          className="truncate font-mono text-[10px] text-fg-muted"
          title={error ?? preset.cwd}
        >
          {error ? (
            <span className="text-danger">{error}</span>
          ) : (
            <>
              {preset.model || "default"} · {preset.permission_mode || "default"}
            </>
          )}
        </div>
      </div>
      <button
        onClick={() => editPreset(preset.id)}
        title="Edit preset"
        className="hidden shrink-0 rounded p-1.5 text-fg-muted hover:bg-hover hover:text-fg group-hover:block"
      >
        <Pencil size={13} />
      </button>
      <button
        onClick={launch}
        disabled={busy}
        title="Launch session"
        className="inline-flex shrink-0 items-center justify-center rounded-md bg-accent/15 p-2 text-accent hover:bg-accent hover:text-white disabled:opacity-50"
      >
        {busy ? <Loader2 size={14} className="animate-spin" /> : <Play size={14} />}
      </button>
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
              void ipc
                .removeSession(summary.id, true)
                .then((warning) => {
                  if (warning) console.warn(warning);
                })
                .catch(() => {});
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
        {summary.worktree_branch && (
          <span className="truncate text-accent" title={summary.worktree_branch}>
            ⎇ {summary.worktree_branch.replace(/^lighter\//, "")}
          </span>
        )}
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
