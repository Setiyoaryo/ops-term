import { useEffect, useState } from "react";

import { logRead, logsList } from "../../lib/ipc";
import { downloadText } from "./csv";
import type { LogEntry } from "../../types";

export function LogsPanel() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => void logsList().then(setLogs).catch(() => {});
  useEffect(refresh, []);

  const exportLog = async (entry: LogEntry) => {
    setError(null);
    try {
      const text = await logRead(entry.filePath);
      const base = entry.filePath.split("/").pop() ?? "session.log";
      downloadText(base, text, "text/plain");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-2 text-xs">
      <div className="flex items-center gap-2">
        <p className="text-[#99A3AE] flex-1">
          Session logs are opt-in and written with <code>0600</code> permissions.
          Enable "Start logging automatically" per device, or toggle LOG in the
          status bar.
        </p>
        <button
          type="button"
          onClick={refresh}
          className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
        >
          Refresh
        </button>
      </div>
      {error && <p className="text-red-400">{error}</p>}
      <div className="flex flex-col gap-1">
        {logs.length === 0 && <p className="text-[#5C6370]">No logs yet.</p>}
        {logs.map((entry) => (
          <div
            key={entry.id}
            data-testid="log-row"
            className="flex items-center gap-2 rounded border border-[#2B323A] px-2 py-1"
          >
            <span
              className={`inline-block h-2 w-2 rounded-full ${
                entry.endedAt ? "bg-zinc-500" : "bg-red-400"
              }`}
              title={entry.endedAt ? "closed" : "recording"}
            />
            <span className="w-32 truncate">{entry.label ?? "session"}</span>
            <span className="flex-1 truncate text-[#5C6370]">{entry.filePath}</span>
            <span className="text-[#5C6370]">
              {(entry.sizeBytes / 1024).toFixed(1)} KB
            </span>
            <span className="text-[#5C6370]">
              {new Date(entry.startedAt).toLocaleString()}
            </span>
            <button
              type="button"
              onClick={() => void exportLog(entry)}
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228]"
            >
              Save as…
            </button>
            <button
              type="button"
              onClick={() => void navigator.clipboard.writeText(entry.filePath)}
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228]"
            >
              Copy path
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
