import type { SessionStatus } from "./generated/SessionStatus";

export function statusColor(status: SessionStatus): string {
  switch (status) {
    case "Working":
    case "Compacting":
      return "bg-accent";
    case "AwaitingApproval":
      return "bg-warning";
    case "Idle":
      return "bg-success";
    case "Failed":
      return "bg-danger";
    case "Exited":
      return "bg-fg-muted";
    default:
      return "bg-fg-muted";
  }
}

/** Dot classes incl. a pulse while the session is actively doing something. */
export function statusDotClass(status: SessionStatus): string {
  const pulse =
    status === "Working" || status === "Compacting" || status === "Starting"
      ? " animate-pulse"
      : "";
  return statusColor(status) + pulse;
}

// ---------------------------------------------------------------------------
// permission modes: one color language everywhere

/** Values accepted by the set_permission_mode control request. */
export const PERMISSION_MODES = [
  "default",
  "plan",
  "acceptEdits",
  "auto",
  "dontAsk",
  "bypassPermissions",
];

export const MODE_DESCRIPTIONS: Record<string, string> = {
  default: "Ask before edits and risky commands",
  plan: "Read-only planning — no changes to files",
  acceptEdits: "Auto-approve file edits, ask for the rest",
  auto: "Classifier auto-approves safe actions",
  dontAsk: "Never prompt — auto-deny anything unapproved",
  bypassPermissions: "No permission checks at all (dangerous)",
};

const MODE_SELECT: Record<string, string> = {
  default: "text-fg-secondary border-border",
  plan: "text-info border-info/50",
  acceptEdits: "text-success border-success/50",
  auto: "text-accent border-accent/50",
  dontAsk: "text-warning border-warning/50",
  bypassPermissions: "text-danger border-danger/50",
};

const MODE_TEXT: Record<string, string> = {
  default: "text-fg-muted",
  plan: "text-info",
  acceptEdits: "text-success",
  auto: "text-accent",
  dontAsk: "text-warning",
  bypassPermissions: "text-danger",
};

const MODE_PILL: Record<string, string> = {
  default: "border-fg-muted/60 bg-hover text-fg",
  plan: "border-info bg-info/15 text-info",
  acceptEdits: "border-success bg-success/15 text-success",
  auto: "border-accent bg-accent/15 text-accent",
  dontAsk: "border-warning bg-warning/15 text-warning",
  bypassPermissions: "border-danger bg-danger/15 text-danger",
};

export function modeSelectClass(mode: string): string {
  return MODE_SELECT[mode] ?? MODE_SELECT.default;
}

export function modeTextClass(mode: string): string {
  return MODE_TEXT[mode] ?? MODE_TEXT.default;
}

export function modePillClass(mode: string, selected: boolean): string {
  return selected
    ? (MODE_PILL[mode] ?? MODE_PILL.default)
    : "border-border bg-elevated text-fg-secondary hover:bg-hover hover:text-fg";
}

export function statusLabel(status: SessionStatus): string {
  switch (status) {
    case "AwaitingApproval":
      return "Needs approval";
    case "Working":
      return "Working";
    case "Compacting":
      return "Compacting";
    case "Starting":
      return "Starting";
    case "Idle":
      return "Idle";
    case "Exited":
      return "Exited";
    case "Failed":
      return "Failed";
  }
}
