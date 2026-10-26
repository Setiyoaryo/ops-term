import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { QuickConnectBar } from "./components/layout/QuickConnectBar";
import { PaneDivider } from "./components/layout/PaneDivider";
import { Sidebar } from "./components/layout/Sidebar";
import { StatusBar } from "./components/layout/StatusBar";
import { TabBar } from "./components/layout/TabBar";
import { TerminalPane } from "./components/terminal/TerminalPane";
import {
  bufferOutput,
  decodeBase64,
  clearState,
  decodeOutput,
  getTerminal,
  registerEncoding,
} from "./components/terminal/terminalRegistry";
import { DEFAULT_PROMPT, PAGER_PRESETS, parseExpect } from "./lib/expect";
import { findVendor } from "./lib/vendors";
import { ButtonBar } from "./features/commands/ButtonBar";
import { CommandComposer } from "./features/commands/CommandComposer";
import { SessionEditor } from "./features/sessions/SessionEditor";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { TerminalSearchBar } from "./features/terminal/TerminalSearchBar";
import { ToolsDrawer } from "./features/tools/ToolsDrawer";
import {
  WorkspacesControl,
  type WorkspaceLayout,
} from "./features/tools/WorkspacesControl";
import {
  automationRun,
  commandDispatch,
  connectionClose,
  connectionReconnect,
  logStart,
  logStop,
  openLocal,
  openSaved,
  openSftp,
  openSsh,
  serialBreak,
  sshAvailable,
} from "./lib/ipc";
import { fallbackSettings, useLibrary } from "./state/library";
import { isDarkUi } from "./theme";
import {
  makeLocalTab,
  makeSavedTab,
  makeSshTab,
  useSessions,
  type Orientation,
} from "./state/sessions";
import type {
  AutomationStep,
  Command,
  ConnectionStateEvent,
  DeviceOverrides,
  SavedSession,
  SshSpec,
  TerminalExitEvent,
  TerminalOutputEvent,
} from "./types";

const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;
const LAST_TABS_KEY = "opsterm.lastTabs";

export default function App() {
  const tabs = useSessions((s) => s.tabs);
  const activeId = useSessions((s) => s.activeId);
  const paneState = useSessions((s) => s.paneState);
  const addTab = useSessions((s) => s.addTab);
  const removeTab = useSessions((s) => s.removeTab);
  const setState = useSessions((s) => s.setState);
  const setLogging = useSessions((s) => s.setLogging);
  const addPane = useSessions((s) => s.addPane);
  const removePane = useSessions((s) => s.removePane);
  const setOrientation = useSessions((s) => s.setOrientation);
  const adjustSizes = useSessions((s) => s.adjustSizes);
  const setActivePane = useSessions((s) => s.setActivePane);

  const load = useLibrary((s) => s.load);
  const settings = useLibrary((s) => s.settings);

  const [busy, setBusy] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const closingRef = useRef<Set<string>>(new Set());
  const reconnectAttemptsRef = useRef<Map<string, number>>(new Map());
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [composerOpen, setComposerOpen] = useState(false);
  const [composerSeed, setComposerSeed] = useState<{ text: string; nonce: number } | null>(null);
  const [editor, setEditor] = useState<{
    session: SavedSession | null;
    defaultFolderId: string | null;
    prefill?: {
      name?: string;
      host?: string;
      port?: number;
      protocol?: "ssh" | "local" | "telnet" | "serial" | "sftp";
      username?: string | null;
      identityFile?: string | null;
      proxyJump?: string | null;
    } | null;
    linkTabId?: string;
  } | null>(null);
  const openedLastTabs = useRef(false);

  const activeTab = useMemo(() => tabs.find((t) => t.id === activeId), [tabs, activeId]);

  const backspaceMode: "del" | "bs" =
    settings?.terminal.backspace === "bs" ? "bs" : "del";
  const savedBackspace = useCallback(
    (saved: SavedSession): "del" | "bs" => {
      const value = (saved.settingsOverride as { backspace?: string } | null)?.backspace;
      if (value === "bs") return "bs";
      if (value === "del") return "del";
      return settings?.terminal.backspace === "bs" ? "bs" : "del";
    },
    [settings],
  );

  /** Runs the per-device "on connect" actions (pager, login script, autolog). */
  const runConnectActions = useCallback(
    (saved: SavedSession, sessionId: string) => {
      const ov = (saved.settingsOverride ?? {}) as DeviceOverrides;
      const steps: AutomationStep[] = [];
      const pager = ov.pager ?? "none";
      if (pager !== "none") {
        const command =
          pager === "custom" ? (ov.pagerCommand ?? "") : PAGER_PRESETS[pager];
        const prompt = findVendor(ov.vendor)?.prompt ?? DEFAULT_PROMPT;
        if (command) {
          steps.push({ type: "waitFor", sessionId, pattern: prompt, timeoutMs: 15000 });
          steps.push({ type: "send", sessionId, data: command, appendNewline: true });
          steps.push({ type: "sleep", ms: 200 });
        }
      }
      if (ov.loginScript?.trim()) steps.push(...parseExpect(ov.loginScript, sessionId));

      if (steps.length > 0) {
        void automationRun(steps)
          .then((results) => {
            const failed = results.find((r) => !r.ok);
            if (failed) setBanner(`Connect script: ${failed.detail}`);
          })
          .catch((e) => setBanner(`Connect script failed: ${String(e)}`));
      }

      if (ov.autolog) {
        const opts = settings ?? fallbackSettings();
        void logStart(sessionId, saved.name, opts.logging.mode, opts.logging.timestamps)
          .then((r) => setLogging(sessionId, true, r.path))
          .catch(() => {});
      }
    },
    [settings, setLogging],
  );

  const savedToSpec = useCallback((saved: SavedSession): SshSpec => ({
    host: saved.host,
    port: saved.port,
    username: saved.username,
    identityFile: saved.identityFile,
    proxyJump: saved.proxyJump,
  }), []);

  const openSavedSession = useCallback(
    async (savedId: string, orientation?: Orientation) => {
      const saved = useLibrary.getState().sessions.find((s) => s.id === savedId);
      if (!saved) return;
      setBanner(null);
      try {
        const id = await openSaved(saved.id, DEFAULT_COLS, DEFAULT_ROWS);
        const encoding = ((saved.settingsOverride ?? {}) as DeviceOverrides).encoding ?? "utf-8";
        registerEncoding(id, encoding);
        addTab({
          ...makeSavedTab(
            id,
            saved,
            saved.protocol === "ssh" ? savedToSpec(saved) : undefined,
          ),
          backspace: savedBackspace(saved),
          encoding,
        });
        if (orientation) setOrientation(id, orientation);
        runConnectActions(saved, id);
      } catch (error) {
        setBanner(`Failed to connect "${saved.name}": ${String(error)}`);
      }
    },
    [addTab, setOrientation, savedToSpec, savedBackspace, runConnectActions],
  );

  useEffect(() => {
    void load()
      .then(() => {
        const current = useLibrary.getState().settings;
        if (current?.general.reopenLastTabs && !openedLastTabs.current) {
          openedLastTabs.current = true;
          try {
            const ids: string[] = JSON.parse(localStorage.getItem(LAST_TABS_KEY) ?? "[]");
            for (const id of ids) void openSavedSession(id);
          } catch {
            /* ignore */
          }
        }
      })
      .catch((e) => setBanner(`Failed to load library: ${String(e)}`));
  }, [load, openSavedSession]);

  useEffect(() => {
    const savedIds = tabs.map((t) => t.savedId).filter(Boolean);
    try {
      localStorage.setItem(LAST_TABS_KEY, JSON.stringify(savedIds));
    } catch {
      /* ignore */
    }
  }, [tabs]);

  useEffect(() => {
    const theme = settings?.appearance.uiTheme ?? "dark";
    const apply = () => {
      document.documentElement.dataset.theme = isDarkUi(theme) ? "dark" : "light";
    };
    apply();
    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: light)");
      mq.addEventListener("change", apply);
      return () => mq.removeEventListener("change", apply);
    }
  }, [settings?.appearance.uiTheme]);

  useEffect(() => {
    void sshAvailable().then((ok) => {
      if (!ok) {
        setBanner(
          "OpenSSH client not found. Install it with: sudo apt install openssh-client",
        );
      }
    });
  }, []);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    (async () => {
      const handleOutput = await listen<TerminalOutputEvent>("terminal-output", (event) => {
          const { sessionId, data } = event.payload;
          const bytes = decodeBase64(data);
          const value = decodeOutput(sessionId, bytes);
          const term = getTerminal(sessionId);
          if (term) term.write(value);
          else bufferOutput(sessionId, value);
      });
      const handleState = await listen<ConnectionStateEvent>("connection-state", (event) => {
        const { sessionId, state, message } = event.payload;
          setState(sessionId, state, message ?? undefined);
          if (state === "connected") {
            reconnectAttemptsRef.current.delete(sessionId);
          }
          if (state === "disconnected" || state === "closed") {
            getTerminal(sessionId)?.write(`\r\n\x1b[33m[connection ${state}]\x1b[0m\r\n`);
            const auto = settingsRef.current?.general.autoReconnect ?? false;
            const userClosed = closingRef.current.has(sessionId);
            const attempts = reconnectAttemptsRef.current.get(sessionId) ?? 0;
            if (state === "disconnected" && auto && !userClosed && attempts < 3) {
              reconnectAttemptsRef.current.set(sessionId, attempts + 1);
              getTerminal(sessionId)?.write(
                `\r\n\x1b[90m[auto-reconnect ${attempts + 1}/3 in 3s]\x1b[0m\r\n`,
              );
              window.setTimeout(() => {
                void connectionReconnect(sessionId, DEFAULT_COLS, DEFAULT_ROWS).catch(() => {});
              }, 3000);
            }
          }
      });
      const handleExit = await listen<TerminalExitEvent>("terminal-exit", (event) => {
        getTerminal(event.payload.sessionId)?.write(
          `\r\n\x1b[90m[${event.payload.reason}]\x1b[0m\r\n`,
        );
      });
      if (disposed) {
        handleOutput();
        handleState();
        handleExit();
      } else {
        unlisteners.push(handleOutput, handleState, handleExit);
      }
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, [setState]);

  const handleConnect = useCallback(
    async (spec: SshSpec, label: string) => {
      setBusy(true);
      setBanner(null);
      try {
        const id = await openSsh(spec, DEFAULT_COLS, DEFAULT_ROWS);
        registerEncoding(id, "utf-8");
        addTab({ ...makeSshTab(id, label, spec.host, spec), backspace: backspaceMode });
      } catch (error) {
        setBanner(`Failed to open session: ${String(error)}`);
      } finally {
        setBusy(false);
      }
    },
    [addTab],
  );

  const handleLocalShell = useCallback(async () => {
    setBusy(true);
    setBanner(null);
    try {
      const id = await openLocal(DEFAULT_COLS, DEFAULT_ROWS);
      registerEncoding(id, "utf-8");
      addTab({ ...makeLocalTab(id), backspace: "del" });
    } catch (error) {
      setBanner(`Failed to open local shell: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [addTab]);

  const handleClose = useCallback(
    async (tabId: string) => {
      const tab = tabs.find((t) => t.id === tabId);
      if (!tab) return;
      const confirm = settings?.general.confirmClose ?? true;
      if (
        confirm &&
        (tab.state === "connected" || tab.state === "connecting") &&
        !window.confirm(`Close "${tab.title}"?`)
      ) {
        return;
      }
      removeTab(tabId);
      for (const pane of tab.panes) {
        closingRef.current.add(pane);
        clearState(pane);
        try {
          await connectionClose(pane);
        } catch {
          /* already gone */
        }
      }
    },
    [removeTab, tabs, settings],
  );

  const closePane = useCallback(
    async (tabId: string, sessionId: string) => {
      const tab = tabs.find((t) => t.id === tabId);
      if (tab && tab.panes.length <= 1) {
        void handleClose(tabId);
        return;
      }
      removePane(tabId, sessionId);
      clearState(sessionId);
      closingRef.current.add(sessionId);
      try {
        await connectionClose(sessionId);
      } catch {
        /* already gone */
      }
    },
    [tabs, removePane, handleClose],
  );

  const restoreWorkspace = useCallback(
    async (layout: WorkspaceLayout) => {
      for (const tabLayout of layout.tabs) {
        const descriptors =
          tabLayout.panes.length > 0 ? tabLayout.panes : [{ kind: "local" as const }];
        const first = descriptors[0];
        let primaryId: string;
        let base;
        try {
          if (first.kind === "saved" && first.savedId) {
            const saved = useLibrary
              .getState()
              .sessions.find((s) => s.id === first.savedId);
            if (!saved) continue;
            primaryId = await openSaved(saved.id, DEFAULT_COLS, DEFAULT_ROWS);
            base = makeSavedTab(
              primaryId,
              saved,
              saved.protocol === "ssh" ? savedToSpec(saved) : undefined,
            );
          } else {
            primaryId = await openLocal(DEFAULT_COLS, DEFAULT_ROWS);
            base = makeLocalTab(primaryId);
          }
        } catch (error) {
          setBanner(`Workspace restore failed: ${String(error)}`);
          continue;
        }
        addTab({
          ...base,
          orientation: tabLayout.orientation,
          backspace:
            first.kind === "saved" && first.savedId
              ? savedBackspace(
                  useLibrary.getState().sessions.find((s) => s.id === first.savedId) ??
                    ({} as SavedSession),
                )
              : "del",
        });
        for (let i = 1; i < descriptors.length; i += 1) {
          try {
            const pid = await openLocal(DEFAULT_COLS, DEFAULT_ROWS);
            addPane(primaryId, pid, "Local Shell");
          } catch {
            /* skip pane that failed to open */
          }
        }
        setOrientation(primaryId, tabLayout.orientation);
      }
    },
    [addTab, addPane, setOrientation, savedToSpec, savedBackspace],
  );

  const duplicateActive = useCallback(async () => {
    const tab = activeTab;
    if (!tab) return;
    if (tab.savedId) {
      await openSavedSession(tab.savedId);
    } else if (tab.protocol === "ssh" && tab.sshSpec) {
      await handleConnect(tab.sshSpec, tab.title);
    } else if (tab.protocol === "local") {
      await handleLocalShell();
    }
  }, [activeTab, openSavedSession, handleConnect, handleLocalShell]);

  const splitActive = useCallback(
    async (orientation: Orientation) => {
      if (!activeId) return;
      try {
        const id = await openLocal(DEFAULT_COLS, DEFAULT_ROWS);
        addPane(activeId, id, "Local Shell");
        setOrientation(activeId, orientation);
        setActivePane(activeId, id);
      } catch (error) {
        setBanner(`Split failed: ${String(error)}`);
      }
    },
    [activeId, addPane, setOrientation, setActivePane],
  );

  const handleReconnect = useCallback(
    async (id?: string) => {
      const target = id ?? activeId;
      if (!target) return;
      try {
        await connectionReconnect(target, DEFAULT_COLS, DEFAULT_ROWS);
      } catch (error) {
        setBanner(`Reconnect failed: ${String(error)}`);
      }
    },
    [activeId],
  );

  const handleOpenSftp = useCallback(
    async (savedId: string) => {
      const saved = useLibrary.getState().sessions.find((s) => s.id === savedId);
      if (!saved) return;
      try {
        const id = await openSftp(savedId, DEFAULT_COLS, DEFAULT_ROWS);
        addTab({
          ...makeSavedTab(id, saved),
          protocol: "sftp",
          title: `(sftp) ${saved.name}`,
        });
        setToolsOpen(false);
      } catch (error) {
        setBanner(`SFTP failed: ${String(error)}`);
      }
    },
    [addTab],
  );

  const toggleLogging = useCallback(async () => {
    if (!activeTab) return;
    const opts = settings ?? fallbackSettings();
    try {
      if (activeTab.logging) {
        await logStop(activeTab.id);
        setLogging(activeTab.id, false);
      } else {
        const result = await logStart(activeTab.id, activeTab.title, opts.logging.mode, opts.logging.timestamps);
        setLogging(activeTab.id, true, result.path);
        setBanner(`Logging to ${result.path}`);
      }
    } catch (error) {
      setBanner(`Logging error: ${String(error)}`);
    }
  }, [activeTab, settings, setLogging]);

  const handleUseCommand = useCallback(
    (command: Command, mode: "insert" | "send") => {
      if (mode === "insert") {
        setComposerSeed({ text: command.content, nonce: Date.now() });
        setComposerOpen(true);
        return;
      }
      if (!activeId) {
        setBanner("No active session to send to.");
        return;
      }
      void commandDispatch([activeId], command.content, true).then((res) => {
        const r = res[0];
        setBanner(r?.ok ? `Sent "${command.name}"` : `Send failed: ${r?.error ?? "unknown error"}`);
      });
    },
    [activeId],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const ctrlShift = event.ctrlKey && event.shiftKey;
      if (event.altKey && event.code === "KeyG") {
        event.preventDefault();
        event.stopPropagation();
        setComposerOpen((v) => !v);
        return;
      }
      if (ctrlShift && event.code === "KeyR") {
        event.preventDefault();
        void handleReconnect();
      } else if (ctrlShift && event.code === "KeyT") {
        event.preventDefault();
        void handleLocalShell();
      } else if (ctrlShift && event.code === "KeyW") {
        event.preventDefault();
        if (activeId) void handleClose(activeId);
      } else if (ctrlShift && event.code === "KeyL") {
        event.preventDefault();
        void toggleLogging();
      } else if (ctrlShift && event.code === "KeyN") {
        event.preventDefault();
        setEditor({ session: null, defaultFolderId: null });
      } else if (ctrlShift && event.code === "Backslash") {
        event.preventDefault();
        void splitActive("row");
      } else if (ctrlShift && event.code === "Minus") {
        event.preventDefault();
        void splitActive("column");
      } else if (event.ctrlKey && event.code === "KeyK") {
        event.preventDefault();
        setSidebarOpen(true);
        requestAnimationFrame(() => {
          document.querySelector<HTMLInputElement>('[data-testid="session-filter"]')?.focus();
        });
      } else if (event.ctrlKey && !event.shiftKey && event.code === "KeyF") {
        if (activeId) {
          event.preventDefault();
          setSearchOpen(true);
        }
      } else if (event.ctrlKey && event.code === "Comma") {
        event.preventDefault();
        setSettingsOpen(true);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [activeId, handleClose, handleLocalShell, handleReconnect, toggleLogging, splitActive]);

  return (
    <div className="flex h-screen w-screen flex-col bg-[#111418] text-[#E5E9EF]">
      <header className="flex items-center gap-2 border-b border-[#2B323A] bg-[#15181C] px-3 py-1.5">
        <button
          type="button"
          title="Toggle sidebar"
          aria-label="Toggle sidebar"
          aria-expanded={sidebarOpen}
          onClick={() => setSidebarOpen((v) => !v)}
          className="rounded px-1.5 text-[#99A3AE] hover:bg-[#2B323A] hover:text-[#E5E9EF]"
        >
          ☰
        </button>
        <span className="text-sm font-semibold tracking-wide">OpsTerm</span>
        <button
          type="button"
          onClick={() => void splitActive("row")}
          disabled={!activeId}
          data-testid="split-right"
          title="Split right (Ctrl+Shift+\\)"
          className="rounded border border-[#2B323A] px-2 py-0.5 text-xs text-[#C7CDD6] hover:bg-[#1D2228] disabled:opacity-40"
        >
          Split ⇢
        </button>
        <button
          type="button"
          onClick={() => void duplicateActive()}
          disabled={!activeId}
          data-testid="duplicate-tab"
          title="Duplicate tab"
          className="rounded border border-[#2B323A] px-2 py-0.5 text-xs text-[#C7CDD6] hover:bg-[#1D2228] disabled:opacity-40"
        >
          Duplicate
        </button>
        <button
          type="button"
          onClick={() => void splitActive("column")}
          disabled={!activeId}
          title="Split down (Ctrl+Shift+-)"
          className="rounded border border-[#2B323A] px-2 py-0.5 text-xs text-[#C7CDD6] hover:bg-[#1D2228] disabled:opacity-40"
        >
          Split ⇣
        </button>
        <div className="ml-auto flex items-center gap-2">
          <WorkspacesControl onRestore={(layout) => void restoreWorkspace(layout)} />
          <button
            type="button"
            onClick={() => setToolsOpen(true)}
            data-testid="open-tools"
            className="rounded border border-[#2B323A] px-2 py-0.5 text-xs text-[#C7CDD6] hover:bg-[#1D2228]"
          >
            Tools
          </button>
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            data-testid="open-settings"
            className="rounded border border-[#2B323A] px-2 py-0.5 text-xs text-[#C7CDD6] hover:bg-[#1D2228]"
          >
            Settings
          </button>
        </div>
      </header>

      <QuickConnectBar
        onConnect={handleConnect}
        onLocalShell={handleLocalShell}
        onSaveAsSession={() => {
          const spec = activeTab?.sshSpec ?? null;
          setEditor({
            session: null,
            defaultFolderId: null,
            linkTabId:
              activeTab && activeTab.protocol === "ssh" ? activeTab.id : undefined,
            prefill: spec
              ? {
                  name: spec.host,
                  host: spec.host,
                  port: spec.port,
                  protocol: "ssh",
                  username: spec.username ?? "",
                  identityFile: spec.identityFile ?? "",
                  proxyJump: spec.proxyJump ?? "",
                }
              : activeTab?.host
                ? {
                    name: activeTab.host,
                    host: activeTab.host,
                    protocol: activeTab.protocol,
                    port: 22,
                  }
                : null,
          });
        }}
        busy={busy}
      />

      <ButtonBar onUse={handleUseCommand} />

      {banner && (
        <div
          role="alert"
          className="flex items-center gap-2 border-b border-[#2B323A] bg-[#3a2a12] px-3 py-1 text-xs text-amber-300"
        >
          <span className="flex-1">{banner}</span>
          <button type="button" onClick={() => setBanner(null)} className="rounded px-1 hover:bg-black/20">
            ✕
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {sidebarOpen && (
          <Sidebar
            onConnect={(saved) => void openSavedSession(saved.id)}
            onEdit={(saved) => setEditor({ session: saved, defaultFolderId: saved.folderId })}
            onNewSession={(folderId) => setEditor({ session: null, defaultFolderId: folderId })}
            onNewFolder={() => {
              const name = window.prompt("Folder name");
              if (name) void useLibrary.getState().saveFolder({ name });
            }}
            onFocus={(id) => useSessions.getState().setActive(id)}
            onCloseActive={handleClose}
            onReconnectActive={(id) => void handleReconnect(id)}
            onUseCommand={handleUseCommand}
          />
        )}

        <div className="flex min-w-0 flex-1 flex-col">
          <TabBar onClose={handleClose} />
          <main className="relative flex-1 overflow-hidden bg-[#111418]">
            {tabs.length === 0 && (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-[#5C6370]">
                <p>No open sessions.</p>
                <p>
                  Use Quick Connect above, pick a saved session, or open a{" "}
                  <button type="button" onClick={handleLocalShell} className="text-[#4F8EE8] underline">
                    local shell
                  </button>
                  .
                </p>
              </div>
            )}
            {tabs.map((tab) => {
              const active = tab.id === activeId;
              return (
                <div
                  key={tab.id}
                  data-testid="pane"
                  data-active={active}
                  data-session-id={tab.id}
                  className="absolute inset-0 p-1"
                  style={{ visibility: active ? "visible" : "hidden", zIndex: active ? 1 : 0 }}
                  aria-hidden={!active}
                >
                  {active && (
                  <div
                    className={`flex h-full w-full ${
                      tab.orientation === "row" ? "flex-row" : "flex-col"
                    }`}
                  >
                    {tab.panes.map((paneId, paneIndex) => {
                      const paneActive = active && tab.activePane === paneId;
                      const weights =
                        tab.sizes && tab.sizes.length === tab.panes.length
                          ? tab.sizes
                          : tab.panes.map(() => 1 / tab.panes.length);
                      return (
                        <Fragment key={paneId}>
                          {paneIndex > 0 && (
                            <PaneDivider
                              orientation={tab.orientation}
                              onDrag={(fraction) =>
                                adjustSizes(tab.id, paneIndex - 1, fraction)
                              }
                            />
                          )}
                        <div
                          data-testid="split-pane"
                          data-pane-state={paneState[paneId] ?? "connecting"}
                          onMouseDown={() => setActivePane(tab.id, paneId)}
                          style={{ flexGrow: weights[paneIndex], flexBasis: 0 }}
                          className={`flex min-h-0 min-w-0 flex-col overflow-hidden rounded border ${
                            paneActive ? "border-[#4F8EE8]" : "border-[#2B323A]"
                          }`}
                        >
                          {tab.panes.length > 1 && (
                            <div className="flex items-center gap-2 border-b border-[#2B323A] bg-[#15181C] px-1.5 py-0.5 text-[10px] text-[#99A3AE]">
                              <span
                                className={`inline-block h-1.5 w-1.5 rounded-full ${
                                  paneState[paneId] === "connected"
                                    ? "bg-emerald-400"
                                    : paneState[paneId] === "connecting"
                                      ? "bg-amber-400"
                                      : "bg-zinc-500"
                                }`}
                              />
                              <span className="flex-1 truncate">
                                {tab.paneTitles[paneId] ?? paneId}
                              </span>
                              <button
                                type="button"
                                aria-label="Close pane"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  void closePane(tab.id, paneId);
                                }}
                                className="rounded px-1 hover:bg-[#2B323A] hover:text-[#E5E9EF]"
                              >
                                ✕
                              </button>
                            </div>
                          )}
                          <div className="relative min-h-0 flex-1">
                            <TerminalPane
                              sessionId={paneId}
                              active={paneActive}
                              backspace={tab.backspace ?? backspaceMode}
                            />
                          </div>
                        </div>
                        </Fragment>
                      );
                    })}
                  </div>
                  )}
                </div>
              );
            })}
            {searchOpen && activeId && (
              <TerminalSearchBar sessionId={activeTab?.activePane ?? activeId} onClose={() => setSearchOpen(false)} />
            )}
          </main>
        </div>
      </div>

      <CommandComposer open={composerOpen} seed={composerSeed} onClose={() => setComposerOpen(false)} />

      <StatusBar
        onToggleLog={() => void toggleLogging()}
        onSerialBreak={() => {
          if (activeId) void serialBreak(activeTab?.activePane ?? activeId).catch(() => {});
        }}
      />

      {editor && (
        <SessionEditor
          session={editor.session}
          defaultFolderId={editor.defaultFolderId}
          prefill={editor.prefill ?? null}
          onClose={() => setEditor(null)}
          onSaved={(saved) => {
            if (editor.linkTabId) {
              useSessions.getState().setSavedId(editor.linkTabId, saved.id, saved.name);
            }
            setEditor(null);
            setBanner(`Saved "${saved.name}" to device inventory`);
          }}
        />
      )}

      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      {toolsOpen && <ToolsDrawer onClose={() => setToolsOpen(false)} onOpenSftp={(id) => void handleOpenSftp(id)} />}
    </div>
  );
}
