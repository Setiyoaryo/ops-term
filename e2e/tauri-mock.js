/*
 * Browser-side Tauri backend simulator for Playwright E2E tests.
 *
 * This is injected before the app bundle loads (page.addInitScript) and
 * implements the same `window.__TAURI_INTERNALS__` contract that the real
 * Tauri WebView provides, including the event-plugin protocol. It then answers
 * the OpsTerm commands with a deterministic, simulated PTY so the *real*
 * frontend (React + xterm.js + IPC wiring) can be exercised end-to-end in a
 * browser.
 *
 * Scope boundary: this validates the frontend and the IPC contract. The real
 * Rust providers are covered separately by `src-tauri/tests/*.rs`, which spawn
 * genuine PTYs and a genuine OpenSSH client. See e2e/README.md.
 */
(() => {
  "use strict";

  const callbacks = new Map();
  let nextCallbackId = 1;
  const listeners = new Map(); // event name -> Set<callback id>
  const sessions = new Map();
  let nextSession = 1;

  // Minimal in-memory library so the real frontend's persistence calls succeed
  // in the browser test harness instead of surfacing an error banner.
  const library = {
    folders: [],
    sessions: [],
    commands: [],
    commandGroups: [],
    tunnels: [],
    transfers: [],
    credentials: {},
    credentialMeta: {},
    runbooks: [],
    workspaces: [],
    settings: {
      general: { confirmClose: true, defaultProtocol: "ssh", reopenLastTabs: false, autoReconnect: false },
      terminal: {
        fontFamily: "monospace",
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
    },
  };
  let nextId = 1;

  function registerCallback(callback, once = false) {
    const id = nextCallbackId++;
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      return callback && callback(data);
    });
    return id;
  }

  function unregisterCallback(id) {
    callbacks.delete(id);
  }

  function runCallback(id, data) {
    const cb = callbacks.get(id);
    if (cb) cb(data);
  }

  function unregisterListener(_event, id) {
    unregisterCallback(id);
  }

  function emit(event, payload) {
    const set = listeners.get(event);
    if (!set) return;
    for (const id of Array.from(set)) {
      runCallback(id, { event, id: nextCallbackId++, payload });
    }
  }

  function base64(text) {
    return btoa(unescape(encodeURIComponent(text)));
  }

  function emitOutput(session, text) {
    emit("terminal-output", {
      sessionId: session.id,
      seq: session.seq++,
      data: base64(text),
    });
  }

  function handleLine(session, line) {
    const cmd = line.trim();
    if (cmd === "") {
      emitOutput(session, "$ ");
      return;
    }
    if (cmd === "exit" || cmd === "logout") {
      emitOutput(session, "logout\r\n");
      emit("terminal-exit", { sessionId: session.id, reason: "process exited" });
      emit("connection-state", {
        sessionId: session.id,
        state: "disconnected",
        message: null,
      });
      sessions.delete(session.id);
      return;
    }
    if (cmd.startsWith("echo ")) {
      emitOutput(session, cmd.slice(5) + "\r\n$ ");
      return;
    }
    if (cmd === "uname -a") {
      emitOutput(session, "Linux lab-node 6.8.0-opsterm #1 SMP x86_64 GNU/Linux\r\n$ ");
      return;
    }
    if (cmd === "stty size") {
      emitOutput(session, `${session.rows} ${session.cols}\r\n$ `);
      return;
    }
    emitOutput(session, `bash: ${cmd}: command not found\r\n$ `);
  }

  function onWrite(session, data) {
    for (const ch of data) {
      if (ch === "\r") {
        emitOutput(session, "\r\n");
        handleLine(session, session.line);
        session.line = "";
      } else if (ch === "\n") {
        // Network CLIs (Huawei/Cisco) execute on CR only; a bare LF is ignored
        // and must not merge lines. Modelled here to catch paste regressions.
      } else if (ch === "\x7f" || ch === "\b") {
        if (session.line.length > 0) {
          session.line = session.line.slice(0, -1);
          emitOutput(session, "\b \b");
        }
      } else if (ch === "\x03") {
        session.line = "";
        emitOutput(session, "^C\r\n$ ");
      } else if (ch >= " ") {
        session.line += ch;
        emitOutput(session, ch);
      }
    }
  }

  function openSession(type, spec) {
    const id = `sess-${nextSession++}`;
    const session = {
      id,
      type,
      line: "",
      seq: 0,
      cols: spec.cols || 80,
      rows: spec.rows || 24,
      host: (spec.spec && (spec.spec.host || spec.spec.device)) || null,
    };
    sessions.set(id, session);
    // Emit connected after the tab has had a chance to mount.
    setTimeout(() => {
      emit("connection-state", {
        sessionId: id,
        state: "connected",
        message: null,
      });
      if (type === "local") {
        emitOutput(session, "OpsTerm local shell (simulated PTY)\r\n$ ");
      } else if (type === "serial") {
        emitOutput(session, `Serial console ${session.host} ready\r\n`);
      } else {
        emitOutput(
          session,
          `Connected to ${session.host}\r\n${session.host}> `,
        );
      }
    }, 15);
    return id;
  }

  async function invoke(cmd, args = {}) {
    switch (cmd) {
      case "plugin:event|listen": {
        if (!listeners.has(args.event)) listeners.set(args.event, new Set());
        listeners.get(args.event).add(args.handler);
        return args.handler;
      }
      case "plugin:event|unlisten": {
        const set = listeners.get(args.event);
        if (set) set.delete(args.eventId);
        return null;
      }
      case "plugin:event|emit": {
        emit(args.event, args.payload);
        return null;
      }
      case "ssh_available":
        return true;
      case "serial_devices_list":
        return ["/dev/ttyUSB0", "/dev/ttyUSB1"];
      case "connection_open_telnet":
        return openSession("telnet", args);
      case "connection_open_serial":
        return openSession("serial", args);
      case "connection_serial_break":
        return null;
      case "session_serial_break":
        return null;
      case "connection_open_sftp": {
        const saved = library.sessions.find((s) => s.id === args.sessionId);
        return openSession("sftp", { ...args, spec: { host: saved ? saved.host : "sftp" } });
      }
      case "tunnel_start":
        return `tunnel-${nextId++}`;
      case "tunnel_stop":
        return null;
      case "tunnel_list":
        return library.tunnels.slice();
      case "sftp_list":
        return [
          { name: "configs", path: `${args.path}/configs`, isDir: true, isLink: false, size: 0, permissions: "drwxr-xr-x", modified: "Jan 1 00:00" },
          { name: "startup.cfg", path: `${args.path}/startup.cfg`, isDir: false, isLink: false, size: 2048, permissions: "-rw-r--r--", modified: "Jan 1 00:00" },
        ];
      case "sftp_mkdir":
      case "sftp_remove":
      case "sftp_rename":
        return null;
      case "sftp_transfer_start":
        return `xfer-${nextId++}`;
      case "transfer_cancel":
        return null;
      case "transfers_list":
        return library.transfers.slice();
      case "session_wait_for":
        return { matched: true, text: args.pattern };
      case "automation_run": {
        const results = [];
        for (let i = 0; i < args.steps.length; i += 1) {
          const step = args.steps[i];
          if (step.type === "send") {
            const session = sessions.get(step.sessionId);
            if (session) onWrite(session, step.data + (step.appendNewline ? "\r" : ""));
            results.push({ index: i, kind: "send", ok: true, detail: "sent" });
          } else if (step.type === "waitFor") {
            results.push({ index: i, kind: "waitFor", ok: true, detail: `matched: ${step.pattern}` });
          } else if (step.type === "sleep") {
            results.push({ index: i, kind: "sleep", ok: true, detail: `slept ${step.ms}ms` });
          }
        }
        return results;
      }
      case "credential_set": {
        library.credentials[args.name] = args.secret;
        library.credentialMeta[args.name] = {
          name: args.name,
          username: args.username ?? null,
          kind: args.kind ?? "password",
          tags: args.tags ?? [],
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        return null;
      }
      case "credential_get":
        return library.credentials[args.name] ?? "";
      case "credential_delete": {
        delete library.credentials[args.name];
        delete library.credentialMeta[args.name];
        return null;
      }
      case "credentials_list":
        return Object.values(library.credentialMeta);
      case "workspace_save": {
        const input = args.input;
        const existing = input.id ? library.workspaces.find((w) => w.id === input.id) : null;
        if (existing) {
          Object.assign(existing, input);
          return existing;
        }
        const created = {
          id: `ws-${nextId++}`,
          name: input.name,
          description: input.description ?? null,
          layout: input.layout ?? {},
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        library.workspaces.push(created);
        return created;
      }
      case "workspace_delete": {
        library.workspaces = library.workspaces.filter((w) => w.id !== args.id);
        return null;
      }
      case "connection_open_local":
        return openSession("local", args);
      case "connection_open_ssh":
        return openSession("ssh", args);
      case "connection_write": {
        const session = sessions.get(args.sessionId);
        if (!session) throw new Error(`session not found: ${args.sessionId}`);
        onWrite(session, args.data);
        return null;
      }
      case "connection_resize": {
        const session = sessions.get(args.sessionId);
        if (session) {
          session.cols = args.cols;
          session.rows = args.rows;
        }
        return null;
      }
      case "connection_close": {
        const session = sessions.get(args.sessionId);
        if (session) sessions.delete(session.id);
        emit("connection-state", {
          sessionId: args.sessionId,
          state: "closed",
          message: null,
        });
        return null;
      }
      case "connection_open_saved": {
        const saved = library.sessions.find((s) => s.id === args.sessionId);
        if (!saved) throw new Error(`session not found: ${args.sessionId}`);
        const session = { spec: { host: saved.host } };
        const id = openSession(saved.protocol, {
          ...args,
          spec: session.spec,
        });
        return id;
      }
      case "connection_reconnect": {
        const session = sessions.get(args.sessionId);
        if (session) {
          session.line = "";
          emit("connection-state", {
            sessionId: session.id,
            state: "reconnecting",
            message: null,
          });
          setTimeout(() => {
            emit("connection-state", {
              sessionId: session.id,
              state: "connected",
              message: null,
            });
            emitOutput(session, "$ ");
          }, 15);
        }
        return null;
      }
      case "sessions_list":
        return library.sessions.slice();
      case "session_get": {
        const found = library.sessions.find((s) => s.id === args.id);
        if (!found) throw new Error(`session not found: ${args.id}`);
        return found;
      }
      case "session_save": {
        const input = args.input;
        const now = new Date().toISOString();
        const existing = input.id
          ? library.sessions.find((s) => s.id === input.id)
          : null;
        if (existing) {
          Object.assign(existing, input, { updatedAt: now });
          return existing;
        }
        const created = {
          id: `saved-${nextId++}`,
          folderId: input.folderId ?? null,
          name: input.name,
          host: input.host,
          port: input.port,
          protocol: input.protocol,
          username: input.username ?? null,
          sshAlias: input.sshAlias ?? null,
          identityFile: input.identityFile ?? null,
          proxyJump: input.proxyJump ?? null,
          tags: input.tags ?? [],
          description: input.description ?? null,
          notes: input.notes ?? null,
          settingsOverride: input.settingsOverride ?? null,
          credentialRef: input.credentialRef ?? null,
          isFavorite: input.isFavorite ?? false,
          sortOrder: input.sortOrder ?? 0,
          createdAt: now,
          updatedAt: now,
        };
        library.sessions.push(created);
        return created;
      }
      case "session_delete": {
        library.sessions = library.sessions.filter((s) => s.id !== args.id);
        return null;
      }
      case "folders_list":
        return library.folders.slice();
      case "folder_save": {
        const input = args.input;
        const existing = input.id
          ? library.folders.find((f) => f.id === input.id)
          : null;
        if (existing) {
          Object.assign(existing, input);
          return existing;
        }
        const created = {
          id: `folder-${nextId++}`,
          parentId: input.parentId ?? null,
          name: input.name,
          sortOrder: input.sortOrder ?? 0,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        library.folders.push(created);
        return created;
      }
      case "folder_delete": {
        library.folders = library.folders.filter((f) => f.id !== args.id);
        return null;
      }
      case "command_dispatch": {
        return args.sessionIds.map((id) => {
          const session = sessions.get(id);
          if (!session) {
            return { sessionId: id, ok: false, error: "session not found" };
          }
          onWrite(session, args.content + (args.appendEnter ? "\r" : ""));
          return { sessionId: id, ok: true, error: null };
        });
      }
      case "commands_list":
        return library.commands.slice();
      case "command_save": {
        const input = args.input;
        const existing = input.id
          ? library.commands.find((c) => c.id === input.id)
          : null;
        if (existing) {
          Object.assign(existing, input);
          return existing;
        }
        const created = {
          id: `cmd-${nextId++}`,
          groupId: input.groupId ?? null,
          name: input.name,
          content: input.content,
          vendorProfile: input.vendorProfile ?? null,
          executeMode: input.executeMode ?? "insert",
          riskLevel: input.riskLevel ?? "low",
          variables: input.variables ?? {},
          tags: input.tags ?? [],
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        library.commands.push(created);
        return created;
      }
      case "command_delete": {
        library.commands = library.commands.filter((c) => c.id !== args.id);
        return null;
      }
      case "runbooks_list":
        return library.runbooks.slice();
      case "runbook_save": {
        const input = args.input;
        const existing = input.id ? library.runbooks.find((r) => r.id === input.id) : null;
        if (existing) {
          Object.assign(existing, input);
          return existing;
        }
        const created = {
          id: `rb-${nextId++}`,
          name: input.name,
          description: input.description ?? null,
          script: input.script ?? "",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        library.runbooks.push(created);
        return created;
      }
      case "runbook_delete": {
        library.runbooks = library.runbooks.filter((r) => r.id !== args.id);
        return null;
      }
      case "command_groups_list":
        return library.commandGroups.slice();
      case "command_group_save": {
        const input = args.input;
        const existing = input.id
          ? library.commandGroups.find((g) => g.id === input.id)
          : null;
        if (existing) {
          Object.assign(existing, input);
          return existing;
        }
        const created = {
          id: `cmdgrp-${nextId++}`,
          parentId: input.parentId ?? null,
          name: input.name,
          sortOrder: input.sortOrder ?? 0,
        };
        library.commandGroups.push(created);
        return created;
      }
      case "command_group_delete": {
        library.commandGroups = library.commandGroups.filter(
          (g) => g.id !== args.id,
        );
        return null;
      }
      case "workspaces_list":
        return library.workspaces.slice();
      case "logs_list":
        return [
          {
            id: "log-1",
            sessionId: null,
            label: "SW-01",
            filePath: "/tmp/opsterm/SW-01_2026-01-01.log",
            startedAt: new Date().toISOString(),
            endedAt: null,
            sizeBytes: 1234,
          },
        ];
      case "log_read":
        return "log line 1\nlog line 2\n";
      case "recent_list":
        return [];
      case "settings_get":
        return library.settings;
      case "settings_set":
        library.settings = args.settings;
        return args.settings;
      case "log_start":
        return { id: `log-${nextId++}`, path: `/tmp/${args.sessionId}.log` };
      case "log_stop":
        return null;
      case "log_status":
        return false;
      default:
        throw new Error(`unmocked command: ${cmd}`);
    }
  }

  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ =
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};
  window.__TAURI_INTERNALS__.invoke = invoke;
  window.__TAURI_INTERNALS__.transformCallback = registerCallback;
  window.__TAURI_INTERNALS__.unregisterCallback = unregisterCallback;
  window.__TAURI_INTERNALS__.runCallback = runCallback;
  window.__TAURI_INTERNALS__.callbacks = callbacks;
  window.__TAURI_INTERNALS__.convertFileSrc = (p) => p;
  window.__TAURI_INTERNALS__.metadata = {
    currentWindow: { label: "main" },
    currentWebview: { windowLabel: "main", label: "main" },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = unregisterListener;
})();
