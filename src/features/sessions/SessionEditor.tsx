import { useEffect, useState } from "react";

import { credentialsList, serialDevicesList } from "../../lib/ipc";
import { findVendor, VENDOR_PROFILES } from "../../lib/vendors";
import { useHighlights } from "../../state/highlight";
import { useLibrary } from "../../state/library";
import type {
  CredentialRecord,
  Folder,
  Protocol,
  SavedSession,
  SavedSessionInput,
} from "../../types";

interface Prefill {
  name?: string;
  host?: string;
  port?: number;
  protocol?: Protocol;
  username?: string | null;
  identityFile?: string | null;
  proxyJump?: string | null;
}

interface Props {
  /** Existing session to edit, or null to create a new one. */
  session: SavedSession | null;
  defaultFolderId: string | null;
  /** Values to seed a brand-new session (e.g. the current ad-hoc SSH tab). */
  prefill?: Prefill | null;
  onClose: () => void;
  onSaved: (saved: SavedSession) => void;
}

const PROTOCOLS: { value: Protocol; label: string }[] = [
  { value: "ssh", label: "SSH" },
  { value: "local", label: "Local shell" },
  { value: "telnet", label: "Telnet (insecure)" },
  { value: "serial", label: "Serial" },
];

const DEFAULT_PORTS: Record<Protocol, number> = {
  ssh: 22,
  local: 22,
  telnet: 23,
  serial: 22,
  sftp: 22,
};

function flattenFolders(folders: Folder[], depth = 0): { id: string; label: string }[] {
  const out: { id: string; label: string }[] = [];
  for (const f of folders.filter((x) => x.parentId === null)) {
    out.push({ id: f.id, label: `${"  ".repeat(depth)}${f.name}` });
    out.push(...flattenFolders(folders.filter((x) => x.parentId === f.id), depth + 1));
  }
  return out;
}

export function SessionEditor({
  session,
  defaultFolderId,
  prefill,
  onClose,
  onSaved,
}: Props) {
  const folders = useLibrary((s) => s.folders);
  const saveSession = useLibrary((s) => s.saveSession);

  const [name, setName] = useState(session?.name ?? prefill?.name ?? "");
  const [protocol, setProtocol] = useState<Protocol>(
    session?.protocol ?? prefill?.protocol ?? "ssh",
  );
  const [host, setHost] = useState(session?.host ?? prefill?.host ?? "");
  const [port, setPort] = useState(session?.port ?? prefill?.port ?? 22);
  const [username, setUsername] = useState(
    session?.username ?? prefill?.username ?? "",
  );
  const [identityFile, setIdentityFile] = useState(
    session?.identityFile ?? prefill?.identityFile ?? "",
  );
  const [proxyJump, setProxyJump] = useState(
    session?.proxyJump ?? prefill?.proxyJump ?? "",
  );
  const [folderId, setFolderId] = useState<string | null>(
    session?.folderId ?? defaultFolderId,
  );
  const [tags, setTags] = useState((session?.tags ?? []).join(", "));
  const [description, setDescription] = useState(session?.description ?? "");
  const [notes, setNotes] = useState(session?.notes ?? "");
  const [favorite, setFavorite] = useState(session?.isFavorite ?? false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [devices, setDevices] = useState<string[]>([]);
  const [credentials, setCredentials] = useState<CredentialRecord[]>([]);
  const [credentialRef, setCredentialRef] = useState<string>(
    session?.credentialRef ?? "",
  );
  const override = (session?.settingsOverride ?? {}) as Record<string, unknown>;
  const [backspace, setBackspace] = useState<"del" | "bs">(
    (override.backspace as "del" | "bs") ?? "del",
  );
  const [encoding, setEncoding] = useState((override.encoding as string) ?? "utf-8");
  const [autolog, setAutolog] = useState(Boolean(override.autolog));
  const [pager, setPager] = useState((override.pager as string) ?? "none");
  const [pagerCommand, setPagerCommand] = useState((override.pagerCommand as string) ?? "");
  const [loginScript, setLoginScript] = useState((override.loginScript as string) ?? "");
  const [vendor, setVendor] = useState((override.vendor as string) ?? "");

  const applyVendor = (id: string) => {
    setVendor(id);
    const profile = findVendor(id);
    if (!profile) return;
    setPager(profile.pager);
    setPagerCommand(profile.pagerCommand ?? "");
    setBackspace(profile.backspace);
    if (!loginScript.trim()) setLoginScript(profile.loginScript);
    for (const preset of profile.highlights) useHighlights.getState().upsert(preset);
    for (const command of profile.commands) {
      void useLibrary.getState().saveCommand({
        name: command.name,
        content: command.content,
        vendorProfile: profile.id,
        tags: [profile.id],
      });
    }
  };

  const serialOverride = (session?.settingsOverride?.serial ?? {}) as {
    baud?: number;
    dataBits?: number;
    parity?: string;
    stopBits?: number;
    flowControl?: string;
    newline?: string;
  };
  const [baud, setBaud] = useState(serialOverride.baud ?? 9600);
  const [dataBits, setDataBits] = useState(serialOverride.dataBits ?? 8);
  const [parity, setParity] = useState(serialOverride.parity ?? "none");
  const [stopBits, setStopBits] = useState(serialOverride.stopBits ?? 1);
  const [flowControl, setFlowControl] = useState(serialOverride.flowControl ?? "none");
  const [newline, setNewline] = useState(serialOverride.newline ?? "crlf");

  useEffect(() => {
    void serialDevicesList().then(setDevices).catch(() => setDevices([]));
    void credentialsList().then(setCredentials).catch(() => setCredentials([]));
  }, []);

  const isTelnet = protocol === "telnet";
  const isSerial = protocol === "serial";
  const needsHost = protocol === "ssh" || isTelnet;
  const needsPort = protocol === "ssh" || isTelnet;
  const selectedCred = credentials.find((c) => c.name === credentialRef);

  const submit = async () => {
    setError(null);
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    if (needsHost && !host.trim()) {
      setError("Host is required.");
      return;
    }
    if (isSerial && !host.trim()) {
      setError("Serial device path is required.");
      return;
    }
    if (needsPort && (port < 1 || port > 65535)) {
      setError("Port must be between 1 and 65535.");
      return;
    }
    const settingsOverride: Record<string, unknown> = {
      ...(session?.settingsOverride ?? {}),
      backspace,
      encoding,
      autolog,
      pager,
      pagerCommand,
      loginScript,
      vendor,
    };
    if (isSerial) {
      settingsOverride.serial = { baud, dataBits, parity, stopBits, flowControl, newline };
    }
    const input: SavedSessionInput = {
      id: session?.id ?? null,
      folderId,
      name: name.trim(),
      host: host.trim(),
      port: isTelnet ? port : port,
      protocol,
      username: isSerial ? null : username.trim() || null,
      identityFile: isSerial ? null : identityFile.trim() || null,
      proxyJump: isSerial ? null : proxyJump.trim() || null,
      tags: tags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean),
      description: description.trim() || null,
      notes: notes.trim() || null,
      settingsOverride,
      credentialRef: isSerial ? null : credentialRef || null,
      isFavorite: favorite,
      sortOrder: session?.sortOrder ?? null,
    };
    setBusy(true);
    try {
      const saved = await saveSession(input);
      onSaved(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const field =
    "w-full rounded border border-[#2B323A] bg-[#111418] px-2 py-1 text-sm text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50">
      <div
        role="dialog"
        aria-label="Session editor"
        data-testid="session-editor"
        className="max-h-[90vh] w-[560px] max-w-[92vw] overflow-y-auto rounded-lg border border-[#2B323A] bg-[#181C20] p-4 text-sm text-[#E5E9EF] shadow-xl"
      >
        <h2 className="mb-3 text-sm font-semibold">
          {session ? "Edit session" : "New session"}
        </h2>
        <div className="grid grid-cols-2 gap-3">
          <label className="col-span-2 flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Name</span>
            <input className={field} value={name} data-testid="field-name" onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Protocol</span>
            <select
              className={field}
              data-testid="field-protocol"
              value={protocol}
              onChange={(e) => {
                const next = e.target.value as Protocol;
                setProtocol(next);
                // If the port is still a protocol default, follow the new
                // protocol's default (SSH 22, Telnet 23); keep custom ports.
                if (port === 22 || port === 23) setPort(DEFAULT_PORTS[next] ?? port);
              }}
            >
              {PROTOCOLS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Folder</span>
            <select
              className={field}
              value={folderId ?? ""}
              onChange={(e) => setFolderId(e.target.value || null)}
            >
              <option value="">(root)</option>
              {flattenFolders(folders).map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </select>
          </label>
          {!isSerial && (
            <label className="col-span-2 flex flex-col gap-1">
              <span className="text-xs text-[#99A3AE]">
                Credential (password from inventory, optional)
              </span>
              <select
                className={field}
                data-testid="field-credential"
                value={credentialRef}
                onChange={(e) => setCredentialRef(e.target.value)}
              >
                <option value="">(prompt each time)</option>
                {credentials.map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.name}
                    {c.tags.length ? ` — ${c.tags.join(", ")}` : ""}
                  </option>
                ))}
              </select>
              {selectedCred && (
                <span className="text-[#5C6370]">
                  {selectedCred.tags.length > 0 && (
                    <>tags: {selectedCred.tags.map((t) => `#${t}`).join(" ")} · </>
                  )}
                  {selectedCred.username
                    ? username.trim()
                      ? `login: ${username.trim()} (device username), password dari kredensial`
                      : `login: ${selectedCred.username} (dari kredensial)`
                    : username.trim()
                      ? `login: ${username.trim()}, password dari kredensial`
                      : "kredensial ini tidak punya username — isi Username di device"}
                </span>
              )}
              {credentials.length === 0 && (
                <span className="text-[#5C6370]">
                  Add credentials in Tools → Credentials, then reference one here.
                </span>
              )}
            </label>
          )}
          <label className="flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">
              {isSerial ? "Device" : "Host"}
            </span>
            <input
              className={field}
              value={host}
              list={isSerial ? "serial-devices" : undefined}
              placeholder={isSerial ? "/dev/ttyUSB0" : "router.example.net"}
              data-testid="field-host"
              onChange={(e) => setHost(e.target.value)}
            />
            {isSerial && (
              <datalist id="serial-devices">
                {devices.map((d) => (
                  <option key={d} value={d} />
                ))}
              </datalist>
            )}
          </label>
          {needsPort && (
            <label className="flex flex-col gap-1">
              <span className="text-xs text-[#99A3AE]">Port</span>
              <input
                className={field}
                type="number"
                data-testid="field-port"
                value={port}
                onChange={(e) => setPort(Number(e.target.value))}
              />
            </label>
          )}
          {!isSerial && (
            <label className="flex flex-col gap-1">
              <span className="text-xs text-[#99A3AE]">Username</span>
              <input className={field} value={username} onChange={(e) => setUsername(e.target.value)} />
            </label>
          )}
          {protocol === "ssh" && (
            <>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Identity file</span>
                <input className={field} value={identityFile} onChange={(e) => setIdentityFile(e.target.value)} />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">ProxyJump</span>
                <input className={field} value={proxyJump} onChange={(e) => setProxyJump(e.target.value)} />
              </label>
            </>
          )}
          <label className="flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Tags (comma separated)</span>
            <input className={field} value={tags} onChange={(e) => setTags(e.target.value)} />
          </label>
          {!isSerial && (
            <label className="flex flex-col gap-1">
              <span className="text-xs text-[#99A3AE]">Backspace</span>
              <select
                className={field}
                data-testid="field-backspace"
                value={backspace}
                onChange={(e) => setBackspace(e.target.value as "del" | "bs")}
              >
                <option value="del">DEL (0x7F) — Linux/modern</option>
                <option value="bs">BS (0x08) — Huawei/Cisco/legacy</option>
              </select>
            </label>
          )}
          {isTelnet && (
            <p className="col-span-2 rounded border border-amber-600/40 bg-[#2a2416] px-2 py-1 text-[11px] text-amber-300">
              Telnet is unencrypted. Credentials and traffic are sent in clear
              text on the network.
            </p>
          )}
          {isSerial && (
            <>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Baud</span>
                <select
                  className={field}
                  value={baud}
                  onChange={(e) => setBaud(Number(e.target.value))}
                >
                  {[1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400].map(
                    (b) => (
                      <option key={b} value={b}>
                        {b}
                      </option>
                    ),
                  )}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Data bits</span>
                <select className={field} value={dataBits} onChange={(e) => setDataBits(Number(e.target.value))}>
                  {[5, 6, 7, 8].map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Parity</span>
                <select className={field} value={parity} onChange={(e) => setParity(e.target.value)}>
                  <option value="none">none</option>
                  <option value="odd">odd</option>
                  <option value="even">even</option>
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Stop bits</span>
                <select className={field} value={stopBits} onChange={(e) => setStopBits(Number(e.target.value))}>
                  <option value={1}>1</option>
                  <option value={2}>2</option>
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Flow control</span>
                <select className={field} value={flowControl} onChange={(e) => setFlowControl(e.target.value)}>
                  <option value="none">none</option>
                  <option value="software">software (XON/XOFF)</option>
                  <option value="hardware">hardware (RTS/CTS)</option>
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-[#99A3AE]">Newline</span>
                <select className={field} value={newline} onChange={(e) => setNewline(e.target.value)}>
                  <option value="cr">CR</option>
                  <option value="lf">LF</option>
                  <option value="crlf">CRLF</option>
                </select>
              </label>
            </>
          )}
          <label className="col-span-2 flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Description</span>
            <input className={field} value={description} onChange={(e) => setDescription(e.target.value)} />
          </label>
          <label className="col-span-2 flex flex-col gap-1">
            <span className="text-xs text-[#99A3AE]">Notes (not secrets)</span>
            <textarea
              className={`${field} h-16 resize-none`}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </label>
          {!isSerial && (
            <fieldset className="col-span-2 rounded border border-[#2B323A] p-2">
              <legend className="px-1 text-[11px] font-semibold uppercase tracking-wide text-[#99A3AE]">
                On connect
              </legend>
              <div className="grid grid-cols-2 gap-2">
                <label className="col-span-2 flex flex-col gap-1">
                  <span className="text-xs text-[#99A3AE]">
                    Vendor profile (applies pager, prompt, backspace, highlight &
                    commands)
                  </span>
                  <select
                    className={field}
                    data-testid="field-vendor"
                    value={vendor}
                    onChange={(e) => applyVendor(e.target.value)}
                  >
                    <option value="">(none)</option>
                    {VENDOR_PROFILES.map((v) => (
                      <option key={v.id} value={v.id}>
                        {v.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-xs text-[#99A3AE]">Disable pager</span>
                  <select
                    className={field}
                    data-testid="field-pager"
                    value={pager}
                    onChange={(e) => setPager(e.target.value)}
                  >
                    <option value="none">Don't send anything</option>
                    <option value="huawei">Huawei — screen-length 0 temporary</option>
                    <option value="cisco">Cisco — terminal length 0</option>
                    <option value="juniper">Juniper — set cli screen-length 0</option>
                    <option value="mikrotik">MikroTik — /terminal print</option>
                    <option value="custom">Custom command…</option>
                  </select>
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-xs text-[#99A3AE]">Encoding</span>
                  <select
                    className={field}
                    data-testid="field-encoding"
                    value={encoding}
                    onChange={(e) => setEncoding(e.target.value)}
                  >
                    <option value="utf-8">UTF-8</option>
                    <option value="gbk">GBK / GB2312</option>
                    <option value="big5">Big5</option>
                    <option value="latin1">Latin-1</option>
                    <option value="windows-1252">Windows-1252</option>
                  </select>
                </label>
                {pager === "custom" && (
                  <label className="col-span-2 flex flex-col gap-1">
                    <span className="text-xs text-[#99A3AE]">Custom pager command</span>
                    <input
                      className={field}
                      value={pagerCommand}
                      onChange={(e) => setPagerCommand(e.target.value)}
                    />
                  </label>
                )}
                <label className="col-span-2 flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={autolog}
                    onChange={(e) => setAutolog(e.target.checked)}
                  />
                  Start logging automatically when this device connects
                </label>
                <label className="col-span-2 flex flex-col gap-1">
                  <span className="text-xs text-[#99A3AE]">
                    Login script (expect): send / sendline / waitfor / sleep
                  </span>
                  <textarea
                    className={`${field} h-24 resize-none font-mono`}
                    data-testid="field-login-script"
                    placeholder={"waitfor [#>]\\s*$\nsleep 200\nsendline enable\nwaitfor [Pp]assword:\nsendline <enable-secret>"}
                    value={loginScript}
                    onChange={(e) => setLoginScript(e.target.value)}
                  />
                </label>
              </div>
            </fieldset>
          )}
          <label className="col-span-2 flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={favorite}
              onChange={(e) => setFavorite(e.target.checked)}
            />
            Favorite
          </label>
        </div>
        {error && (
          <p role="alert" className="mt-2 text-xs text-red-400">
            {error}
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-[#2B323A] px-3 py-1 text-sm hover:bg-[#1D2228]"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void submit()}
            data-testid="session-save"
            className="rounded bg-[#4F8EE8] px-3 py-1 text-sm font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-50"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
