import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "@/lib/generated/AppSettings";

interface SettingsState {
  settings: AppSettings;
  dialogOpen: boolean;
}

const EMPTY: AppSettings = {
  claude_bin: null,
  worktree_base: null,
  default_model: null,
  default_permission_mode: null,
  skill_plugins: [],
};

export const settingsStore = createStore<SettingsState>(() => ({
  settings: EMPTY,
  dialogOpen: false,
}));

export function useSettings<T>(selector: (s: SettingsState) => T): T {
  return useStore(settingsStore, selector);
}

export async function loadSettings() {
  const settings = await invoke<AppSettings>("get_settings");
  settingsStore.setState({ settings });
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  const saved = await invoke<AppSettings>("save_settings", {
    newSettings: settings,
  });
  settingsStore.setState({ settings: saved });
  return saved;
}

export function openSettingsDialog(open: boolean) {
  settingsStore.setState({ dialogOpen: open });
}
