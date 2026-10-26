# Development plan

Phases 0–5 plus the polish and M4 advanced-parity items are implemented
(Telnet, serial, splits, workspaces, Command Manager/Composer, import/export,
SFTP, tunnels, keyring, automation, light/dark chrome, drag-and-drop,
auto-decoration, SSH compression/X11/agent forwarding, certificates/PKCS#11,
and a native `russh` SSH engine behind the same provider seam with strict
`known_hosts` verification). Remaining work is a full accessibility audit and
the lab/GUI exit gates.

| Phase | Goal | Exit gate |
|---|---|---|
| 0 ✅ | Foundation, ADRs, PTY spike | `cargo test` opens/reads/writes/resizes/closes a PTY |
| 1 ✅ | Terminal engine, real SSH, tabs | AT-01, AT-03, AT-04, AT-05 on a real OpenSSH lab |
| 2 ✅ | SQLite sessions/folders, settings, logging, search | AT-02, AT-11, AT-12 |
| 3 ✅ | Splits, workspaces, Command Manager/Composer, import/export | AT-06–AT-09, AT-13 |
| 4 ✅ | Telnet + serial | AT-10 + negotiation tests |
| 5 ✅ | SFTP, tunnels, keyring, automation | threat-model + integration tests |

## Phase 2 — done

- SQLite migrations and `folders`/`sessions`/`command_groups`/`commands`/
  `workspaces`/`application_settings`/`log_index`/`recent_connections` models
  (`src-tauri/src/storage`), covered by `tests/storage.rs`.
- Saved Session Manager with nested folders, filter/search and CRUD.
- Settings + terminal color schemes + typography persisted.
- Logging with `0600` files, rotation, opt-in and a bounded index.
- Find-in-terminal (Ctrl+F) and a persisted search UI.

## Phase 2 — remaining

1. Apply `uiTheme` to the shell chrome (light mode), not just the terminal.
2. Reopen-last-tabs behaviour behind the existing setting.
3. Drag-and-drop reordering for folders/sessions.
4. `.deb` desktop smoke test and the M1 exit gate on a lab host.

## Phase 3 — in progress

Done: Command Manager, Button Bar, Command Composer with scoped broadcast and
arm/review, Active Sessions manager.

Remaining: recursive split panes, workspace snapshots, keyword/regex highlight
presets, CSV/TSV import/export and backups.
