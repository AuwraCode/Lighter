// Skillsmith — author, validate and (later) eval Agent Skills from inside
// Lighter. Phase 1: the deterministic validator.

import { useCallback, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  FolderOpen,
  Loader2,
  RotateCcw,
  XCircle,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { Diagnostic } from "@/lib/generated/Diagnostic";
import type { ValidationReport } from "@/lib/generated/ValidationReport";
import { SkillsEval } from "./SkillsEval";
import { SkillsNew } from "./SkillsNew";

const TABS = { new: "New skill", validate: "Validate", eval: "Trigger eval" } as const;
type Tab = keyof typeof TABS;

export function SkillsView() {
  const [tab, setTab] = useState<Tab>("new");

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border-subtle px-3 py-1.5">
        {(Object.keys(TABS) as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={cn(
              "rounded-md px-2.5 py-1 text-xs font-medium",
              tab === t
                ? "bg-elevated text-fg"
                : "text-fg-secondary hover:bg-hover hover:text-fg",
            )}
          >
            {TABS[t]}
          </button>
        ))}
      </div>
      {tab === "new" ? (
        <SkillsNew />
      ) : tab === "validate" ? (
        <ValidatePanel />
      ) : (
        <SkillsEval />
      )}
    </div>
  );
}

function ValidatePanel() {
  const [dir, setDir] = useState<string | null>(null);
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [strict, setStrict] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const validate = useCallback(
    async (path: string, strictMode: boolean) => {
      setBusy(true);
      setError(null);
      try {
        setReport(await ipc.skillValidate(path, strictMode));
      } catch (e) {
        setError(String(e));
        setReport(null);
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const pick = useCallback(async () => {
    const picked = await openFolder({ directory: true });
    if (typeof picked === "string") {
      setDir(picked);
      void validate(picked, strict);
    }
  }, [validate, strict]);

  const errors = report?.diagnostics.filter((d) => d.severity === "Error") ?? [];
  const warnings = report?.diagnostics.filter((d) => d.severity === "Warning") ?? [];

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl p-6">
        <div className="mb-1 flex items-center gap-2">
          <h1 className="text-sm font-semibold tracking-tight">Skills</h1>
          <span className="rounded bg-elevated px-1.5 py-0.5 text-[10px] text-fg-muted">
            validator
          </span>
        </div>
        <p className="mb-4 text-xs text-fg-secondary">
          Validate a SKILL.md against the format spec. Errors mean the skill
          silently fails to load; warnings mean it loads but wastes context.
        </p>

        <div className="flex items-center gap-2">
          <button
            onClick={pick}
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-elevated px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
          >
            <FolderOpen size={13} /> {dir ? "Change folder" : "Pick skill folder"}
          </button>
          {dir && (
            <>
              <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-fg-muted">
                {dir}
              </span>
              <label
                className="inline-flex shrink-0 cursor-pointer items-center gap-1.5 text-[11px] text-fg-secondary"
                title="Treat body-size limits (500 lines / ~5000 tokens) as errors"
              >
                <input
                  type="checkbox"
                  checked={strict}
                  onChange={(e) => {
                    setStrict(e.target.checked);
                    if (dir) void validate(dir, e.target.checked);
                  }}
                  className="accent-accent"
                />
                Strict
              </label>
              <button
                onClick={() => void validate(dir, strict)}
                disabled={busy}
                className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
              >
                {busy && <Loader2 size={12} className="animate-spin" />}
                Revalidate
              </button>
            </>
          )}
        </div>

        {error && (
          <div className="mt-4 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </div>
        )}

        {report && (
          <div className="mt-5">
            <div className="mb-3 flex items-center gap-2">
              {report.ok ? (
                <span className="inline-flex items-center gap-1.5 rounded-md bg-success/15 px-2 py-1 text-xs font-medium text-success">
                  <CheckCircle2 size={13} />
                  {report.name ?? "Skill"} loads
                </span>
              ) : (
                <span className="inline-flex items-center gap-1.5 rounded-md bg-danger/15 px-2 py-1 text-xs font-medium text-danger">
                  <XCircle size={13} />
                  Will not load
                </span>
              )}
              <span className="text-[11px] text-fg-muted">
                {errors.length} error{errors.length === 1 ? "" : "s"} ·{" "}
                {warnings.length} warning{warnings.length === 1 ? "" : "s"}
              </span>
            </div>

            {report.diagnostics.length === 0 ? (
              <div className="rounded-lg border border-success/30 bg-success/5 px-3 py-4 text-center text-xs text-success">
                No issues — clean SKILL.md.
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {errors.map((d, i) => (
                  <DiagnosticRow key={`e${i}`} d={d} />
                ))}
                {warnings.map((d, i) => (
                  <DiagnosticRow key={`w${i}`} d={d} />
                ))}
              </div>
            )}

            <div className="mt-4 flex items-start gap-2 rounded-md border border-border-subtle bg-surface px-3 py-2 text-[11px] text-fg-muted">
              <RotateCcw size={12} className="mt-0.5 shrink-0" />
              Skills are snapshotted at session start. After editing a skill,
              restart affected sessions — changes don&apos;t apply to running ones.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function DiagnosticRow({ d }: { d: Diagnostic }) {
  const isError = d.severity === "Error";
  return (
    <div
      className={cn(
        "flex items-start gap-2.5 rounded-lg border px-3 py-2 text-xs",
        isError ? "border-danger/40 bg-danger/5" : "border-warning/40 bg-warning/5",
      )}
    >
      {isError ? (
        <XCircle size={14} className="mt-0.5 shrink-0 text-danger" />
      ) : (
        <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warning" />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <code
            className={cn(
              "rounded px-1.5 py-0.5 font-mono text-[10px] font-medium",
              isError ? "bg-danger/15 text-danger" : "bg-warning/15 text-warning",
            )}
          >
            {d.code}
          </code>
          {d.file && (
            <span className="truncate font-mono text-[10px] text-fg-muted">{d.file}</span>
          )}
        </div>
        <div className="mt-1 text-fg-secondary">{d.message}</div>
      </div>
    </div>
  );
}
