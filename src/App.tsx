import { useEffect } from "react";
import { Toaster } from "sonner";
import { Titlebar } from "@/components/Titlebar";
import { Sidebar } from "@/components/Sidebar";
import { SessionView } from "@/components/SessionView";
import { NewSessionDialog } from "@/components/NewSessionDialog";
import { PresetDialog } from "@/components/PresetDialog";
import { CommandPalette } from "@/components/CommandPalette";
import { Dashboard } from "@/views/Dashboard";
import { loadPresets } from "@/stores/presets";
import {
  focusSession,
  initRegistry,
  openNewSession,
  openPalette,
  registryStore,
  useRegistry,
} from "@/stores/registry";

function App() {
  const focusedId = useRegistry((s) => s.focusedId);

  useEffect(() => {
    void initRegistry();
    void loadPresets();
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
          {focusedId ? (
            <SessionView key={focusedId} sessionId={focusedId} />
          ) : (
            <Dashboard />
          )}
        </main>
      </div>
      <NewSessionDialog />
      <PresetDialog />
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
