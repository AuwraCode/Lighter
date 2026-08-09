// Account manager: profiles = named CLAUDE_CONFIG_DIRs, each with its own
// signed-in account. Auth status is fetched live via `claude auth status`.

import { useCallback, useEffect, useState } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { toast } from "sonner";
import {
  Check,
  FolderSearch,
  Loader2,
  LogIn,
  Plus,
  RefreshCw,
  Trash2,
  UserRound,
  X,
} from "lucide-react";
import { cn } from "@/lib/cn";
import type { Profile } from "@/lib/generated/Profile";
import {
  deleteProfile,
  discoverProfiles,
  loadProfiles,
  openLoginTerminal,
  openProfilesDialog,
  profilesStore,
  refreshAuthStatus,
  saveProfile,
  setDefaultProfile,
  useProfiles,
} from "@/stores/profiles";

function slugify(name: string): string {
  return (
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "profile"
  );
}

async function suggestedDir(name: string): Promise<string> {
  const home = (await homeDir()).replace(/[\\/]+$/, "");
  return `${home}\\.claude-${slugify(name)}`;
}

function refreshAllStatuses() {
  for (const p of profilesStore.getState().profiles) {
    void refreshAuthStatus(p);
  }
}

export function ProfilesDialog() {
  const open = useProfiles((s) => s.dialogOpen);
  const profiles = useProfiles((s) => s.profiles);
  const defaultId = useProfiles((s) => s.defaultProfileId);
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDir, setNewDir] = useState("");
  const [placeholder, setPlaceholder] = useState("");

  useEffect(() => {
    if (!open) return;
    setAdding(false);
    void loadProfiles().then(refreshAllStatuses);
  }, [open]);

  useEffect(() => {
    if (adding) {
      void suggestedDir(newName || "work").then(setPlaceholder);
    }
  }, [adding, newName]);

  const close = useCallback(() => openProfilesDialog(false), []);

  const discover = useCallback(async () => {
    const found = await discoverProfiles();
    if (found.length === 0) {
      toast.info("No additional signed-in config directories found.");
      return;
    }
    for (const p of found) {
      await saveProfile(p);
    }
    toast.success(
      `Added ${found.length} discovered profile${found.length > 1 ? "s" : ""}.`,
    );
    refreshAllStatuses();
  }, []);

  const addManual = useCallback(async () => {
    if (!newName.trim()) return;
    try {
      const dir = newDir.trim() || (await suggestedDir(newName));
      const profile: Profile = {
        id: crypto.randomUUID(),
        name: newName.trim(),
        config_dir: dir,
        created_at_ms: 0n as unknown as bigint,
      };
      await saveProfile(profile);
      setAdding(false);
      setNewName("");
      setNewDir("");
      void refreshAuthStatus(profile);
    } catch (e) {
      toast.error(String(e));
    }
  }, [newName, newDir]);

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
      <div className="w-[560px] rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold">Accounts</h2>
          <button
            onClick={close}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        <p className="mb-3 text-xs text-fg-secondary">
          Each profile is a Claude config directory with its own signed-in
          account. Sessions and presets can pick which one to use.
        </p>

        <div className="flex flex-col gap-1.5">
          {profiles.map((p) => (
            <ProfileRow key={p.id} profile={p} isDefault={p.id === defaultId} />
          ))}
        </div>

        {adding ? (
          <div className="mt-3 flex items-end gap-2 rounded-lg border border-border bg-surface p-2.5 text-xs">
            <div className="flex-1">
              <label className="mb-1 block text-fg-secondary">Name</label>
              <input
                autoFocus
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="e.g. Work"
                className="w-full rounded-md border border-border bg-elevated px-2 py-1.5 focus:border-accent focus:outline-none"
              />
            </div>
            <div className="flex-[2]">
              <label className="mb-1 block text-fg-secondary">Config directory</label>
              <input
                value={newDir}
                onChange={(e) => setNewDir(e.target.value)}
                placeholder={placeholder}
                className="w-full rounded-md border border-border bg-elevated px-2 py-1.5 font-mono focus:border-accent focus:outline-none"
              />
            </div>
            <button
              onClick={addManual}
              disabled={!newName.trim()}
              className="rounded-md bg-accent px-2.5 py-1.5 font-medium text-white hover:bg-accent-hover disabled:opacity-50"
            >
              Add
            </button>
            <button
              onClick={() => setAdding(false)}
              className="rounded-md border border-border px-2.5 py-1.5 text-fg-secondary hover:bg-hover"
            >
              Cancel
            </button>
          </div>
        ) : (
          <div className="mt-3 flex gap-2">
            <button
              onClick={() => setAdding(true)}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <Plus size={12} /> Add profile
            </button>
            <button
              onClick={discover}
              title="Scan your home directory for signed-in .claude* config dirs"
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <FolderSearch size={12} /> Discover existing
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function ProfileRow({ profile, isDefault }: { profile: Profile; isDefault: boolean }) {
  const auth = useProfiles((s) => s.auth[profile.id]);
  const [checking, setChecking] = useState(false);

  const recheck = async () => {
    setChecking(true);
    await refreshAuthStatus(profile);
    setChecking(false);
  };

  const signIn = async () => {
    await openLoginTerminal(profile.config_dir);
    toast.info(
      "A sign-in window opened. Finish the browser login there, then hit refresh here.",
    );
  };

  return (
    <div className="group flex items-center gap-2.5 rounded-lg border border-border bg-surface px-3 py-2 text-xs">
      <UserRound size={14} className="shrink-0 text-fg-muted" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="font-medium">{profile.name}</span>
          {isDefault && (
            <span className="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-medium text-accent">
              default
            </span>
          )}
          {auth === undefined ? (
            <Loader2 size={10} className="animate-spin text-fg-muted" />
          ) : auth?.loggedIn ? (
            <span className="truncate text-[11px] text-success">
              {auth.email}
              {auth.subscriptionType ? ` · ${auth.subscriptionType}` : ""}
            </span>
          ) : (
            <span className="text-[11px] text-danger">not signed in</span>
          )}
        </div>
        <div className="truncate font-mono text-[10px] text-fg-muted">
          {profile.config_dir ?? "system default (~\\.claude)"}
        </div>
      </div>

      <button
        onClick={recheck}
        title="Refresh auth status"
        className={cn(
          "shrink-0 rounded p-1.5 text-fg-muted hover:bg-hover hover:text-fg",
          checking && "animate-spin",
        )}
      >
        <RefreshCw size={12} />
      </button>
      <button
        onClick={signIn}
        title="Open a terminal running claude auth login for this profile"
        className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-[11px] text-fg-secondary hover:bg-hover hover:text-fg"
      >
        <LogIn size={11} /> Sign in
      </button>
      {!isDefault && (
        <>
          <button
            onClick={() =>
              void setDefaultProfile(profile.id).catch((e) => toast.error(String(e)))
            }
            title="Use this account by default"
            className="hidden shrink-0 rounded p-1.5 text-fg-muted hover:bg-hover hover:text-success group-hover:block"
          >
            <Check size={12} />
          </button>
          <button
            onClick={() =>
              void deleteProfile(profile.id).catch((e) => toast.error(String(e)))
            }
            title="Remove profile (keeps the directory and account)"
            className="hidden shrink-0 rounded p-1.5 text-fg-muted hover:bg-hover hover:text-danger group-hover:block"
          >
            <Trash2 size={12} />
          </button>
        </>
      )}
    </div>
  );
}
