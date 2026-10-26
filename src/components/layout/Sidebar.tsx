import { useState } from "react";

import { ActiveSessionsPanel } from "../../features/sessions/ActiveSessionsPanel";
import { CommandManagerPanel } from "../../features/commands/CommandManagerPanel";
import { SavedSessionsPanel } from "./SessionSidebar";
import type { Command, SavedSession } from "../../types";

type Tab = "saved" | "active" | "commands";

interface Props {
  onConnect: (saved: SavedSession) => void;
  onEdit: (saved: SavedSession) => void;
  onNewSession: (folderId: string | null) => void;
  onNewFolder: (parentId: string | null) => void;
  onFocus: (id: string) => void;
  onCloseActive: (id: string) => void;
  onReconnectActive: (id: string) => void;
  onUseCommand: (command: Command, mode: "insert" | "send") => void;
}

const TABS: { id: Tab; label: string }[] = [
  { id: "saved", label: "Saved" },
  { id: "active", label: "Active" },
  { id: "commands", label: "Commands" },
];

export function Sidebar(props: Props) {
  const [tab, setTab] = useState<Tab>("saved");

  return (
    <aside className="relative flex h-full w-72 min-w-60 max-w-96 shrink-0 flex-col border-r border-[#2B323A] bg-[#15181C]">
      <div className="flex items-stretch border-b border-[#2B323A]">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            data-testid={`sidebar-tab-${t.id}`}
            onClick={() => setTab(t.id)}
            className={`flex-1 px-2 py-1.5 text-[11px] font-medium ${
              tab === t.id
                ? "bg-[#111418] text-[#E5E9EF]"
                : "text-[#99A3AE] hover:bg-[#1D2228]"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        {tab === "saved" && (
          <SavedSessionsPanel
            onConnect={props.onConnect}
            onEdit={props.onEdit}
            onNewSession={props.onNewSession}
            onNewFolder={props.onNewFolder}
          />
        )}
        {tab === "active" && (
          <ActiveSessionsPanel
            onFocus={props.onFocus}
            onClose={props.onCloseActive}
            onReconnect={props.onReconnectActive}
          />
        )}
        {tab === "commands" && (
          <CommandManagerPanel onUse={props.onUseCommand} />
        )}
      </div>
    </aside>
  );
}
