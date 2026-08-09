import { CircleStop, Columns2, LayoutGrid, Plus, Settings, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import { statusDotClass } from "@/lib/status";
import { openSettingsDialog } from "@/stores/settings";
import { dropSessionStore } from "@/stores/session";
import {
  focusSession,
  openNewSession,
  openWorkspace,
  registryStore,
  useRegistry,
} from "@/stores/registry";
import {
  addSessionToWorkspace,
  collectLeaves,
  useWorkspace,
} from "@/stores/workspace";

export function Sidebar() {
  const order = useRegistry((s) => s.order);
  const focusedId = useRegistry((s) => s.focusedId);
  const view = useRegistry((s) => s.view);
  const paneCount = useWorkspace((s) => collectLeaves(s.root).length);

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-border-subtle bg-surface">
      <button
        onClick={() => focusSession(null)}
        className={cn(
          "mx-2 mt-2 flex items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium",
          view === "dashboard"
            ? "bg-elevated text-fg"
            : "text-fg-secondary hover:bg-hover hover:text-fg",
        )}
      >
        <LayoutGrid size={14} />
        Dashboard
      </button>
      <button
        onClick={openWorkspace}
        className={cn(
          "mx-2 mt-1 flex items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium",
          view === "workspace"
            ? "bg-elevated text-fg"
            : "text-fg-secondary hover:bg-hover hover:text-fg",
        )}
      >
        <Columns2 size={14} />
        Split view
        {paneCount > 0 && (
          <span className="ml-auto rounded bg-accent/15 px-1.5 py-0.5 font-mono text-[10px] text-accent">
            {paneCount}
          </span>
        )}
        <span className={cn("font-mono text-[9px] text-fg-muted", paneCount > 0 && "hidden")}>
          ⌃G
        </span>
      </button>

      <div className="mt-3 flex items-center justify-between px-4">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-muted">
          Sessions
        </span>
        <button
          onClick={() => openNewSession(true)}
          title="New session (Ctrl+N)"
          className="rounded p-0.5 text-fg-muted hover:bg-hover hover:text-fg"
        >
          <Plus size={13} />
        </button>
      </div>

      <div className="mt-1 flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-2">
        {order.map((id, i) => (
          <SidebarRow key={id} id={id} index={i} active={focusedId === id} />
        ))}
        {order.length === 0 && (
          <div className="px-2.5 py-1.5 text-xs text-fg-muted">No sessions</div>
        )}
      </div>

      <div className="border-t border-border-subtle p-2">
        <button
          onClick={() => openSettingsDialog(true)}
          className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium text-fg-secondary hover:bg-hover hover:text-fg"
        >
          <Settings size={14} />
          Settings
        </button>
      </div>
    </aside>
  );
}

function SidebarRow({
  id,
  index,
  active,
}: {
  id: string;
  index: number;
  active: boolean;
}) {
  const summary = useRegistry((s) => s.sessions[id]);
  if (!summary) return null;
  return (
    <div
      onClick={() => focusSession(id)}
      className={cn(
        "group flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs",
        active ? "bg-elevated text-fg" : "text-fg-secondary hover:bg-hover hover:text-fg",
      )}
      title={summary.cwd}
    >
      <span
        className={cn("h-1.5 w-1.5 shrink-0 rounded-full", statusDotClass(summary.status))}
      />
      <span className="min-w-0 flex-1 truncate">{summary.title}</span>
      {summary.pending_permissions > 0 && (
        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-warning" />
      )}
      <span className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
        <span
          onClick={(e) => {
            e.stopPropagation();
            addSessionToWorkspace(id);
            openWorkspace();
          }}
          title="Add to split view"
          className="cursor-pointer rounded p-0.5 text-fg-muted hover:bg-raised hover:text-accent"
        >
          <Columns2 size={11} />
        </span>
        <span
          onClick={(e) => {
            e.stopPropagation();
            const gone =
              summary.status === "Exited" || summary.status === "Failed";
            if (gone) {
              void ipc
                .removeSession(id, true)
                .then((warning) => {
                  if (warning) toast.warning(warning);
                })
                .catch(() => {});
              dropSessionStore(id);
              if (registryStore.getState().focusedId === id) {
                focusSession(null);
              }
            } else {
              void ipc.stopSession(id).catch(() => {});
            }
          }}
          title={
            summary.status === "Exited" || summary.status === "Failed"
              ? "Remove session"
              : "Stop session"
          }
          className="cursor-pointer rounded p-0.5 text-fg-muted hover:bg-raised hover:text-danger"
        >
          {summary.status === "Exited" || summary.status === "Failed" ? (
            <Trash2 size={11} />
          ) : (
            <CircleStop size={11} />
          )}
        </span>
      </span>
      {index < 8 && (
        <span className="shrink-0 font-mono text-[9px] text-fg-muted group-hover:hidden">
          ⌃{index + 1}
        </span>
      )}
    </div>
  );
}
