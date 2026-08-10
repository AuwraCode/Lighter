// The Skills hub: one place to see what's actually active for an account —
// the auto-provisioned marketplace plugins (toggles moved here from Settings)
// plus your own user- and project-scope skills, each with a jump to Validate
// or Trigger eval.

import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  Check,
  CheckCircle2,
  FolderOpen,
  Loader2,
  Play,
  RotateCcw,
  Sparkles,
  Wand2,
  X,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { SkillPluginInfo } from "@/lib/generated/SkillPluginInfo";
import type { LocalSkill } from "@/lib/generated/LocalSkill";
import { useProfiles } from "@/stores/profiles";
import { useSettings, saveSettings } from "@/stores/settings";
import { openValidateFor, openEvalFor } from "@/stores/skillsNav";

const PLUGIN_LABELS: Record<string, string> = {
  "example-skills": "Example skills",
  "document-skills": "Document skills",
};

/** Drop the last path segment — the skill folder's parent is the skills root. */
function parentDir(p: string): string {
  const norm = p.replace(/[\\/]+$/, "");
  const cut = Math.max(norm.lastIndexOf("\\"), norm.lastIndexOf("/"));
  return cut > 0 ? norm.slice(0, cut) : norm;
}

export function SkillsInstalled() {
  const profiles = useProfiles((s) => s.profiles);
  const defaultProfileId = useProfiles((s) => s.defaultProfileId);
  const settings = useSettings((s) => s.settings);

  const [profileId, setProfileId] = useState<string | null>(null);
  const [plugins, setPlugins] = useState<SkillPluginInfo[]>([]);
  const [skills, setSkills] = useState<LocalSkill[]>([]);
  const [projectDir, setProjectDir] = useState<string | null>(null);
  const [busy, setBusy] = useState<"load" | "install" | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Effective account: explicit choice → default → first.
  const selectedId = profileId ?? defaultProfileId ?? profiles[0]?.id ?? null;
  const selected = profiles.find((p) => p.id === selectedId);
  const configDir = selected?.config_dir ?? null;

  const refresh = useCallback(async () => {
    setBusy("load");
    setError(null);
    try {
      const [pl, sk] = await Promise.all([
        ipc.skillPluginsInfo(configDir),
        ipc.skillListLocal(configDir, projectDir),
      ]);
      setPlugins(pl);
      setSkills(sk);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, [configDir, projectDir]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const enabled = new Set(settings.skill_plugins);
  const toggle = useCallback(
    (id: string) => {
      const next = new Set(settings.skill_plugins);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      void saveSettings({ ...settings, skill_plugins: [...next] }).catch((e) =>
        toast.error(String(e)),
      );
    },
    [settings],
  );

  const installNow = useCallback(async () => {
    setBusy("install");
    try {
      const updated = await ipc.installSkillPlugins(configDir);
      setPlugins(updated);
      toast.success(`Skills installed for ${selected?.name ?? "account"}.`);
      void refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [configDir, selected, refresh]);

  const pickProject = useCallback(async () => {
    const picked = await openFolder({ directory: true });
    if (typeof picked === "string") setProjectDir(picked);
  }, []);

  const infoFor = (id: string) => plugins.find((p) => p.id === id);
  const userSkills = useMemo(() => skills.filter((s) => s.scope === "user"), [skills]);
  const projectSkills = useMemo(
    () => skills.filter((s) => s.scope === "project"),
    [skills],
  );

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-3xl p-6">
        <div className="mb-1 flex items-center gap-2">
          <h1 className="text-sm font-semibold tracking-tight">Skills</h1>
          <span className="rounded bg-elevated px-1.5 py-0.5 text-[10px] text-fg-muted">
            installed
          </span>
          <div className="flex-1" />
          {profiles.length > 0 && (
            <label className="flex items-center gap-1.5 text-[11px] text-fg-secondary">
              Account
              <select
                value={selectedId ?? ""}
                onChange={(e) => setProfileId(e.target.value || null)}
                className="rounded-md border border-border bg-surface px-2 py-1 text-[11px] text-fg outline-none focus:border-accent"
              >
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                    {p.id === defaultProfileId ? " (default)" : ""}
                  </option>
                ))}
              </select>
            </label>
          )}
          <button
            onClick={() => void refresh()}
            disabled={busy === "load"}
            title="Refresh"
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[11px] text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
          >
            {busy === "load" ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <RotateCcw size={12} />
            )}
          </button>
        </div>
        <p className="mb-4 text-xs text-fg-secondary">
          Everything active for this account. Plugins install at user scope, so
          they apply to every repo and session. Editing a skill only takes effect
          in sessions started afterwards.
        </p>

        {error && (
          <div className="mb-4 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </div>
        )}

        {/* Marketplace plugins ------------------------------------------------ */}
        <SectionLabel>Plugins · anthropics/skills</SectionLabel>
        <div className="flex flex-col gap-1.5">
          {(["example-skills", "document-skills"] as const).map((id) => {
            const on = enabled.has(id);
            const bundles = infoFor(id)?.bundles ?? "";
            const installed = infoFor(id)?.installed;
            return (
              <button
                key={id}
                onClick={() => toggle(id)}
                className={cn(
                  "flex items-start gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-colors",
                  on
                    ? "border-accent bg-accent/10"
                    : "border-border bg-surface hover:bg-hover",
                )}
              >
                <span
                  className={cn(
                    "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded border",
                    on ? "border-accent bg-accent text-white" : "border-border",
                  )}
                >
                  {on && <Check size={11} />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-1.5 text-xs font-medium text-fg">
                    {PLUGIN_LABELS[id] ?? id}
                    {installed && (
                      <span className="rounded bg-success/15 px-1.5 py-0.5 text-[9px] font-medium text-success">
                        installed
                      </span>
                    )}
                  </span>
                  <span className="mt-0.5 block font-mono text-[10px] text-fg-muted">
                    {bundles}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
        <button
          onClick={() => void installNow()}
          disabled={busy === "install" || settings.skill_plugins.length === 0}
          className="mt-2 inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          {busy === "install" ? (
            <Loader2 size={12} className="animate-spin" />
          ) : (
            <Sparkles size={12} />
          )}
          Install enabled for {selected?.name ?? "this account"}
        </button>

        {/* Your skills -------------------------------------------------------- */}
        <div className="mt-6 flex items-center gap-2">
          <SectionLabel>Your skills</SectionLabel>
          <div className="flex-1" />
          {projectDir ? (
            <span className="flex items-center gap-1 text-[10px] text-fg-muted">
              <span className="max-w-[240px] truncate font-mono">{projectDir}</span>
              <button
                onClick={() => setProjectDir(null)}
                title="Stop scanning this project"
                className="rounded p-0.5 hover:bg-hover hover:text-fg"
              >
                <X size={11} />
              </button>
            </span>
          ) : (
            <button
              onClick={() => void pickProject()}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[10px] text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <FolderOpen size={11} /> Scan a project
            </button>
          )}
        </div>

        {userSkills.length + projectSkills.length === 0 ? (
          <div className="rounded-lg border border-border-subtle bg-surface px-3 py-5 text-center text-xs text-fg-muted">
            No skills for this account yet. Create one in{" "}
            <span className="text-fg-secondary">New skill</span>
            {projectDir ? "" : ", or scan a project for its .claude/skills"}.
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {userSkills.map((s) => (
              <SkillRow key={`u:${s.dir}`} skill={s} />
            ))}
            {projectSkills.map((s) => (
              <SkillRow key={`p:${s.dir}`} skill={s} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
      {children}
    </div>
  );
}

function SkillRow({ skill }: { skill: LocalSkill }) {
  return (
    <div className="flex items-start gap-2.5 rounded-lg border border-border bg-surface px-3 py-2">
      <CheckCircle2 size={14} className="mt-0.5 shrink-0 text-fg-muted" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-xs font-medium text-fg">{skill.name}</span>
          <span
            className={cn(
              "rounded px-1.5 py-0.5 text-[9px] font-medium",
              skill.scope === "project"
                ? "bg-accent/15 text-accent"
                : "bg-elevated text-fg-muted",
            )}
          >
            {skill.scope}
          </span>
          {!skill.parsed && (
            <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[9px] font-medium text-warning">
              unparsed
            </span>
          )}
        </div>
        {skill.description && (
          <div className="mt-0.5 line-clamp-2 text-[11px] text-fg-secondary">
            {skill.description}
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <button
          onClick={() => openValidateFor(skill.dir)}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[10px] text-fg-secondary hover:bg-hover hover:text-fg"
        >
          <Wand2 size={11} /> Validate
        </button>
        <button
          onClick={() => openEvalFor(parentDir(skill.dir))}
          className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[10px] text-fg-secondary hover:bg-hover hover:text-fg"
        >
          <Play size={11} /> Eval
        </button>
      </div>
    </div>
  );
}
