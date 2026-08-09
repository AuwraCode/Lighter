// Shared session header controls (used by the full view and split-view panes).

import type { HandshakeInfo } from "@/lib/generated/HandshakeInfo";

// The set_permission_mode control request accepts "default" (not "manual",
// which is flag-only) — see PROTOCOL.md.
export const MODES = [
  "default",
  "plan",
  "acceptEdits",
  "auto",
  "dontAsk",
  "bypassPermissions",
];

export function ModeSwitcher({
  current,
  disabled,
  onChange,
}: {
  current: string;
  disabled: boolean;
  onChange: (mode: string) => void;
}) {
  const options = MODES.includes(current) ? MODES : [current, ...MODES];
  return (
    <select
      value={current}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      title="Permission mode (live)"
      className="shrink-0 rounded border border-border bg-elevated px-1.5 py-0.5 text-xs disabled:opacity-50"
    >
      {options.map((m) => (
        <option key={m} value={m}>
          {m}
        </option>
      ))}
    </select>
  );
}

export interface ModelOption {
  value: string;
  displayName: string;
  resolvedModel?: string;
}

export function ModelSwitcher({
  current,
  handshake,
  disabled,
  onChange,
}: {
  current: string;
  handshake: HandshakeInfo | null;
  disabled: boolean;
  onChange: (model: string) => void;
}) {
  const models: ModelOption[] = Array.isArray(handshake?.models)
    ? (handshake!.models as unknown as ModelOption[])
    : [];
  // meta.model is the RESOLVED id (claude-haiku-4-5-…); options use aliases.
  const selected =
    models.find((m) => m.resolvedModel === current || m.value === current)?.value ??
    current;
  const hasSelected = models.some((m) => m.value === selected);
  return (
    <select
      value={selected}
      disabled={disabled || models.length === 0}
      onChange={(e) => onChange(e.target.value)}
      title="Model (live)"
      className="max-w-40 shrink-0 truncate rounded border border-border bg-elevated px-1.5 py-0.5 text-xs disabled:opacity-50"
    >
      {!hasSelected && <option value={selected}>{selected || "model"}</option>}
      {models.map((m) => (
        <option key={m.value} value={m.value}>
          {m.displayName}
        </option>
      ))}
    </select>
  );
}

/** Which account this session actually runs as (from the handshake). */
export function AccountChip({ handshake }: { handshake: HandshakeInfo | null }) {
  const account = handshake?.account as { email?: string } | null;
  if (!account?.email) return null;
  return (
    <span
      className="hidden shrink-0 truncate text-[10px] text-fg-muted xl:inline"
      title={`Signed in as ${account.email}`}
    >
      {account.email}
    </span>
  );
}
