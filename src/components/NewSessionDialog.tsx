import { useCallback, useEffect, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { Folder, Loader2, X } from "lucide-react";
import * as ipc from "@/lib/ipc";
import { focusSession, openNewSession, useRegistry } from "@/stores/registry";
import { profileById, useProfiles } from "@/stores/profiles";

const MODELS = ["haiku", "sonnet", "opus[1m]", "default"];
const MODES = ["default", "plan", "acceptEdits", "auto", "dontAsk", "bypassPermissions"];

export function NewSessionDialog() {
  const open = useRegistry((s) => s.newSessionOpen);
  const profiles = useProfiles((s) => s.profiles);
  const defaultProfileId = useProfiles((s) => s.defaultProfileId);
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
      setError(null);
      setBusy(false);
      setProfileId(defaultProfileId ?? "");
    }
  }, [open, defaultProfileId]);

  const close = useCallback(() => openNewSession(false), []);

  const pickFolder = useCallback(async () => {
    const dir = await openFolder({ directory: true });
    if (typeof dir === "string") setCwd(dir);
  }, []);

  const start = useCallback(async () => {
    if (!cwd) {
      setError("Pick a working directory first.");
      return;
    }
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
        // SessionView attaches on mount and hydrates from the snapshot;
        // nothing to do with pre-attach batches.
        () => {},
      );
      close();
      setPrompt("");
      focusSession(info.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [cwd, title, model, mode, prompt, profileId, close]);

  if (!open) return null;

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
      <div className="w-[480px] rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">New session</h2>
          <button
            onClick={close}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex flex-col gap-3 text-xs">
          <div>
            <label className="mb-1 block text-fg-secondary">Working directory</label>
            <button
              onClick={pickFolder}
              className="inline-flex w-full items-center gap-1.5 rounded-md border border-border bg-surface px-2.5 py-2 text-left hover:bg-hover"
            >
              <Folder size={13} className="shrink-0 text-fg-muted" />
              <span className={cwd ? "truncate" : "text-fg-muted"}>
                {cwd || "Pick a folder…"}
              </span>
            </button>
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">Title (optional)</label>
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Defaults to the folder name"
              className="w-full rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
            />
          </div>

          <div className="flex gap-3">
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Model</label>
              <select
                value={model}
                onChange={(e) => setModel(e.target.value)}
                className="w-full rounded-md border border-border bg-surface px-2 py-2"
              >
                {MODELS.map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </div>
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Permission mode</label>
              <select
                value={mode}
                onChange={(e) => setMode(e.target.value)}
                className="w-full rounded-md border border-border bg-surface px-2 py-2"
              >
                {MODES.map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </div>
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Account</label>
              <select
                value={profileId}
                onChange={(e) => setProfileId(e.target.value)}
                className="w-full rounded-md border border-border bg-surface px-2 py-2"
              >
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">
              Initial prompt (optional)
            </label>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              placeholder="Sent as the first message right after launch"
              className="w-full resize-none rounded-md border border-border bg-surface px-2.5 py-2 focus:border-accent focus:outline-none"
            />
          </div>

          {error && <div className="text-danger">{error}</div>}

          <div className="flex justify-end gap-2 pt-1">
            <button
              onClick={close}
              className="rounded-md border border-border px-3 py-1.5 text-fg-secondary hover:bg-hover"
            >
              Cancel
            </button>
            <button
              onClick={start}
              disabled={busy}
              className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
            >
              {busy && <Loader2 size={12} className="animate-spin" />}
              Start session
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
