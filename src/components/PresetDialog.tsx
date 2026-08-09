import { useCallback, useEffect, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { Folder, Loader2, Trash2, X } from "lucide-react";
import type { Preset } from "@/lib/generated/Preset";
import {
  deletePreset,
  editPreset,
  savePreset,
  usePresets,
} from "@/stores/presets";
import { useProfiles } from "@/stores/profiles";

const MODELS = ["", "haiku", "sonnet", "opus[1m]", "default"];
const MODES = ["", "default", "plan", "acceptEdits", "auto", "dontAsk", "bypassPermissions"];
const EFFORTS = ["", "low", "medium", "high", "xhigh", "max"];
const WORKTREE = ["auto", "always", "never"];

function blank(): Preset {
  return {
    id: crypto.randomUUID(),
    name: "",
    cwd: "",
    model: null,
    permission_mode: null,
    effort: null,
    allowed_tools: [],
    disallowed_tools: [],
    append_system_prompt: null,
    initial_prompt: null,
    worktree_policy: "auto",
    profile_id: null,
    created_at_ms: 0n as unknown as bigint,
  };
}

export function PresetDialog() {
  const editing = usePresets((s) => s.editing);
  const presets = usePresets((s) => s.presets);
  const profiles = useProfiles((s) => s.profiles);
  const [draft, setDraft] = useState<Preset>(blank());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isNew = editing === "new";

  useEffect(() => {
    if (!editing) return;
    setError(null);
    setBusy(false);
    if (editing === "new") {
      setDraft(blank());
    } else {
      const existing = presets.find((p) => p.id === editing);
      if (existing) setDraft({ ...existing });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editing]);

  const close = useCallback(() => editPreset(null), []);

  const set = <K extends keyof Preset>(key: K, value: Preset[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const pickFolder = useCallback(async () => {
    const dir = await openFolder({ directory: true });
    if (typeof dir === "string") set("cwd", dir);
  }, []);

  const save = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await savePreset(draft);
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [draft, close]);

  const remove = useCallback(async () => {
    setBusy(true);
    try {
      await deletePreset(draft.id);
      close();
    } finally {
      setBusy(false);
    }
  }, [draft.id, close]);

  if (!editing) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") close();
      }}
    >
      <div className="max-h-[85vh] w-[520px] overflow-y-auto rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">
            {isNew ? "New preset" : "Edit preset"}
          </h2>
          <button
            onClick={close}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex flex-col gap-3 text-xs">
          <div className="flex gap-3">
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Name</label>
              <input
                value={draft.name}
                onChange={(e) => set("name", e.target.value)}
                placeholder="e.g. Backend / plan mode"
                className="w-full rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
              />
            </div>
            <div className="w-36">
              <label className="mb-1 block text-fg-secondary">Worktree</label>
              <select
                value={draft.worktree_policy}
                onChange={(e) => set("worktree_policy", e.target.value)}
                title="Isolate this session in a git worktree"
                className="w-full rounded-md border border-border bg-surface px-2 py-2"
              >
                {WORKTREE.map((w) => (
                  <option key={w}>{w}</option>
                ))}
              </select>
            </div>
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">Working directory</label>
            <button
              onClick={pickFolder}
              className="inline-flex w-full items-center gap-1.5 rounded-md border border-border bg-surface px-2.5 py-2 text-left hover:bg-hover"
            >
              <Folder size={13} className="shrink-0 text-fg-muted" />
              <span className={draft.cwd ? "truncate" : "text-fg-muted"}>
                {draft.cwd || "Pick a folder…"}
              </span>
            </button>
          </div>

          <div className="flex gap-3">
            <Select
              label="Model"
              value={draft.model ?? ""}
              options={MODELS}
              onChange={(v) => set("model", v || null)}
            />
            <Select
              label="Permission mode"
              value={draft.permission_mode ?? ""}
              options={MODES}
              onChange={(v) => set("permission_mode", v || null)}
            />
            <Select
              label="Effort"
              value={draft.effort ?? ""}
              options={EFFORTS}
              onChange={(v) => set("effort", v || null)}
            />
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Account</label>
              <select
                value={draft.profile_id ?? ""}
                onChange={(e) => set("profile_id", e.target.value || null)}
                className="w-full rounded-md border border-border bg-surface px-2 py-2"
              >
                <option value="">(default)</option>
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="flex gap-3">
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">
                Allowed tools (comma separated)
              </label>
              <input
                value={draft.allowed_tools.join(", ")}
                onChange={(e) =>
                  set(
                    "allowed_tools",
                    e.target.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  )
                }
                placeholder={'e.g. Bash(git *), Edit'}
                className="w-full rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
              />
            </div>
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Disallowed tools</label>
              <input
                value={draft.disallowed_tools.join(", ")}
                onChange={(e) =>
                  set(
                    "disallowed_tools",
                    e.target.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  )
                }
                className="w-full rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
              />
            </div>
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">
              Append to system prompt (optional)
            </label>
            <textarea
              value={draft.append_system_prompt ?? ""}
              onChange={(e) => set("append_system_prompt", e.target.value || null)}
              rows={2}
              className="w-full resize-none rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
            />
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">
              Initial prompt (optional)
            </label>
            <textarea
              value={draft.initial_prompt ?? ""}
              onChange={(e) => set("initial_prompt", e.target.value || null)}
              rows={3}
              placeholder="Sent automatically right after launch"
              className="w-full resize-none rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
            />
          </div>

          {error && <div className="text-danger">{error}</div>}

          <div className="flex items-center justify-between pt-1">
            {!isNew ? (
              <button
                onClick={remove}
                disabled={busy}
                className="inline-flex items-center gap-1 rounded-md px-2 py-1.5 text-danger hover:bg-danger/10"
              >
                <Trash2 size={12} /> Delete
              </button>
            ) : (
              <span />
            )}
            <div className="flex gap-2">
              <button
                onClick={close}
                className="rounded-md border border-border px-3 py-1.5 text-fg-secondary hover:bg-hover"
              >
                Cancel
              </button>
              <button
                onClick={save}
                disabled={busy || !draft.name.trim() || !draft.cwd}
                className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
              >
                {busy && <Loader2 size={12} className="animate-spin" />}
                Save preset
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Select({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex-1">
      <label className="mb-1 block text-fg-secondary">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-md border border-border bg-surface px-2 py-2"
      >
        {options.map((o) => (
          <option key={o} value={o}>
            {o === "" ? "(default)" : o}
          </option>
        ))}
      </select>
    </div>
  );
}
