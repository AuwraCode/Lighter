import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Preset } from "@/lib/generated/Preset";
import { focusSession } from "./registry";
import * as ipc from "@/lib/ipc";

interface PresetsState {
  presets: Preset[];
  /** null = closed, "new" = create, otherwise the preset id being edited. */
  editing: string | null;
}

export const presetsStore = createStore<PresetsState>(() => ({
  presets: [],
  editing: null,
}));

export function usePresets<T>(selector: (s: PresetsState) => T): T {
  return useStore(presetsStore, selector);
}

export async function loadPresets() {
  const presets = await invoke<Preset[]>("list_presets");
  presetsStore.setState({ presets });
}

export async function savePreset(preset: Preset) {
  await invoke("save_preset", { preset });
  await loadPresets();
}

export async function deletePreset(presetId: string) {
  await invoke("delete_preset", { presetId });
  await loadPresets();
}

export function editPreset(id: string | null) {
  presetsStore.setState({ editing: id });
}

/** One click on the dashboard: preset → running, focused session. */
export async function launchPreset(preset: Preset) {
  const info = await ipc.createSession(
    {
      cwd: preset.cwd,
      title: preset.name,
      model: preset.model,
      permission_mode: preset.permission_mode,
      effort: preset.effort,
      allowed_tools: preset.allowed_tools,
      disallowed_tools: preset.disallowed_tools,
      append_system_prompt: preset.append_system_prompt,
      initial_prompt: preset.initial_prompt,
      resume_session_id: null,
      worktree_policy: preset.worktree_policy,
    },
    () => {},
  );
  focusSession(info.id);
}
