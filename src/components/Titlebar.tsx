import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, Copy, X } from "lucide-react";
import logo from "@/assets/logo.png";
import { cn } from "@/lib/cn";

const appWindow = getCurrentWindow();

function ControlButton({
  onClick,
  danger,
  children,
  label,
}: {
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
  label: string;
}) {
  return (
    <button
      aria-label={label}
      onClick={onClick}
      className={cn(
        "inline-flex h-9 w-11 items-center justify-center text-fg-secondary transition-colors",
        danger
          ? "hover:bg-danger hover:text-white"
          : "hover:bg-hover hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

export function Titlebar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    appWindow.isMaximized().then(setMaximized);
    appWindow
      .onResized(() => {
        appWindow.isMaximized().then(setMaximized);
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  return (
    <header
      data-tauri-drag-region
      className="relative z-50 flex h-9 shrink-0 items-center border-b border-border-subtle bg-bg"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 pl-3">
        <img
          data-tauri-drag-region
          src={logo}
          alt=""
          className="pointer-events-none h-4 w-4"
        />
        <span
          data-tauri-drag-region
          className="text-[13px] font-medium tracking-tight text-fg-secondary"
        >
          Lighter
        </span>
      </div>

      <div data-tauri-drag-region className="flex-1" />

      <div className="flex">
        <ControlButton label="Minimize" onClick={() => appWindow.minimize()}>
          <Minus size={15} strokeWidth={1.5} />
        </ControlButton>
        <ControlButton
          label={maximized ? "Restore" : "Maximize"}
          onClick={() => appWindow.toggleMaximize()}
        >
          {maximized ? (
            <Copy size={13} strokeWidth={1.5} className="-scale-x-100" />
          ) : (
            <Square size={13} strokeWidth={1.5} />
          )}
        </ControlButton>
        <ControlButton label="Close" danger onClick={() => appWindow.close()}>
          <X size={16} strokeWidth={1.5} />
        </ControlButton>
      </div>
    </header>
  );
}
