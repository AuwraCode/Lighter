// Split-view workspace: a binary layout tree (tmux/VS Code style).
// Leaves are session panes; splits carry a direction and ratio. All tree
// operations are pure functions (unit-tested in workspace.test.ts).

import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
// Function-level usage only — safe module cycle (registry ↔ workspace).
import { pushVisibleSessions, registryStore } from "./registry";

export type DropZone = "center" | "left" | "right" | "top" | "bottom";

export type LayoutNode =
  | { type: "leaf"; id: string; sessionId: string }
  | {
      type: "split";
      id: string;
      dir: "row" | "col";
      ratio: number;
      a: LayoutNode;
      b: LayoutNode;
    };

const MIN_RATIO = 0.15;
const STORAGE_KEY = "lighter.workspace.v1";

let counter = 0;
function nextId(): string {
  counter += 1;
  return `n${Date.now().toString(36)}${counter}`;
}

export function leaf(sessionId: string): LayoutNode {
  return { type: "leaf", id: nextId(), sessionId };
}

// ---------------------------------------------------------------------------
// pure tree operations

export function collectLeaves(node: LayoutNode | null): Extract<LayoutNode, { type: "leaf" }>[] {
  if (!node) return [];
  if (node.type === "leaf") return [node];
  return [...collectLeaves(node.a), ...collectLeaves(node.b)];
}

export function findLeaf(
  node: LayoutNode | null,
  paneId: string,
): Extract<LayoutNode, { type: "leaf" }> | null {
  return collectLeaves(node).find((l) => l.id === paneId) ?? null;
}

/** Depth of a leaf (used to alternate split direction on add). */
function leafDepthDir(node: LayoutNode | null, paneId: string, dir: "row" | "col"): "row" | "col" | null {
  if (!node) return null;
  if (node.type === "leaf") return node.id === paneId ? dir : null;
  return (
    leafDepthDir(node.a, paneId, node.dir) ?? leafDepthDir(node.b, paneId, node.dir)
  );
}

/** Split the given leaf, placing `newNode` after it. */
export function splitLeaf(
  root: LayoutNode,
  paneId: string,
  newNode: LayoutNode,
  dir: "row" | "col",
  before = false,
): LayoutNode {
  if (root.type === "leaf") {
    if (root.id !== paneId) return root;
    const [a, b] = before ? [newNode, root] : [root, newNode];
    return { type: "split", id: nextId(), dir, ratio: 0.5, a, b };
  }
  return {
    ...root,
    a: splitLeaf(root.a, paneId, newNode, dir, before),
    b: splitLeaf(root.b, paneId, newNode, dir, before),
  };
}

/** Remove a leaf; the sibling replaces the parent split. */
export function removeLeaf(root: LayoutNode | null, paneId: string): LayoutNode | null {
  if (!root) return null;
  if (root.type === "leaf") {
    return root.id === paneId ? null : root;
  }
  const a = removeLeaf(root.a, paneId);
  const b = removeLeaf(root.b, paneId);
  if (a === null) return b;
  if (b === null) return a;
  if (a === root.a && b === root.b) return root;
  return { ...root, a, b };
}

export function setRatio(root: LayoutNode, splitId: string, ratio: number): LayoutNode {
  if (root.type === "leaf") return root;
  if (root.id === splitId) {
    return { ...root, ratio: Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, ratio)) };
  }
  return {
    ...root,
    a: setRatio(root.a, splitId, ratio),
    b: setRatio(root.b, splitId, ratio),
  };
}

function swapSessions(root: LayoutNode, paneA: string, paneB: string): LayoutNode {
  const a = findLeaf(root, paneA);
  const b = findLeaf(root, paneB);
  if (!a || !b) return root;
  const map = (node: LayoutNode): LayoutNode => {
    if (node.type === "leaf") {
      if (node.id === paneA) return { ...node, sessionId: b.sessionId };
      if (node.id === paneB) return { ...node, sessionId: a.sessionId };
      return node;
    }
    return { ...node, a: map(node.a), b: map(node.b) };
  };
  return map(root);
}

/** Drag-and-drop: move `srcPaneId` onto `targetPaneId` at `zone`.
 *  center = swap sessions; edges = detach + split the target. */
export function movePane(
  root: LayoutNode,
  srcPaneId: string,
  targetPaneId: string,
  zone: DropZone,
): LayoutNode {
  if (srcPaneId === targetPaneId) return root;
  if (zone === "center") return swapSessions(root, srcPaneId, targetPaneId);
  const src = findLeaf(root, srcPaneId);
  if (!src || !findLeaf(root, targetPaneId)) return root;
  const without = removeLeaf(root, srcPaneId);
  // Removing the source can never empty the tree here (target still exists).
  if (!without) return root;
  const dir: "row" | "col" = zone === "left" || zone === "right" ? "row" : "col";
  const before = zone === "left" || zone === "top";
  const fresh = { ...src, id: nextId() };
  return splitLeaf(without, targetPaneId, fresh, dir, before);
}

/** Add a session as a new pane (splits the active/last leaf). */
export function addPane(
  root: LayoutNode | null,
  sessionId: string,
  activePaneId: string | null,
): { root: LayoutNode; paneId: string } {
  const fresh = leaf(sessionId);
  if (!root) return { root: fresh, paneId: fresh.id };
  const leaves = collectLeaves(root);
  const target =
    leaves.find((l) => l.id === activePaneId) ?? leaves[leaves.length - 1];
  // Alternate direction relative to the parent split for a balanced grid.
  const parentDir = leafDepthDir(root, target.id, "row") ?? "row";
  const dir = parentDir === "row" ? "col" : "row";
  return { root: splitLeaf(root, target.id, fresh, dir), paneId: fresh.id };
}

/** Drop panes whose sessions no longer exist. */
export function pruneMissing(
  root: LayoutNode | null,
  isAlive: (sessionId: string) => boolean,
): LayoutNode | null {
  if (!root) return null;
  for (const l of collectLeaves(root)) {
    if (!isAlive(l.sessionId)) {
      return pruneMissing(removeLeaf(root, l.id), isAlive);
    }
  }
  return root;
}

// ---------------------------------------------------------------------------
// store

interface WorkspaceState {
  root: LayoutNode | null;
  activePaneId: string | null;
}

export const workspaceStore = createStore<WorkspaceState>(() => load());

export function useWorkspace<T>(selector: (s: WorkspaceState) => T): T {
  return useStore(workspaceStore, selector);
}

function load(): WorkspaceState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const root = JSON.parse(raw) as LayoutNode | null;
      return { root, activePaneId: collectLeaves(root)[0]?.id ?? null };
    }
  } catch {
    // corrupted layout → start empty
  }
  return { root: null, activePaneId: null };
}

function persist(root: LayoutNode | null) {
  try {
    if (root) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(root));
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    // storage full/unavailable — layout just won't survive reloads
  }
}

function commit(root: LayoutNode | null, activePaneId?: string | null) {
  workspaceStore.setState((prev) => {
    const leaves = collectLeaves(root);
    const active =
      activePaneId !== undefined
        ? activePaneId
        : leaves.some((l) => l.id === prev.activePaneId)
          ? prev.activePaneId
          : (leaves[0]?.id ?? null);
    return { root, activePaneId: active };
  });
  persist(root);
  pushVisibleSessions();
}

export function workspaceSessionIds(): string[] {
  return [...new Set(collectLeaves(workspaceStore.getState().root).map((l) => l.sessionId))];
}

export function addSessionToWorkspace(sessionId: string) {
  const { root, activePaneId } = workspaceStore.getState();
  if (collectLeaves(root).some((l) => l.sessionId === sessionId)) return;
  const next = addPane(root, sessionId, activePaneId);
  commit(next.root, next.paneId);
}

export function closePane(paneId: string) {
  const { root } = workspaceStore.getState();
  commit(removeLeaf(root, paneId));
}

export function movePaneTo(srcPaneId: string, targetPaneId: string, zone: DropZone) {
  const { root } = workspaceStore.getState();
  if (!root) return;
  commit(movePane(root, srcPaneId, targetPaneId, zone), srcPaneId);
}

export function resizeSplit(splitId: string, ratio: number) {
  const { root } = workspaceStore.getState();
  if (!root) return;
  const next = setRatio(root, splitId, ratio);
  workspaceStore.setState({ root: next });
  persist(next);
}

export function activatePane(paneId: string) {
  const pane = findLeaf(workspaceStore.getState().root, paneId);
  if (!pane) return;
  workspaceStore.setState({ activePaneId: paneId });
  // Palette / slash-command inserts target the focused session.
  registryStore.setState({ focusedId: pane.sessionId });
}

export function pruneWorkspace() {
  const { root } = workspaceStore.getState();
  const sessions = registryStore.getState().sessions;
  const pruned = pruneMissing(root, (id) => id in sessions);
  if (pruned !== root) commit(pruned);
}
