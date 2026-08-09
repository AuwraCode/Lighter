import { useEffect } from "react";
import { Titlebar } from "@/components/Titlebar";
import { Sidebar } from "@/components/Sidebar";
import { SessionView } from "@/components/SessionView";
import { NewSessionDialog } from "@/components/NewSessionDialog";
import { Dashboard } from "@/views/Dashboard";
import {
  focusSession,
  initRegistry,
  openNewSession,
  registryStore,
  useRegistry,
} from "@/stores/registry";

function App() {
  const focusedId = useRegistry((s) => s.focusedId);

  useEffect(() => {
    void initRegistry();
  }, []);

  // Global shortcuts: Ctrl+1..8 focus nth session, Ctrl+D dashboard, Ctrl+N new.
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
    </div>
  );
}

export default App;
