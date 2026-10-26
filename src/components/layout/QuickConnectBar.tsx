import { useState } from "react";

import { parseQuickConnect, specLabel } from "../../features/connections/quickConnect";
import type { SshSpec } from "../../types";

interface Props {
  onConnect: (spec: SshSpec, label: string) => void;
  onLocalShell: () => void;
  onSaveAsSession: () => void;
  busy: boolean;
}

export function QuickConnectBar({
  onConnect,
  onLocalShell,
  onSaveAsSession,
  busy,
}: Props) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = () => {
    const result = parseQuickConnect(value);
    if (!result.ok || !result.spec) {
      setError(result.error ?? "Invalid input.");
      return;
    }
    setError(null);
    onConnect(result.spec, specLabel(result.spec));
  };

  return (
    <div className="flex items-center gap-2 border-b border-[#2B323A] bg-[#15181C] px-3 py-2">
      <input
        aria-label="Quick connect"
        data-testid="quick-connect"
        placeholder="user@hostname, host:port, or ssh alias"
        value={value}
        disabled={busy}
        spellCheck={false}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") submit();
        }}
        className="min-w-0 flex-1 rounded border border-[#2B323A] bg-[#111418] px-2 py-1 text-sm text-[#E5E9EF] outline-none placeholder:text-[#5C6370] focus:border-[#4F8EE8]"
      />
      <button
        type="button"
        disabled={busy}
        onClick={submit}
        data-testid="connect"
        className="rounded bg-[#4F8EE8] px-3 py-1 text-sm font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-50"
      >
        Connect
      </button>
      <button
        type="button"
        disabled={busy}
        onClick={onLocalShell}
        data-testid="local-shell"
        className="rounded border border-[#2B323A] px-3 py-1 text-sm text-[#E5E9EF] hover:bg-[#1D2228] disabled:opacity-50"
      >
        Local Shell
      </button>
      <button
        type="button"
        disabled={busy}
        onClick={onSaveAsSession}
        title="Save as a new session"
        data-testid="save-session"
        className="rounded border border-[#2B323A] px-3 py-1 text-sm text-[#E5E9EF] hover:bg-[#1D2228] disabled:opacity-50"
      >
        Save…
      </button>
      {error && (
        <span role="alert" className="text-xs text-red-400">
          {error}
        </span>
      )}
    </div>
  );
}
