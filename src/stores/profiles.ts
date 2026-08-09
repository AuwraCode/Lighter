// Account profiles: each profile is a named CLAUDE_CONFIG_DIR, i.e. its own
// signed-in Anthropic account.

import { createStore } from "zustand/vanilla";
import { useStore } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Profile } from "@/lib/generated/Profile";
import type { ProfilesInfo } from "@/lib/generated/ProfilesInfo";

export interface AuthStatus {
  loggedIn?: boolean;
  email?: string;
  subscriptionType?: string;
  authMethod?: string;
}

interface ProfilesState {
  profiles: Profile[];
  defaultProfileId: string | null;
  dialogOpen: boolean;
  /** profile id → last fetched auth status (undefined = loading/unknown). */
  auth: Record<string, AuthStatus | null>;
}

export const profilesStore = createStore<ProfilesState>(() => ({
  profiles: [],
  defaultProfileId: null,
  dialogOpen: false,
  auth: {},
}));

export function useProfiles<T>(selector: (s: ProfilesState) => T): T {
  return useStore(profilesStore, selector);
}

export async function loadProfiles() {
  const info = await invoke<ProfilesInfo>("list_profiles");
  profilesStore.setState({
    profiles: info.profiles,
    defaultProfileId: info.default_profile_id,
  });
}

export async function saveProfile(profile: Profile) {
  await invoke("save_profile", { profile });
  await loadProfiles();
}

export async function deleteProfile(profileId: string) {
  await invoke("delete_profile", { profileId });
  await loadProfiles();
}

export async function setDefaultProfile(profileId: string) {
  await invoke("set_default_profile", { profileId });
  await loadProfiles();
}

export async function discoverProfiles(): Promise<Profile[]> {
  return invoke<Profile[]>("discover_profiles");
}

export function openProfilesDialog(open: boolean) {
  profilesStore.setState({ dialogOpen: open });
}

export async function refreshAuthStatus(profile: Profile) {
  try {
    const status = await invoke<AuthStatus>("profile_auth_status", {
      configDir: profile.config_dir,
    });
    profilesStore.setState((s) => ({ auth: { ...s.auth, [profile.id]: status } }));
  } catch {
    profilesStore.setState((s) => ({ auth: { ...s.auth, [profile.id]: null } }));
  }
}

export function openLoginTerminal(configDir: string | null) {
  return invoke("open_login_terminal", { configDir });
}

export function defaultProfile(): Profile | undefined {
  const s = profilesStore.getState();
  return (
    s.profiles.find((p) => p.id === s.defaultProfileId) ?? s.profiles[0]
  );
}

export function profileById(id: string | null | undefined): Profile | undefined {
  if (!id) return undefined;
  return profilesStore.getState().profiles.find((p) => p.id === id);
}
