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
