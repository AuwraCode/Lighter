// Pure layout-tree operations: split, remove, drag-move (swap/edge-snap),
// resize clamping and pruning.

import { describe, expect, test } from "vitest";
import {
  addPane,
  collectLeaves,
  leaf,
  movePane,
  pruneMissing,
  removeLeaf,
  setRatio,
  splitLeaf,
  type LayoutNode,
} from "./workspace";

function sessions(node: LayoutNode | null): string[] {
  return collectLeaves(node).map((l) => l.sessionId);
}

describe("layout tree", () => {
  test("addPane splits the active leaf and alternates direction", () => {
    const a = addPane(null, "s1", null);
    expect(sessions(a.root)).toEqual(["s1"]);

    const b = addPane(a.root, "s2", a.paneId);
    expect(sessions(b.root)).toEqual(["s1", "s2"]);
    expect(b.root.type).toBe("split");

    const c = addPane(b.root, "s3", b.paneId);
    expect(sessions(c.root)).toEqual(["s1", "s2", "s3"]);
    // s2's parent was a col split (root leaf splits as col by default
    // heuristic), so the next split alternates.
    const root = c.root as Extract<LayoutNode, { type: "split" }>;
    const inner = root.b as Extract<LayoutNode, { type: "split" }>;
    expect(inner.type).toBe("split");
    expect(inner.dir).not.toBe(root.dir);
  });

  test("removeLeaf collapses the parent split", () => {
    const l1 = leaf("s1");
    const root = splitLeaf(l1, l1.id, leaf("s2"), "row");
    const leaves = collectLeaves(root);
    const after = removeLeaf(root, leaves[0].id);
    expect(after?.type).toBe("leaf");
    expect(sessions(after)).toEqual(["s2"]);
    expect(removeLeaf(after, collectLeaves(after)[0].id)).toBeNull();
  });

  test("movePane center swaps the two sessions in place", () => {
    const l1 = leaf("s1");
    let root = splitLeaf(l1, l1.id, leaf("s2"), "row");
    const [a, b] = collectLeaves(root);
    root = movePane(root, a.id, b.id, "center");
    const after = collectLeaves(root);
    // Same pane ids, swapped sessions — geometry untouched.
    expect(after.map((l) => l.id)).toEqual([a.id, b.id]);
    expect(after.map((l) => l.sessionId)).toEqual(["s2", "s1"]);
  });

  test("movePane to an edge detaches and splits the target side", () => {
    // [s1 | s2] then drag s1 UNDER s2 → single col split [s2 / s1].
    const l1 = leaf("s1");
    const root = splitLeaf(l1, l1.id, leaf("s2"), "row");
    const [a, b] = collectLeaves(root);
    const after = movePane(root, a.id, b.id, "bottom");
    expect(sessions(after)).toEqual(["s2", "s1"]);
    const split = after as Extract<LayoutNode, { type: "split" }>;
    expect(split.type).toBe("split");
    expect(split.dir).toBe("col");

    // "top" places it before the target instead.
    const after2 = movePane(root, a.id, b.id, "top");
    expect(sessions(after2)).toEqual(["s1", "s2"]);
    expect((after2 as Extract<LayoutNode, { type: "split" }>).dir).toBe("col");
  });

  test("movePane onto itself is a no-op", () => {
    const l1 = leaf("s1");
    const root = splitLeaf(l1, l1.id, leaf("s2"), "row");
    const [a] = collectLeaves(root);
    expect(movePane(root, a.id, a.id, "left")).toBe(root);
  });

  test("setRatio clamps to sane bounds", () => {
    const l1 = leaf("s1");
    const root = splitLeaf(l1, l1.id, leaf("s2"), "row") as Extract<
      LayoutNode,
      { type: "split" }
    >;
    expect((setRatio(root, root.id, 0.01) as typeof root).ratio).toBeCloseTo(0.15);
    expect((setRatio(root, root.id, 0.99) as typeof root).ratio).toBeCloseTo(0.85);
    expect((setRatio(root, root.id, 0.6) as typeof root).ratio).toBeCloseTo(0.6);
  });

  test("pruneMissing drops dead sessions and collapses", () => {
    const l1 = leaf("s1");
    let root: LayoutNode | null = splitLeaf(l1, l1.id, leaf("s2"), "row");
    const [, b] = collectLeaves(root);
    root = splitLeaf(root, b.id, leaf("s3"), "col");
    expect(sessions(root)).toEqual(["s1", "s2", "s3"]);

    const alive = (id: string) => id !== "s2";
    const pruned = pruneMissing(root, alive);
    expect(sessions(pruned)).toEqual(["s1", "s3"]);

    expect(pruneMissing(pruned, () => false)).toBeNull();
    expect(pruneMissing(pruned, () => true)).toBe(pruned);
  });
});
