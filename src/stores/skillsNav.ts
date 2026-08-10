// Which tab the Skills view shows, plus one-shot hand-offs so the Installed
// hub can send a specific folder straight into Validate or Trigger eval.

import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";

export type SkillsTab = "installed" | "new" | "validate" | "eval";

interface SkillsNavState {
  tab: SkillsTab;
  /** A skill folder to load into the Validate tab (consumed on pickup). */
  validateDir: string | null;
  /** A skills *parent* folder to load into the Eval tab (consumed on pickup). */
  evalDir: string | null;
}

export const skillsNavStore = createStore<SkillsNavState>(() => ({
  tab: "installed",
  validateDir: null,
  evalDir: null,
}));

export function useSkillsNav<T>(selector: (s: SkillsNavState) => T): T {
  return useStore(skillsNavStore, selector);
}

export function setSkillsTab(tab: SkillsTab) {
  skillsNavStore.setState({ tab });
}

/** Jump to Validate with a skill folder preloaded. */
export function openValidateFor(dir: string) {
  skillsNavStore.setState({ tab: "validate", validateDir: dir });
}

/** Jump to Trigger eval with a skills parent folder preloaded. */
export function openEvalFor(dir: string) {
  skillsNavStore.setState({ tab: "eval", evalDir: dir });
}

export function consumeValidateDir() {
  skillsNavStore.setState({ validateDir: null });
}

export function consumeEvalDir() {
  skillsNavStore.setState({ evalDir: null });
}
