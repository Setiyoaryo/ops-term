import { useMemo, useState } from "react";

import { useLibrary } from "../../state/library";
import type { Folder, SavedSession } from "../../types";

interface Props {
  onConnect: (saved: SavedSession) => void;
  onEdit: (saved: SavedSession) => void;
  onNewSession: (folderId: string | null) => void;
  onNewFolder: (parentId: string | null) => void;
}

/** Case-insensitive match over metadata only (never credentials). */
function matches(saved: SavedSession, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return (
    saved.name.toLowerCase().includes(q) ||
    saved.host.toLowerCase().includes(q) ||
    (saved.username ?? "").toLowerCase().includes(q) ||
    saved.tags.some((t) => t.toLowerCase().includes(q))
  );
}

export function SavedSessionsPanel({
  onConnect,
  onEdit,
  onNewSession,
  onNewFolder,
}: Props) {
  const folders = useLibrary((s) => s.folders);
  const sessions = useLibrary((s) => s.sessions);
  const deleteSession = useLibrary((s) => s.deleteSession);
  const deleteFolder = useLibrary((s) => s.deleteFolder);
  const saveSession = useLibrary((s) => s.saveSession);
  const saveFolder = useLibrary((s) => s.saveFolder);

  const [filter, setFilter] = useState("");
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  const SESSION_MIME = "application/x-opsterm-session";
  const FOLDER_MIME = "application/x-opsterm-folder";

  /** True if `candidate` is `folderId` or one of its descendants. */
  const isDescendant = (candidate: string, folderId: string): boolean => {
    let current = candidate;
    let guard = 0;
    while (current && guard < 64) {
      if (current === folderId) return true;
      current = folders.find((f) => f.id === current)?.parentId ?? "";
      guard += 1;
    }
    return false;
  };

  const moveSessionTo = async (sessionId: string, folderId: string | null) => {
    const saved = sessions.find((s) => s.id === sessionId);
    if (!saved) return;
    await saveSession({
      id: saved.id,
      folderId,
      name: saved.name,
      host: saved.host,
      port: saved.port,
      protocol: saved.protocol,
      username: saved.username,
      sshAlias: saved.sshAlias,
      identityFile: saved.identityFile,
      proxyJump: saved.proxyJump,
      tags: saved.tags,
      description: saved.description,
      notes: saved.notes,
      settingsOverride: saved.settingsOverride,
      credentialRef: saved.credentialRef,
      isFavorite: saved.isFavorite,
      sortOrder: saved.sortOrder,
    });
  };

  const moveFolderTo = async (folderId: string, parentId: string | null) => {
    if (folderId === parentId) return;
    if (parentId && isDescendant(parentId, folderId)) return;
    const folder = folders.find((f) => f.id === folderId);
    if (!folder) return;
    await saveFolder({ id: folder.id, parentId, name: folder.name, sortOrder: folder.sortOrder });
  };

  const filtered = useMemo(
    () => sessions.filter((s) => matches(s, filter)),
    [sessions, filter],
  );

  const toggle = (id: string) =>
    setCollapsed((c) => ({ ...c, [id]: !c[id] }));

  const sessionsIn = (folderId: string | null) =>
    filtered.filter((s) => s.folderId === folderId);

  const renderSession = (saved: SavedSession) => (
    <div
      key={saved.id}
      data-testid="saved-session"
      data-session-name={saved.name}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(SESSION_MIME, saved.id);
        e.dataTransfer.effectAllowed = "move";
      }}
      className="group flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-xs text-[#C7CDD6] hover:bg-[#1D2228]"
      onClick={() => onConnect(saved)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onConnect(saved);
      }}
      role="button"
      tabIndex={0}
    >
      <span className="text-[#5C6370]">{saved.isFavorite ? "★" : "•"}</span>
      <span className="min-w-0 flex-1 truncate">{saved.name}</span>
      <span className="hidden text-[10px] uppercase text-[#5C6370] group-hover:inline">
        {saved.protocol}
      </span>
      <button
        type="button"
        aria-label={`Edit ${saved.name}`}
        className="hidden rounded px-1 text-[#99A3AE] hover:text-[#E5E9EF] group-hover:inline"
        onClick={(e) => {
          e.stopPropagation();
          onEdit(saved);
        }}
      >
        ✎
      </button>
      <button
        type="button"
        aria-label={`Duplicate ${saved.name}`}
        className="hidden rounded px-1 text-[#99A3AE] hover:text-[#E5E9EF] group-hover:inline"
        onClick={(e) => {
          e.stopPropagation();
          void saveSession({
            folderId: saved.folderId,
            name: `${saved.name} copy`,
            host: saved.host,
            port: saved.port,
            protocol: saved.protocol,
            username: saved.username,
            sshAlias: saved.sshAlias,
            identityFile: saved.identityFile,
            proxyJump: saved.proxyJump,
            tags: saved.tags,
            description: saved.description,
            notes: saved.notes,
            settingsOverride: saved.settingsOverride,
            credentialRef: saved.credentialRef,
            isFavorite: false,
          });
        }}
      >
        ⧉
      </button>
      <button
        type="button"
        aria-label={`Delete ${saved.name}`}
        className="hidden rounded px-1 text-[#99A3AE] hover:text-red-400 group-hover:inline"
        onClick={(e) => {
          e.stopPropagation();
          if (window.confirm(`Delete saved session "${saved.name}"?`)) {
            void deleteSession(saved.id);
          }
        }}
      >
        ✕
      </button>
    </div>
  );

  const renderFolder = (folder: Folder, depth: number): JSX.Element => {
    const childFolders = folders.filter((f) => f.parentId === folder.id);
    const isCollapsed = collapsed[folder.id];
    return (
      <div key={folder.id}>
        <div
          className={`group flex cursor-pointer items-center gap-1 rounded px-2 py-1 text-xs text-[#99A3AE] hover:bg-[#1D2228] ${
            dropTarget === folder.id ? "ring-1 ring-[#4F8EE8]" : ""
          }`}
          style={{ paddingLeft: 8 + depth * 12 }}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData(FOLDER_MIME, folder.id);
            e.dataTransfer.effectAllowed = "move";
          }}
          onDragOver={(e) => {
            const types = Array.from(e.dataTransfer.types);
            if (types.includes(SESSION_MIME) || types.includes(FOLDER_MIME)) {
              e.preventDefault();
              setDropTarget(folder.id);
            }
          }}
          onDragLeave={() => setDropTarget((t) => (t === folder.id ? null : t))}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setDropTarget(null);
            const sid = e.dataTransfer.getData(SESSION_MIME);
            const fid = e.dataTransfer.getData(FOLDER_MIME);
            if (sid) void moveSessionTo(sid, folder.id);
            else if (fid) void moveFolderTo(fid, folder.id);
          }}
          onClick={() => toggle(folder.id)}
        >
          <span>{isCollapsed ? "▸" : "▾"}</span>
          <span className="min-w-0 flex-1 truncate font-medium">{folder.name}</span>
          <button
            type="button"
            aria-label={`New session in ${folder.name}`}
            className="hidden px-1 hover:text-[#E5E9EF] group-hover:inline"
            onClick={(e) => {
              e.stopPropagation();
              onNewSession(folder.id);
            }}
          >
            +
          </button>
          <button
            type="button"
            aria-label={`Delete folder ${folder.name}`}
            className="hidden px-1 hover:text-red-400 group-hover:inline"
            onClick={(e) => {
              e.stopPropagation();
              if (
                window.confirm(
                  `Delete folder "${folder.name}"? Sessions inside are kept.`,
                )
              ) {
                void deleteFolder(folder.id);
              }
            }}
          >
            ✕
          </button>
        </div>
        {!isCollapsed && (
          <div>
            {childFolders.map((f) => renderFolder(f, depth + 1))}
            <div style={{ paddingLeft: depth * 12 }}>
              {sessionsIn(folder.id).map(renderSession)}
            </div>
          </div>
        )}
      </div>
    );
  };

  const rootFolders = folders.filter((f) => f.parentId === null);
  const rootSessions = sessionsIn(null);

  return (
    <div
      data-testid="session-sidebar"
      className="flex h-full min-h-0 flex-col"
    >
      <div className="flex items-center gap-1 border-b border-[#2B323A] px-2 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-[#99A3AE]">
          Saved Sessions
        </span>
        <div className="ml-auto flex gap-1">
          <button
            type="button"
            title="New session"
            aria-label="New session"
            data-testid="new-session"
            onClick={() => onNewSession(null)}
            className="rounded px-1.5 text-[#99A3AE] hover:bg-[#2B323A] hover:text-[#E5E9EF]"
          >
            +
          </button>
          <button
            type="button"
            title="New folder"
            aria-label="New folder"
            data-testid="new-folder"
            onClick={() => onNewFolder(null)}
            className="rounded px-1.5 text-[#99A3AE] hover:bg-[#2B323A] hover:text-[#E5E9EF]"
          >
            ▤
          </button>
        </div>
      </div>
      <div className="border-b border-[#2B323A] px-2 py-1.5">
        <input
          aria-label="Filter sessions"
          data-testid="session-filter"
          placeholder="Filter..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="w-full rounded border border-[#2B323A] bg-[#111418] px-2 py-1 text-xs text-[#E5E9EF] outline-none placeholder:text-[#5C6370] focus:border-[#4F8EE8]"
        />
      </div>
      <div
        className="flex-1 overflow-y-auto py-1"
        onDragOver={(e) => {
          const types = Array.from(e.dataTransfer.types);
          if (types.includes(SESSION_MIME) || types.includes(FOLDER_MIME)) {
            e.preventDefault();
            setDropTarget("__root__");
          }
        }}
        onDragLeave={() => setDropTarget((t) => (t === "__root__" ? null : t))}
        onDrop={(e) => {
          e.preventDefault();
          setDropTarget(null);
          const sid = e.dataTransfer.getData(SESSION_MIME);
          const fid = e.dataTransfer.getData(FOLDER_MIME);
          if (sid) void moveSessionTo(sid, null);
          else if (fid) void moveFolderTo(fid, null);
        }}
      >
        {folders.length === 0 && sessions.length === 0 && (
          <p className="px-3 py-2 text-xs text-[#5C6370]">
            No saved sessions yet.
          </p>
        )}
        {rootFolders.map((f) => renderFolder(f, 0))}
        <div>{rootSessions.map(renderSession)}</div>
      </div>
    </div>
  );
}
