import type { SessionTab } from "../../types";
import { useSessions } from "../../state/sessions";

const STATE_LABEL: Record<SessionTab["state"], string> = {
  connecting: "connecting",
  connected: "connected",
  reconnecting: "reconnecting",
  disconnected: "disconnected",
  failed: "failed",
  closed: "closed",
};

const STATE_COLOR: Record<SessionTab["state"], string> = {
  connecting: "bg-amber-400",
  connected: "bg-emerald-400",
  reconnecting: "bg-amber-400",
  disconnected: "bg-zinc-500",
  failed: "bg-red-500",
  closed: "bg-zinc-600",
};

interface Props {
  onClose: (id: string) => void;
}

export function TabBar({ onClose }: Props) {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);
  const setActive = useSessions((s) => s.setActive);
  const moveTab = useSessions((s) => s.moveTab);

  return (
    <div
      role="tablist"
      className="flex items-stretch overflow-x-auto bg-[#181C20] border-b border-[#2B323A]"
    >
      {tabs.map((tab, index) => {
        const active = tab.id === activeId;
        return (
          <div
            key={tab.id}
            role="tab"
            aria-selected={active}
            tabIndex={0}
            data-testid="tab"
            data-tab-title={tab.title}
            data-tab-state={tab.state}
            draggable
            onDragStart={(e) => e.dataTransfer.setData("application/x-opsterm-tab", String(index))}
            onDragOver={(e) => {
              if (Array.from(e.dataTransfer.types).includes("application/x-opsterm-tab")) {
                e.preventDefault();
              }
            }}
            onDrop={(e) => {
              const from = Number(e.dataTransfer.getData("application/x-opsterm-tab"));
              if (Number.isFinite(from)) {
                e.preventDefault();
                moveTab(from, index);
              }
            }}
            onClick={() => setActive(tab.id)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                setActive(tab.id);
              }
            }}
            className={`group flex cursor-pointer items-center gap-2 px-3 py-1.5 text-xs whitespace-nowrap border-r border-[#2B323A] ${
              active
                ? "bg-[#111418] text-[#E5E9EF]"
                : "bg-[#15181C] text-[#99A3AE] hover:bg-[#1D2228]"
            }`}
          >
            <span
              className={`inline-block h-2 w-2 rounded-full ${STATE_COLOR[tab.state]}`}
              aria-label={STATE_LABEL[tab.state]}
            />
            <span className="max-w-[180px] truncate">{tab.title}</span>
            <button
              type="button"
              aria-label={`Close ${tab.title}`}
              data-testid="tab-close"
              onClick={(event) => {
                event.stopPropagation();
                onClose(tab.id);
              }}
              className="rounded px-1 text-[#99A3AE] opacity-0 hover:bg-[#2B323A] hover:text-[#E5E9EF] group-hover:opacity-100"
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
