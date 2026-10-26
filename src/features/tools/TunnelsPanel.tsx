import { useEffect, useState } from "react";

import { tunnelList, tunnelStart, tunnelStop } from "../../lib/ipc";
import { useLibrary } from "../../state/library";
import type { TunnelView } from "../../types";

export function TunnelsPanel() {
  const sessions = useLibrary((s) => s.sessions);
  const [tunnels, setTunnels] = useState<TunnelView[]>([]);
  const [savedId, setSavedId] = useState<string>("");
  const [kind, setKind] = useState<"local" | "remote" | "dynamic">("local");
  const [bindAddress, setBindAddress] = useState("127.0.0.1");
  const [bindPort, setBindPort] = useState(8080);
  const [destHost, setDestHost] = useState("");
  const [destPort, setDestPort] = useState(80);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => void tunnelList().then(setTunnels).catch(() => {});
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, []);

  const sshSessions = sessions.filter((s) => s.protocol === "ssh");

  const start = async () => {
    setError(null);
    const saved = sshSessions.find((s) => s.id === savedId);
    if (!saved) {
      setError("Select an SSH session to tunnel through.");
      return;
    }
    try {
      await tunnelStart({
        kind,
        bindAddress,
        bindPort,
        destHost: kind === "dynamic" ? null : destHost,
        destPort: kind === "dynamic" ? null : destPort,
        sshHost: saved.host,
        sshPort: saved.port,
        sshUser: saved.username,
        identityFile: saved.identityFile,
        label: saved.name,
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
        <select
          className={field}
          value={kind}
          onChange={(e) => setKind(e.target.value as typeof kind)}
        >
          <option value="local">Local (-L)</option>
          <option value="remote">Remote (-R)</option>
          <option value="dynamic">Dynamic SOCKS (-D)</option>
        </select>
        <input
          className={field}
          value={bindAddress}
          title="Bind address (defaults to loopback)"
          onChange={(e) => setBindAddress(e.target.value)}
        />
        <input
          className={`${field} w-20`}
          type="number"
          value={bindPort}
          onChange={(e) => setBindPort(Number(e.target.value))}
        />
        {kind !== "dynamic" && (
          <>
            <input
              className={field}
              placeholder="dest host"
              value={destHost}
              onChange={(e) => setDestHost(e.target.value)}
            />
            <input
              className={`${field} w-20`}
              type="number"
              value={destPort}
              onChange={(e) => setDestPort(Number(e.target.value))}
            />
          </>
        )}
        <button
          type="button"
          onClick={() => void start()}
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          Start tunnel
        </button>
      </div>
      {error && <p className="text-red-400">{error}</p>}
      <div className="flex flex-col gap-1">
        {tunnels.length === 0 && <p className="text-[#5C6370]">No tunnels running.</p>}
        {tunnels.map((t) => (
          <div
            key={t.id}
            data-testid="tunnel-row"
            className="flex items-center gap-2 rounded border border-[#2B323A] px-2 py-1"
          >
            <span className={t.running ? "text-emerald-400" : "text-red-400"}>
              {t.running ? "●" : "✕"}
            </span>
            <span className="uppercase text-[#99A3AE]">{t.kind}</span>
            <span>
              {t.bind}
              {t.dest ? ` → ${t.dest}` : ""}
            </span>
            <span className="text-[#5C6370]">via {t.sshHost}</span>
            {t.error && <span className="truncate text-red-400">{t.error}</span>}
            <button
              type="button"
              onClick={() => void tunnelStop(t.id).then(refresh)}
              className="ml-auto rounded border border-[#2B323A] px-1.5 hover:bg-[#1D2228]"
            >
              Stop
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
