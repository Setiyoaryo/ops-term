import { useState } from "react";

import { AutomationPanel } from "./AutomationPanel";
import { CredentialsPanel } from "./CredentialsPanel";
import { FileBrowserPanel } from "./FileBrowserPanel";
import { HighlightPanel } from "./HighlightPanel";
import { ImportExportPanel } from "./ImportExportPanel";
import { RunbooksPanel } from "./RunbooksPanel";
import { LogsPanel } from "./LogsPanel";
import { TransfersPanel } from "./TransfersPanel";
import { TunnelsPanel } from "./TunnelsPanel";

type Tab = "tunnels" | "files" | "transfers" | "automation" | "runbooks" | "logs" | "credentials" | "highlight" | "io";

const TABS: { id: Tab; label: string }[] = [
  { id: "tunnels", label: "Tunnels" },
  { id: "files", label: "Files (SFTP)" },
  { id: "transfers", label: "Transfers" },
  { id: "automation", label: "Automation" },
  { id: "runbooks", label: "Runbooks" },
  { id: "logs", label: "Logs" },
  { id: "credentials", label: "Credentials" },
  { id: "highlight", label: "Highlight" },
  { id: "io", label: "Import/Export" },
];

interface Props {
  onClose: () => void;
  onOpenSftp: (savedId: string) => void;
}

export function ToolsDrawer({ onClose, onOpenSftp }: Props) {
  const [tab, setTab] = useState<Tab>("tunnels");
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50">
      <div
        role="dialog"
        aria-label="Tools"
        data-testid="tools-drawer"
        className="flex h-[560px] w-[820px] max-w-[95vw] overflow-hidden rounded-lg border border-[#2B323A] bg-[#181C20] text-sm text-[#E5E9EF] shadow-xl"
      >
        <nav className="w-40 shrink-0 border-r border-[#2B323A] bg-[#15181C] py-2">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              data-testid={`tools-tab-${t.id}`}
              onClick={() => setTab(t.id)}
              className={`block w-full px-3 py-1.5 text-left text-xs ${
                tab === t.id ? "bg-[#111418] text-[#E5E9EF]" : "text-[#99A3AE] hover:bg-[#1D2228]"
              }`}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <div className="flex flex-1 flex-col">
          <div className="flex items-center border-b border-[#2B323A] px-3 py-1.5">
            <span className="text-xs font-semibold uppercase tracking-wide text-[#99A3AE]">
              {TABS.find((t) => t.id === tab)?.label}
            </span>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close tools"
              data-testid="tools-close"
              className="ml-auto rounded px-1.5 hover:bg-[#2B323A]"
            >
              ✕
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-3">
            {tab === "tunnels" && <TunnelsPanel />}
            {tab === "files" && <FileBrowserPanel onOpenSftp={onOpenSftp} />}
            {tab === "transfers" && <TransfersPanel onOpenSftp={onOpenSftp} />}
            {tab === "automation" && <AutomationPanel />}
            {tab === "runbooks" && <RunbooksPanel />}
            {tab === "logs" && <LogsPanel />}
            {tab === "credentials" && <CredentialsPanel />}
            {tab === "highlight" && <HighlightPanel />}
            {tab === "io" && <ImportExportPanel />}
          </div>
        </div>
      </div>
    </div>
  );
}
