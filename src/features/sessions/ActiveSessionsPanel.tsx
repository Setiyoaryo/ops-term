import { useState } from "react";

import { useSessions } from "../../state/sessions";

interface Props {
  onFocus: (id: string) => void;
  onClose: (id: string) => void;
  onReconnect: (id: string) => void;
}

export function ActiveSessionsPanel({ onFocus, onClose, onReconnect }: Props) {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div data-testid="active-sessions" className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-1 border-b border-[#2B323A] px-2 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-[#99A3AE]">
          Active Sessions
        </span>
        <span className="ml-auto text-[10px] text-[#5C6370]">{tabs.length}</span>
      </div>
      <div className="flex gap-1 border-b border-[#2B323A] px-2 py-1">
        <button
          type="button"
          disabled={selected.size === 0}
          onClick={() => {
            if (
              window.confirm(`Disconnect ${selected.size} session(s)?`)
            ) {
              selected.forEach((id) => onClose(id));
              setSelected(new Set());
            }
          }}
          className="rounded border border-[#2B323A] px-2 py-0.5 text-[11px] text-[#C7CDD6] hover:bg-[#1D2228] disabled:opacity-40"
        >
          Disconnect selected
        </button>
        <button
          type="button"
          disabled={selected.size === 0}
          onClick={() => {
            selected.forEach((id) => onReconnect(id));
          }}
          className="rounded border border-[#2B323A] px-2 py-0.5 text-[11px] text-[#C7CDD6] hover:bg-[#1D2228] disabled:opacity-40"
        >
          Reconnect selected
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {tabs.length === 0 && (
          <p className="px-3 py-2 text-xs text-[#5C6370]">No live sessions.</p>
        )}
        {tabs.map((tab) => (
          <div
            key={tab.id}
            data-testid="active-session"
            className={`flex items-center gap-2 px-2 py-1 text-xs ${
              tab.id === activeId ? "bg-[#1D2228]" : ""
            }`}
          >
            <input
              type="checkbox"
              checked={selected.has(tab.id)}
              onChange={() => toggle(tab.id)}
              aria-label={`Select ${tab.title}`}
            />
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left hover:text-white"
              onClick={() => onFocus(tab.id)}
            >
              <span
                className={`mr-1.5 inline-block h-2 w-2 rounded-full ${
                  tab.state === "connected"
                    ? "bg-emerald-400"
                    : tab.state === "connecting" || tab.state === "reconnecting"
                      ? "bg-amber-400"
                      : "bg-zinc-500"
                }`}
              />
              {tab.title}
            </button>
            <span className="text-[10px] uppercase text-[#5C6370]">
              {tab.protocol}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
