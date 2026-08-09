import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast, Toaster } from "sonner";
import type { AppInfo } from "@/lib/generated/AppInfo";
import { Titlebar } from "@/components/Titlebar";
import { Sidebar } from "@/components/Sidebar";
import { SessionView } from "@/components/SessionView";
import { NewSessionDialog } from "@/components/NewSessionDialog";
import { PresetDialog } from "@/components/PresetDialog";
import { ProfilesDialog } from "@/components/ProfilesDialog";
import { SettingsDialog } from "@/components/SettingsDialog";
import { CommandPalette } from "@/components/CommandPalette";
import { Dashboard } from "@/views/Dashboard";
import { Workspace } from "@/views/Workspace";
import { loadPresets } from "@/stores/presets";
import { loadProfiles } from "@/stores/profiles";
import { loadSettings } from "@/stores/settings";
import {
  focusSession,
  initRegistry,
  openNewSession,
  openPalette,
  openWorkspace,
  registryStore,
  useRegistry,
} from "@/stores/registry";

async function checkCliVersion() {
  try {
    const info = await invoke<AppInfo>("get_app_info");
    if (!info.claude_path) {
      toast.error(
        "Claude Code CLI not found on PATH. Install it and restart Lighter.",
        { duration: Infinity },
      );
    } else if (
      info.claude_version &&
      info.claude_version !== info.tested_cli_version
    ) {
      toast.info(
        `claude ${info.claude_version} detected — Lighter's protocol layer was verified against ${info.tested_cli_version}. Unknown frames are tolerated, but re-run the probe after big CLI updates.`,
        { duration: 10000 },
      );
    }
  } catch {
    // Non-fatal; sessions will surface concrete spawn errors themselves.
  }
}

function App() {
  const focusedId = useRegistry((s) => s.focusedId);
  const view = useRegistry((s) => s.view);

  useEffect(() => {
    void initRegistry();
    void loadPresets();
    void loadProfiles();
    void loadSettings();
    void checkCliVersion();
  }, []);

  // Global shortcuts: Ctrl+1..8 focus nth session, Ctrl+D dashboard,
  // Ctrl+N new session, Ctrl+K command palette.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      if (e.key === "d" || e.key === "D") {
        e.preventDefault();
        focusSession(null);
        return;
      }
      if (e.key === "n" || e.key === "N") {
        e.preventDefault();
        openNewSession(true);
        return;
      }
      if (e.key === "k" || e.key === "K") {
        e.preventDefault();
        openPalette(!registryStore.getState().paletteOpen);
        return;
      }
      if (e.key === "g" || e.key === "G") {
        e.preventDefault();
        openWorkspace();
        return;
      }
      const n = Number(e.key);
      if (n >= 1 && n <= 8) {
        const id = registryStore.getState().order[n - 1];
        if (id) {
          e.preventDefault();
          focusSession(id);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-h-0 flex-1 flex-col">
          {view === "workspace" ? (
            <Workspace />
          ) : view === "session" && focusedId ? (
            <SessionView key={focusedId} sessionId={focusedId} />
          ) : (
            <Dashboard />
          )}
        </main>
      </div>
      <NewSessionDialog />
      <PresetDialog />
      <ProfilesDialog />
      <SettingsDialog />
      <CommandPalette />
      <Toaster
        theme="dark"
        position="bottom-right"
        toastOptions={{
          style: {
            background: "var(--color-elevated)",
            border: "1px solid var(--color-border)",
            color: "var(--color-fg)",
          },
        }}
      />
    </div>
  );
}

export default App;
