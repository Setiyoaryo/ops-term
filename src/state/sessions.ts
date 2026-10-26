import { create } from "zustand";
import type { ConnectionState, Protocol, SessionTab, SshSpec } from "../types";

export type Orientation = "row" | "column";

interface SessionsState {
  tabs: SessionTab[];
  activeId: string | null;
  /** Live connection state for every pane (a tab may host several panes). */
  paneState: Record<string, ConnectionState>;
  addTab: (tab: SessionTab) => void;
  removeTab: (id: string) => void;
  setActive: (id: string) => void;
  setState: (id: string, state: ConnectionState, detail?: string) => void;
  renameTab: (id: string, title: string) => void;
  setLogging: (id: string, logging: boolean, logPath?: string | null) => void;
  addPane: (tabId: string, sessionId: string, title: string) => void;
  removePane: (tabId: string, sessionId: string) => void;
  setOrientation: (tabId: string, orientation: Orientation) => void;
  setActivePane: (tabId: string, sessionId: string) => void;
  setSavedId: (tabId: string, savedId: string, title?: string) => void;
  moveTab: (from: number, to: number) => void;
  adjustSizes: (tabId: string, index: number, delta: number) => void;
  replaceAll: (tabs: SessionTab[], activeId: string | null) => void;
}

export const useSessions = create<SessionsState>((set) => ({
  tabs: [],
  activeId: null,
  paneState: {},
  addTab: (tab) =>
    set((s) => ({
      tabs: [...s.tabs, tab],
      activeId: tab.id,
      paneState: { ...s.paneState, [tab.id]: tab.state },
    })),
  removeTab: (id) =>
    set((s) => {
      const tabs = s.tabs.filter((t) => t.id !== id);
      const activeId =
        s.activeId === id ? (tabs.length ? tabs[tabs.length - 1].id : null) : s.activeId;
      const paneState = { ...s.paneState };
      const removed = s.tabs.find((t) => t.id === id);
      removed?.panes.forEach((p) => delete paneState[p]);
      return { tabs, activeId, paneState };
    }),
  setActive: (id) => set({ activeId: id }),
  setState: (id, state, detail) =>
    set((s) => ({
      paneState: { ...s.paneState, [id]: state },
      tabs: s.tabs.map((t) =>
        t.id === id ? { ...t, state, detail } : t,
      ),
    })),
  renameTab: (id, title) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === id ? { ...t, title } : t)),
    })),
  setLogging: (id, logging, logPath) =>
    set((s) => ({
      tabs: s.tabs.map((t) =>
        t.id === id ? { ...t, logging, logPath: logPath ?? t.logPath } : t,
      ),
    })),
  addPane: (tabId, sessionId, title) =>
    set((s) => ({
      tabs: s.tabs.map((t) =>
        t.id === tabId
          ? {
              ...t,
              panes: [...t.panes, sessionId],
              activePane: sessionId,
              paneTitles: { ...t.paneTitles, [sessionId]: title },
            }
          : t,
      ),
      paneState: { ...s.paneState, [sessionId]: "connecting" },
    })),
  removePane: (tabId, sessionId) =>
    set((s) => {
      const paneState = { ...s.paneState };
      delete paneState[sessionId];
      return {
        paneState,
        tabs: s.tabs.map((t) => {
          if (t.id !== tabId) return t;
          const panes = t.panes.filter((p) => p !== sessionId);
          const paneTitles = { ...t.paneTitles };
          delete paneTitles[sessionId];
          const activePane =
            t.activePane === sessionId ? (panes[0] ?? t.activePane) : t.activePane;
          return { ...t, panes, paneTitles, activePane };
        }),
      };
    }),
  setOrientation: (tabId, orientation) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, orientation } : t)),
    })),
  setActivePane: (tabId, sessionId) =>
    set((s) => ({
      tabs: s.tabs.map((t) =>
        t.id === tabId ? { ...t, activePane: sessionId } : t,
      ),
    })),
  setSavedId: (tabId, savedId, title) =>
    set((s) => ({
      tabs: s.tabs.map((t) =>
        t.id === tabId ? { ...t, savedId, title: title ?? t.title } : t,
      ),
    })),
  moveTab: (from, to) =>
    set((s) => {
      if (from === to || from < 0 || to < 0 || from >= s.tabs.length || to >= s.tabs.length) {
        return {};
      }
      const tabs = [...s.tabs];
      const [moved] = tabs.splice(from, 1);
      tabs.splice(to, 0, moved);
      return { tabs };
    }),
  adjustSizes: (tabId, index, delta) =>
    set((s) => ({
      tabs: s.tabs.map((t) => {
        if (t.id !== tabId) return t;
        const n = t.panes.length;
        if (index < 0 || index + 1 >= n) return t;
        const sizes =
          t.sizes && t.sizes.length === n ? [...t.sizes] : new Array(n).fill(1 / n);
        sizes[index] = Math.max(0.1, sizes[index] + delta);
        sizes[index + 1] = Math.max(0.1, sizes[index + 1] - delta);
        return { ...t, sizes };
      }),
    })),
  replaceAll: (tabs, activeId) =>
    set({ tabs, activeId, paneState: {} }),
}));

function baseTab(
  id: string,
  title: string,
  protocol: Protocol,
  host?: string,
  savedId?: string | null,
  sshSpec?: SshSpec | null,
  backspace?: "del" | "bs",
): SessionTab {
  return {
    id,
    title,
    protocol,
    host,
    state: "connecting",
    savedId: savedId ?? null,
    panes: [id],
    paneTitles: { [id]: title },
    orientation: "row",
    activePane: id,
    sshSpec: sshSpec ?? null,
    backspace,
  };
}

export function makeLocalTab(id: string): SessionTab {
  return baseTab(id, "Local Shell", "local");
}

export function makeSshTab(
  id: string,
  title: string,
  host: string,
  spec?: SshSpec,
): SessionTab {
  return baseTab(id, title, "ssh", host, null, spec ?? null);
}

export function makeSavedTab(
  id: string,
  saved: { id: string; name: string; protocol: Protocol; host: string },
  spec?: SshSpec,
): SessionTab {
  return baseTab(id, saved.name, saved.protocol, saved.host, saved.id, spec ?? null);
}

export function makeLoggingTab(
  id: string,
  protocol: Protocol,
  title: string,
): SessionTab {
  return baseTab(id, title, protocol);
}
