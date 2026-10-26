import { useMemo, useRef, useState } from "react";

import { useLibrary } from "../../state/library";
import type { Protocol, SavedSession, SavedSessionInput } from "../../types";
import { detectDelimiter, downloadText, parseDelimited, toDelimited } from "./csv";

const COLUMNS = [
  "name",
  "host",
  "port",
  "protocol",
  "username",
  "identity_file",
  "proxy_jump",
  "tags",
  "description",
] as const;

/** Parse a SecureCRT session `.ini` (metadata only, no secrets). */
export function parseSecureCrt(text: string, fallbackName: string): SavedSessionInput | null {
  const map: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^[SDC]:"(.*?)"=(.*)$/);
    if (m) map[m[1]] = m[2];
  }
  const host = map["Hostname"];
  if (!host) return null;
  const protoName = (map["Protocol Name"] ?? "SSH2").toLowerCase();
  const protocol: Protocol = protoName.includes("telnet") ? "telnet" : "ssh";
  let port = protocol === "telnet" ? 23 : 22;
  const portRaw =
    map["[SSH2] Port"] ?? map["[Telnet] Port"] ?? map["[SSH1] Port"] ?? "";
  if (/^[0-9A-Fa-f]{8}$/.test(portRaw)) port = parseInt(portRaw, 16);
  else if (/^\d+$/.test(portRaw)) port = Number(portRaw);
  return {
    name: fallbackName,
    host,
    port: Number.isFinite(port) && port > 0 ? port : protocol === "telnet" ? 23 : 22,
    protocol,
    username: map["Username"]?.trim() || null,
    description: map["Description"]?.trim() || null,
  };
}

function folderPath(session: SavedSession, folders: { id: string; parentId: string | null; name: string }[]): string {
  const parts: string[] = [];
  let current = folders.find((f) => f.id === session.folderId);
  let guard = 0;
  while (current && guard < 32) {
    parts.unshift(current.name);
    current = folders.find((f) => f.id === current?.parentId);
    guard += 1;
  }
  return parts.join("/");
}

export function ImportExportPanel() {
  const sessions = useLibrary((s) => s.sessions);
  const folders = useLibrary((s) => s.folders);
  const saveSession = useLibrary((s) => s.saveSession);

  const fileRef = useRef<HTMLInputElement>(null);
  const crtRef = useRef<HTMLInputElement>(null);
  const [preview, setPreview] = useState<SavedSessionInput[] | null>(null);
  const [crtPreview, setCrtPreview] = useState<{ input: SavedSessionInput; folders: string[] }[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const rows = useMemo(
    () =>
      sessions.map((s) => [
        s.name,
        s.host,
        String(s.port),
        s.protocol,
        s.username ?? "",
        s.identityFile ?? "",
        s.proxyJump ?? "",
        s.tags.join("; "),
        s.description ?? "",
      ]),
    [sessions],
  );

  const exportCsv = () =>
    downloadText("opsterm-sessions.csv", toDelimited([[...COLUMNS], ...rows], ","), "text/csv");
  const exportTsv = () =>
    downloadText("opsterm-sessions.tsv", toDelimited([[...COLUMNS], ...rows], "\t"), "text/tab-separated-values");

  const exportJson = () => {
    const payload = {
      schemaVersion: 1,
      sessions: sessions.map((s) => ({
        id: s.id,
        name: s.name,
        protocol: s.protocol,
        host: s.host,
        port: s.port,
        username: s.username,
        folderPath: folderPath(s, folders).split("/").filter(Boolean),
        tags: s.tags,
        sshAlias: s.sshAlias,
        identityFile: s.identityFile,
      })),
    };
    downloadText("opsterm-sessions.json", JSON.stringify(payload, null, 2), "application/json");
  };

  const onFile = async (file: File) => {
    setError(null);
    setNote(null);
    setPreview(null);
    const text = await file.text();
    const delimiter = detectDelimiter(text);
    const table = parseDelimited(text, delimiter);
    if (table.length === 0) {
      setError("Empty file.");
      return;
    }
    const header = table[0].map((h) => h.trim().toLowerCase());
    const idx = (name: string) => header.indexOf(name);
    const nameIdx = idx("name");
    const hostIdx = idx("host");
    if (nameIdx === -1 || hostIdx === -1) {
      setError("Header must include at least 'name' and 'host'.");
      return;
    }
    const items: SavedSessionInput[] = [];
    for (const row of table.slice(1)) {
      const name = row[nameIdx]?.trim() ?? "";
      const host = row[hostIdx]?.trim() ?? "";
      if (!name || !host) continue;
      const protocol = (idx("protocol") >= 0 ? row[idx("protocol")]?.trim() : "ssh") || "ssh";
      items.push({
        name,
        host,
        port: Number(idx("port") >= 0 ? row[idx("port")] || 22 : 22) || 22,
        protocol: protocol as Protocol,
        username: idx("username") >= 0 ? row[idx("username")]?.trim() || null : null,
        identityFile: idx("identity_file") >= 0 ? row[idx("identity_file")]?.trim() || null : null,
        proxyJump: idx("proxy_jump") >= 0 ? row[idx("proxy_jump")]?.trim() || null : null,
        tags:
          idx("tags") >= 0
            ? (row[idx("tags")] ?? "")
                .split(/[;,]/)
                .map((t) => t.trim())
                .filter(Boolean)
            : [],
        description: idx("description") >= 0 ? row[idx("description")]?.trim() || null : null,
      });
    }
    if (items.length === 0) {
      setError("No valid rows found.");
      return;
    }
    setPreview(items);
  };

  const onCrtFiles = async (files: FileList) => {
    setError(null);
    setNote(null);
    setCrtPreview(null);
    const items: { input: SavedSessionInput; folders: string[] }[] = [];
    for (const file of Array.from(files)) {
      if (!file.name.toLowerCase().endsWith(".ini")) continue;
      const text = await file.text();
      const input = parseSecureCrt(text, file.name.replace(/\.ini$/i, ""));
      if (!input) continue;
      const rel = (file as File & { webkitRelativePath?: string }).webkitRelativePath ?? "";
      let folders: string[] = [];
      if (rel) {
        const parts = rel.split("/");
        parts.pop();
        folders = parts.filter((p) => p && p.toLowerCase() !== "sessions");
      }
      items.push({ input, folders });
    }
    if (items.length === 0) {
      setError("No SecureCRT sessions found in the selection.");
      return;
    }
    setCrtPreview(items);
  };

  const ensureFolder = async (parts: string[]): Promise<string | null> => {
    let parent: string | null = null;
    for (const part of parts) {
      const existing = useLibrary
        .getState()
        .folders.find((f) => f.parentId === parent && f.name === part);
      parent = existing
        ? existing.id
        : (await useLibrary.getState().saveFolder({ parentId: parent, name: part })).id;
    }
    return parent;
  };

  const confirmCrtImport = async () => {
    if (!crtPreview) return;
    let ok = 0;
    let failed = 0;
    for (const item of crtPreview) {
      try {
        const folderId = item.folders.length ? await ensureFolder(item.folders) : null;
        await useLibrary.getState().saveSession({ ...item.input, folderId });
        ok += 1;
      } catch {
        failed += 1;
      }
    }
    setNote(`Imported ${ok} SecureCRT session(s), ${failed} skipped.`);
    setCrtPreview(null);
  };

  const confirmImport = async () => {
    if (!preview) return;
    let ok = 0;
    let failed = 0;
    for (const item of preview) {
      try {
        await saveSession(item);
        ok += 1;
      } catch {
        failed += 1;
      }
    }
    setNote(`Imported ${ok} session(s), ${failed} skipped.`);
    setPreview(null);
  };

  return (
    <div className="flex flex-col gap-2 text-xs">
      <p className="text-[#99A3AE]">
        Exports contain metadata only, never secrets. Imports are validated by
        the backend before anything is stored.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" onClick={exportCsv} className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]">
          Export CSV
        </button>
        <button type="button" onClick={exportTsv} className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]">
          Export TSV
        </button>
        <button type="button" onClick={exportJson} className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]">
          Export JSON
        </button>
        <button
          type="button"
          onClick={() => fileRef.current?.click()}
          className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
        >
          Import CSV/TSV…
        </button>
        <button
          type="button"
          onClick={() => crtRef.current?.click()}
          data-testid="import-securecrt"
          className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
        >
          Import SecureCRT folder…
        </button>
        <input
          ref={fileRef}
          type="file"
          accept=".csv,.tsv,text/csv,text/tab-separated-values"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void onFile(file);
            e.target.value = "";
          }}
        />
        <input
          ref={crtRef}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => {
            if (e.target.files) void onCrtFiles(e.target.files);
            e.target.value = "";
          }}
          {...({ webkitdirectory: "true", directory: "true" } as Record<string, string>)}
        />
      </div>
      {crtPreview && (
        <div className="flex flex-col gap-1">
          <p className="text-[#99A3AE]">
            {crtPreview.length} SecureCRT session(s) — password/credentials are
            never imported:
          </p>
          <div className="max-h-40 overflow-y-auto rounded border border-[#2B323A]">
            {crtPreview.map((item, i) => (
              <div key={i} className="flex gap-2 px-2 py-0.5">
                <span className="w-40 truncate">{item.input.name}</span>
                <span className="flex-1 truncate text-[#99A3AE]">{item.input.host}</span>
                <span className="text-[#5C6370]">
                  {item.folders.join("/") || "—"}
                </span>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              data-testid="import-crt-confirm"
              onClick={() => void confirmCrtImport()}
              className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
            >
              Import {crtPreview.length}
            </button>
            <button
              type="button"
              onClick={() => setCrtPreview(null)}
              className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
      {note && <p className="text-emerald-400">{note}</p>}
      {error && <p className="text-red-400">{error}</p>}
      {preview && (
        <div className="flex flex-col gap-1">
          <p className="text-[#99A3AE]">{preview.length} row(s) ready to import:</p>
          <div className="max-h-40 overflow-y-auto rounded border border-[#2B323A]">
            {preview.map((p, i) => (
              <div key={i} className="flex gap-2 px-2 py-0.5">
                <span className="w-40 truncate">{p.name}</span>
                <span className="flex-1 truncate text-[#99A3AE]">{p.host}</span>
                <span className="text-[#5C6370]">{p.protocol}</span>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              data-testid="import-confirm"
              onClick={() => void confirmImport()}
              className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
            >
              Import {preview.length}
            </button>
            <button
              type="button"
              onClick={() => setPreview(null)}
              className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
