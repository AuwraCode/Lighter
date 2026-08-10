import { Channel, invoke } from "@tauri-apps/api/core";
import type { Batch } from "./generated/Batch";
import type { PermissionDecisionDto } from "./generated/PermissionDecisionDto";
import type { RegistryBatch } from "./generated/RegistryBatch";
import type { SessionConfig } from "./generated/SessionConfig";
import type { SessionInfo } from "./generated/SessionInfo";
import type { SessionRecord } from "./generated/SessionRecord";
import type { SessionSnapshot } from "./generated/SessionSnapshot";
import type { SessionSummary } from "./generated/SessionSummary";
import type { TranscriptItem } from "./generated/TranscriptItem";
import type { ValidationReport } from "./generated/ValidationReport";
import type { SkillMeta } from "./generated/SkillMeta";
import type { TriggerSet } from "./generated/TriggerSet";
import type { EvalReport } from "./generated/EvalReport";
import type { DescriptionFix } from "./generated/DescriptionFix";
import type { IntentKind } from "./generated/IntentKind";
import type { DraftInput } from "./generated/DraftInput";
import type { Redundancy } from "./generated/Redundancy";
import type { ScriptSuggestion } from "./generated/ScriptSuggestion";
import type { SkillSpec } from "./generated/SkillSpec";
import type { ScaffoldResult } from "./generated/ScaffoldResult";
import type { SkillPluginInfo } from "./generated/SkillPluginInfo";
import type { LocalSkill } from "./generated/LocalSkill";
import type { CatalogPage } from "./generated/CatalogPage";
import type { McpInstall } from "./generated/McpInstall";
import type { InstalledMcp } from "./generated/InstalledMcp";
import type { RepoStars } from "./generated/RepoStars";

export type BatchHandler = (batch: Batch) => void;

function makeChannel(onBatch: BatchHandler): Channel<Batch> {
  const channel = new Channel<Batch>();
  channel.onmessage = onBatch;
  return channel;
}

export function createSession(
  config: SessionConfig,
  onBatch: BatchHandler,
): Promise<SessionInfo> {
  return invoke("create_session", { config, channel: makeChannel(onBatch) });
}

export function attachSession(
  sessionId: string,
  onBatch: BatchHandler,
): Promise<SessionSnapshot> {
  return invoke("attach_session", {
    sessionId,
    channel: makeChannel(onBatch),
  });
}

export function sendUserMessage(sessionId: string, text: string): Promise<void> {
  return invoke("send_user_message", { sessionId, text });
}

export function respondPermission(
  sessionId: string,
  requestId: string,
  decision: PermissionDecisionDto,
): Promise<void> {
  return invoke("respond_permission", { sessionId, requestId, decision });
}

export function setPermissionMode(sessionId: string, mode: string): Promise<void> {
  return invoke("set_permission_mode", { sessionId, mode });
}

export function setModel(sessionId: string, model: string): Promise<void> {
  return invoke("set_model", { sessionId, model });
}

export function interruptSession(sessionId: string): Promise<void> {
  return invoke("interrupt_session", { sessionId });
}

export function stopSession(sessionId: string, graceful = true): Promise<void> {
  return invoke("stop_session", { sessionId, graceful });
}

/** Resolves with a warning string when worktree cleanup was refused. */
export function removeSession(
  sessionId: string,
  cleanupWorktree = false,
): Promise<string | null> {
  return invoke("remove_session", { sessionId, cleanupWorktree });
}

export function listSessions(): Promise<SessionInfo[]> {
  return invoke("list_sessions");
}

/** Only visible sessions receive text deltas (single view: one id; split
 *  view: all pane ids; dashboard: none). */
export function setVisibleSessions(sessionIds: string[]): Promise<void> {
  return invoke("set_visible_sessions", { sessionIds });
}

export function skillValidate(
  path: string,
  strict: boolean,
): Promise<ValidationReport> {
  return invoke("skill_validate", { path, strict });
}

export function skillModelKind(configDir: string | null): Promise<string> {
  return invoke("skill_model_kind", { configDir });
}

export function skillBuildCatalog(dir: string): Promise<SkillMeta[]> {
  return invoke("skill_build_catalog", { dir });
}

export function skillLoadTestset(dir: string): Promise<TriggerSet> {
  return invoke("skill_load_testset", { dir });
}

export function skillSaveTestset(dir: string, testset: TriggerSet): Promise<void> {
  return invoke("skill_save_testset", { dir, testset });
}

export function skillGenerateTestset(
  dir: string,
  configDir: string | null,
): Promise<TriggerSet> {
  return invoke("skill_generate_testset", { dir, configDir });
}

export function skillRunEval(
  dir: string,
  configDir: string | null,
): Promise<EvalReport> {
  return invoke("skill_run_eval", { dir, configDir });
}

export function skillProposeFix(
  dir: string,
  skill: string,
  configDir: string | null,
): Promise<DescriptionFix> {
  return invoke("skill_propose_fix", { dir, skill, configDir });
}

export function skillApplyDescription(
  dir: string,
  skill: string,
  description: string,
): Promise<ValidationReport> {
  return invoke("skill_apply_description", { dir, skill, description });
}

export function skillIntentRedirect(kind: IntentKind): Promise<string | null> {
  return invoke("skill_intent_redirect", { kind });
}

export function skillCheckPersona(text: string): Promise<string[]> {
  return invoke("skill_check_persona", { text });
}

export function skillSuggestName(title: string): Promise<string> {
  return invoke("skill_suggest_name", { title });
}

export function skillDraftDescription(
  configDir: string | null,
  input: DraftInput,
): Promise<string> {
  return invoke("skill_draft_description", { configDir, input });
}

export function skillDraftBody(
  configDir: string | null,
  input: DraftInput,
): Promise<string> {
  return invoke("skill_draft_body", { configDir, input });
}

export function skillRedundancyCheck(
  configDir: string | null,
  name: string,
  description: string,
): Promise<Redundancy> {
  return invoke("skill_redundancy_check", { configDir, name, description });
}

export function skillSuggestScript(
  configDir: string | null,
  body: string,
): Promise<ScriptSuggestion | null> {
  return invoke("skill_suggest_script", { configDir, body });
}

export function skillListLocal(
  configDir: string | null,
  projectDir: string | null,
): Promise<LocalSkill[]> {
  return invoke("skill_list_local", { configDir, projectDir });
}

export function skillPluginsInfo(
  configDir: string | null,
): Promise<SkillPluginInfo[]> {
  return invoke("skill_plugins_info", { configDir });
}

export function installSkillPlugins(
  configDir: string | null,
): Promise<SkillPluginInfo[]> {
  return invoke("install_skill_plugins", { configDir });
}

export function skillScaffold(
  parentDir: string,
  spec: SkillSpec,
): Promise<ScaffoldResult> {
  return invoke("skill_scaffold", { parentDir, spec });
}

/** Subscribe to dashboard summaries; resolves with the current full list. */
export function mcpSearch(
  query: string | null,
  cursor: string | null,
): Promise<CatalogPage> {
  return invoke("mcp_search", { query, cursor });
}

export function mcpInstalled(
  configDir: string | null,
  projectDir: string | null,
): Promise<InstalledMcp[]> {
  return invoke("mcp_installed", { configDir, projectDir });
}

export function mcpInstall(p: {
  configDir: string | null;
  projectDir: string | null;
  scope: string;
  alias: string;
  install: McpInstall;
  values: Record<string, string>;
}): Promise<string> {
  return invoke("mcp_install", p);
}

export function mcpRemove(p: {
  configDir: string | null;
  projectDir: string | null;
  scope: string | null;
  name: string;
}): Promise<string> {
  return invoke("mcp_remove", p);
}

export function mcpLoginTerminal(
  configDir: string | null,
  name: string,
): Promise<void> {
  return invoke("mcp_login_terminal", { configDir, name });
}

export function mcpRepoStars(repoUrl: string): Promise<RepoStars | null> {
  return invoke("mcp_repo_stars", { repoUrl });
}

export function openExternal(url: string): Promise<void> {
  return invoke("open_external", { url });
}

export function attachRegistry(
  onBatch: (batch: RegistryBatch) => void,
): Promise<SessionSummary[]> {
  const channel = new Channel<RegistryBatch>();
  channel.onmessage = onBatch;
  return invoke("attach_registry", { channel });
}

export function listSessionRecords(): Promise<SessionRecord[]> {
  return invoke("list_session_records");
}

export function deleteSessionRecord(recordId: string): Promise<void> {
  return invoke("delete_session_record", { recordId });
}

export function resumeSession(recordId: string): Promise<SessionInfo> {
  const channel = new Channel<Batch>();
  channel.onmessage = () => {};
  return invoke("resume_session", { recordId, channel });
}

export function loadHistory(
  sessionId: string,
  cwd: string,
  claudeConfigDir: string | null,
): Promise<TranscriptItem[]> {
  return invoke("load_history", { sessionId, cwd, claudeConfigDir });
}
