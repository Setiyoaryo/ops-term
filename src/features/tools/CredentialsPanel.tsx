import { useEffect, useState } from "react";

import {
  credentialDelete,
  credentialGet,
  credentialSet,
  credentialsList,
} from "../../lib/ipc";
import type { CredentialRecord } from "../../types";

const KINDS = [
  { value: "password", label: "Login password" },
  { value: "passphrase", label: "Key passphrase" },
  { value: "enable", label: "Enable / secret" },
];

export function CredentialsPanel() {
  const [records, setRecords] = useState<CredentialRecord[]>([]);
  const [name, setName] = useState("");
  const [username, setUsername] = useState("");
  const [kind, setKind] = useState("password");
  const [tags, setTags] = useState("");
  const [secret, setSecret] = useState("");
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const refresh = () => void credentialsList().then(setRecords).catch(() => {});
  useEffect(refresh, []);

  const parseTags = (value: string) =>
    value
      .split(/[,\s]+/)
      .map((t) => t.trim())
      .filter(Boolean);

  const save = async () => {
    setError(null);
    setNote(null);
    if (!name.trim() || !secret) {
      setError("Name and secret are required.");
      return;
    }
    try {
      await credentialSet(
        name.trim(),
        secret,
        username.trim() || null,
        kind,
        parseTags(tags),
      );
      setNote("Stored in the OS keyring.");
      setName("");
      setUsername("");
      setTags("");
      setSecret("");
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const reveal = async (n: string) => {
    try {
      const value = await credentialGet(n);
      setRevealed((prev) => ({ ...prev, [n]: value }));
    } catch (e) {
      setError(String(e));
    }
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex flex-col gap-2 text-xs">
      <p className="text-[#99A3AE]">
        Named credentials live only in the OS keyring. Tag them so a device can
        reference one credential instead of storing a password per node. OpsTerm
        never writes secrets to the database, logs or backups.
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <input
          className={field}
          placeholder="name / tag e.g. OLT-JKT"
          data-testid="cred-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <input
          className={field}
          placeholder="username (optional)"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <select className={field} value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
        <input
          className={field}
          placeholder="tags (space/comma)"
          value={tags}
          onChange={(e) => setTags(e.target.value)}
        />
        <input
          className={field}
          type="password"
          placeholder="secret"
          data-testid="cred-secret"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
        />
        <button
          type="button"
          onClick={() => void save()}
          data-testid="cred-save"
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          Store
        </button>
      </div>
      {note && <p className="text-emerald-400">{note}</p>}
      {error && <p className="text-red-400">{error}</p>}
      <div className="flex flex-col gap-1">
        {records.length === 0 && (
          <p className="text-[#5C6370]">No stored credentials.</p>
        )}
        {records.map((r) => (
          <div
            key={r.name}
            data-testid="credential-row"
            className="flex items-center gap-2 rounded border border-[#2B323A] px-2 py-1"
          >
            <span className="w-32 truncate font-medium">{r.name}</span>
            <span className="w-24 truncate text-[#99A3AE]">
              {r.username ?? "—"}
            </span>
            <span className="w-24 text-[#5C6370]">{r.kind}</span>
            <span className="flex-1 truncate text-[#4F8EE8]">
              {r.tags.map((t) => `#${t}`).join(" ")}
            </span>
            {revealed[r.name] !== undefined && (
              <code className="truncate text-amber-300">{revealed[r.name]}</code>
            )}
            <button
              type="button"
              onClick={() => void reveal(r.name)}
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228]"
            >
              Reveal
            </button>
            <button
              type="button"
              onClick={() =>
                void credentialDelete(r.name).then(() => {
                  setRevealed(({ [r.name]: _drop, ...rest }) => rest);
                  refresh();
                })
              }
              className="rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228] hover:text-red-400"
            >
              Delete
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
