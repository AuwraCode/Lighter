// Browse the official MCP registry and install servers into an account, plus
// manage what's already configured. Search hits the registry live; install goes
// through `claude mcp add` under the selected account (and project dir for
// local/project scope).

import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openFolder } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  Download,
  FolderOpen,
  Loader2,
  LogIn,
  Plug,
  RotateCcw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { cn } from "@/lib/cn";
import * as ipc from "@/lib/ipc";
import type { McpEntry } from "@/lib/generated/McpEntry";
import type { McpInstall } from "@/lib/generated/McpInstall";
import type { InstalledMcp } from "@/lib/generated/InstalledMcp";
import { useProfiles } from "@/stores/profiles";

const STATUS_DOT: Record<string, string> = {
  connected: "bg-success",
  failed: "bg-danger",
  pending: "bg-warning",
  unknown: "bg-fg-muted",
};

function previewOf(install: McpInstall): string {
  if (install.kind === "remote") return `${install.transport.toUpperCase()}  ${install.url}`;
  if (install.kind === "stdio") return [install.command, ...install.args].join(" ");
  return "—";
}

export function McpView() {
  const profiles = useProfiles((s) => s.profiles);
  const defaultProfileId = useProfiles((s) => s.defaultProfileId);

  const [profileId, setProfileId] = useState<string | null>(null);
  const [projectDir, setProjectDir] = useState<string | null>(null);

  const selectedId = profileId ?? defaultProfileId ?? profiles[0]?.id ?? null;
  const selected = profiles.find((p) => p.id === selectedId);
  const configDir = selected?.config_dir ?? null;

  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<McpEntry[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [installed, setInstalled] = useState<InstalledMcp[]>([]);
  const [target, setTarget] = useState<McpEntry | null>(null);

  const search = useCallback(async (q: string, cur: string | null) => {
    setLoading(true);
    setError(null);
    try {
      const page = await ipc.mcpSearch(q || null, cur);
      setEntries((prev) => (cur ? [...prev, ...page.entries] : page.entries));
      setCursor(page.next_cursor);
    } catch (e) {
      setError(String(e));
      if (!cur) setEntries([]);
    } finally {
      setLoading(false);
    }
  }, []);

  // Debounced live search (and initial browse on mount / query clear).
  useEffect(() => {
    const t = setTimeout(() => void search(query, null), 350);
    return () => clearTimeout(t);
  }, [query, search]);

  const refreshInstalled = useCallback(async () => {
    try {
      setInstalled(await ipc.mcpInstalled(configDir, projectDir));
    } catch {
      setInstalled([]);
    }
  }, [configDir, projectDir]);

  useEffect(() => {
    void refreshInstalled();
  }, [refreshInstalled]);

  const pickProject = useCallback(async () => {
    const picked = await openFolder({ directory: true });
    if (typeof picked === "string") setProjectDir(picked);
  }, []);

  const remove = useCallback(
    async (m: InstalledMcp) => {
      try {
        await ipc.mcpRemove({ configDir, projectDir, scope: null, name: m.name });
        toast.success(`Removed ${m.name}.`);
        void refreshInstalled();
      } catch (e) {
        toast.error(String(e));
      }
    },
    [configDir, projectDir, refreshInstalled],
  );

  const installedNames = useMemo(
    () => new Set(installed.map((m) => m.name)),
    [installed],
  );

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="w-full p-6">
        <div className="mb-1 flex items-center gap-2">
          <Plug size={15} className="text-accent" />
          <h1 className="text-sm font-semibold tracking-tight">MCP servers</h1>
          <div className="flex-1" />
          {profiles.length > 0 && (
            <label className="flex items-center gap-1.5 text-[11px] text-fg-secondary">
              Account
              <select
                value={selectedId ?? ""}
                onChange={(e) => setProfileId(e.target.value || null)}
                className="rounded-md border border-border bg-surface px-2 py-1 text-[11px] text-fg outline-none focus:border-accent"
              >
                {profiles.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                    {p.id === defaultProfileId ? " (default)" : ""}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
        <p className="mb-4 text-xs text-fg-secondary">
          Search the official MCP registry and install servers for this account.
          User scope applies everywhere; local/project scope needs a project
          folder and writes there.
        </p>

        {/* Project dir (for installed listing + local/project installs) ------ */}
        <div className="mb-4 flex items-center gap-2 text-[11px]">
          <span className="text-fg-muted">Project:</span>
          {projectDir ? (
            <>
              <span className="max-w-[360px] truncate font-mono text-fg-secondary">
                {projectDir}
              </span>
              <button
                onClick={() => setProjectDir(null)}
                title="Clear"
                className="rounded p-0.5 text-fg-muted hover:bg-hover hover:text-fg"
              >
                <X size={11} />
              </button>
            </>
          ) : (
            <button
              onClick={() => void pickProject()}
              className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-fg-secondary hover:bg-hover hover:text-fg"
            >
              <FolderOpen size={11} /> Pick a project (for project scope)
            </button>
          )}
        </div>

        {/* Installed --------------------------------------------------------- */}
        <div className="mb-2 flex items-center gap-2">
          <div className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
            Installed{selected ? ` · ${selected.name}` : ""}
          </div>
          <div className="flex-1" />
          <button
            onClick={() => void refreshInstalled()}
            title="Refresh installed"
            className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[10px] text-fg-secondary hover:bg-hover hover:text-fg"
          >
            <RotateCcw size={11} />
          </button>
        </div>
        {installed.length === 0 ? (
          <div className="mb-5 rounded-lg border border-border-subtle bg-surface px-3 py-3 text-center text-[11px] text-fg-muted">
            No MCP servers configured for this account
            {projectDir ? " / project" : ""} yet.
          </div>
        ) : (
          <div className="mb-5 grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-1.5">
            {installed.map((m) => (
              <div
                key={m.name}
                className="flex items-center gap-2.5 rounded-lg border border-border bg-surface px-3 py-2"
              >
                <span
                  className={cn(
                    "h-2 w-2 shrink-0 rounded-full",
                    STATUS_DOT[m.status] ?? STATUS_DOT.unknown,
                  )}
                  title={m.status}
                />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs font-medium text-fg">{m.name}</div>
                  <div className="truncate font-mono text-[10px] text-fg-muted">
                    {m.detail}
                  </div>
                </div>
                {(m.status === "pending" || m.status === "failed" || m.detail.includes("http")) && (
                  <button
                    onClick={() => void ipc.mcpLoginTerminal(configDir, m.name)}
                    title="Sign in (claude mcp login)"
                    className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-[10px] text-fg-secondary hover:bg-hover hover:text-fg"
                  >
                    <LogIn size={11} /> Sign in
                  </button>
                )}
                <button
                  onClick={() => void remove(m)}
                  title="Remove"
                  className="rounded-md border border-border p-1 text-fg-muted hover:bg-hover hover:text-danger"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Browse registry --------------------------------------------------- */}
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
          Browse registry
        </div>
        <div className="relative mb-3 max-w-2xl">
          <span className="pointer-events-none absolute inset-y-0 left-2.5 flex items-center text-fg-muted">
            <Search size={13} />
          </span>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search MCP servers (e.g. github, postgres, filesystem)…"
            className="w-full rounded-md border border-border bg-surface py-2 pl-8 pr-8 text-xs outline-none focus:border-accent"
          />
          {loading && (
            <span className="absolute inset-y-0 right-2.5 flex items-center text-fg-muted">
              <Loader2 size={13} className="animate-spin" />
            </span>
          )}
        </div>

        {error && (
          <div className="mb-3 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </div>
        )}

        {entries.length === 0 && !loading && !error ? (
          <div className="rounded-lg border border-border-subtle bg-surface px-3 py-6 text-center text-xs text-fg-muted">
            No servers match “{query}”.
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] items-stretch gap-1.5">
            {entries.map((e) => (
              <EntryRow
                key={e.name + e.version}
                entry={e}
                installed={installedNames.has(e.default_alias)}
                onInstall={() => setTarget(e)}
              />
            ))}
          </div>
        )}

        {cursor && (
          <button
            onClick={() => void search(query, cursor)}
            disabled={loading}
            className="mt-3 inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs text-fg-secondary hover:bg-hover hover:text-fg disabled:opacity-50"
          >
            {loading && <Loader2 size={12} className="animate-spin" />}
            Load more
          </button>
        )}
      </div>

      {target && (
        <InstallDialog
          entry={target}
          configDir={configDir}
          projectDir={projectDir}
          accountName={selected?.name ?? "this account"}
          onClose={() => setTarget(null)}
          onInstalled={() => {
            setTarget(null);
            void refreshInstalled();
          }}
        />
      )}
    </div>
  );
}

function EntryRow({
  entry,
  installed,
  onInstall,
}: {
  entry: McpEntry;
  installed: boolean;
  onInstall: () => void;
}) {
  const unsupported = entry.install.kind === "unsupported";
  return (
    <div className="flex h-full flex-col rounded-lg border border-border bg-surface px-3 py-2">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="min-w-0 truncate text-xs font-medium text-fg">
          {entry.display_name}
        </span>
        <span className="rounded bg-elevated px-1.5 py-0.5 text-[9px] font-medium text-fg-muted">
          {entry.transport_label}
        </span>
        {entry.version && (
          <span className="font-mono text-[9px] text-fg-muted">v{entry.version}</span>
        )}
        {installed && (
          <span className="rounded bg-success/15 px-1.5 py-0.5 text-[9px] font-medium text-success">
            installed
          </span>
        )}
      </div>
      <div className="truncate font-mono text-[10px] text-fg-muted">{entry.name}</div>
      {entry.description && (
        <div className="mt-0.5 line-clamp-2 text-[11px] text-fg-secondary">
          {entry.description}
        </div>
      )}
      <div className="mt-auto flex justify-end pt-2">
        <button
          onClick={onInstall}
          disabled={unsupported}
          title={unsupported ? "No installable package or remote in this entry" : "Install"}
          className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1.5 text-[11px] font-medium text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Download size={12} /> Install
        </button>
      </div>
    </div>
  );
}

const SCOPES = [
  { id: "user", label: "User", hint: "every repo & session of this account" },
  { id: "project", label: "Project", hint: "written to the project's .mcp.json" },
  { id: "local", label: "Local", hint: "private to you in this project" },
] as const;

function InstallDialog({
  entry,
  configDir,
  projectDir,
  accountName,
  onClose,
  onInstalled,
}: {
  entry: McpEntry;
  configDir: string | null;
  projectDir: string | null;
  accountName: string;
  onClose: () => void;
  onInstalled: () => void;
}) {
  const install = entry.install;
  const inputs = useMemo(
    () =>
      install.kind === "remote"
        ? install.headers
        : install.kind === "stdio"
          ? install.env
          : [],
    [install],
  );

  const [scope, setScope] = useState<string>("user");
  const [alias, setAlias] = useState(entry.default_alias);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      inputs.filter((i) => i.default).map((i) => [i.name, i.default as string]),
    ),
  );
  const [busy, setBusy] = useState(false);

  const needsProject = scope !== "user";
  const missingRequired = inputs.some(
    (i) => i.required && !(values[i.name] ?? "").trim(),
  );
  const blocked = busy || !alias.trim() || missingRequired || (needsProject && !projectDir);

  const doInstall = async () => {
    setBusy(true);
    try {
      const msg = await ipc.mcpInstall({
        configDir,
        projectDir,
        scope,
        alias: alias.trim(),
        install,
        values,
      });
      toast.success(msg || `Installed ${alias}.`);
      if (install.kind === "remote") {
        toast.info("Remote server may need sign-in: use “Sign in” on its row.", {
          duration: 6000,
        });
      }
      onInstalled();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div className="max-h-[85vh] w-[520px] overflow-y-auto rounded-xl border border-border bg-elevated p-4 shadow-2xl">
        <div className="mb-3 flex items-start justify-between">
          <div className="min-w-0">
            <div className="text-sm font-semibold">{entry.display_name}</div>
            <div className="truncate font-mono text-[10px] text-fg-muted">{entry.name}</div>
          </div>
          <button
            onClick={onClose}
            className="rounded p-1 text-fg-muted hover:bg-hover hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex flex-col gap-3 text-xs">
          <div>
            <label className="mb-1 block text-fg-secondary">Name</label>
            <input
              value={alias}
              onChange={(e) => setAlias(e.target.value)}
              className="w-full rounded-md border border-border bg-surface px-2.5 py-2 font-mono focus:border-accent focus:outline-none"
            />
          </div>

          <div>
            <label className="mb-1 block text-fg-secondary">Scope · {accountName}</label>
            <div className="grid grid-cols-3 gap-1.5">
              {SCOPES.map((s) => (
                <button
                  key={s.id}
                  onClick={() => setScope(s.id)}
                  title={s.hint}
                  className={cn(
                    "rounded-md border px-2 py-1.5 text-[11px]",
                    scope === s.id
                      ? "border-accent bg-accent/10 text-fg"
                      : "border-border bg-surface text-fg-secondary hover:bg-hover",
                  )}
                >
                  {s.label}
                </button>
              ))}
            </div>
            <p className="mt-1 text-[10px] text-fg-muted">
              {SCOPES.find((s) => s.id === scope)?.hint}
            </p>
            {needsProject && !projectDir && (
              <p className="mt-1 text-[10px] text-warning">
                Pick a project folder (top of the page) for {scope} scope.
              </p>
            )}
          </div>

          {inputs.length > 0 && (
            <div>
              <label className="mb-1 block text-fg-secondary">
                {install.kind === "remote" ? "Headers" : "Environment"}
              </label>
              <div className="flex flex-col gap-2">
                {inputs.map((i) => (
                  <div key={i.name}>
                    <div className="mb-0.5 flex items-center gap-1.5">
                      <span className="font-mono text-[11px] text-fg">{i.name}</span>
                      {i.required ? (
                        <span className="text-[9px] text-danger">required</span>
                      ) : (
                        <span className="text-[9px] text-fg-muted">optional</span>
                      )}
                      {i.secret && (
                        <span className="text-[9px] text-fg-muted">secret</span>
                      )}
                    </div>
                    <input
                      type={i.secret ? "password" : "text"}
                      value={values[i.name] ?? ""}
                      placeholder={i.description || i.name}
                      onChange={(e) =>
                        setValues((v) => ({ ...v, [i.name]: e.target.value }))
                      }
                      className="w-full rounded-md border border-border bg-surface px-2.5 py-1.5 font-mono focus:border-accent focus:outline-none"
                    />
                  </div>
                ))}
              </div>
            </div>
          )}

          <div>
            <label className="mb-1 block text-fg-secondary">Resolves to</label>
            <div className="overflow-x-auto rounded-md border border-border-subtle bg-surface px-2.5 py-2">
              <code className="whitespace-pre font-mono text-[10px] text-fg-muted">
                {previewOf(install)}
              </code>
            </div>
          </div>

          {entry.repository && (
            <div className="truncate font-mono text-[10px] text-fg-muted">
              {entry.repository}
            </div>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md border border-border px-3 py-1.5 text-xs text-fg-secondary hover:bg-hover"
          >
            Cancel
          </button>
          <button
            onClick={() => void doInstall()}
            disabled={blocked}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent-hover disabled:opacity-50"
          >
            {busy ? <Loader2 size={12} className="animate-spin" /> : <Download size={12} />}
            Install
          </button>
        </div>
      </div>
    </div>
  );
}
