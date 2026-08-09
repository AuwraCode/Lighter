import { Titlebar } from "@/components/Titlebar";
import { Zap } from "lucide-react";

function App() {
  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <main className="flex flex-1 items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-border bg-elevated">
            <Zap size={26} className="text-accent" />
          </div>
          <div className="text-center">
            <h1 className="text-lg font-semibold tracking-tight">Lighter</h1>
            <p className="mt-1 text-sm text-fg-secondary">
              Multi-session cockpit for Claude Code
            </p>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
