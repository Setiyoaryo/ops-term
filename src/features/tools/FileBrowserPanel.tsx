import { useState } from "react";

import {
  sftpList,
  sftpMkdir,
  sftpRemove,
  sftpRename,
  transferStart,
} from "../../lib/ipc";
import { useLibrary } from "../../state/library";
import type { SavedSession, SftpEntry } from "../../types";

interface Props {
  onOpenSftp: (savedId: string) => void;
}

function parentOf(path: string): string {
  if (path === "/" || path === "") return "/";
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx <= 0 ? "/" : trimmed.slice(0, idx);
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function FileBrowserPanel({ onOpenSftp }: Props) {
  const sessions = useLibrary((s) => s.sessions);
  const [savedId, setSavedId] = useState("");
  const [path, setPath] = useState("/");
  const [entries, setEntries] = useState<SftpEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const sshSessions = sessions.filter((s) => s.protocol === "ssh");
  const current: SavedSession | undefined = sshSessions.find((s) => s.id === savedId);

  const browse = async (target: string) => {
    if (!savedId) {
      setError("Select an SSH session first.");
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const list = await sftpList(savedId, target);
      setEntries(list);
      setPath(target);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const transfer = async (entry: SftpEntry) => {
    if (!current) return;
    const localPath = window.prompt("Local path", `/tmp/${entry.name}`);
    if (!localPath) return;
    await transferStart({
      spec: {
        host: current.host,
        port: current.port,
        username: current.username,
        identityFile: current.identityFile,
        proxyJump: current.proxyJump,
      },
      direction: "download",
      localPath,
      remotePath: entry.path,
    }).catch((e) => setError(String(e)));
  };

  const upload = async () => {
    if (!current) return;
    const localPath = window.prompt("Local file path to upload");
    if (!localPath) return;
    const remotePath = window.prompt("Remote destination path", `${path}/${localPath.split("/").pop()}`);
    if (!remotePath) return;
    await transferStart({
      spec: {
        host: current.host,
        port: current.port,
        username: current.username,
        identityFile: current.identityFile,
        proxyJump: current.proxyJump,
      },
      direction: "upload",
      localPath,
      remotePath,
    }).catch((e) => setError(String(e)));
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex flex-col gap-2 text-xs">
      <div className="flex items-center gap-2">
        <select
          className={field}
          data-testid="sftp-session"
          value={savedId}
          onChange={(e) => setSavedId(e.target.value)}
        >
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
          Open sftp tab
        </button>
        <button
          type="button"
          disabled={!savedId || busy}
          onClick={() => void browse("/")}
          data-testid="sftp-connect"
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-40"
        >
          Browse
        </button>
      </div>

      {entries && (
        <>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void browse(parentOf(path))}
              className="rounded border border-[#2B323A] px-2 py-0.5 hover:bg-[#1D2228]"
            >
              ↑ Up
            </button>
            <input
              className={`${field} flex-1`}
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void browse(path);
              }}
            />
            <button
              type="button"
              onClick={() => {
                const name = window.prompt("New folder name");
                if (name) void sftpMkdir(savedId, `${path}/${name}`).then(() => browse(path));
              }}
              className="rounded border border-[#2B323A] px-2 py-0.5 hover:bg-[#1D2228]"
            >
              + Folder
            </button>
            <button
              type="button"
              onClick={() => void upload()}
              className="rounded border border-[#2B323A] px-2 py-0.5 hover:bg-[#1D2228]"
            >
              Upload
            </button>
          </div>

          <div className="max-h-64 overflow-y-auto rounded border border-[#2B323A]">
            {entries.length === 0 && <p className="px-2 py-1 text-[#5C6370]">Empty directory.</p>}
            {entries.map((entry) => (
              <div
                key={entry.path}
                data-testid="sftp-entry"
                className="group flex items-center gap-2 px-2 py-0.5 hover:bg-[#1D2228]"
              >
                <span>{entry.isDir ? "📁" : entry.isLink ? "🔗" : "📄"}</span>
                <button
                  type="button"
                  className="min-w-0 flex-1 truncate text-left"
                  onDoubleClick={() => entry.isDir && void browse(entry.path)}
                  onClick={() => entry.isDir && void browse(entry.path)}
                >
                  {entry.name}
                </button>
                <span className="w-20 text-right text-[#5C6370]">
                  {entry.isDir ? "" : formatSize(entry.size)}
                </span>
                {!entry.isDir && (
                  <button
                    type="button"
                    title="Download"
                    className="hidden px-1 hover:text-[#E5E9EF] group-hover:inline"
                    onClick={() => void transfer(entry)}
                  >
                    ⭳
                  </button>
                )}
                <button
                  type="button"
                  title="Rename"
                  className="hidden px-1 hover:text-[#E5E9EF] group-hover:inline"
                  onClick={() => {
                    const next = window.prompt("New name", entry.name);
                    if (next && next !== entry.name) {
                      void sftpRename(savedId, entry.path, `${path}/${next}`).then(() => browse(path));
                    }
                  }}
                >
                  ✎
                </button>
                <button
                  type="button"
                  title="Delete"
                  className="hidden px-1 hover:text-red-400 group-hover:inline"
                  onClick={() => {
                    if (window.confirm(`Delete "${entry.name}"?`)) {
                      void sftpRemove(savedId, entry.path, entry.isDir).then(() => browse(path));
                    }
                  }}
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        </>
      )}
      {error && <p className="text-red-400">{error}</p>}
    </div>
  );
}
