// Trigger eval: does each skill's description route the right queries to it,
// and NOT steal queries meant for the user's other skills? Headline = the
// cross-skill confusion matrix.

import { useCallback, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  FolderOpen,
  Loader2,
  Lock,
  LockOpen,
  Play,
  Plus,
  RotateCcw,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { SkillMeta } from "@/lib/generated/SkillMeta";
import type { TriggerSet } from "@/lib/generated/TriggerSet";
import type { TriggerCase } from "@/lib/generated/TriggerCase";
import type { EvalReport } from "@/lib/generated/EvalReport";
import type { DescriptionFix } from "@/lib/generated/DescriptionFix";
import { defaultProfile } from "@/stores/profiles";

const NONE = "∅ none";

export function SkillsEval() {
  const [dir, setDir] = useState<string | null>(null);
  const [modelKind, setModelKind] = useState<string>("");
  const [catalog, setCatalog] = useState<SkillMeta[]>([]);
  const [testset, setTestset] = useState<TriggerSet | null>(null);
  const [report, setReport] = useState<EvalReport | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const configDir = () => defaultProfile()?.config_dir ?? null;

  const load = useCallback(async (path: string) => {
    setError(null);
    setBusy("load");
    try {
      const [cat, ts, kind] = await Promise.all([
        ipc.skillBuildCatalog(path),
        ipc.skillLoadTestset(path),
        ipc.skillModelKind(configDir()),
      ]);
      setCatalog(cat);
      setTestset(ts);
      setModelKind(kind);
      setReport(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, []);

  const pick = useCallback(async () => {
    const picked = await openFolder({ directory: true });
    if (typeof picked === "string") {
      setDir(picked);
      void load(picked);
    }
  }, [load]);

  const generate = useCallback(async () => {
    if (!dir) return;
    setBusy("generate");
    setError(null);
    try {
      setTestset(await ipc.skillGenerateTestset(dir, configDir()));
      toast.success("Test set generated — review and edit before running.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, [dir]);

  const save = useCallback(async () => {
    if (!dir || !testset) return;
    setBusy("save");
    try {
      await ipc.skillSaveTestset(dir, testset);
      toast.success("Test set saved.");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [dir, testset]);

  const run = useCallback(async () => {
    if (!dir) return;
    setBusy("run");
    setError(null);
    try {
      setReport(await ipc.skillRunEval(dir, configDir()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, [dir]);

  const editCase = (id: number, patch: Partial<TriggerCase>) => {
    setTestset((ts) =>
      ts
        ? {
            ...ts,
            cases: ts.cases.map((c) =>
              c.id === id ? { ...c, ...patch, source: "Manual" } : c,
            ),
          }
        : ts,
    );
  };
  const deleteCase = (id: number) =>
    setTestset((ts) => (ts ? { ...ts, cases: ts.cases.filter((c) => c.id !== id) } : ts));
  const addCase = () =>
    setTestset((ts) =>
      ts
        ? {
            ...ts,
            cases: [
              ...ts.cases,
              {
                id: Math.max(0, ...ts.cases.map((c) => c.id)) + 1,
                query: "",
                intended: null,
                source: "Manual",
                locked: true,
              },
            ],
          }
        : ts,
    );

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-5xl p-6">
        <div className="mb-1 flex items-center gap-2">
          <h1 className="text-sm font-semibold tracking-tight">Skills</h1>
          <span className="rounded bg-elevated px-1.5 py-0.5 text-[10px] text-fg-muted">
            trigger eval
          </span>
          {modelKind && (
            <span
              className="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent"
              title={
                modelKind === "api"
                  ? "Using the Anthropic API (deterministic)"
                  : "Using claude -p on your subscription"
              }
            >
              {modelKind === "api" ? "API" : "claude -p"}
            </span>
          )}
        </div>
        <p className="mb-4 text-xs text-fg-secondary">
          Routes each query against the <em>name + description</em> of all your
          skills (never their bodies) and measures where they collide.
        </p>

        <div className="flex items-center gap-2">
          <button
            onClick={pick}
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-elevated px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
          >
            <FolderOpen size={13} /> {dir ? "Change folder" : "Pick skills folder"}
          </button>
          {dir && (
            <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-fg-muted">
              {dir} · {catalog.length} skill{catalog.length === 1 ? "" : "s"}
            </span>
          )}
        </div>

        {error && (
          <div className="mt-4 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </div>
        )}

        {dir && testset && (
          <TestsetEditor
            testset={testset}
            catalog={catalog}
            busy={busy}
            onGenerate={generate}
            onSave={save}
            onRun={run}
            onEdit={editCase}
            onDelete={deleteCase}
            onAdd={addCase}
          />
        )}

        {report && (
          <ReportView
            report={report}
            dir={dir!}
            configDir={configDir()}
            onApplied={() => void run()}
          />
        )}
      </div>
    </div>
  );
}

function TestsetEditor({
  testset,
  catalog,
  busy,
  onGenerate,
  onSave,
  onRun,
  onEdit,
  onDelete,
  onAdd,
}: {
  testset: TriggerSet;
  catalog: SkillMeta[];
  busy: string | null;
  onGenerate: () => void;
  onSave: () => void;
  onRun: () => void;
  onEdit: (id: number, patch: Partial<TriggerCase>) => void;
  onDelete: (id: number) => void;
  onAdd: () => void;
}) {
  return (
    <div className="mt-5">
      <div className="mb-2 flex items-center gap-2">
        <h2 className="text-xs font-semibold">
          Test set{" "}
          <span className="font-normal text-fg-muted">
            ({testset.cases.length} cases)
          </span>
        </h2>
        <div className="ml-auto flex gap-1.5">
          <button
            onClick={onGenerate}
            disabled={!!busy}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
          >
            {busy === "generate" ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <Sparkles size={12} />
            )}
            {testset.cases.length ? "Regenerate" : "Generate"}
          </button>
          <button
            onClick={onAdd}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
          >
            <Plus size={12} /> Add
          </button>
          <button
            onClick={onSave}
            disabled={!!busy}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
          >
            Save
          </button>
          <button
            onClick={onRun}
            disabled={!!busy || testset.cases.length === 0}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {busy === "run" ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <Play size={12} />
            )}
            Run eval
          </button>
        </div>
      </div>
      {testset.cases.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-xs text-fg-muted">
          No cases yet. Generate a starter set (should-trigger + near-negatives
          per skill), then edit freely — your edits are never overwritten.
        </div>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border">
          <div className="max-h-72 overflow-y-auto">
            {testset.cases.map((c) => (
              <div
                key={c.id}
                className="flex items-center gap-2 border-b border-border-subtle px-2 py-1.5 last:border-0"
              >
                <input
                  value={c.query}
                  onChange={(e) => onEdit(c.id, { query: e.target.value })}
                  placeholder="realistic user query…"
                  className="min-w-0 flex-1 rounded border border-transparent bg-transparent px-1.5 py-1 text-xs hover:border-border focus:border-accent focus:outline-none"
                />
                <select
                  value={c.intended ?? ""}
                  onChange={(e) =>
                    onEdit(c.id, { intended: e.target.value || null })
                  }
                  title="Which skill this query is meant for"
                  className="shrink-0 rounded border border-border bg-elevated px-1 py-1 text-[11px]"
                >
                  <option value="">none</option>
                  {catalog.map((s) => (
                    <option key={s.name} value={s.name}>
                      {s.name}
                    </option>
                  ))}
                </select>
                <span
                  className={cn(
                    "shrink-0 rounded px-1 py-0.5 text-[9px] font-medium",
                    c.source === "Manual"
                      ? "bg-accent/15 text-accent"
                      : "bg-elevated text-fg-muted",
                  )}
                >
                  {c.source === "Manual" ? "manual" : "gen"}
                </span>
                <button
                  onClick={() => onEdit(c.id, { locked: !c.locked })}
                  title={c.locked ? "Locked (kept on regenerate)" : "Unlocked"}
                  className="shrink-0 rounded p-1 text-fg-muted hover:text-fg"
                >
                  {c.locked ? <Lock size={11} /> : <LockOpen size={11} />}
                </button>
                <button
                  onClick={() => onDelete(c.id)}
                  className="shrink-0 rounded p-1 text-fg-muted hover:text-danger"
                >
                  <Trash2 size={11} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function ReportView({
  report,
  dir,
  configDir,
  onApplied,
}: {
  report: EvalReport;
  dir: string;
  configDir: string | null;
  onApplied: () => void;
}) {
  return (
    <div className="mt-6">
      <div className="mb-3 flex items-center gap-3 text-xs">
        <span className="font-semibold">Results</span>
        <span className="text-fg-muted">
          accuracy {(report.accuracy * 100).toFixed(0)}% · {report.correct}/
          {report.total_cases} routed correctly
        </span>
      </div>

      <ConfusionMatrix report={report} />

      {report.collisions.length > 0 && (
        <div className="mt-5">
          <h3 className="mb-1.5 text-xs font-semibold">Collisions</h3>
          <div className="flex flex-col gap-1">
            {report.collisions.map((c, i) => (
              <div
                key={i}
                className="flex items-center gap-2 rounded border border-danger/30 bg-danger/5 px-2.5 py-1 text-xs"
              >
                <span className="font-mono text-fg">{c.intended}</span>
                <span className="text-fg-muted">↦</span>
                <span className="font-mono text-danger">{c.routed}</span>
                <span className="ml-auto font-mono text-fg-muted">
                  {c.count}/{c.intended_total}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {report.latent.length > 0 && (
        <div className="mt-4">
          <h3 className="mb-1.5 text-xs font-semibold">
            Latent overlap{" "}
            <span className="font-normal text-fg-muted">
              (also-plausible, even when routing was correct)
            </span>
          </h3>
          <div className="flex flex-wrap gap-1.5">
            {report.latent.map((l, i) => (
              <span
                key={i}
                className="rounded bg-warning/10 px-2 py-0.5 font-mono text-[11px] text-warning"
              >
                {l.skill} ~ {l.also} ×{l.count}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="mt-5">
        <h3 className="mb-1.5 text-xs font-semibold">Per-skill (worst first)</h3>
        <div className="overflow-hidden rounded-lg border border-border">
          {report.metrics.map((m) => (
            <SkillRow
              key={m.name}
              metric={m}
              dir={dir}
              configDir={configDir}
              onApplied={onApplied}
            />
          ))}
        </div>
      </div>

      <div className="mt-4 flex items-start gap-2 rounded-md border border-border-subtle bg-surface px-3 py-2 text-[11px] text-fg-muted">
        <RotateCcw size={12} className="mt-0.5 shrink-0" />
        Skills are snapshotted at session start — after applying a description,
        restart affected sessions for it to take effect.
      </div>
    </div>
  );
}

function ConfusionMatrix({ report }: { report: EvalReport }) {
  const { labels, rows } = report.matrix;
  const cellColor = (intended: string, colLabel: string, count: number, total: number) => {
    if (count === 0) return "text-fg-muted/40";
    const intensity = total > 0 ? count / total : 0;
    if (intended === colLabel)
      return intensity > 0.66 ? "bg-success/30 text-success" : "bg-success/15 text-success";
    return intensity > 0.5
      ? "bg-danger/30 text-danger"
      : "bg-danger/15 text-danger";
  };
  return (
    <div className="overflow-x-auto rounded-lg border border-border">
      <table className="w-full border-collapse text-[11px]">
        <thead>
          <tr>
            <th className="sticky left-0 bg-surface px-2 py-1.5 text-left font-medium text-fg-muted">
              intended ↓ / routed →
            </th>
            {labels.map((l) => (
              <th
                key={l}
                className="px-2 py-1.5 text-center font-mono font-medium text-fg-secondary"
                title={l}
              >
                {shorten(l)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.intended} className="border-t border-border-subtle">
              <td className="sticky left-0 bg-surface px-2 py-1.5 font-mono text-fg-secondary">
                {shorten(row.intended)}{" "}
                <span className="text-fg-muted">({row.total})</span>
              </td>
              {row.counts.map((count, j) => (
                <td
                  key={j}
                  className={cn(
                    "px-2 py-1.5 text-center font-mono",
                    cellColor(row.intended, labels[j], count, row.total),
                  )}
                >
                  {count || ""}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SkillRow({
  metric,
  dir,
  configDir,
  onApplied,
}: {
  metric: EvalReport["metrics"][number];
  dir: string;
  configDir: string | null;
  onApplied: () => void;
}) {
  const [fix, setFix] = useState<DescriptionFix | null>(null);
  const [busy, setBusy] = useState(false);
  const weak = metric.recall < 0.8 || metric.fp > 0;

  const propose = async () => {
    setBusy(true);
    try {
      setFix(await ipc.skillProposeFix(dir, metric.name, configDir));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };
  const apply = async () => {
    if (!fix) return;
    setBusy(true);
    try {
      await ipc.skillApplyDescription(dir, metric.name, fix.new);
      toast.success(`Updated ${metric.name}'s description.`);
      setFix(null);
      onApplied();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="border-b border-border-subtle last:border-0">
      <div className="flex items-center gap-3 px-2.5 py-1.5 text-xs">
        <span className="font-mono">{metric.name}</span>
        <span className={cn("font-mono", metric.recall < 0.8 ? "text-danger" : "text-success")}>
          recall {(metric.recall * 100).toFixed(0)}%
        </span>
        <span className={cn("font-mono", metric.fp > 0 ? "text-warning" : "text-fg-muted")}>
          precision {(metric.precision * 100).toFixed(0)}%
        </span>
        <span className="font-mono text-fg-muted">
          tp{metric.tp} fp{metric.fp} fn{metric.fn}
        </span>
        {weak && (
          <button
            onClick={propose}
            disabled={busy}
            className="ml-auto inline-flex items-center gap-1 rounded border border-border px-2 py-0.5 text-[11px] text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
          >
            {busy && !fix ? (
              <Loader2 size={10} className="animate-spin" />
            ) : (
              <Wand2 size={10} />
            )}
            Propose fix
          </button>
        )}
      </div>
      {fix && (
        <div className="border-t border-border-subtle bg-surface px-2.5 py-2 text-xs">
          <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-fg-muted">
            before → after
          </div>
          <div className="mb-2 flex gap-3 font-mono text-[11px]">
            <span>
              recall {(fix.before.recall * 100).toFixed(0)}% →{" "}
              <span className={fix.after.recall >= fix.before.recall ? "text-success" : "text-danger"}>
                {(fix.after.recall * 100).toFixed(0)}%
              </span>
            </span>
            <span>
              precision {(fix.before.precision * 100).toFixed(0)}% →{" "}
              <span
                className={fix.after.precision >= fix.before.precision ? "text-success" : "text-danger"}
              >
                {(fix.after.precision * 100).toFixed(0)}%
              </span>
            </span>
            <span>
              collisions {fix.before_collisions} →{" "}
              <span className={fix.after_collisions <= fix.before_collisions ? "text-success" : "text-danger"}>
                {fix.after_collisions}
              </span>
            </span>
          </div>
          <div className="grid grid-cols-2 gap-2">
            <div>
              <div className="mb-0.5 text-[10px] text-fg-muted">current</div>
              <div className="rounded border border-border bg-elevated px-2 py-1.5 text-[11px] text-fg-secondary">
                {fix.old}
              </div>
            </div>
            <div>
              <div className="mb-0.5 text-[10px] text-fg-muted">proposed</div>
              <div className="rounded border border-accent/40 bg-accent/5 px-2 py-1.5 text-[11px]">
                {fix.new}
              </div>
            </div>
          </div>
          <div className="mt-2 flex justify-end gap-1.5">
            <button
              onClick={() => setFix(null)}
              className="rounded px-2.5 py-1 text-fg-muted hover:bg-hover hover:text-fg"
            >
              Discard
            </button>
            <button
              onClick={apply}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
            >
              {busy && <Loader2 size={11} className="animate-spin" />}
              Apply
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function shorten(label: string): string {
  if (label === NONE) return "∅";
  return label.length > 14 ? label.slice(0, 13) + "…" : label;
}
