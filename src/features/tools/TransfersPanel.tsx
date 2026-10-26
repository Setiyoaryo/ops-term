import { useEffect, useState } from "react";

import { transferStart, transfersList } from "../../lib/ipc";
import { useLibrary } from "../../state/library";
import type { TransferView } from "../../types";

interface Props {
  onOpenSftp: (savedId: string) => void;
}

export function TransfersPanel({ onOpenSftp }: Props) {
  const sessions = useLibrary((s) => s.sessions);
  const [jobs, setJobs] = useState<TransferView[]>([]);
  const [savedId, setSavedId] = useState("");
  const [direction, setDirection] = useState<"upload" | "download">("upload");
  const [localPath, setLocalPath] = useState("");
  const [remotePath, setRemotePath] = useState("");
  const [recursive, setRecursive] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => void transfersList().then(setJobs).catch(() => {});
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => clearInterval(timer);
  }, []);

  const sshSessions = sessions.filter((s) => s.protocol === "ssh");

  const start = async () => {
    setError(null);
    const saved = sshSessions.find((s) => s.id === savedId);
    if (!saved) {
      setError("Select an SSH session.");
      return;
    }
    if (!localPath || !remotePath) {
      setError("Both local and remote paths are required.");
      return;
    }
    try {
      await transferStart({
        spec: {
          host: saved.host,
          port: saved.port,
          username: saved.username,
          identityFile: saved.identityFile,
          proxyJump: saved.proxyJump,
        },
        direction,
        localPath,
        remotePath,
        recursive,
      });
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex flex-col gap-2 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <select className={field} value={savedId} onChange={(e) => setSavedId(e.target.value)}>
          <option value="">(SSH session)</option>
          {sshSessions.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          disabled={!savedId}
          onClick={() => onOpenSftp(savedId)}
          className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228] disabled:opacity-40"
        >
          Open interactive SFTP tab
        </button>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <select
          className={field}
          value={direction}
          onChange={(e) => setDirection(e.target.value as typeof direction)}
        >
          <option value="upload">Upload</option>
          <option value="download">Download</option>
        </select>
        <input
          className={`${field} flex-1`}
          placeholder="local path"
          value={localPath}
          onChange={(e) => setLocalPath(e.target.value)}
        />
        <input
          className={`${field} flex-1`}
          placeholder="remote path"
          value={remotePath}
          onChange={(e) => setRemotePath(e.target.value)}
        />
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={recursive}
            onChange={(e) => setRecursive(e.target.checked)}
          />
          recursive
        </label>
        <button
          type="button"
          onClick={() => void start()}
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          Start transfer
        </button>
      </div>
      {error && <p className="text-red-400">{error}</p>}
      <div className="flex flex-col gap-1">
        {jobs.length === 0 && <p className="text-[#5C6370]">No transfers yet.</p>}
        {jobs.map((j) => (
          <div
            key={j.id}
            className="flex items-center gap-2 rounded border border-[#2B323A] px-2 py-1"
          >
            <span
              className={
                j.status === "done"
                  ? "text-emerald-400"
                  : j.status === "failed"
                    ? "text-red-400"
                    : "text-amber-400"
              }
            >
              {j.status}
            </span>
            <span className="uppercase text-[#99A3AE]">{j.direction}</span>
            <span className="truncate">
              {j.localPath} ⇄ {j.remotePath}
            </span>
            {j.message && (
              <span className="truncate text-red-400">{j.message}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
