import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openPicker } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Folder, Loader2, Sparkles, UserRound, X } from "lucide-react";
import type { AppSettings } from "@/lib/generated/AppSettings";
import type { AppInfo } from "@/lib/generated/AppInfo";
import {
  openSettingsDialog,
  saveSettings,
  settingsStore,
  useSettings,
} from "@/stores/settings";
import { openProfilesDialog } from "@/stores/profiles";
import { openSkills } from "@/stores/registry";

const MODELS = ["", "haiku", "sonnet", "opus[1m]", "default"];
const MODES = ["", "default", "plan", "acceptEdits", "auto", "dontAsk", "bypassPermissions"];

export function SettingsDialog() {
  const open = useSettings((s) => s.dialogOpen);
  const [draft, setDraft] = useState<AppSettings>(settingsStore.getState().settings);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDraft({ ...settingsStore.getState().settings });
    void invoke<AppInfo>("get_app_info").then(setInfo).catch(() => {});
  }, [open]);

  const close = useCallback(() => openSettingsDialog(false), []);

  const set = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));

  const save = useCallback(async () => {
    setBusy(true);
    try {
      await saveSettings(draft);
      toast.success("Settings saved.");
      close();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }, [draft, close]);

  const pickClaude = useCallback(async () => {
    const file = await openPicker({
      filters: [{ name: "Executable", extensions: ["exe", "cmd"] }],
    });
    if (typeof file === "string") set("claude_bin", file);
  }, []);

  const pickWorktreeBase = useCallback(async () => {
    const dir = await openPicker({ directory: true });
    if (typeof dir === "string") set("worktree_base", dir);
  }, []);

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
      <div className="max-h-[85vh] w-[560px] overflow-y-auto rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">Settings</h2>
          <button
            onClick={close}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex flex-col gap-4 text-xs">
          <Section title="Claude CLI">
            <div className="mb-2 rounded-md border border-border-subtle bg-surface px-2.5 py-2 font-mono text-[11px] text-fg-muted">
              {info ? (
                info.claude_path ? (
                  <>
                    detected: {info.claude_path}
                    {info.claude_version && ` (v${info.claude_version})`}
                    {info.claude_version &&
                      info.claude_version !== info.tested_cli_version && (
                        <span className="text-warning">
                          {" "}
                          — protocol verified against {info.tested_cli_version}
                        </span>
                      )}
                  </>
                ) : (
                  <span className="text-danger">claude not found on PATH</span>
                )
              ) : (
                "checking…"
              )}
            </div>
            <label className="mb-1 block text-fg-secondary">
              Binary override (optional)
            </label>
            <PathInput
              value={draft.claude_bin ?? ""}
              placeholder="Use the binary from PATH"
              onChange={(v) => set("claude_bin", v || null)}
              onPick={pickClaude}
            />
          </Section>

          <Section title="New session defaults">
            <div className="flex gap-3">
              <div className="flex-1">
                <label className="mb-1 block text-fg-secondary">Model</label>
                <select
                  value={draft.default_model ?? ""}
                  onChange={(e) => set("default_model", e.target.value || null)}
                  className="w-full rounded-md border border-border bg-surface px-2 py-2"
                >
                  {MODELS.map((m) => (
                    <option key={m} value={m}>
                      {m === "" ? "(app default)" : m}
                    </option>
                  ))}
                </select>
              </div>
              <div className="flex-1">
                <label className="mb-1 block text-fg-secondary">
                  Permission mode
                </label>
                <select
                  value={draft.default_permission_mode ?? ""}
                  onChange={(e) =>
                    set("default_permission_mode", e.target.value || null)
                  }
                  className="w-full rounded-md border border-border bg-surface px-2 py-2"
                >
                  {MODES.map((m) => (
                    <option key={m} value={m}>
                      {m === "" ? "(app default)" : m}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          </Section>

          <Section title="Git worktrees">
            <label className="mb-1 block text-fg-secondary">
              Base directory for isolation worktrees
            </label>
            <PathInput
              value={draft.worktree_base ?? ""}
              placeholder="~\.lighter\worktrees"
              onChange={(v) => set("worktree_base", v || null)}
              onPick={pickWorktreeBase}
            />
          </Section>

          <Section title="Skills">
            <p className="mb-2 text-fg-muted">
              Skill plugins and everything installed per account now live in the
              Skills view.
            </p>
            <button
              onClick={() => {
                close();
                openSkills();
              }}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <Sparkles size={12} /> Open Skills
            </button>
          </Section>

          <Section title="Accounts">
            <button
              onClick={() => {
                close();
                openProfilesDialog(true);
              }}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <UserRound size={12} /> Manage accounts…
            </button>
          </Section>

          <div className="flex items-center justify-between border-t border-border-subtle pt-3">
            <span className="font-mono text-[10px] text-fg-muted">
              Lighter {info?.app_version ?? ""}
            </span>
            <div className="flex gap-2">
              <button
                onClick={close}
                className="rounded-md border border-border px-3 py-1.5 text-fg-secondary hover:bg-hover"
              >
                Cancel
              </button>
              <button
                onClick={save}
                disabled={busy}
                className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
              >
                {busy && <Loader2 size={12} className="animate-spin" />}
                Save
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-fg-muted">
        {title}
      </div>
      {children}
    </div>
  );
}

function PathInput({
  value,
  placeholder,
  onChange,
  onPick,
}: {
  value: string;
  placeholder: string;
  onChange: (v: string) => void;
  onPick: () => void;
}) {
  return (
    <div className="flex gap-1.5">
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="min-w-0 flex-1 rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
      />
      <button
        onClick={onPick}
        title="Browse…"
        className="shrink-0 rounded-md border border-border px-2.5 text-fg-secondary hover:bg-hover hover:text-fg"
      >
        <Folder size={13} />
      </button>
    </div>
  );
}
