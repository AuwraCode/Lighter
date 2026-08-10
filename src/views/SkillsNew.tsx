// Guarded new-skill wizard: the discriminator gate first (skill vs CLAUDE.md /
// slash command / subagent), then an interview that MUST ask "when should this
// NOT trigger", model-assisted drafting with a persona guard and a redundancy
// check, then scaffold + validate.

import { useCallback, useEffect, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  FileWarning,
  FolderOpen,
  Loader2,
  RotateCcw,
  Sparkles,
  Wand2,
  XCircle,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { IntentKind } from "@/lib/generated/IntentKind";
import type { DraftInput } from "@/lib/generated/DraftInput";
import type { Redundancy } from "@/lib/generated/Redundancy";
import type { ScriptSuggestion } from "@/lib/generated/ScriptSuggestion";
import type { ScaffoldResult } from "@/lib/generated/ScaffoldResult";
import { defaultProfile } from "@/stores/profiles";

const INTENTS: { kind: IntentKind; title: string; hint: string }[] = [
  {
    kind: "ContextualProcedure",
    title: "A procedure Claude should follow when a situation comes up",
    hint: "e.g. how to write tests in this repo, how to fill our PDF forms — this is a skill.",
  },
  {
    kind: "AlwaysRule",
    title: "A rule that always applies",
    hint: "e.g. always use tabs, never commit to main — belongs in CLAUDE.md.",
  },
  {
    kind: "DeliberateInvoke",
    title: "Something I invoke deliberately by name",
    hint: "e.g. /deploy, /changelog — belongs in a slash command.",
  },
  {
    kind: "ContextHeavy",
    title: "Something context-heavy done in isolation",
    hint: "e.g. audit the whole codebase — belongs in a subagent.",
  },
];

type Step = "intent" | "interview" | "draft" | "create";

export function SkillsNew() {
  const [step, setStep] = useState<Step>("intent");
  const [redirect, setRedirect] = useState<string | null>(null);

  const [form, setForm] = useState<DraftInput>({
    name: "",
    what: "",
    when_to: "",
    when_not: "",
    teammate_pitfall: "",
    output: "",
  });
  const [description, setDescription] = useState("");
  const [body, setBody] = useState("");
  const [personas, setPersonas] = useState<string[]>([]);
  const [redundancy, setRedundancy] = useState<Redundancy | null>(null);
  const [script, setScript] = useState<ScriptSuggestion | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<ScaffoldResult | null>(null);

  const configDir = () => defaultProfile()?.config_dir ?? null;
  const set = <K extends keyof DraftInput>(k: K, v: DraftInput[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const chooseIntent = useCallback(async (kind: IntentKind) => {
    const r = await ipc.skillIntentRedirect(kind);
    if (r) {
      setRedirect(r);
    } else {
      setRedirect(null);
      setStep("interview");
    }
  }, []);

  // Persona guard runs continuously on the draft.
  useEffect(() => {
    const text = `${description}\n${body}`;
    if (!text.trim()) {
      setPersonas([]);
      return;
    }
    void ipc.skillCheckPersona(text).then(setPersonas).catch(() => {});
  }, [description, body]);

  const suggestName = useCallback(async () => {
    const base = form.name || form.what;
    if (!base) return;
    set("name", await ipc.skillSuggestName(base));
  }, [form.name, form.what]);

  const draft = useCallback(
    async (which: "description" | "body") => {
      setBusy(which);
      try {
        if (which === "description") {
          setDescription(await ipc.skillDraftDescription(configDir(), form));
        } else {
          setBody(await ipc.skillDraftBody(configDir(), form));
        }
      } catch (e) {
        toast.error(String(e));
      } finally {
        setBusy(null);
      }
    },
    [form],
  );

  const checkRedundancy = useCallback(async () => {
    setBusy("redundancy");
    try {
      setRedundancy(await ipc.skillRedundancyCheck(configDir(), form.name, description));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [form.name, description]);

  const checkScript = useCallback(async () => {
    setBusy("script");
    try {
      setScript(await ipc.skillSuggestScript(configDir(), body));
      if (!script) toast.message("No deterministic sequence worth scripting.");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [body, script]);

  const create = useCallback(async () => {
    const parent = await openFolder({ directory: true });
    if (typeof parent !== "string") return;
    setBusy("create");
    try {
      setResult(await ipc.skillScaffold(parent, { name: form.name, description, body }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [form.name, description, body]);

  const interviewComplete =
    form.name.trim() &&
    form.what.trim() &&
    form.when_to.trim() &&
    form.when_not.trim();

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl p-6">
        <Steps step={step} />

        {step === "intent" && (
          <div className="mt-4">
            <h2 className="mb-1 text-[13px] font-medium">
              What are you trying to capture?
            </h2>
            <p className="mb-3 text-xs text-fg-muted">
              Not everything should be a skill — a skill only loads when its
              description triggers.
            </p>
            <div className="flex flex-col gap-2">
              {INTENTS.map((it) => (
                <button
                  key={it.kind}
                  onClick={() => void chooseIntent(it.kind)}
                  className={cn(
                    "rounded-lg border px-3 py-2.5 text-left transition-colors",
                    it.kind === "ContextualProcedure"
                      ? "border-accent/40 bg-accent/5 hover:bg-accent/10"
                      : "border-border bg-surface hover:bg-hover",
                  )}
                >
                  <div className="text-xs font-medium">{it.title}</div>
                  <div className="mt-0.5 text-[11px] text-fg-muted">{it.hint}</div>
                </button>
              ))}
            </div>
            {redirect && (
              <div className="mt-3 flex items-start gap-2 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2.5 text-xs text-warning">
                <FileWarning size={15} className="mt-0.5 shrink-0" />
                {redirect}
              </div>
            )}
          </div>
        )}

        {step === "interview" && (
          <div className="mt-4 flex flex-col gap-3 text-xs">
            <Field label="Skill name" hint="lowercase, hyphenated — must match the folder">
              <div className="flex gap-1.5">
                <input
                  value={form.name}
                  onChange={(e) => set("name", e.target.value)}
                  placeholder="pdf-form-filler"
                  className="min-w-0 flex-1 rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
                />
                <button
                  onClick={suggestName}
                  title="Suggest a valid name"
                  className="rounded-md border border-border px-2.5 text-fg-secondary hover:bg-hover hover:text-fg"
                >
                  <Wand2 size={13} />
                </button>
              </div>
            </Field>
            <Field label="What should this let Claude do?">
              <TextArea value={form.what} onChange={(v) => set("what", v)} rows={2} />
            </Field>
            <Field label="When SHOULD it trigger?" hint="phrasings, situations, keywords">
              <TextArea value={form.when_to} onChange={(v) => set("when_to", v)} rows={2} />
            </Field>
            <Field
              label="When should it NOT trigger?"
              hint="required — this is what kills false triggers"
              required
            >
              <TextArea value={form.when_not} onChange={(v) => set("when_not", v)} rows={2} />
            </Field>
            <Field label="What would a new teammate get wrong doing this?" hint="the real, project-specific knowledge">
              <TextArea
                value={form.teammate_pitfall}
                onChange={(v) => set("teammate_pitfall", v)}
                rows={2}
              />
            </Field>
            <Field label="Expected output (optional)">
              <TextArea value={form.output} onChange={(v) => set("output", v)} rows={1} />
            </Field>
            <Nav
              onBack={() => setStep("intent")}
              onNext={() => setStep("draft")}
              nextDisabled={!interviewComplete}
            />
          </div>
        )}

        {step === "draft" && (
          <div className="mt-4 flex flex-col gap-3 text-xs">
            <DraftField
              label="Description"
              hint="the ONLY text that triggers the skill — ≤1024 chars"
              value={description}
              onChange={setDescription}
              rows={4}
              onDraft={() => draft("description")}
              drafting={busy === "description"}
            />
            <div className="flex gap-2">
              <button
                onClick={checkRedundancy}
                disabled={!!busy || !description.trim()}
                className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
              >
                {busy === "redundancy" ? (
                  <Loader2 size={12} className="animate-spin" />
                ) : (
                  <Sparkles size={12} />
                )}
                Redundancy check
              </button>
            </div>
            {redundancy && (
              <div
                className={cn(
                  "flex items-start gap-2 rounded-lg border px-3 py-2 text-xs",
                  redundancy.redundant
                    ? "border-danger/40 bg-danger/10 text-danger"
                    : "border-success/40 bg-success/10 text-success",
                )}
              >
                {redundancy.redundant ? (
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                ) : (
                  <CheckCircle2 size={14} className="mt-0.5 shrink-0" />
                )}
                <span>
                  {redundancy.redundant
                    ? "Likely redundant — a model could do this from the description alone. "
                    : "Not redundant — needs project-specific instructions. "}
                  {redundancy.note}
                </span>
              </div>
            )}

            <DraftField
              label="Body"
              hint="imperative steps + pitfalls; skip general knowledge"
              value={body}
              onChange={setBody}
              rows={8}
              onDraft={() => draft("body")}
              drafting={busy === "body"}
            />
            <div className="flex gap-2">
              <button
                onClick={checkScript}
                disabled={!!busy || !body.trim()}
                className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
              >
                {busy === "script" ? (
                  <Loader2 size={12} className="animate-spin" />
                ) : (
                  <Sparkles size={12} />
                )}
                Suggest a script
              </button>
            </div>
            {script && (
              <div className="rounded-lg border border-info/40 bg-info/10 px-3 py-2 text-xs text-info">
                Consider a script:{" "}
                <span className="font-mono">{script.filename}</span> ({script.language}) —{" "}
                {script.reason}
              </div>
            )}

            {personas.length > 0 && (
              <div className="flex items-start gap-2 rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
                <XCircle size={14} className="mt-0.5 shrink-0" />
                Persona detected ({personas.join(", ")}) — remove it. Personas add
                context cost with no information, and scaffolding is blocked while
                one is present.
              </div>
            )}

            <Nav
              onBack={() => setStep("interview")}
              onNext={() => setStep("create")}
              nextDisabled={!description.trim() || !body.trim() || personas.length > 0}
              nextLabel="Continue"
            />
          </div>
        )}

        {step === "create" && (
          <div className="mt-4 text-xs">
            {!result ? (
              <>
                <p className="mb-3 text-fg-secondary">
                  Pick the folder to create{" "}
                  <span className="font-mono text-fg">{form.name}/</span> in. It
                  will be written and validated immediately.
                </p>
                <button
                  onClick={create}
                  disabled={busy === "create"}
                  className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
                >
                  {busy === "create" ? (
                    <Loader2 size={12} className="animate-spin" />
                  ) : (
                    <FolderOpen size={13} />
                  )}
                  Choose folder & create
                </button>
                <div className="mt-3">
                  <Nav onBack={() => setStep("draft")} />
                </div>
              </>
            ) : (
              <div>
                <div className="mb-2 flex items-center gap-2">
                  {result.report.ok ? (
                    <span className="inline-flex items-center gap-1.5 rounded-md bg-success/15 px-2 py-1 font-medium text-success">
                      <CheckCircle2 size={13} /> Created & valid
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1.5 rounded-md bg-danger/15 px-2 py-1 font-medium text-danger">
                      <XCircle size={13} /> Created with errors
                    </span>
                  )}
                  <span className="font-mono text-[11px] text-fg-muted">
                    {result.skill_dir}
                  </span>
                </div>
                {result.report.diagnostics.length > 0 && (
                  <div className="flex flex-col gap-1">
                    {result.report.diagnostics.map((d, i) => (
                      <div
                        key={i}
                        className={cn(
                          "rounded border px-2.5 py-1.5",
                          d.severity === "Error"
                            ? "border-danger/40 bg-danger/5 text-danger"
                            : "border-warning/40 bg-warning/5 text-warning",
                        )}
                      >
                        <span className="font-mono text-[10px]">{d.code}</span> {d.message}
                      </div>
                    ))}
                  </div>
                )}
                <div className="mt-3 flex items-start gap-2 rounded-md border border-border-subtle bg-surface px-3 py-2 text-[11px] text-fg-muted">
                  <RotateCcw size={12} className="mt-0.5 shrink-0" />
                  Skills are snapshotted at session start — start a new session to
                  use this skill.
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function Steps({ step }: { step: Step }) {
  const order: Step[] = ["intent", "interview", "draft", "create"];
  const labels: Record<Step, string> = {
    intent: "Type",
    interview: "Interview",
    draft: "Draft",
    create: "Create",
  };
  const idx = order.indexOf(step);
  return (
    <div className="flex items-center gap-1.5">
      {order.map((s, i) => (
        <span
          key={s}
          className={cn(
            "rounded-full px-2 py-0.5 text-[10px] font-medium",
            i === idx
              ? "bg-accent/20 text-accent"
              : i < idx
                ? "text-fg-secondary"
                : "text-fg-muted/60",
          )}
        >
          {i < idx ? "✓ " : ""}
          {labels[s]}
        </span>
      ))}
    </div>
  );
}

function Field({
  label,
  hint,
  required,
  children,
}: {
  label: string;
  hint?: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="mb-1 block text-fg-secondary">
        {label}
        {required && <span className="text-danger"> *</span>}
        {hint && <span className="ml-1.5 text-[10px] text-fg-muted">{hint}</span>}
      </label>
      {children}
    </div>
  );
}

function TextArea({
  value,
  onChange,
  rows,
}: {
  value: string;
  onChange: (v: string) => void;
  rows: number;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      rows={rows}
      className="w-full resize-none rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
    />
  );
}

function DraftField({
  label,
  hint,
  value,
  onChange,
  rows,
  onDraft,
  drafting,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  rows: number;
  onDraft: () => void;
  drafting: boolean;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <label className="text-fg-secondary">
          {label} <span className="text-[10px] text-fg-muted">{hint}</span>
        </label>
        <button
          onClick={onDraft}
          disabled={drafting}
          className="inline-flex items-center gap-1 rounded border border-border px-2 py-0.5 text-[11px] text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          {drafting ? (
            <Loader2 size={10} className="animate-spin" />
          ) : (
            <Wand2 size={10} />
          )}
          Draft with AI
        </button>
      </div>
      <TextArea value={value} onChange={onChange} rows={rows} />
    </div>
  );
}

function Nav({
  onBack,
  onNext,
  nextDisabled,
  nextLabel,
}: {
  onBack: () => void;
  onNext?: () => void;
  nextDisabled?: boolean;
  nextLabel?: string;
}) {
  return (
    <div className="mt-1 flex items-center justify-between">
      <button
        onClick={onBack}
        className="inline-flex items-center gap-1 rounded-md px-2.5 py-1.5 text-fg-secondary hover:bg-hover hover:text-fg"
      >
        <ArrowLeft size={12} /> Back
      </button>
      {onNext && (
        <button
          onClick={onNext}
          disabled={nextDisabled}
          className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3.5 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
        >
          {nextLabel ?? "Next"} <ArrowRight size={12} />
        </button>
      )}
    </div>
  );
}
