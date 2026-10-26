# Architecture

OpsTerm is a Tauri v2 desktop application: a Rust backend that owns all
processes and I/O, and a React/TypeScript renderer that owns presentation and
input. The renderer is untrusted; every process, filesystem and network action
goes through typed, validated Rust commands.

## Layers

```text
┌──────────────────────────────────────────────────────────────────────┐
│ Renderer (React + TypeScript + xterm.js)                             │
│  • TerminalPane (xterm) • tabs • Quick Connect • status bar          │
│  • terminalRegistry routes base64 output to the right xterm instance │
└───────────────▲───────────────────────────┬──────────────────────────┘
                │ events                     │ invoke (typed IPC)
      terminal-output / connection-state      │ connection_open_ssh,
      terminal-exit                           │ connection_write, resize, close…
┌───────────────┴───────────────────────────▼──────────────────────────┐
│ Rust backend (Tauri)                                                  │
│  commands/            typed IPC surface, validation entrypoints      │
│  session_supervisor/  registry of live sessions, reader threads,     │
│                       ordered batched output, cleanup                │
│  providers/           ConnectionProvider trait                        │
│    ├── local_pty.rs    user shell under a PTY                         │
│    └── openssh_pty.rs  /usr/bin/ssh under a PTY                       │
│  security/            host/port/user/path/jump validation,            │
│                       structured ssh argv builder                     │
│  domain/              typed DTOs and state machine                   │
└──────────────────────────────────────────────────────────────────────┘
```

## Why a PTY-based OpenSSH subprocess (M1)

Spawning the installed `ssh` client under a pseudo-terminal inherits the
user's existing `~/.ssh/config`, `known_hosts`, agent keys, `ProxyJump` and
algorithm policy. A from-scratch SSH client would have to re-earn that
compatibility (GSSAPI, certificates, legacy vendor devices) before it could be
trusted. The `ConnectionProvider` trait keeps this replaceable so a native
Rust engine (e.g. `russh`) can be adopted later behind the same interface
without touching the UI. See [`docs/adr/0001`](adr/0001-connection-engine-openssh-pty.md).

## Data flow

1. User types in an xterm → `onData` → `connection_write(sessionId, data)`.
2. Backend validates the session id, writes bytes to the PTY.
3. A per-session reader thread reads up to 8 KiB chunks, base64-encodes them
   and emits `terminal-output { sessionId, seq, data }`.
4. The renderer decodes and routes each chunk to the matching xterm instance
   (buffering early output until the pane mounts).
5. Resize: xterm `FitAddon` → `connection_resize` → PTY `TIOCSWINSZ` →
   remote shell receives the new grid.

State is separated deliberately: provider handles, PTYs and terminal buffers
are **ephemeral** and never serialized; only metadata (Phase 2+) is persisted.

## Modules

| Path | Responsibility |
|---|---|
| `src-tauri/src/commands` | Tauri command handlers, the only IPC surface |
| `src-tauri/src/session_supervisor` | session registry, reader threads, events, cleanup |
| `src-tauri/src/providers` | `ConnectionProvider` trait, `PtySession`, SSH/local spawn |
| `src-tauri/src/security` | all validation + safe argv construction |
| `src-tauri/src/domain` | DTOs, protocol/state enums |
| `src/components/terminal` | xterm pane + output routing registry |
| `src/components/layout` | tabs, quick connect, status bar |
| `src/features/connections` | quick-connect parsing |
| `src/state` | ephemeral UI state (zustand) |

## Non-goals for M1

Splits/workspaces, saved sessions (SQLite), logging UI, Telnet, serial, SFTP,
keyring and scripting are tracked in [`FEATURE_STATUS.md`](FEATURE_STATUS.md)
and implemented in later phases per the PRD milestone gates.
