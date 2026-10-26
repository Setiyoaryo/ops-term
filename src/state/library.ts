import { create } from "zustand";

import * as ipc from "../lib/ipc";
import type {
  AppSettings,
  Command,
  CommandGroup,
  CommandInput,
  Folder,
  FolderInput,
  SavedSession,
  SavedSessionInput,
} from "../types";

interface LibraryState {
  folders: Folder[];
  sessions: SavedSession[];
  commandGroups: CommandGroup[];
  commands: Command[];
  settings: AppSettings | null;
  loaded: boolean;
  load: () => Promise<void>;
  reloadSessions: () => Promise<void>;
  reloadFolders: () => Promise<void>;
  reloadCommands: () => Promise<void>;
  saveSession: (input: SavedSessionInput) => Promise<SavedSession>;
  deleteSession: (id: string) => Promise<void>;
  saveFolder: (input: FolderInput) => Promise<Folder>;
  deleteFolder: (id: string) => Promise<void>;
  saveCommand: (input: CommandInput) => Promise<Command>;
  deleteCommand: (id: string) => Promise<void>;
  saveCommandGroup: (input: {
    id?: string | null;
    parentId?: string | null;
    name: string;
  }) => Promise<CommandGroup>;
  deleteCommandGroup: (id: string) => Promise<void>;
  updateSettings: (settings: AppSettings) => Promise<void>;
}

export const useLibrary = create<LibraryState>((set, get) => ({
  folders: [],
  sessions: [],
  commandGroups: [],
  commands: [],
  settings: null,
  loaded: false,

  load: async () => {
    const [folders, sessions, settings, commandGroups, commands] =
      await Promise.all([
        ipc.foldersList(),
        ipc.sessionsList(),
        ipc.settingsGet(),
        ipc.commandGroupsList(),
        ipc.commandsList(),
      ]);
    set({ folders, sessions, settings, commandGroups, commands, loaded: true });
  },

  reloadSessions: async () => {
    set({ sessions: await ipc.sessionsList() });
  },

  reloadFolders: async () => {
    set({ folders: await ipc.foldersList() });
  },

  reloadCommands: async () => {
    const [commandGroups, commands] = await Promise.all([
      ipc.commandGroupsList(),
      ipc.commandsList(),
    ]);
    set({ commandGroups, commands });
  },

  saveSession: async (input) => {
    const saved = await ipc.sessionSave(input);
    await get().reloadSessions();
    return saved;
  },

  deleteSession: async (id) => {
    await ipc.sessionDelete(id);
    await get().reloadSessions();
  },

  saveFolder: async (input) => {
    const folder = await ipc.folderSave(input);
    await get().reloadFolders();
    return folder;
  },

  deleteFolder: async (id) => {
    await ipc.folderDelete(id);
    await Promise.all([get().reloadFolders(), get().reloadSessions()]);
  },

  saveCommand: async (input) => {
    const command = await ipc.commandSave(input);
    await get().reloadCommands();
    return command;
  },

  deleteCommand: async (id) => {
    await ipc.commandDelete(id);
    await get().reloadCommands();
  },

  saveCommandGroup: async (input) => {
    const group = await ipc.commandGroupSave(input);
    await get().reloadCommands();
    return group;
  },

  deleteCommandGroup: async (id) => {
    await ipc.commandGroupDelete(id);
    await get().reloadCommands();
  },

  updateSettings: async (settings) => {
    const saved = await ipc.settingsSet(settings);
    set({ settings: saved });
  },
}));

/** Default settings used before the backend responds. */
export function fallbackSettings(): AppSettings {
  return {
    general: { confirmClose: true, defaultProtocol: "ssh", reopenLastTabs: false, autoReconnect: false },
    terminal: {
      fontFamily:
        '"JetBrains Mono", "DejaVu Sans Mono", "Ubuntu Mono", monospace',
      fontSize: 13,
      scrollback: 5000,
      colorScheme: "default-dark",
      cursorStyle: "block",
      cursorBlink: true,
      backspace: "del",
      copyOnSelect: false,
    },
    ssh: {
      connectTimeoutSecs: 20,
      serverAliveIntervalSecs: 60,
      term: "xterm-256color",
      identityFile: null,
      compression: false,
      forwardAgent: false,
      x11Forwarding: false,
      certificateFile: null,
      pkcs11Provider: null,
      engine: "openssh",
    },
    logging: {
      enabledByDefault: false,
      logDir: null,
      mode: "raw",
      timestamps: false,
    },
    appearance: { uiTheme: "dark", density: "compact" },
  };
}
