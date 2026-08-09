// Reducer parity: replay the Rust normalizer's event streams (generated from
// real CLI fixtures by `cargo test generate_normalized_dumps`) and assert the
// TypeScript store lands on exactly the Rust snapshot.

import { describe, expect, test } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Batch } from "@/lib/generated/Batch";
import {
  applyBatch,
  dropSessionStore,
  getOrCreateSessionStore,
} from "./session";

const dir = fileURLToPath(
  new URL("../../src-tauri/tests/normalized", import.meta.url),
);

const names = [
  ...new Set(
    readdirSync(dir)
      .filter((f) => f.endsWith(".events.json"))
      .map((f) => f.replace(".events.json", "")),
  ),
].sort();

const DEFAULT_STATS = {
  total_cost_usd: 0,
  turns: 0,
  context_used_tokens: null,
  context_window: null,
};

describe("applyBatch parity with Rust SessionState", () => {
  expect(names.length).toBeGreaterThan(0);

  for (const name of names) {
    test(name, () => {
      const events = JSON.parse(
        readFileSync(join(dir, `${name}.events.json`), "utf8"),
      );
      const rust = JSON.parse(
        readFileSync(join(dir, `${name}.state.json`), "utf8"),
      );

      // Run 1: one batch per event (streaming-like delivery).
      const perEvent = getOrCreateSessionStore(`per-${name}`);
      for (const env of events) {
        applyBatch(perEvent, { session_id: "t", events: [env] } as Batch);
      }
      // Run 2: everything in a single batch (attach-like delivery).
      const single = getOrCreateSessionStore(`single-${name}`);
      applyBatch(single, { session_id: "t", events } as Batch);

      for (const store of [perEvent, single]) {
        const s = store.getState();
        expect(s.items).toEqual(rust.items);
        expect(s.status).toEqual(rust.status);
        expect(s.pending).toEqual(rust.pending_permissions);
        expect(s.stats ?? DEFAULT_STATS).toEqual(rust.stats);
      }

      // Replaying the same events must be a no-op (seq dedupe).
      const before = JSON.stringify(perEvent.getState().items);
      applyBatch(perEvent, { session_id: "t", events } as Batch);
      expect(JSON.stringify(perEvent.getState().items)).toBe(before);

      dropSessionStore(`per-${name}`);
      dropSessionStore(`single-${name}`);
    });
  }
});
