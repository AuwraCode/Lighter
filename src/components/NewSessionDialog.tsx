// Step-by-step session wizard: Directory → Title → Model → Permissions →
// Account → Prompt. Enter advances, completed steps are clickable, Esc closes.

import { useCallback, useEffect, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, ArrowRight, Check, Folder, Loader2, Play, X } from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import {
  MODE_DESCRIPTIONS,
  modePillClass,
  modeTextClass,
  PERMISSION_MODES,
} from "@/lib/status";
import {
  focusSession,
  openNewSession,
  registryStore,
  useRegistry,
} from "@/stores/registry";
import { addSessionToWorkspace } from "@/stores/workspace";
import { profileById, useProfiles } from "@/stores/profiles";
import { settingsStore } from "@/stores/settings";

const MODEL_OPTIONS: { value: string; label: string; description: string }[] = [
  { value: "default", label: "Default", description: "Your account's default model" },
  { value: "opus[1m]", label: "Opus (1M)", description: "Strongest, huge context" },
  { value: "sonnet", label: "Sonnet", description: "Efficient for routine work" },
  { value: "haiku", label: "Haiku", description: "Fastest and cheapest" },
];

const STEPS = ["Directory", "Title", "Model", "Permissions", "Account", "Prompt"];

function lastSegment(path: string): string {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function NewSessionDialog() {
  const open = useRegistry((s) => s.newSessionOpen);
  const profiles = useProfiles((s) => s.profiles);
  const defaultProfileId = useProfiles((s) => s.defaultProfileId);

  const [step, setStep] = useState(0);
  const [cwd, setCwd] = useState("");
  const [title, setTitle] = useState("");
  const [model, setModel] = useState("default");
  const [mode, setMode] = useState("default");
  const [profileId, setProfileId] = useState<string>("");
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      const defaults = settingsStore.getState().settings;
      setStep(0);
      setCwd("");
      setTitle("");
      setModel(defaults.default_model ?? "default");
      setMode(defaults.default_permission_mode ?? "default");
      setProfileId(defaultProfileId ?? "");
      setPrompt("");
      setError(null);
      setBusy(false);
    }
  }, [open, defaultProfileId]);

  const close = useCallback(() => openNewSession(false), []);

  const pickFolder = useCallback(async () => {
    const dir = await openFolder({ directory: true });
    if (typeof dir === "string") setCwd(dir);
  }, []);

  const start = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const info = await ipc.createSession(
        {
          cwd,
          title: title.trim() || null,
          model,
          permission_mode: mode,
          effort: null,
          allowed_tools: [],
          disallowed_tools: [],
          append_system_prompt: null,
          initial_prompt: prompt.trim() || null,
          resume_session_id: null,
          worktree_policy: null,
          claude_config_dir: profileById(profileId)?.config_dir ?? null,
        },
        () => {},
      );
      close();
      // Creating from the split view drops the session straight into it.
      if (registryStore.getState().view === "workspace") {
        addSessionToWorkspace(info.id);
      } else {
        focusSession(info.id);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [cwd, title, model, mode, prompt, profileId, close]);

  const canNext = step === 0 ? cwd.trim().length > 0 : true;
  const isLast = step === STEPS.length - 1;

  const next = useCallback(() => {
    if (!canNext || busy) return;
    if (isLast) {
      void start();
    } else {
      setStep((s) => s + 1);
    }
  }, [canNext, busy, isLast, start]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          close();
          return;
        }
        // Choice steps advance with Enter; input steps handle it themselves.
        if (e.key === "Enter" && step >= 2 && step <= 4) {
          e.preventDefault();
          next();
        }
      }}
    >
      <div className="w-[520px] rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-sm font-semibold">New session</h2>
          <button
            onClick={close}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        {/* Step breadcrumbs: past steps are clickable. */}
        <div className="mb-4 flex items-center gap-1">
          {STEPS.map((label, i) => (
            <button
              key={label}
              disabled={i > step && !(i === step + 1 && canNext)}
              onClick={() => i <= step && setStep(i)}
              className={cn(
                "rounded-full px-2 py-0.5 text-[10px] font-medium transition-colors",
                i === step
                  ? "bg-accent/20 text-accent"
                  : i < step
                    ? "text-fg-secondary hover:bg-hover hover:text-fg"
                    : "text-fg-muted/60",
              )}
            >
              {i < step ? "✓ " : ""}
              {label}
            </button>
          ))}
        </div>

        <div className="min-h-44 text-xs">
          {step === 0 && (
            <StepShell
              title="Where should Claude work?"
              hint="The session runs inside this folder."
            >
              <div className="flex gap-1.5">
                <input
                  autoFocus
                  value={cwd}
                  onChange={(e) => setCwd(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && next()}
                  placeholder="C:\projects\my-app"
                  className="min-w-0 flex-1 rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
                />
                <button
                  onClick={pickFolder}
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-secondary hover:bg-hover hover:text-fg"
                >
                  <Folder size={13} /> Browse
                </button>
              </div>
            </StepShell>
          )}

          {step === 1 && (
            <StepShell
              title="Name the session (optional)"
              hint="Shows in the sidebar, tiles and worktree branch names."
            >
              <input
                autoFocus
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && next()}
                placeholder={cwd ? lastSegment(cwd) : "Defaults to the folder name"}
                className="w-full rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
              />
            </StepShell>
          )}

          {step === 2 && (
            <StepShell title="Pick a model" hint="Switchable live once the session runs.">
              <div className="grid grid-cols-2 gap-1.5">
                {MODEL_OPTIONS.map((m) => (
                  <ChoiceCard
                    key={m.value}
                    label={m.label}
                    description={m.description}
                    selected={model === m.value}
                    onClick={() => setModel(m.value)}
                  />
                ))}
              </div>
            </StepShell>
          )}

          {step === 3 && (
            <StepShell
              title="Permission mode"
              hint="Also switchable live from the session header."
            >
              <div className="grid grid-cols-2 gap-1.5">
                {PERMISSION_MODES.map((m) => (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    className={cn(
                      "rounded-lg border px-2.5 py-2 text-left transition-colors",
                      modePillClass(m, mode === m),
                    )}
                  >
                    <div className="flex items-center gap-1.5 font-medium">
                      {mode === m && <Check size={11} />}
                      {m}
                    </div>
                    <div
                      className={cn(
                        "mt-0.5 text-[10px]",
                        mode === m ? "opacity-80" : "text-fg-muted",
                      )}
                    >
                      {MODE_DESCRIPTIONS[m]}
                    </div>
                  </button>
                ))}
              </div>
            </StepShell>
          )}

          {step === 4 && (
            <StepShell title="Account" hint="Which signed-in Claude account to use.">
              <div className="flex flex-col gap-1.5">
                {profiles.map((p) => (
                  <ChoiceCard
                    key={p.id}
                    label={p.name}
                    description={p.config_dir ?? "system default (~\\.claude)"}
                    mono
                    selected={profileId === p.id}
                    onClick={() => setProfileId(p.id)}
                  />
                ))}
              </div>
            </StepShell>
          )}

          {step === 5 && (
            <StepShell
              title="Initial prompt (optional)"
              hint="Sent as the first message right after launch."
            >
              <div className="mb-2 flex flex-wrap items-center gap-1.5 font-mono text-[10px] text-fg-muted">
                <span className="rounded bg-surface px-1.5 py-0.5">
                  {lastSegment(cwd)}
                </span>
                <span className="rounded bg-surface px-1.5 py-0.5">{model}</span>
                <span
                  className={cn(
                    "rounded bg-surface px-1.5 py-0.5",
                    modeTextClass(mode),
                  )}
                >
                  {mode}
                </span>
                <span className="rounded bg-surface px-1.5 py-0.5">
                  {profileById(profileId)?.name ?? "default account"}
                </span>
              </div>
              <textarea
                autoFocus
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    next();
                  }
                }}
                rows={4}
                placeholder="e.g. Review the failing tests and fix them… (Ctrl+Enter to start)"
                className="w-full resize-none rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
              />
            </StepShell>
          )}
        </div>

        {error && <div className="mt-2 text-xs text-danger">{error}</div>}

        <div className="mt-3 flex items-center justify-between">
          <button
            onClick={() => setStep((s) => Math.max(0, s - 1))}
            disabled={step === 0 || busy}
            className="inline-flex items-center gap-1 rounded-md px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-40"
          >
            <ArrowLeft size={12} /> Back
          </button>
          <button
            onClick={next}
            disabled={!canNext || busy}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3.5 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {busy ? (
              <Loader2 size={12} className="animate-spin" />
            ) : isLast ? (
              <Play size={12} />
            ) : (
              <ArrowRight size={12} />
            )}
            {isLast ? "Start session" : "Next"}
          </button>
        </div>
      </div>
    </div>
  );
}

function StepShell({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="mb-0.5 text-[13px] font-medium text-fg">{title}</div>
      <div className="mb-3 text-fg-muted">{hint}</div>
      {children}
    </div>
  );
}

function ChoiceCard({
  label,
  description,
  selected,
  mono,
  onClick,
}: {
  label: string;
  description: string;
  selected: boolean;
  mono?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "rounded-lg border px-2.5 py-2 text-left transition-colors",
        selected
          ? "border-accent bg-accent/10 text-fg"
          : "border-border bg-elevated text-fg-secondary hover:bg-hover hover:text-fg",
      )}
    >
      <div className="flex items-center gap-1.5 font-medium">
        {selected && <Check size={11} className="text-accent" />}
        {label}
      </div>
      <div
        className={cn(
          "mt-0.5 truncate text-[10px] text-fg-muted",
          mono && "font-mono",
        )}
      >
        {description}
      </div>
    </button>
  );
}
