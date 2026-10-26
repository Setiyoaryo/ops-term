import { useState } from "react";

import { parseExpect } from "../../lib/expect";
import { automationRun } from "../../lib/ipc";
import { useSessions } from "../../state/sessions";
import type { AutomationStepResult } from "../../types";

const HELP = `# One step per line. Target is the selected session.
send <text>              send without a trailing newline
sendline <text>          send with a trailing newline
waitfor <regex> [ms]     wait for a regex (default 10000 ms)
sleep <ms>               pause

# Example
sendline terminal length 0
waitfor [#>]\\s*$
sendline show version
waitfor [#>]\\s*$`;


export function AutomationPanel() {
  const tabs = useSessions((s) => s.tabs);
  const [sessionId, setSessionId] = useState<string>("");
  const [script, setScript] = useState("sendline echo OPSTERM_AUTOMATION\nwaitfor OPSTERM_AUTOMATION");
  const [results, setResults] = useState<AutomationStepResult[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const connected = tabs.filter((t) => t.state === "connected");

  const run = async () => {
    setError(null);
    const target = sessionId || connected[0]?.id;
    if (!target) {
      setError("No connected session.");
      return;
    }
    const steps = parseExpect(script, target);
    if (steps.length === 0) {
      setError("No executable steps.");
      return;
    }
    setBusy(true);
    try {
      setResults(await automationRun(steps));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex flex-col gap-2 text-xs">
      <p className="text-[#99A3AE]">
        Explicit, user-triggered automation only. Steps run against real
        sessions with bounded timeouts; there is no scheduler.
      </p>
      <div className="flex items-center gap-2">
        <select className={field} value={sessionId} onChange={(e) => setSessionId(e.target.value)}>
          <option value="">(first connected)</option>
          {connected.map((t) => (
            <option key={t.id} value={t.id}>
              {t.title}
            </option>
          ))}
        </select>
        <button
          type="button"
          disabled={busy}
          onClick={() => void run()}
          data-testid="automation-run"
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-40"
        >
          Run
        </button>
      </div>
      <textarea
        className="h-36 resize-none rounded border border-[#2B323A] bg-[#111418] px-2 py-1 font-mono text-xs outline-none focus:border-[#4F8EE8]"
        value={script}
        onChange={(e) => setScript(e.target.value)}
      />
      <details className="text-[#5C6370]">
        <summary className="cursor-pointer">Syntax</summary>
        <pre className="whitespace-pre-wrap">{HELP}</pre>
      </details>
      {error && <p className="text-red-400">{error}</p>}
      {results && (
        <div data-testid="automation-results" className="flex flex-col gap-0.5">
          {results.map((r) => (
            <span key={r.index} className={r.ok ? "text-emerald-400" : "text-red-400"}>
              {r.index + 1}. {r.kind}: {r.detail}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
