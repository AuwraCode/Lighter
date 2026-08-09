// Split view: several sessions tiled in one screen. Drag a pane header onto
// another pane to snap it — center = swap, edges = split that side. Drag the
// dividers to resize. Layout persists across restarts (localStorage).

import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useStore } from "zustand";
import { toast } from "sonner";
import {
  Columns2,
  GripVertical,
  Maximize2,
  Plus,
  X,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import { statusColor, statusDotClass } from "@/lib/status";
import { SessionView } from "@/components/SessionView";
import { ModeSwitcher } from "@/components/SessionControls";
import {
  focusSession,
  openNewSession,
  pushVisibleSessions,
  registryStore,
  useRegistry,
} from "@/stores/registry";
import { getOrCreateSessionStore } from "@/stores/session";
import {
  activatePane,
  addSessionToWorkspace,
  closePane,
  collectLeaves,
  movePaneTo,
  pruneWorkspace,
  resizeSplit,
  useWorkspace,
  type DropZone,
  type LayoutNode,
} from "@/stores/workspace";

const DRAG_THRESHOLD_PX = 6;
const EDGE_BAND = 0.25;

interface DropTarget {
  paneId: string;
  zone: DropZone;
}

interface OverlayRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function Workspace() {
  const root = useWorkspace((s) => s.root);
  const containerRef = useRef<HTMLDivElement>(null);
  const [draggingPane, setDraggingPane] = useState<string | null>(null);
  const [overlay, setOverlay] = useState<OverlayRect | null>(null);
  const dropRef = useRef<DropTarget | null>(null);

  useEffect(() => {
    pruneWorkspace();
    pushVisibleSessions();
    // Keep pruning as sessions get removed elsewhere.
    return registryStore.subscribe(() => pruneWorkspace());
  }, []);

  // ------------------------------------------------------------------ drag
  const beginDrag = useCallback(
    (paneId: string, startEvent: React.PointerEvent) => {
      if (startEvent.button !== 0) return;
      const startX = startEvent.clientX;
      const startY = startEvent.clientY;
      let started = false;

      const hitTest = (x: number, y: number) => {
        const el = document
          .elementFromPoint(x, y)
          ?.closest<HTMLElement>("[data-pane-id]");
        const container = containerRef.current;
        if (!el || !container) {
          dropRef.current = null;
          setOverlay(null);
          return;
        }
        const targetId = el.dataset.paneId!;
        if (targetId === paneId) {
          dropRef.current = null;
          setOverlay(null);
          return;
        }
        const rect = el.getBoundingClientRect();
        const fx = (x - rect.left) / rect.width;
        const fy = (y - rect.top) / rect.height;
        const edges: [DropZone, number][] = [
          ["left", fx],
          ["right", 1 - fx],
          ["top", fy],
          ["bottom", 1 - fy],
        ];
        edges.sort((a, b) => a[1] - b[1]);
        const [edgeZone, edgeDist] = edges[0];
        const zone: DropZone = edgeDist > EDGE_BAND ? "center" : edgeZone;

        const base = containerRef.current!.getBoundingClientRect();
        let o: OverlayRect = {
          left: rect.left - base.left,
          top: rect.top - base.top,
          width: rect.width,
          height: rect.height,
        };
        if (zone === "left") o = { ...o, width: rect.width / 2 };
        if (zone === "right")
          o = { ...o, left: o.left + rect.width / 2, width: rect.width / 2 };
        if (zone === "top") o = { ...o, height: rect.height / 2 };
        if (zone === "bottom")
          o = { ...o, top: o.top + rect.height / 2, height: rect.height / 2 };
        dropRef.current = { paneId: targetId, zone };
        setOverlay(o);
      };

      const onMove = (e: PointerEvent) => {
        if (!started) {
          if (
            Math.abs(e.clientX - startX) < DRAG_THRESHOLD_PX &&
            Math.abs(e.clientY - startY) < DRAG_THRESHOLD_PX
          ) {
            return;
          }
          started = true;
          setDraggingPane(paneId);
        }
        hitTest(e.clientX, e.clientY);
      };

      const cleanup = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("keydown", onKey, { capture: true });
        setDraggingPane(null);
        setOverlay(null);
      };

      const onUp = () => {
        if (started && dropRef.current) {
          movePaneTo(paneId, dropRef.current.paneId, dropRef.current.zone);
        } else if (!started) {
          activatePane(paneId);
        }
        dropRef.current = null;
        cleanup();
      };

      const onKey = (e: KeyboardEvent) => {
        if (e.key === "Escape") {
          // Cancel the drag without interrupting any session.
          e.stopImmediatePropagation();
          e.preventDefault();
          dropRef.current = null;
          cleanup();
        }
      };

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("keydown", onKey, { capture: true });
    },
    [],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-1.5 text-xs">
        <Columns2 size={13} className="text-fg-muted" />
        <span className="font-medium">Split view</span>
        <span className="hidden text-fg-muted lg:inline">
          Drag pane headers to snap · center swaps, edges split · drag dividers
          to resize
        </span>
        <div className="ml-auto">
          <AddSessionMenu />
        </div>
      </div>

      <div
        ref={containerRef}
        className={cn(
          "relative min-h-0 flex-1 p-1.5",
          draggingPane && "cursor-grabbing",
        )}
      >
        {root ? (
          <LayoutView node={root} onDragStart={beginDrag} dragging={!!draggingPane} />
        ) : (
          <EmptyWorkspace />
        )}
        {overlay && (
          <div
            className="pointer-events-none absolute z-40 rounded-lg border-2 border-accent bg-accent/15 transition-all duration-100"
            style={{
              left: overlay.left,
              top: overlay.top,
              width: overlay.width,
              height: overlay.height,
            }}
          />
        )}
      </div>
    </div>
  );
}

function EmptyWorkspace() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-border bg-elevated">
        <Columns2 size={26} className="text-accent" />
      </div>
      <div className="text-center">
        <div className="text-sm font-medium">Split view is empty</div>
        <p className="mt-1 max-w-72 text-xs text-fg-secondary">
          Add running sessions and watch them side by side. Drag headers to
          rearrange; edges snap into splits.
        </p>
      </div>
      <AddSessionMenu big />
    </div>
  );
}

function AddSessionMenu({ big }: { big?: boolean }) {
  const [open, setOpen] = useState(false);
  const order = useRegistry((s) => s.order);
  const sessions = useRegistry((s) => s.sessions);
  const root = useWorkspace((s) => s.root);
  const inWorkspace = new Set(collectLeaves(root).map((l) => l.sessionId));
  const candidates = order.filter((id) => !inWorkspace.has(id));

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={cn(
          "inline-flex items-center gap-1.5 rounded-md bg-accent font-medium text-white hover:bg-accent-hover",
          big ? "px-3 py-1.5 text-xs" : "px-2.5 py-1 text-xs",
        )}
      >
        <Plus size={12} /> Add session
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute right-0 z-50 mt-1 w-64 rounded-lg border border-border bg-elevated p-1 shadow-2xl">
            {candidates.map((id) => (
              <button
                key={id}
                onClick={() => {
                  addSessionToWorkspace(id);
                  setOpen(false);
                }}
                className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-fg-secondary hover:bg-hover hover:text-fg"
              >
                <span
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    statusColor(sessions[id]?.status ?? "Starting"),
                  )}
                />
                <span className="truncate">{sessions[id]?.title ?? id}</span>
              </button>
            ))}
            {candidates.length === 0 && (
              <div className="px-2.5 py-1.5 text-xs text-fg-muted">
                All running sessions are already here.
              </div>
            )}
            <div className="my-1 h-px bg-border-subtle" />
            <button
              onClick={() => {
                setOpen(false);
                openNewSession(true);
              }}
              className="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <Plus size={12} /> New session…
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// recursive layout

function LayoutView({
  node,
  onDragStart,
  dragging,
}: {
  node: LayoutNode;
  onDragStart: (paneId: string, e: React.PointerEvent) => void;
  dragging: boolean;
}) {
  if (node.type === "leaf") {
    return <Pane node={node} onDragStart={onDragStart} dragging={dragging} />;
  }
  const horizontal = node.dir === "row";
  return (
    <div
      className={cn("flex h-full w-full", horizontal ? "flex-row" : "flex-col")}
    >
      <div
        className="min-h-0 min-w-0 overflow-hidden"
        style={{ flex: `0 0 calc(${node.ratio * 100}% - 3px)` }}
      >
        <LayoutView node={node.a} onDragStart={onDragStart} dragging={dragging} />
      </div>
      <Divider splitId={node.id} horizontal={horizontal} />
      <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
        <LayoutView node={node.b} onDragStart={onDragStart} dragging={dragging} />
      </div>
    </div>
  );
}

function Divider({ splitId, horizontal }: { splitId: string; horizontal: boolean }) {
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const parent = e.currentTarget.parentElement;
    if (!parent) return;
    const rect = parent.getBoundingClientRect();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      const ratio = horizontal
        ? (ev.clientX - rect.left) / rect.width
        : (ev.clientY - rect.top) / rect.height;
      resizeSplit(splitId, ratio);
    };
    const onUp = () => {
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
    };
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
  };

  return (
    <div
      onPointerDown={onPointerDown}
      className={cn(
        "group z-10 flex shrink-0 items-center justify-center",
        horizontal ? "w-1.5 cursor-col-resize" : "h-1.5 cursor-row-resize",
      )}
    >
      <div
        className={cn(
          "rounded-full bg-border transition-colors group-hover:bg-accent",
          horizontal ? "h-8 w-[3px]" : "h-[3px] w-8",
        )}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// pane

function Pane({
  node,
  onDragStart,
  dragging,
}: {
  node: Extract<LayoutNode, { type: "leaf" }>;
  onDragStart: (paneId: string, e: React.PointerEvent) => void;
  dragging: boolean;
}) {
  const active = useWorkspace((s) => s.activePaneId === node.id);
  return (
    <div
      data-pane-id={node.id}
      onPointerDown={() => activatePane(node.id)}
      className={cn(
        "flex h-full w-full flex-col overflow-hidden rounded-lg border bg-bg transition-colors",
        active ? "border-accent/50" : "border-border",
        dragging && "pointer-events-auto",
      )}
    >
      <PaneChrome node={node} active={active} onDragStart={onDragStart} />
      <SessionView
        key={node.sessionId}
        sessionId={node.sessionId}
        chrome="none"
        active={active}
      />
    </div>
  );
}

function PaneChrome({
  node,
  active,
  onDragStart,
}: {
  node: Extract<LayoutNode, { type: "leaf" }>;
  active: boolean;
  onDragStart: (paneId: string, e: React.PointerEvent) => void;
}) {
  const store = getOrCreateSessionStore(node.sessionId);
  const meta = useStore(store, (s) => s.meta);
  const status = useStore(store, (s) => s.status);
  const stats = useStore(store, (s) => s.stats);
  const exited = useStore(store, (s) => s.exited);

  const changeMode = useCallback(
    (mode: string) => {
      void ipc
        .setPermissionMode(node.sessionId, mode)
        .catch((e) => toast.error(`Mode change failed: ${e}`));
    },
    [node.sessionId],
  );

  return (
    <div
      onPointerDown={(e) => {
        const el = e.target as HTMLElement;
        if (el.closest("button") || el.closest("select")) return;
        onDragStart(node.id, e);
      }}
      className={cn(
        "flex h-8 shrink-0 cursor-grab items-center gap-2 border-b border-border-subtle px-2 text-xs active:cursor-grabbing",
        active ? "bg-elevated" : "bg-surface",
      )}
      title="Drag to rearrange"
    >
      <GripVertical size={12} className="shrink-0 text-fg-muted" />
      <span className={cn("h-2 w-2 shrink-0 rounded-full", statusDotClass(status))} />
      <span className="min-w-0 flex-1 truncate font-medium">
        {meta?.title ?? "Session"}
      </span>
      <ModeSwitcher
        current={meta?.permission_mode ?? "default"}
        disabled={!!exited}
        onChange={changeMode}
      />
      {stats && (
        <span className="hidden shrink-0 font-mono text-[10px] text-fg-muted md:inline">
          ${stats.total_cost_usd.toFixed(3)}
        </span>
      )}
      <button
        onClick={() => focusSession(node.sessionId)}
        title="Open full view"
        className="shrink-0 rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
      >
        <Maximize2 size={11} />
      </button>
      <button
        onClick={() => closePane(node.id)}
        title="Remove pane (session keeps running)"
        className="shrink-0 rounded p-1 text-fg-muted hover:bg-hover hover:text-danger"
      >
        <X size={12} />
      </button>
    </div>
  );
}
