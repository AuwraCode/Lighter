// Ctrl+K palette: app actions + the focused session's slash commands
// (names, descriptions and argument hints come from the initialize handshake).

import { Command } from "cmdk";
import { useStore } from "zustand";
import {
  CircleStop,
  LayoutGrid,
  OctagonX,
  Plus,
  SlashSquare,
  UserRound,
} from "lucide-react";
import { openProfilesDialog } from "@/stores/profiles";
import * as ipc from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { statusColor } from "@/lib/status";
import {
  focusSession,
  insertIntoComposer,
  openNewSession,
  openPalette,
  useRegistry,
} from "@/stores/registry";
import { getOrCreateSessionStore } from "@/stores/session";

interface SlashCommand {
  name: string;
  description?: string;
  argumentHint?: string;
  aliases?: string[];
}

export function CommandPalette() {
  const open = useRegistry((s) => s.paletteOpen);
  const focusedId = useRegistry((s) => s.focusedId);
  const order = useRegistry((s) => s.order);
  const sessions = useRegistry((s) => s.sessions);

  if (!open) return null;

  const close = () => openPalette(false);
  const run = (fn: () => void) => {
    close();
    fn();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-[15vh]"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <Command
        label="Command palette"
        className="w-[560px] overflow-hidden rounded-xl border border-border bg-elevated shadow-2xl"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      >
        <Command.Input
          autoFocus
          placeholder="Type a command or search…"
          className="w-full border-b border-border-subtle bg-transparent px-4 py-3 text-sm outline-none placeholder:text-fg-muted"
        />
        <Command.List className="max-h-[50vh] overflow-y-auto p-1.5">
          <Command.Empty className="px-3 py-6 text-center text-xs text-fg-muted">
            No results.
          </Command.Empty>

          <Command.Group heading="Actions" className="palette-group">
            <PaletteItem
              icon={<Plus size={14} />}
              label="New session"
              shortcut="Ctrl+N"
              onSelect={() => run(() => openNewSession(true))}
            />
            <PaletteItem
              icon={<LayoutGrid size={14} />}
              label="Go to dashboard"
              shortcut="Ctrl+D"
              onSelect={() => run(() => focusSession(null))}
            />
            <PaletteItem
              icon={<UserRound size={14} />}
              label="Manage accounts"
              onSelect={() => run(() => openProfilesDialog(true))}
            />
            {focusedId && (
              <>
                <PaletteItem
                  icon={<OctagonX size={14} />}
                  label="Interrupt current session"
                  shortcut="Esc"
                  onSelect={() =>
                    run(() => void ipc.interruptSession(focusedId).catch(() => {}))
                  }
                />
                <PaletteItem
                  icon={<CircleStop size={14} />}
                  label="Stop current session"
                  onSelect={() =>
                    run(() => void ipc.stopSession(focusedId).catch(() => {}))
                  }
                />
              </>
            )}
          </Command.Group>

          {order.length > 0 && (
            <Command.Group heading="Sessions" className="palette-group">
              {order.map((id) => (
                <PaletteItem
                  key={id}
                  icon={
                    <span
                      className={cn(
                        "mx-0.5 h-2 w-2 rounded-full",
                        statusColor(sessions[id]?.status ?? "Starting"),
                      )}
                    />
                  }
                  label={sessions[id]?.title ?? id}
                  value={`session ${sessions[id]?.title ?? ""} ${id}`}
                  onSelect={() => run(() => focusSession(id))}
                />
              ))}
            </Command.Group>
          )}

          {focusedId && <SlashCommands sessionId={focusedId} onDone={close} />}
        </Command.List>
      </Command>
    </div>
  );
}

function SlashCommands({
  sessionId,
  onDone,
}: {
  sessionId: string;
  onDone: () => void;
}) {
  const store = getOrCreateSessionStore(sessionId);
  const handshake = useStore(store, (s) => s.handshake);
  const commands: SlashCommand[] = Array.isArray(handshake?.commands)
    ? (handshake!.commands as unknown as SlashCommand[])
    : [];
  if (commands.length === 0) return null;
  return (
    <Command.Group heading="Slash commands" className="palette-group">
      {commands.map((c) => (
        <PaletteItem
          key={c.name}
          icon={<SlashSquare size={14} />}
          label={`/${c.name}`}
          hint={c.argumentHint || undefined}
          description={c.description}
          value={`/${c.name} ${c.description ?? ""}`}
          onSelect={() => {
            onDone();
            insertIntoComposer(
              sessionId,
              `/${c.name}${c.argumentHint ? " " : ""}`,
            );
          }}
        />
      ))}
    </Command.Group>
  );
}

function PaletteItem({
  icon,
  label,
  description,
  hint,
  shortcut,
  value,
  onSelect,
}: {
  icon: React.ReactNode;
  label: string;
  description?: string;
  hint?: string;
  shortcut?: string;
  value?: string;
  onSelect: () => void;
}) {
  return (
    <Command.Item
      value={value ?? label}
      onSelect={onSelect}
      className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 text-[13px] text-fg-secondary aria-selected:bg-hover aria-selected:text-fg"
    >
      <span className="shrink-0 text-fg-muted">{icon}</span>
      <span className="shrink-0">{label}</span>
      {hint && <span className="shrink-0 font-mono text-[10px] text-fg-muted">{hint}</span>}
      {description && (
        <span className="min-w-0 flex-1 truncate text-xs text-fg-muted">
          {description}
        </span>
      )}
      {shortcut && (
        <kbd className="ml-auto shrink-0 rounded border border-border bg-surface px-1.5 py-0.5 font-mono text-[10px] text-fg-muted">
          {shortcut}
        </kbd>
      )}
    </Command.Item>
  );
}
