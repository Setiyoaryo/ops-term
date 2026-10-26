import { useState } from "react";

import { fallbackSettings, useLibrary } from "../../state/library";
import { COLOR_SCHEME_OPTIONS } from "../../theme";
import type { AppSettings } from "../../types";

interface Props {
  onClose: () => void;
}

type Tab = "general" | "terminal" | "appearance" | "ssh" | "logging";

const TABS: { id: Tab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "terminal", label: "Terminal" },
  { id: "appearance", label: "Appearance" },
  { id: "ssh", label: "SSH" },
  { id: "logging", label: "Logging" },
];

export function SettingsPanel({ onClose }: Props) {
  const settings = useLibrary((s) => s.settings) ?? fallbackSettings();
  const updateSettings = useLibrary((s) => s.updateSettings);
  const [draft, setDraft] = useState<AppSettings>(() =>
    structuredClone(settings),
  );
  const [tab, setTab] = useState<Tab>("general");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const update = <K extends keyof AppSettings>(group: K, patch: Partial<AppSettings[K]>) =>
    setDraft((d) => ({ ...d, [group]: { ...d[group], ...patch } }));

  const save = async () => {
    setError(null);
    setBusy(true);
    try {
      await updateSettings(draft);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const field =
    "rounded border border-[#2B323A] bg-[#111418] px-2 py-1 text-sm text-[#E5E9EF] outline-none focus:border-[#4F8EE8]";

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50">
      <div
        role="dialog"
        aria-label="Settings"
        data-testid="settings-panel"
        className="flex h-[520px] w-[720px] max-w-[94vw] overflow-hidden rounded-lg border border-[#2B323A] bg-[#181C20] text-sm text-[#E5E9EF] shadow-xl"
      >
        <nav className="w-40 shrink-0 border-r border-[#2B323A] bg-[#15181C] py-2">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              className={`block w-full px-3 py-1.5 text-left text-xs ${
                tab === t.id
                  ? "bg-[#111418] text-[#E5E9EF]"
                  : "text-[#99A3AE] hover:bg-[#1D2228]"
              }`}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <div className="flex flex-1 flex-col">
          <div className="flex-1 overflow-y-auto p-4">
            {tab === "general" && (
              <section className="flex flex-col gap-3">
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.general.confirmClose}
                    onChange={(e) => update("general", { confirmClose: e.target.checked })}
                  />
                  Confirm before closing a connected tab
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.general.reopenLastTabs}
                    onChange={(e) => update("general", { reopenLastTabs: e.target.checked })}
                  />
                  Reopen last tabs on startup
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    data-testid="auto-reconnect"
                    checked={draft.general.autoReconnect}
                    onChange={(e) => update("general", { autoReconnect: e.target.checked })}
                  />
                  Auto-reconnect dropped sessions (up to 3 attempts)
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Default protocol
                  <select
                    className={field}
                    value={draft.general.defaultProtocol}
                    onChange={(e) => update("general", { defaultProtocol: e.target.value })}
                  >
                    <option value="ssh">SSH</option>
                    <option value="local">Local shell</option>
                    <option value="telnet">Telnet</option>
                    <option value="serial">Serial</option>
                  </select>
                </label>
              </section>
            )}

            {tab === "terminal" && (
              <section className="flex flex-col gap-3">
                <label className="flex flex-col gap-1 text-xs">
                  Font family
                  <input
                    className={field}
                    value={draft.terminal.fontFamily}
                    onChange={(e) => update("terminal", { fontFamily: e.target.value })}
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Font size ({draft.terminal.fontSize}px)
                  <input
                    type="range"
                    min={10}
                    max={24}
                    value={draft.terminal.fontSize}
                    onChange={(e) =>
                      update("terminal", { fontSize: Number(e.target.value) })
                    }
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Scrollback
                  <select
                    className={field}
                    value={draft.terminal.scrollback}
                    onChange={(e) =>
                      update("terminal", { scrollback: Number(e.target.value) })
                    }
                  >
                    {[2000, 5000, 10000, 50000].map((n) => (
                      <option key={n} value={n}>
                        {n.toLocaleString()} lines
                      </option>
                    ))}
                  </select>
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.terminal.cursorBlink}
                    onChange={(e) =>
                      update("terminal", { cursorBlink: e.target.checked })
                    }
                  />
                  Blinking cursor
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    data-testid="copy-on-select"
                    checked={draft.terminal.copyOnSelect}
                    onChange={(e) =>
                      update("terminal", { copyOnSelect: e.target.checked })
                    }
                  />
                  Copy selection to clipboard automatically
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Backspace sends (default for new sessions)
                  <select
                    className={field}
                    data-testid="backspace-mode"
                    value={draft.terminal.backspace}
                    onChange={(e) => update("terminal", { backspace: e.target.value })}
                  >
                    <option value="del">DEL — 0x7F (Linux, modern)</option>
                    <option value="bs">BS — 0x08 (Huawei, Cisco, legacy gear)</option>
                  </select>
                </label>
              </section>
            )}

            {tab === "appearance" && (
              <section className="flex flex-col gap-3">
                <label className="flex flex-col gap-1 text-xs">
                  UI theme
                  <select
                    className={field}
                    data-testid="ui-theme"
                    value={draft.appearance.uiTheme}
                    onChange={(e) => update("appearance", { uiTheme: e.target.value })}
                  >
                    <option value="dark">Dark</option>
                    <option value="light">Light</option>
                    <option value="system">System</option>
                  </select>
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Terminal color scheme
                  <select
                    className={field}
                    value={draft.terminal.colorScheme}
                    onChange={(e) =>
                      update("terminal", { colorScheme: e.target.value })
                    }
                  >
                    {COLOR_SCHEME_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Density
                  <select
                    className={field}
                    value={draft.appearance.density}
                    onChange={(e) => update("appearance", { density: e.target.value })}
                  >
                    <option value="compact">Compact</option>
                    <option value="comfortable">Comfortable</option>
                  </select>
                </label>
              </section>
            )}

            {tab === "ssh" && (
              <section className="flex flex-col gap-3">
                <label className="flex flex-col gap-1 text-xs">
                  SSH engine
                  <select
                    className={field}
                    data-testid="ssh-engine"
                    value={draft.ssh.engine}
                    onChange={(e) => update("ssh", { engine: e.target.value })}
                  >
                    <option value="openssh">OpenSSH client (default)</option>
                    <option value="native">Native (russh, experimental)</option>
                  </select>
                  <span className="text-[#5C6370]">
                    Native engine: strict known_hosts verification, key/agent auth
                    only, no ProxyJump.
                  </span>
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Connect timeout (seconds)
                  <input
                    className={field}
                    type="number"
                    min={1}
                    max={600}
                    value={draft.ssh.connectTimeoutSecs}
                    onChange={(e) =>
                      update("ssh", { connectTimeoutSecs: Number(e.target.value) })
                    }
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  ServerAliveInterval (seconds)
                  <input
                    className={field}
                    type="number"
                    min={0}
                    max={3600}
                    value={draft.ssh.serverAliveIntervalSecs}
                    onChange={(e) =>
                      update("ssh", { serverAliveIntervalSecs: Number(e.target.value) })
                    }
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  TERM
                  <input
                    className={field}
                    value={draft.ssh.term}
                    onChange={(e) => update("ssh", { term: e.target.value })}
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Default identity file (optional)
                  <input
                    className={field}
                    value={draft.ssh.identityFile ?? ""}
                    onChange={(e) =>
                      update("ssh", { identityFile: e.target.value || null })
                    }
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  User certificate file (optional, advanced)
                  <input
                    className={field}
                    value={draft.ssh.certificateFile ?? ""}
                    onChange={(e) =>
                      update("ssh", { certificateFile: e.target.value || null })
                    }
                  />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  PKCS#11 provider library (optional, advanced)
                  <input
                    className={field}
                    value={draft.ssh.pkcs11Provider ?? ""}
                    onChange={(e) =>
                      update("ssh", { pkcs11Provider: e.target.value || null })
                    }
                  />
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.ssh.compression}
                    onChange={(e) => update("ssh", { compression: e.target.checked })}
                  />
                  Enable compression (-C)
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.ssh.x11Forwarding}
                    onChange={(e) => update("ssh", { x11Forwarding: e.target.checked })}
                  />
                  X11 forwarding (-X)
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.ssh.forwardAgent}
                    onChange={(e) => update("ssh", { forwardAgent: e.target.checked })}
                  />
                  Forward SSH agent (-A)
                  <span className="text-amber-400">
                    ⚠ only forward to trusted hosts
                  </span>
                </label>
              </section>
            )}

            {tab === "logging" && (
              <section className="flex flex-col gap-3">
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.logging.enabledByDefault}
                    onChange={(e) =>
                      update("logging", { enabledByDefault: e.target.checked })
                    }
                  />
                  Start logging automatically for new sessions
                </label>
                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={draft.logging.timestamps}
                    onChange={(e) =>
                      update("logging", { timestamps: e.target.checked })
                    }
                  />
                  Prepend timestamps to each line
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Log mode
                  <select
                    className={field}
                    value={draft.logging.mode}
                    onChange={(e) => update("logging", { mode: e.target.value })}
                  >
                    <option value="raw">Raw received output</option>
                    <option value="text">Normalized text</option>
                  </select>
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  Log directory (blank = app data)
                  <input
                    className={field}
                    value={draft.logging.logDir ?? ""}
                    onChange={(e) =>
                      update("logging", { logDir: e.target.value || null })
                    }
                  />
                </label>
              </section>
            )}
          </div>
          {error && (
            <p role="alert" className="px-4 pb-1 text-xs text-red-400">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2 border-t border-[#2B323A] p-3">
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
              onClick={() => void save()}
              data-testid="settings-save"
              className="rounded bg-[#4F8EE8] px-3 py-1 text-sm font-medium text-white hover:bg-[#3F7ED8] disabled:opacity-50"
            >
              Save
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
