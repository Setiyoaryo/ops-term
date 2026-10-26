import { useSessions } from "../../state/sessions";

interface Props {
  onToggleLog: () => void;
  onSerialBreak: () => void;
}

export function StatusBar({ onToggleLog, onSerialBreak }: Props) {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);
  const active = tabs.find((t) => t.id === activeId);

  return (
    <div
      data-testid="status"
      className="flex items-center gap-4 border-t border-[#2B323A] bg-[#181C20] px-3 py-1 text-xs text-[#99A3AE]"
    >
      {active ? (
        <>
          <span className="flex items-center gap-1.5">
            <span
              className={`inline-block h-2 w-2 rounded-full ${
                active.state === "connected"
                  ? "bg-emerald-400"
                  : active.state === "connecting" || active.state === "reconnecting"
                    ? "bg-amber-400"
                    : "bg-zinc-500"
              }`}
            />
            {active.state}
          </span>
          <span className="text-[#E5E9EF]">{active.title}</span>
          <span>{active.protocol.toUpperCase()}</span>
          {active.protocol === "telnet" && (
            <span className="rounded bg-amber-600/30 px-1 text-amber-300">
              INSECURE
            </span>
          )}
          {active.host && <span>{active.host}</span>}
          {active.protocol === "serial" && (
            <button
              type="button"
              onClick={onSerialBreak}
              data-testid="serial-break"
              title="Send serial break"
              className="rounded border border-[#2B323A] px-1.5 py-0.5 hover:bg-[#1D2228]"
            >
              Break
            </button>
          )}
          <button
            type="button"
            onClick={onToggleLog}
            data-testid="log-toggle"
            data-logging={active.logging ? "on" : "off"}
            title={active.logPath ?? "Toggle terminal logging"}
            className={`rounded border px-1.5 py-0.5 ${
              active.logging
                ? "border-red-500/60 text-red-400"
                : "border-[#2B323A] text-[#99A3AE]"
            } hover:bg-[#1D2228]`}
          >
            {active.logging ? "● LOG" : "○ LOG"}
          </button>
        </>
      ) : (
        <span>No active session</span>
      )}
      <span className="ml-auto">
        {tabs.length} tab{tabs.length === 1 ? "" : "s"}
      </span>
    </div>
  );
}
