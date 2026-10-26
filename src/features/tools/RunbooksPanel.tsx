import { useEffect, useState } from "react";

import { parseExpect } from "../../lib/expect";
import { automationRun, runbookDelete, runbookSave, runbooksList } from "../../lib/ipc";
import { useRecorder } from "../../state/recorder";
import { useSessions } from "../../state/sessions";
import type { AutomationStepResult, Runbook } from "../../types";

type Scope = "active" | "all";

export function RunbooksPanel() {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);
  const [runbooks, setRunbooks] = useState<Runbook[]>([]);
  const [selected, setSelected] = useState("");
  const [name, setName] = useState("");
  const [script, setScript] = useState("");
  const [scope, setScope] = useState<Scope>("active");
  const [results, setResults] = useState<AutomationStepResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const recorder = useRecorder();

  const refresh = () => void runbooksList().then(setRunbooks).catch(() => {});
  useEffect(refresh, []);

  const connected = tabs.filter((t) => t.state === "connected");

  const load = (rb: Runbook) => {
    setSelected(rb.id);
    setName(rb.name);
    setScript(rb.script);
  };

  const newRunbook = () => {
    setSelected("");
    setName("");
    setScript("");
  };

  const save = async () => {
    setError(null);
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    const saved = await runbookSave({
      id: selected || null,
      name,
      script,
    });
    setSelected(saved.id);
    refresh();
  };

  const run = async () => {
    setError(null);
    setResults(null);
    const targets = scope === "active" ? (activeId ? [activeId] : []) : connected.map((t) => t.id);
    if (targets.length === 0) {
      setError("No target sessions.");
      return;
    }
    const all: AutomationStepResult[] = [];
    for (const id of targets) {
      const steps = parseExpect(script, id);
      if (steps.length === 0) continue;
      try {
        all.push(...(await automationRun(steps)));
      } catch (e) {
        setError(String(e));
      }
    }
    setResults(all);
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-0.5 text-xs text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="flex flex-col gap-2 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <select
          className={field}
          value={selected}
          onChange={(e) => {
            const rb = runbooks.find((r) => r.id === e.target.value);
            if (rb) load(rb);
          }}
        >
          <option value="">(choose runbook)</option>
          {runbooks.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name}
            </option>
          ))}
        </select>
        <input
          className={field}
          placeholder="runbook name"
          data-testid="runbook-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="button" onClick={newRunbook} className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]">
          New
        </button>
        <button
          type="button"
          onClick={() => void save()}
          data-testid="runbook-save"
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          Save
        </button>
        {selected && (
          <button
            type="button"
            onClick={() => void runbookDelete(selected).then(() => { newRunbook(); refresh(); })}
            className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228] hover:text-red-400"
          >
            Delete
          </button>
        )}
      </div>

      <textarea
        className={`${field} h-40 resize-none font-mono`}
        data-testid="runbook-script"
        placeholder={"waitfor [#>]\\s*$\nsendline show version\nwaitfor [#>]\\s*$"}
        value={script}
        onChange={(e) => setScript(e.target.value)}
      />

      <div className="flex flex-wrap items-center gap-2">
        <select className={field} value={scope} onChange={(e) => setScope(e.target.value as Scope)}>
          <option value="active">Active session</option>
          <option value="all">All connected</option>
        </select>
        <button
          type="button"
          onClick={() => void run()}
          data-testid="runbook-run"
          className="rounded bg-[#4F8EE8] px-2 py-1 font-medium text-white hover:bg-[#3F7ED8]"
        >
          Run
        </button>
        <span className="ml-auto flex items-center gap-2">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              data-testid="recorder-toggle"
              checked={recorder.recording}
              onChange={() => recorder.toggle()}
            />
            Recorder
          </label>
          <button
            type="button"
            onClick={() => {
              setScript(recorder.script());
              recorder.reset();
            }}
            className="rounded border border-[#2B323A] px-2 py-1 hover:bg-[#1D2228]"
          >
            Use recording
          </button>
        </span>
      </div>

      {recorder.recording && (
        <p className="text-amber-400">
          Recording… type in a session; lines are captured as <code>sendline</code>.
          Passwords would be captured too — review before saving.
        </p>
      )}
      {recorder.steps.length > 0 && (
        <pre className="max-h-24 overflow-auto rounded border border-[#2B323A] bg-[#111418] p-1 font-mono text-[11px] text-[#99A3AE]">
          {recorder.script()}
        </pre>
      )}

      {error && <p className="text-red-400">{error}</p>}
      {results && (
        <div data-testid="runbook-results" className="flex flex-col gap-0.5">
          {results.map((r, i) => (
            <span key={i} className={r.ok ? "text-emerald-400" : "text-red-400"}>
              {i + 1}. {r.kind}: {r.detail}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
