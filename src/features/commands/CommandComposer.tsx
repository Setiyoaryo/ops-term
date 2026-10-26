import { useEffect, useMemo, useState } from "react";

import { commandDispatch } from "../../lib/ipc";
import { useSessions } from "../../state/sessions";
import type { DispatchResult } from "../../types";

interface Props {
  open: boolean;
  onClose: () => void;
  seed: { text: string; nonce: number } | null;
}

type Scope = "active" | "selected" | "all";

export function CommandComposer({ open, onClose, seed }: Props) {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);

  const [scope, setScope] = useState<Scope>("active");
  const [explicit, setExplicit] = useState<Set<string>>(new Set());
  const [content, setContent] = useState("");
  const [appendEnter, setAppendEnter] = useState(true);
  const [armed, setArmed] = useState(false);
  const [results, setResults] = useState<DispatchResult[] | null>(null);
  const [busy, setBusy] = useState(false);

  const connected = useMemo(
    () => tabs.filter((t) => t.state === "connected"),
    [tabs],
  );

  const targets = useMemo(() => {
    if (scope === "active") return activeId ? [activeId] : [];
    if (scope === "all") return connected.map((t) => t.id);
    return connected.filter((t) => explicit.has(t.id)).map((t) => t.id);
  }, [scope, activeId, connected, explicit]);

  const targetNames = targets
    .map((id) => tabs.find((t) => t.id === id)?.title ?? id)
    .join(", ");

  const isBroadcast = targets.length > 1;

  useEffect(() => {
    if (!isBroadcast) setArmed(false);
  }, [isBroadcast]);

  useEffect(() => {
    if (scope !== "selected") setExplicit(new Set());
  }, [scope]);

  useEffect(() => {
    if (seed) setContent(seed.text);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed?.nonce]);

  if (!open) return null;

  const send = async () => {
    if (!content) return;
    if (isBroadcast && !armed) return;
    setBusy(true);
    setResults(null);
    try {
      const res = await commandDispatch(targets, content, appendEnter);
      setResults(res);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      data-testid="command-composer"
      className="border-t border-[#2B323A] bg-[#15181C] px-3 py-2 text-xs text-[#E5E9EF]"
    >
      <div className="mb-1.5 flex flex-wrap items-center gap-2">
        <span className="font-semibold uppercase tracking-wide text-[#99A3AE]">
          Command Composer
        </span>
        <select
          aria-label="Target scope"
          data-testid="composer-scope"
          value={scope}
          onChange={(e) => setScope(e.target.value as Scope)}
          className="rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5"
        >
          <option value="active">Active pane</option>
          <option value="selected">Selected sessions</option>
          <option value="all">All connected</option>
        </select>
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={appendEnter}
            onChange={(e) => setAppendEnter(e.target.checked)}
          />
          Send with Enter
        </label>
        <span className="text-[#99A3AE]">
          Targets ({targets.length}): {targetNames || "none"}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="ml-auto rounded px-1.5 hover:bg-[#2B323A]"
          title="Close (Alt+G)"
        >
          ✕
        </button>
      </div>

      {scope === "selected" && (
        <div className="mb-1.5 flex flex-wrap gap-2">
          {connected.map((t) => (
            <label key={t.id} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={explicit.has(t.id)}
                onChange={() =>
                  setExplicit((prev) => {
                    const next = new Set(prev);
                    if (next.has(t.id)) next.delete(t.id);
                    else next.add(t.id);
                    return next;
                  })
                }
              />
              {t.title}
            </label>
          ))}
          {connected.length === 0 && (
            <span className="text-[#5C6370]">No connected sessions.</span>
          )}
        </div>
      )}

      <div className="flex gap-2">
        <textarea
          aria-label="Composer input"
          data-testid="composer-input"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Compose command(s) — review targets before sending"
          className="h-16 flex-1 resize-none rounded border border-[#2B323A] bg-[#111418] px-2 py-1 font-mono text-xs outline-none focus:border-[#4F8EE8]"
        />
        <div className="flex flex-col gap-1">
          {isBroadcast && (
            <label className="flex items-center gap-1 text-amber-300">
              <input
                type="checkbox"
                checked={armed}
                onChange={(e) => setArmed(e.target.checked)}
                data-testid="arm-broadcast"
              />
              Arm broadcast
            </label>
          )}
          <button
            type="button"
            disabled={busy || !content || targets.length === 0 || (isBroadcast && !armed)}
            onClick={() => void send()}
            data-testid="composer-send"
            className="rounded bg-[#4F8EE8] px-3 py-1 font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-40"
          >
            {isBroadcast ? "Send to all" : "Send"}
          </button>
        </div>
      </div>

      {results && (
        <div data-testid="composer-results" className="mt-1.5 flex flex-col gap-0.5">
          {results.map((r) => (
            <span
              key={r.sessionId}
              className={r.ok ? "text-emerald-400" : "text-red-400"}
            >
              {r.ok ? "✓" : "✕"}{" "}
              {tabs.find((t) => t.id === r.sessionId)?.title ?? r.sessionId}
              {r.error ? ` — ${r.error}` : ""}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
