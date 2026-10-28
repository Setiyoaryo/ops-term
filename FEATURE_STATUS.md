# Feature Status

28 Oktober 2026 — v0.1.8: arah auto-scroll seleksi diperbaiki dan diuji dengan drag/hold/copy ke scrollback. Pencatatan otomatis tanpa isi terminal ditambahkan untuk menelusuri freeze acak; penyebab freeze belum terkonfirmasi. [Bukti dan batasan](docs/scroll-selection-and-stalls-2026-10-28.md).

27 Oktober 2026 — v0.1.7: paket native/AppImage/arsip portabel untuk x86_64 dan ARM64 dengan baseline glibc 2.35; updater AppImage mengganti file luar bertanda tangan. Konfirmasi X hanya untuk koneksi aktif atau pekerjaan lokal terdeteksi, bukan tab idle/terputus. [Rincian pengujian](docs/linux-portability-2026-10-27.md).

27 Oktober 2026 — v0.1.5: kompatibilitas keyboard terminal (DECCKM/ncdu, F1–F12, modifier, bracketed paste, IME), mouse reporting/scroll opencode, truecolor + OSC/DA/DSR untuk tema Codex/opencode, dan render reverse-video. 117 test egui + 55 test core lulus. [Catatan rilis](docs/releases/v0.1.5.md).

27 Oktober 2026 — v0.1.4: command console Telnet GNS3 membuka satu jendela dengan nama node, tanpa local shell tambahan. Host/port/title divalidasi; roundtrip Telnet, dua jendela bersamaan, koneksi gagal dan konfirmasi close diuji. [Bukti pengujian](docs/gns3-console-2026-10-27.md).

27 Oktober 2026 — v0.1.3: paste clipboard gambar ke CLI agent dan konfirmasi X tab/aplikasi/restart. PNG diuji pada clipboard asli dan composer Codex. [Bukti pengujian](docs/image-paste-close-2026-10-27.md).


26 Oktober 2026 — v0.1.2: operasi transport egui dipindahkan ke worker dengan antrean terbatas; shutdown Telnet, retry resize, dan Ctrl+C native diuji. Importer pihak ketiga lama dihapus; CSV tetap tersedia. [Bukti pengujian](docs/responsiveness-2026-10-26.md).


## Repository layout (workspace)

- `core/` — `opsterm-core`: all UI-agnostic logic (domain, security/argv,
  providers, SQLite storage, logging, credentials, tunnels, SFTP). Shared.
- `src-tauri/` — the Tauri desktop app (web UI + IPC). Re-exports core.
- `native/` — `opsterm-native`: a GTK4 + VTE desktop app. **Binary ~1.5 MB**,
  its own window (not GNOME Terminal), reuses core for storage/security/argv.
  Build: `cargo build -p opsterm-native --release`.

Memory reality (measured): GTK4/VTE baseline ≈ 65 MB PSS (122 MB RSS) with no
tab open; the Tauri/WebKitGTK baseline is similar or higher. The egui client
(custom renderer, no toolkit UI) is the lightest at ≈ 50 MB PSS / 11 MB binary.
Idle tabs are cheap in every client.

## egui client (`opsterm-egui`) — custom-rendered, no toolkit/webview

**Public release v0.1.1:** source and signed Linux x86_64 assets are published on GitHub. Default HTTPS update checks, real old-to-new installation/rollback, CI and the release workflow are verified. [Evidence and next-release instructions](docs/github-release-2026-10-26.md).

Pilihan font monospace, empat palet teks, brightness dan preview kini tersedia di Settings. Penyimpanan, pergantian warna dan restart diuji melalui GUI asli. [Panduan dan verifikasi](docs/terminal-appearance-2026-10-26.md).

26 Oktober 2026: perbaikan hover X, scroll terminal per piksel dan kecepatan wheel 2×, serta pengurangan atlas font telah diuji. Hasil desktop: RSS idle 94,61 → 90,42 MiB; target 15–30 MiB RSS belum tercapai. [Laporan pengukuran dan verifikasi](docs/performance-2026-10-26.md).

| Area | Feature | Status |
|---|---|---|
| Transport | Local shell · SSH (OpenSSH + native russh) · Telnet (auto-login) · Serial | Implemented |
| Integration | GNS3 desktop custom Telnet console command (one window per launch) | Tested |
| Layout | Tabs (drag-reorder) · Split (drag-resize) · 2×2 grid monitor | Implemented |
| Devices | Editor (host/port/proto/user/credential/pager/backspace/login) · vendor profiles · import CSV · filter | Implemented |
| Commands | Library + button bar · broadcast to selected targets · runbooks (expect) · auto pager-disable & auto login/enable | Implemented |
| Output | Keyword highlight (editable rules) · find · logging per session (`0600`) · scratchpad · double-click copy · select-all · export output | Implemented |
| Tools | SFTP browser · SSH tunnels (L/R/D) · subnet calculator · ping · traceroute · DNS | Implemented |
| Session | Workspace save/restore · auto-reconnect · bell + desktop notification | Implemented |
| Look | macOS-style menu bar · 4 UI themes · font family/weight + zoom · 4 text palettes · brightness + preview | Implemented |
| Tests | 141 egui tests + native GUI smoke (local PTY, tabs, composer, appearance, agent image paste, close confirmation, GNS3 Telnet) | Tested |


Legend: **Planned** · **In Progress** · **Implemented** (works, tests pass) ·
**Tested** (verified end-to-end on the target platform).

Last updated: 2026-10-16 (Phases 0–5 implemented through M3 feature set).

## Foundation, transports and terminal

| ID | Feature | Status | Evidence |
|---|---|---|---|
| — | Tauri v2 + React/TS/Vite scaffold | Implemented | `npm run build`, `cargo build` |
| — | Safe process spawn architecture (argv, no shell strings) | Implemented | `security::tests::*` |
| FR-01 | SSH engine (OpenSSH via PTY) | Tested | `tests/openssh_pty.rs`; host-key policy never weakened |
| FR-02 | Local shell / PTY | Tested | `tests/pty_shell.rs` |
| FR-03 | Terminal renderer (xterm, themes) | Implemented | `TerminalPane`, `theme.ts` |
| FR-03 | Clipboard: Ctrl+Shift+C/V, Ctrl+Insert/Shift+Insert, right-click menu, middle-click paste | Implemented | `TerminalPane`, e2e |
| FR-03 | Multi-line paste review modal (replaced `window.confirm`) | Implemented | `TerminalPane`, e2e |
| FR-03 | Backspace normalization per device/global (DEL 0x7F vs BS 0x08 for Huawei/Cisco) | Implemented | SessionEditor field + Settings; `TerminalPane` |
| FR-03 | Paste newline normalization to CR (+ optional paced line-by-line) | Implemented | `TerminalPane`; e2e with CR-only mock |
| FR-04 | Quick Connect | Tested | `quickConnect.test.ts`, e2e |
| FR-06 | Tabs / session lifecycle | Implemented | e2e isolation + close |
| FR-06 | Idle-tab hibernation (serialize + dispose; bounded buffer) | Implemented | `terminalRegistry` state cache, `TerminalPane` |
| FR-14 | Telnet client (IAC negotiation, insecure marker) | Implemented | `telnet.rs` tests; STATUS "INSECURE" |
| FR-14 | Telnet opt-in auto-login (username/password from inventory) | Implemented | `LoginAutomation` tests; prompt-tail match, single attempt |
| FR-15 | Serial console (termios, baud/parity/flow, break) | Implemented | `serial.rs` tests; `connection_serial_break` |
| FR-17 | SFTP/SCP (interactive `sftp` tab + `scp` jobs) | Implemented | `sftp_pty.rs`, `transfer/` tests |

## Persistence, settings, logging, search

| ID | Feature | Status | Evidence |
|---|---|---|---|
| FR-05 | Saved Session Manager (SQLite, WAL, migrations) | Implemented | `tests/storage.rs`, e2e |
| FR-05 | Nested folders + filter/search | Implemented | `SavedSessionsPanel` |
| FR-05 | Drag-and-drop move (sessions & folders) | Implemented | HTML5 DnD + cycle guard |
| FR-07 | Settings (general/terminal/SSH/appearance/logging) | Implemented | `SettingsPanel` persisted |
| FR-07 | Terminal color schemes (dark/light/solarized/green) | Implemented | `theme.ts` |
| FR-07 | UI light/dark/system chrome (`uiTheme`) | Implemented | `data-theme` + CSS remap; e2e light-theme |
| FR-08 | Find in terminal (Ctrl+F) | Implemented | `TerminalSearchBar` |
| FR-08 | Reusable highlight presets | Implemented | `highlight` store + `HighlightPanel` |
| FR-08 | Automatic on-output keyword decoration | Implemented | debounced, bottom-only, bounded regex |
| FR-09 | Logging / evidence capture (0600, rotation, index) | Implemented | `logging::tests` |
| — | Recent connections | Implemented | `recent_connections` |
| — | Reopen last tabs (opt-in) | Implemented | `general.reopenLastTabs` |

## Workspaces, commands, tools (M2/M3)

| ID | Feature | Status | Evidence |
|---|---|---|---|
| FR-10 | Recursive split panes (row/column) | Implemented | store + App; e2e "splits a tab" |
| FR-10 | Workspace snapshots (persist panes incl. ad-hoc) | Implemented | `WorkspacesControl`, e2e save/restore |
| FR-11 | Command Manager (folders, vendors, CRUD) | Implemented | `CommandManagerPanel` |
| — | Vendor profiles (pager/prompt/backspace/highlight/commands preset) | Implemented | `vendors.ts`, SessionEditor; e2e |
| FR-20 | Runbooks (saved expect scripts) + recorder | Implemented | `runbooks` table, `RunbooksPanel`; e2e |
| — | Login scripts / expect per-device (send/sendline/waitfor/sleep) | Implemented | `expect.ts`, `runConnectActions`; e2e |
| — | Auto pager-disable on connect (Huawei/Cisco/Juniper/MikroTik presets) | Implemented | SessionEditor → On connect |
| — | Auto-logging per device + Logs browser (export) | Implemented | `LogsPanel`, `log_read` (dir-confined) |
| — | SFTP browser (list/mkdir/rename/delete + upload/download) | Implemented | `sftp/`, `FileBrowserPanel`; parser tests |
| FR-06 | Duplicate/clone tab + drag-reorder tabs | Implemented | store + TabBar; e2e |
| FR-06 | Auto-reconnect dropped sessions (bounded) | Implemented | Settings → General |
| FR-08 | Multi-colour highlight rules + copy-on-select | Implemented | `TerminalPane` decorations |
| FR-10 | Draggable split-pane dividers | Implemented | `PaneDivider`, `adjustSizes` |
| — | Per-device output encoding (UTF-8/GBK/Big5/Latin) | Implemented | `decodeOutput` + SessionEditor |
| — | Bell → OS notification when unfocused | Implemented | `TerminalPane` |
| FR-11 | Button Bar (Insert vs Send) | Implemented | `ButtonBar`, e2e |
| FR-12 | Command Composer + scoped broadcast + arm/review | Implemented | `CommandComposer`, e2e |
| FR-13 | Active Sessions manager (multi-select actions) | Implemented | `ActiveSessionsPanel` |
| FR-16 | Import/export CSV/TSV + JSON (no secrets) | Implemented | `csv.test.ts`, `ImportExportPanel` |
| FR-18 | SSH tunnels (`-L`/`-R`/`-D`, audited argv) | Implemented | `tunnel::tests`, `TunnelsPanel` |
| FR-19 | Credential inventory (named + tags + username + kind) in OS keyring | Implemented | CredentialsPanel; metadata in `credentials` table |
| FR-19 | Device references one credential (`credentialRef`) instead of per-node passwords | Implemented | SessionEditor credential select; e2e |
| FR-19 | Opt-in login-password autofill via `SSH_ASKPASS` (OpenSSH) and native engine password auth | Implemented | `openssh_pty` askpass test; no `sshpass`; 0600 temp, removed on close |
| FR-20 | Automation API + step runner (`send`/`waitFor`/`sleep`) | Implemented | ring buffer + `automation_run`, e2e |
| — | SSH advanced options (compression, X11, agent forwarding) | Implemented | `security::tests`, Settings → SSH |
| — | SSH certificates + PKCS#11 provider (`-o CertificateFile`/`PKCS11Provider`) | Implemented | `security::tests` |
| — | Native Rust SSH engine (`russh`) behind `ConnectionProvider` | Implemented | `russh_ssh.rs`; strict known_hosts; Settings → SSH engine |
| — | Accessibility basics (dialog roles, aria-labels, keyboard ops) | Implemented | dialog `aria-label`, labelled icon buttons |
| — | Build/packaging `.deb` (binary, icons, `.desktop`) | Tested (build) | `tauri build` artifact verified (3.31 MiB) |

## Native SSH engine (M4) — behaviour

- Opt-in per settings (`SSH engine: native`); the default remains the OpenSSH
  subprocess, which is never silently replaced.
- Host keys are checked against `~/.ssh/known_hosts`; unknown **and** changed
  keys are rejected — there is no auto-accept.
- Authentication: SSH agent, then an identity file (`~/.ssh/id_ed25519`,
  `id_ecdsa`, `id_rsa`, or the profile's file).
- Deliberate limits reported as errors, not silent fallbacks: no `ProxyJump`,
  no password/keyboard-interactive auth.

## Not yet done

- M1 lab exit gates: AT-01, AT-05, AT-14 (20-session soak) and AT-15
  (interacting with the installed `.deb` from the desktop) require a real
  OpenSSH lab host and an interactive GUI session. The `.deb` builds and is
  structurally verified (binary + icons + `OpsTerm.desktop`).
- A full WCAG accessibility audit (screen-reader pass, focus-trap in dialogs).
- AppImage bundle target (build with `npm run tauri:build -- --bundles appimage`;
  requires network to fetch the appimage tooling).

## Security posture

- No `StrictHostKeyChecking=no`, no `UserKnownHostsFile=/dev/null`, no
  `sshpass`; every spawn is a structured argv with a validated absolute binary.
- Telnet is unencrypted and labelled **INSECURE** before and during use.
- Secrets live only in the OS keyring; never in SQLite, logs, argv or backups.
- Broadcast is off by default, shows targets explicitly and requires arming for
  more than one target, with per-target results.
