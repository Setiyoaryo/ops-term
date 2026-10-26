# PRODUCT REQUIREMENTS DOCUMENT (PRD)
# OpsTerm — Linux-First Secure Terminal Client

> **Project:** OpsTerm\
> **Category:** Desktop terminal emulator & connection/session manager for network operations\
> **Platform V1:** Ubuntu 22.04/24.04 LTS x86_64; GNOME/KDE on Wayland and X11 (verify separately)\
> **Document version:** 1.0\
> **Research cutoff:** 14 October 2026\
> **Primary reference:** SecureCRT 9.7.x by VanDyke Software\
> **Implementation target:** Codex-assisted development, repository baru dan kode orisinal

---

## 0. Executive summary

Build an **independent desktop application inspired by the workflows and capabilities of SecureCRT**, specifically for network engineers, NOC analysts, system administrators, and infrastructure teams using Linux. The app must feel as reliable as a purpose-built terminal, not like a demo web page with a terminal-shaped box.

It must connect to actual SSH devices, support local PTY shells, manage hundreds/thousands of saved device profiles, organize simultaneous sessions in tabs/splits, search and highlight CLI output, save terminal logs, execute reusable commands, and progressively support Telnet, serial-console, file transfer, and network automation.

**Non-negotiable:** SecureCRT is proprietary commercial software. Reimplement independently using public protocols, observed functionality, public documentation, and original design/code/assets. Do not decompile, copy code, redistribute copyrighted branding, clone the exact UI pixel-for-pixel, or label the product “SecureCRT.” Feature parity is an aspiration, **not** a claim of binary compatibility or endorsement.

### Product promise

> “A serious Linux terminal workspace for managing routers, switches, OLTs, firewalls, servers, and infrastructure at scale—fast, organized, scriptable, and safe.”

### Definition of a usable release

A release only qualifies as usable when SSH and local-shell input/output actually work end-to-end, user settings persist, terminal resizing behaves correctly, host-key warnings remain secure, multi-tab sessions remain independent, and packaging works on a real Linux desktop. A frontend-only mockup does **not** count.

---

## 1. Research findings and factual baseline

SecureCRT **9.7.3**, released **1 July 2026**, is the latest official release listed at the time of research. SecureCRT itself supports Linux. VanDyke's stated Linux support includes Ubuntu/Kubuntu 22.04 and 24.04 LTS (64-bit). Our product is an independent alternative, **not** a Linux port of VanDyke code.

### 1.1 Capabilities observed in official material

| Capability | SecureCRT reference | Reimplementation strategy |
|---|---|---|
| SSH2 terminal | Core capability | V1 via local OpenSSH client in PTY |
| Telnet, serial connections | Listed protocols | Telnet with warning; native serial backend |
| Local shell | Supported Linux session type | PTY-spawn default shell |
| Saved sessions and nested folders | Dockable Session Manager | SQLite tree + filtering + drag/drop |
| Active Sessions Manager | Status/operations on live sessions | Distinct live-session view |
| Tabs, tab groups, tiling | Multiple concurrent sessions | Tab model + recursive split layout |
| Workspace snapshots | Introduced in 9.7 | Persist tab/split layouts independently from connections |
| Keyword highlighting | Plain words, phrases, regex | Terminal decoration/search with bounded work |
| Ad hoc highlight input | 9.7 | Temporary highlight search in toolbar |
| Command Manager | Searchable reusable commands/folders | Command library and command composer |
| Button Bar | User-customizable commands/scripts/actions | Compact configurable toolbar |
| Command Window | Target active/all/selected sessions | Composer + safe scoped broadcast |
| Script recorder/Python | Official automation options on Linux | Independent automation API in later phase |
| Credential manager | Named shared credentials | Linux Secret Service in later phase |
| SFTP tabs/transfer | Present in SecureCRT 9.7 | Separate transfer view, not full SecureFX clone |
| CSV/TSV import/export | Official feature | Safe preview + field mapping + duplicate handling |
| Logging / log rotation | Official feature | Structured raw-output logging, optional rotation |

**Scope caveat:** Some VanDyke features differ by OS. E.g. RDP and some script editor capabilities are Windows-specific, and SecureFX is a separate companion product. Do not incorrectly advertise Windows-only functions as universally available in SecureCRT for Linux.

### 1.2 Relevant official sources

1. [SecureCRT Linux overview](https://www.vandyke.com/products/securecrt/unix.html)
2. [SecureCRT features](https://www.vandyke.com/products/beta/securecrt/features.html) (9.7 feature reference; some content on a historical beta page)
3. [What's new in SecureCRT 9.7](https://www.vandyke.com/products/securecrt/new-in.html)
4. [SecureCRT version history](https://www.vandyke.com/products/history.php?prod=securecrt)
5. [Current official versions](https://www.vandyke.com/download/latestreleases.html)
6. [Official system requirements](https://www.vandyke.com/products/system_req.html)
7. [Broadcast with Command Window](https://www.vandyke.com/support/tips/chatsendcom.html)
8. [Command Window automation and modes](https://www.vandyke.com/support/scripting/scripting-examples/command-window-automation.html)
9. [Button Bar usage](https://www.vandyke.com/support/tips/buttonbar.html)
10. [SecureCRT tiling behavior](https://www.vandyke.com/support/tips/tiling_overview.html)

These sources are references for behavior, **not** source code or product assets to be copied.

---

## 2. Users, jobs, and critical scenarios

### 2.1 Primary personas

**P1 — ISP IP-core / NOC engineer**: manages Cisco IOS/IOS-XE/IOS-XR, Juniper Junos, Huawei VRP, MikroTik RouterOS CLI, ZTE OLT CLI, Linux servers; jumps among many live devices; needs command accuracy and time-stamped evidence.

**P2 — Datacenter / systems administrator**: connects to Linux servers through keys and bastion hosts, uploads/downloading diagnostic files, requires stable SSH session management, terminal search, and profiles.

**P3 — Network deployment engineer**: configures many similar devices over SSH, Telnet (legacy) and USB-serial console; uses snippets, custom commands, log captures, and deliberate multi-session execution.

### 2.2 Core user stories

- As a NOC engineer, I can search `CORE-JKT`, double-click a saved session, and SSH into it without reconstructing parameters.
- As an IP-core engineer, I can open 10 routers in tabs, split two panes for comparison, and preserve each terminal's state.
- As an engineer, I can run `show bgp summary` and `show interfaces` with a command button appropriate to the selected vendor profile.
- As a troubleshooter, I can highlight `down`, `error`, `BGP`, `flap`, or a regex matching IPv4 addresses within a large terminal stream.
- As a change operator, I can compose multiline CLI config safely, review target hostname and content, then send explicitly.
- As a site engineer, I can attach to `/dev/ttyUSB0`, choose 9600/115200 baud, and interact with a switch console.
- As a user behind a bastion, I can use `~/.ssh/config`, `ProxyJump`, agent keys, and existing trusted host keys without weakening SSH security.
- As an incident responder, I can save an evidence log labeled with session name, host, and timestamp, then reopen it later.

### 2.3 Success metrics (targets, not claims)

- A new SSH session can be added and connected in **≤ 30 seconds**, excluding device authentication latency.
- Search among **1,000 saved sessions** returns matching records in **≤ 200 ms** on the reference test laptop.
- First interactive terminal paint on warm start in **≤ 3 s** on reference Ubuntu laptop; record actual measurements.
- **20 concurrent SSH sessions** remain responsive with independent output, cancellation and cleanup.
- **No silent loss, swap, or duplication** of terminal input across sessions in automated stress tests.
- **No plaintext credentials in SQLite, logs, backup, error reports, or process command arguments.**
- **Zero bypasses** of unknown/changed SSH host-key verification in the production default configuration.

---

## 3. Scope and release strategy

### 3.1 Milestone matrix

**M0 — Foundation / spike**: App shell, Tauri/PTY verification, true SSH to lab, xterm rendering, keyboard handling, host key prompts, packaging proof-of-concept.

**M1 — Core MVP (real daily-use SSH terminal)**:
- OpenSSH-backed SSH sessions, local shell, SSH config awareness, live terminal, terminal resize.
- Quick Connect, session database, folders, searchable sidebar, connection status, tabs, reconnect/disconnect.
- Terminal options: font, color palette, scrollback, copy/paste, search, output logging.
- Minimum reliable security defaults and Ubuntu `.deb` packaging.

**M2 — Network Engineer Release**:
- Horizontal/vertical splits, layouts/snapshots, Active Sessions panel.
- Command Manager, Command Window, Button Bar, network-vendor profiles.
- Regex highlighting, CSV import/export, multiline paste review, command broadcast safety.
- Telnet and serial console, if protocol certification passes.

**M3 — Advanced Desktop Client**:
- SFTP/SCP UI, SSH tunnels, keyboard mapping, auto-reconnect policies, improved profiles.
- Keyring-backed credential references, safer scripted sequences, command/session history.
- Scripting API, recorded scripts, scheduling/runbooks if safely implemented.

**M4 — Optional parity & differentiation**:
- Native Rust SSH engine evaluation, X11 forwarding, agent forwarding, advanced algorithms.
- SSH certificate/PKCS#11 support, accessibility refinements, interoperable import formats.
- Fleet-scale workflows, prompt detection, backups with diff, configuration safeguards.

**Do not** implement M1–M4 simultaneously. Complete, test and stabilize each milestone first.

### 3.2 Explicitly out of scope for M1

- RDP/VNC desktop remoting; SecureFX-level bidirectional sync/FTPS/WebDAV/S3.
- Full emulation of vintage terminals (TN3270, Wyse, SCO ANSI).
- SSH1 (obsolete/insecure); do not implement by default.
- Web-based SSH relay, cloud account, telemetry pipeline, public multi-user server.
- Automatic discovery/scanning of public internet, mass execution by default.
- Copying SecureCRT proprietary config decryption, protected credential extraction, branding, copyrighted assets.
- Fully automated production configuration modification before safety controls are proven.

---

## 4. Product experience and navigation

### 4.1 Layout specification

A high-density professional desktop UI with compact visual hierarchy; original styling influenced by contemporary IDEs and tools such as VS Code, not a 1:1 SecureCRT screenshot. **No neon, no excessive gradients, no giant decorative cards.**

```text
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ OpsTerm      File  Session  View  Tools  Transfer  Settings               [—][□][×] │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ [+ Quick Connect: user@hostname / SSH alias ] [Connect] [Split] [Log] [⌕] [⋯]    │
├───────────────┬──────────────────────────────────────────────────────────────────────┤
│ SAVED SESSIONS│ [✓ CORE-JKT-01 ×] [✓ CORE-SBY-02 ×] [● OLT-05 ×] [+]               │
│ [⌕ Filter...] ├───────────────────────────────┬──────────────────────────────────────┤
│ ▼ Production  │ CORE-JKT-01                   │ CORE-SBY-02                          │
│   ▼ Core      │ router# show bgp summary      │ router# show interfaces brief        │
│     CORE-JKT  │ ...                           │ ...                                  │
│     CORE-SBY  │ ...                           │ ...                                  │
│   ▼ Access    │                               │                                      │
│     SW-01     │                               │                                      │
│     OLT-05    │                               │                                      │
│ ▼ Lab         │                               │                                      │
│   R1          │                               │                                      │
│   R2          │                               │                                      │
├───────────────┴───────────────────────────────┴──────────────────────────────────────┤
│ COMMAND COMPOSER [Active ▼] [Targets: CORE-JKT-01] [Review & Send]                  │
│ > show bgp summary                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ● SSH connected │ CORE-JKT-01 │ 172.16.0.10:22 │ UTF-8 │ 128 × 40 │ LOG ● │ 2 tabs │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

Wireframe is conceptual, not an exact-match mockup. User may hide sidebar and Command Composer to maximize working space.

### 4.2 Regions

1. **Menu/title**: native-feeling application controls; keyboard accessible.
2. **Quick Connect**: `ssh user@host`, `user@host`, saved alias; parsing/validation; port and protocol picker in advanced modal.
3. **Sessions sidebar**: saved connection tree, optional tabs `Saved / Active / Commands`; persistent width; collapsible and keyboard-operable.
4. **Workspace**: multiple tabs and recursively splittable terminal panes; each has isolated terminal buffer/process.
5. **Command Composer**: dockable bottom pane with target scope clearly visible; hidden by default on M1.
6. **Status bar**: connection state, device hostname, protocol/port, character encoding, dimensions, latency only if measured, logging state.
7. **Settings**: global defaults and per-session override; no confusing duplicate settings.

### 4.3 Visual tokens

- Default dark background: charcoal `#111418`, side panel `#181C20`, border `#2B323A`, text `#E5E9EF`, muted `#99A3AE`, accent `#4F8EE8`.
- Light mode: soft neutral/off-white, strong text contrast; follow system or manual override.
- Terminal themes: `Default Dark`, `Default Light`, `Solarized`, `Classic Green`, user-imported palettes (not copyrighted proprietary palettes).
- Typography: UI `Inter`/system sans-serif (licensed properly); terminal `JetBrains Mono`, `DejaVu Sans Mono` or system mono, with fallback.
- Density modes: Compact/Comfortable; compact default for NOC.
- Clear statuses: connected, connecting, reconnecting, disconnected, failed, locked; do not communicate status with color alone.
- User-resizable 240–400px sidebar; pane header visible on split; focus ring on active pane.
- No animations that delay keyboard input; use discreet 120–180ms UI transitions if any.

### 4.4 Desktop keyboard shortcuts (configurable eventually)

| Action | Suggested shortcut |
|---|---|
| New Quick Connect | Ctrl+Shift+N |
| New local shell | Ctrl+Shift+T |
| Search sessions | Ctrl+K |
| Find in terminal | Ctrl+F |
| Next/previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Split right | Ctrl+Shift+\\ |
| Split down | Ctrl+Shift+- |
| Close tab | Ctrl+Shift+W |
| Copy terminal selection | Ctrl+Shift+C |
| Paste into terminal | Ctrl+Shift+V |
| Reconnect | Ctrl+Shift+R |
| Toggle Command Composer | Alt+G |
| Open Settings | Ctrl+, |

Ensure conflicts with terminal input (e.g. Ctrl+C = ETX and Ctrl+D = EOT) are resolved: **do not steal Ctrl+C from the remote shell** unless a selection action is explicitly available; Ctrl+Shift+C copies.

---

## 5. Functional requirements and acceptance criteria

Priority legend: **P0 required before M1 shipped**; **P1 M2**; **P2 M3+**.

### FR-01 — SSH connection engine [P0]

**Fields**: session name, hostname/IP, IPv6 literal, port (default 22), username, SSH config alias, identity file path, authentication mode `agent/key/interactive`, optional bastion/ProxyJump, connect timeout, keepalive profile, terminal emulation identifier.

**Implementation path M1**: spawn installed `/usr/bin/ssh` (or discovered absolute trusted executable) through `portable-pty` using structured argv, **never a shell command string**. Prefer existing OpenSSH config and `known_hosts`. Make SSH CLI launch a replaceable `ConnectionProvider` so future `russh` adoption will not require UI rewrite.

**Requirements**:
- Interactive SSH password and keyboard-interactive challenge should display safely in the terminal; no password embedded in command-line argv.
- Accept OpenSSH `~/.ssh/config` and `ProxyJump` via the system client, obeying user's existing policy. Support aliases but keep identity-file options explicit.
- Default to OpenSSH's trusted host-key verification; unknown keys use OpenSSH's interactive confirmation; changed keys block/warn appropriately; do not auto-accept.
- No automatic `StrictHostKeyChecking=no`, no `UserKnownHostsFile=/dev/null`, no silent insecure downgrade.
- Modern SSH defaults; user-custom legacy algorithm settings only in separate explicit advanced mode later and marked risky.
- PTY resize synchronizes to remote (SSH `-tt`/interactive PTY); SSH sessions independent.
- Use graceful disconnect then forced process kill after bounded timeout, reclaim backend channels and UI listeners.
- Status machine: `idle → connecting → authenticating → connected → reconnecting/disconnected/failed → closed` (not every OpenSSH auth phase is unambiguously observable; do not falsely claim precise handshake progress).
- Redact private key file contents, passwords and passphrases; never put them into command-line args or renderer events.

**Acceptance**: real SSH into Ubuntu OpenSSH server; login via agent, key, interactive password; challenge/host-key prompt surfaces; known-host mismatch blocked; remote resize; terminal Ctrl+C works; disconnect cleans child process; two sessions with same host do not share buffers.

**Important constraint**: M1 does NOT promise a centrally saved SSH-password feature. OpenSSH interactive prompts and agent keys are enough for baseline. Future credential vault requires a deliberate secure integration design, not `sshpass`.

### FR-02 — Local shell / PTY [P0]

- Launch default interactive `$SHELL` (or discovered `/bin/bash` fallback) via PTY; handle shell exit, UTF-8, ANSI/VT sequences, terminal resizing and window focus.
- Working directory defaults to home, optionally configured; only execute absolute valid shells; do not concatenate untrusted strings.
- Local shell fully usable with `vim`, `htop`, `less`, `top`, pager controls.

**Acceptance**: open shell; run `ls -la`, `vim`, `top`; resize while inside full-screen TUI; `Ctrl+C` interrupts running process; closing tab terminates intended child/process group per documented policy.

### FR-03 — Terminal renderer [P0]

- Use `@xterm/xterm`, Fit, Search, Unicode/WebLinks addons; WebGL renderer optional with fallback to default renderer on Linux WebKitGTK.
- Support 16 ANSI colors + truecolor where terminal backend/device emits it, UTF-8, line wrapping, alt-screen, selection, clipboard, cursor shape, bell policy, mouse scroll, configurable scrollback, bracketed paste when enabled by remote app.
- Select TERM based on compatible capabilities (default `xterm-256color` only when supported); do not declare unsupported features.
- Implement clipboard copy/paste with context-menu and keyboard shortcuts; selection should not unintentionally send text.
- Debounce terminal size measurement, send `rows/cols` only when changed, avoid zero-dimension resize.
- Terminal buffer remains intact when switching tabs or splitting layouts.
- Clipboard can contain multiline commands: handle through paste-review rule below.

**Acceptance**: ANSI/truecolor samples, box drawing, Unicode Indonesian text, `vim`, `less`, `top`, Junos/Cisco pagers, terminal alternate screen; tabs retain buffer and correct viewport; WebGL fallback does not crash.

### FR-04 — Quick Connect [P0]

- Input forms: `router.example.net`, `operator@router.example.net`, `ssh operator@router.example.net`, configured SSH alias; optional protocol dropdown and advanced settings.
- Display destination/port/user in a final summary for dangerous or unrecognized parsing, not automatic guesswork.
- Quick session opens tab immediately with `Connecting...` and useful error feedback (host not found, auth denied, connection refused, timeout).
- Optional `Save Session` turns ad hoc session into a saved profile without dropping connection.
- Validate host/port; host cannot be injected as SSH command-line switches; user input never becomes arbitrary process flags.

**Acceptance**: connect to IPv4, IPv6, DNS and config alias; malformed inputs rejected without launching subprocess; new ad hoc tab can be saved, reconnected.

### FR-05 — Saved Session Manager [P0]

- Nested folders, create/rename/move/duplicate/delete sessions, drag-and-drop with keyboard alternative, filter by name/host/tag/folder.
- Fields: ID, display name, host, port, protocol, user, folder, tags, description, notes (not secrets), SSH alias, identity path, terminal overrides, keepalive, logging profile, created/modified timestamps.
- Default profile per protocol, global settings, per-session overrides; bulk editing (P1) with clear changed-field semantics.
- Ordering: user-defined/manual first; optional alphabetical; favorites and recently used.
- Context menu: Connect, Connect in new tab, Edit, Clone, Move, Copy safe link/hostname, Export, Delete.
- Search should operate on metadata, not credentials.
- SQLite persistence, WAL, migrations, unique IDs; changing visible name must not break references.
- Warn on delete if session currently open (do not silently terminate session).

**Acceptance**: 1,000 session records can be searched quickly; close/restart app retains data; nested folder moves and clone retain expected metadata; deleted saved profile does not corrupt open sessions.

### FR-06 — Tabs, window and session lifecycle [P0]

- Tabs open/reorder/rename visually, connection state icon, close/reconnect, clone (new connection not shared PTY), status details.
- Lock an active session as read-only at UI layer (P1), with locked badge; lock cannot be mistaken for cryptographic session lock.
- On close, optionally confirm if multiple connected sessions will terminate; configurable default requires confirmation.
- Restore **layout and saved-session references**, not live socket/process state. Never imply dead SSH TCP connections survive app restart.
- Switching tabs must not pause backend output or mix input.
- Destroy frontend xterm instance and backend child after close; no orphaned SSH processes.

**Acceptance**: 20 tabs open; switching and closing one does not impact others; 30 repeated connect/disconnect cycles do not leak child processes or listeners.

### FR-07 — Settings and themes [P0]

- Settings groups: General, Terminal, SSH, Sessions, Security, Appearance, Logging, Shortcuts (P2), Advanced.
- Global settings, then session overrides; show when setting is overridden and provide Reset to Default.
- Settings persist to SQLite/config safely, with schema versions and validation.
- Include dark/light/system theme; terminal font size 10–24 by normal UI control; scrollback presets 2k/5k/10k/50k; no infinite buffer.

**Acceptance**: change color/theme, font, scrollback; restart app; settings persist and renderer updates without breaking active sessions.

### FR-08 — Search, copying, highlighting [P0 basic / P1 regex]

- Ctrl+F opens terminal search bar, next/previous, case sensitivity, match count with bounded indexing.
- Copy current selection, copy visible screen, copy terminal content subject to maximum/confirmation; paste honors multiline confirmation.
- P1: reusable keyword presets, regex rules, case sensitivity, priority, stop/start, temporary ad hoc highlight field, visible legend.
- Example keyword sets: Error (`error|failed|down|invalid`), Routing (`BGP|OSPF|ISIS|EVPN`), Interface (`up|down|administratively`), IPv4 regex; escape untrusted regex and bound evaluation to prevent UI freezes.
- Clarify that regex on terminal output has line boundaries/ANSI styling complexities; use tested parser/decoration approach, not indiscriminate DOM text replacement.

**Acceptance**: find text that spans scrollback, toggle case-sensitive, highlight visible matches, clear highlight instantly; malicious catastrophic regex cannot freeze UI.

### FR-09 — Logging / evidence capture [P0 baseline / P1 rotation]

- Start/stop recording on a given terminal with status indicator, customizable log dir under user control, mode `raw received output` or `normalized text` (latter later), clear policy of what is recorded.
- Default file pattern: `{session}_{YYYY-MM-DD}_{HH-mm-ss}.log`; sanitize filenames; create directory with least privilege, prevent path traversal/collisions, `0600` files where applicable.
- No recording by default; startup auto-log is explicit user opt-in at global/session scope.
- Treat remote output as potentially containing sensitive data; logs and exports are sensitive artifacts; no web upload/analytics.
- Interactive password typed into SSH PTY must **never** be copied to the app's command history; logging received remote bytes may still include device-provided secrets and must be clearly disclosed.
- Optional timestamps per line and rotate at midnight/size; store close reason in metadata sidecar separate from raw output.

**Acceptance**: log per-tab output, switch tabs independently, output file correctly contains displayed remote text, app crash leaves valid partial file, no log written for a session with logging off.

### FR-10 — Multi-pane workspaces [P1]

- Split selected pane horizontally or vertically; drag tabs into pane; visible active pane indicator.
- Pane tree supports mixed rows/columns with reasonable limit (e.g. four visible terminal panes by default, configurable).
- Resize dividers, focus pane, unsplit without disconnecting; closing pane doesn't accidentally close tab unless specifically requested.
- Tab group/pane relationships persist as JSON layout references.
- Workspace snapshots: name, description, list of saved session IDs, tab orders, split tree, panel visibility; restore opens **new connections after consent**.
- Snapshot save should not include SSH credentials or captured terminal buffers.

**Acceptance**: two SSH sessions side by side, both independently interactive; move one to bottom split, save snapshot, restart, restore layout, review reconnect target list before opening remote connections.

### FR-11 — Command Manager and Button Bar [P1]

- Saved library of commands/snippets in folders with search/tags; item fields: label, content, description, vendor family, mode, newline behavior, risk level, variables.
- Examples: Cisco `show ip bgp summary`, Juniper `show bgp summary`, Huawei `display bgp peer`, MikroTik `/routing bgp peer print` (ensure version-specific CLI), ZTE selected show commands validated in lab.
- Button Bar: create buttons from saved command, paste-to-composer, or explicit action. Distinguish `Insert` vs `Send Now` via visual label; default `Insert`.
- Templates may use typed variables `${interface}` or `${asn}`; show preview. No secret variables stored as plaintext in snippet definitions.
- Device profiles are suggestions, not a promise of complete syntax parity across firmware versions.

**Acceptance**: make folder, save snippet, search, assign button, click to insert, view exact bytes to be sent, execute on a test router.

### FR-12 — Command Composer & safe broadcast [P1]

- Composer input separate from terminal key events; default target = `Active Pane`, not every open session.
- Scopes: Active Pane / Selected Checked Sessions / Visible Pane Group / All Connected in Workspace; scope must be **visible and explicit**.
- Show list of destination hostnames and count. When target count >1, require `Arm broadcast` and a final review before first send; auto-disarm after send/scope change/reconnect.
- Clipboard multiline paste always opens review modal before sending to remote in P1; show literal linebreaks, destination, escaped control character warning.
- Option to `Insert only` vs `Send with Enter`; multi-line mode should show line count; pacing per character/line for CLI devices; opt-in.
- Locked and disconnected sessions are never targeted. Treat auth prompts/config mode as unsafe states for broadcast unless confirmed; there is **no reliable universal prompt detector** across vendors.
- Broadcast includes per-destination result feedback, **not** transactional guarantee; partial success must be reported.
- Do not inject commands to devices automatically when switching session/group.

**Acceptance**: with 3 SSH tabs, select exactly 2 targets, review each target, send a benign diagnostic command; third tab receives nothing; when command fails in one destination, report partial outcomes.

### FR-13 — Active Sessions manager [P1]

- Separate view of live tabs/connections and their states, sortable/filterable by host, status, workspace.
- Multi-select only for intentional operations: disconnect, close, reconnect; confirmation for destructive multi-actions.
- Mark focused live session clearly; an open session may be associated with a saved profile or be ad hoc.

### FR-14 — Telnet client [P1]

- Clearly label **INSECURE / UNENCRYPTED** before first connection and in status bar; user must actively enable Telnet per profile.
- Handle Telnet IAC negotiation, binary/echo/SGA/NAWS as appropriate, CRLF rules, encoding, negotiated terminal type. Use a dedicated Rust client/provider; don't render negotiation bytes as terminal text.
- Telnet default port 23; never save cleartext password automatically; never imply encrypted protection.

**Acceptance**: interactive CLI on Telnet lab server and one known legacy network device, correct echo/line endings/window-size if negotiated; security indicator persists.

### FR-15 — Serial console client [P1]

- List available Linux serial devices (`/dev/ttyUSB*`, `/dev/ttyACM*`, `/dev/ttyS*`, with actual enumeration). Device path explicit; show permission errors and non-destructive guidance about `dialout` where applicable.
- Options: baud rate, data bits, parity, stop bits, flow control, DTR/RTS handling; common presets `9600 8N1`, `115200 8N1`.
- Back end via async serial library (`tokio-serial` candidate); serialize reads/writes; ensure device released on tab close.
- Send Break action and newline mode (CR, LF, CRLF); optional hex inspector in future.

**Acceptance**: Linux USB-to-serial adapter loopback + physical/network lab console; baud/parity changes, break action, unplug/replug error state, exclusive port ownership.

### FR-16 — Import/export and backups [P1]

- CSV and TSV imports with column mapping preview: name, host, username, port, protocol, folder path, tags; validate per row, display conflicts and skipped records.
- Import `~/.ssh/config` aliases as **references**; preserve original file, do not copy SSH secrets or private key contents.
- Export session metadata and settings to versioned JSON and CSV without passwords; secret references may be omitted or names only, never secret values.
- Backup should be atomic, with schema version and checksum, import preview; conflicting session IDs remapped safely.
- SecureCRT-specific session import: optional future **metadata-only** migration if lawful format documented/tested; no decryption/circumvention.

**Acceptance**: import 100 CSV sessions, preview 2 invalid rows, avoid duplicate creation; export/reimport preserves nested folders and profiles, no password in output.

### FR-17 — File transfer [P2]

- Add SFTP browse/upload/download/rename/delete/chmod in a separate tab or drawer with explicit permissions confirmation.
- Reuse SSH identity/host trust policy. M1 may offer `Open external sftp` action but must not present it as built-in SFTP UI.
- Local/remote path navigation, progress, cancel, resume only if supported/verified; prevent accidental overwrite by default.
- SCP support as optional independent provider, not assumed to be SFTP-compatible.

**Acceptance**: upload/download a test file and verify checksums, cancel mid-transfer without corrupting unrelated files, host mismatch safely blocked.

### FR-18 — SSH tunnel management [P2]

- Profiles for local (`-L`), remote (`-R`), dynamic SOCKS (`-D`) forwarding via audited OpenSSH arguments.
- Clearly show bind interfaces and security implications; local-forward defaults `127.0.0.1`, not `0.0.0.0`.
- Display running/failed tunnel processes; allow stop independently of interactive terminal.

### FR-19 — Credential management [P2]

- Integrate Linux Secret Service API / GNOME Keyring / KWallet for **secret storage only**, metadata references in SQLite. When keyring unavailable or locked, prompt; no plaintext fallback.
- Prefer SSH agent and OpenSSH existing mechanisms for SSH keys; do not extract secrets from ssh-agent.
- Distinguish passwords, private-key passphrases, device enable secrets and other login challenges; do not automatically send enable passwords based on fragile prompt matching.
- Password autofill into OpenSSH prompts requires a secure architecture that avoids logging, argv, environment, clipboard, and untrusted-process injection; until this is implemented and tested, **do not claim it**. Never use `sshpass` as a shortcut.
- Vault export always excludes secrets by default; any future encrypted vault backup must use a reviewed KDF/AEAD design.

### FR-20 — Automation & script recorder [P2]

- Define provider-independent automation API (`send`, `sendLine`, `waitFor`, `readUntil`, `sleep`, `getSession`, `setLog`, `connect`) with timeouts and cancellation.
- Explicit user-triggered scripts only; per-run permissions, destination preview, dry-run for composed commands.
- Python 3 script runner as **separate, permission-bounded process**; untrusted scripts can execute arbitrary code if not sandboxed, so never advertise a security sandbox without one.
- Script Recorder records keystrokes/operations and generates a reviewed script; exclude password input, authentication challenge data, clipboard secrets and key material.
- Scripts may control sessions and send config, but production guardrails (confirm targets, risky operation banner) remain enforced at the shared command-dispatch layer.
- Support session output waits using bounded ring buffers and regex timeouts; avoid races on highly asynchronous device CLI.

**Acceptance**: manually run script to execute harmless command on lab router and wait for expected prompt, cancel safely; no secrets in recorded script.

---

## 6. Device interoperability matrix

A vendor profile is **CLI defaults and suggestions**, not a separate network protocol. SSH/Telnet/Serial remain the actual transports.

| Family | Typical transport | M1 | M2 | Notes |
|---|---|---|---|---|
| Ubuntu/Debian Linux | SSH/local shell | Full core | Snippets/logging | Validate shell PTY and ANSI |
| Cisco IOS / IOS-XE | SSH/Serial/Telnet | SSH | Serial, keyword/snippets | Pagination and `terminal length` optional per session |
| Cisco IOS-XR / NX-OS | SSH/Serial | SSH | Device profiles | Prompt and command syntax differ |
| Juniper Junos | SSH/Serial | SSH | Profiles, evidence logs | Screen width + pager tests |
| Huawei VRP | SSH/Serial/Telnet | SSH | Profiles | `display` CLI |
| MikroTik RouterOS | SSH/Serial/Telnet | SSH | Profiles | ROS v6 vs v7 syntax differences |
| ZTE C300/C320 and other OLTs | SSH/Telnet/Serial | SSH | Lab-tested serial/Telnet | Vendor firmware/pager nuances |
| Generic BSD/Linux server | SSH | SSH | SFTP/tunnels | Agent, ProxyJump |

Do not hardcode unsolicited pager-disable commands (such as `terminal length 0`) upon every connection; offer an opt-in command preset with compatibility explanation.

---

## 7. Architecture decision and rationale

### 7.1 Recommended stack (initial)

| Layer | Choice | Why |
|---|---|---|
| Desktop shell | **Tauri v2** | Rust backend + modern frontend, Linux packaging, explicit IPC boundaries |
| UI | **React + TypeScript + Vite** | Robust component ecosystem and Codex familiarity |
| Styling | Tailwind CSS + accessible headless primitives | Professional compact UI; customizable |
| Terminal | **xterm.js** + Fit/Search addons | Mature terminal behavior instead of hand-writing ANSI parser |
| Async backend | Rust + Tokio | Process I/O, serial/network streams and cancellation |
| SSH for M1 | **System OpenSSH via PTY** | Existing `ssh_config`, ProxyJump, agent, known_hosts, compatibility |
| Local shell | `portable-pty` / compatible PTY abstraction | True interactive process and terminal size |
| Serial M2 | `tokio-serial` (evaluate maintained release) | Async Linux serial access |
| Telnet M2 | Dedicated Telnet protocol adapter | Explicit negotiation and unsafe-protocol labeling |
| Data store | SQLite (`rusqlite` or `sqlx`) | Reliable local sessions, folders, migrations |
| Secret store | Linux Secret Service (M3) | Keep credentials out of regular database |
| State/UI | TanStack Query for DB RPC, Zustand or equivalent for ephemeral UI | Clearly separate persisted vs runtime state |
| Test automation | Rust unit/integration, Vitest, Playwright/webview-compatible UI tests + real device lab | Test real protocols, not screenshots alone |
| Packaging | `.deb` first, AppImage later | Ubuntu-native distribution and convenience |

### 7.2 Crucial technical tradeoff: OpenSSH subprocess vs embedded SSH library

**Why OpenSSH subprocess first:** direct support for established SSH configuration, authentication agents, host-key database, bastions, and existing Linux behavior. A homemade SSH client may miss GSSAPI, certs, agent support, or legacy vendor interoperability even if it connects to a demo Ubuntu server.

**Disadvantages:** per-connection process overhead, harder credential automation, terminal-mode/auth-stage detection limitations. Abstract behind a provider interface from day one. Evaluate `russh` (or an audited equivalent) as a **separate technical spike** once compliance tests exist. The native library is not a prerequisite for M1 and must not silently replace OpenSSH.

OpenSSH subprocess construction must use a fixed binary plus individual validated arguments; **never** `sh -c "ssh ${userInput}"`. Treat SSH config alias, host, port, identity path and `ProxyJump` as separate typed values. Hostname begins with `-` => reject. Disallow arbitrary extra CLI flags in untrusted import records.

### 7.3 Runtime data flow

```text
User types / UI button
      │
      ▼
React terminal component → xterm.onData / onResize
      │
      ▼
Typed Tauri IPC: session_write(sessionId, bytes) / session_resize(rows, cols)
      │
      ▼
Rust SessionSupervisor (sessionId → ConnectionProvider, cancellation, bounded queue)
      │
      ├── OpenSSHProcessProvider ── PTY ── /usr/bin/ssh ── remote network device
      ├── LocalShellProvider  ── PTY ── user's shell
      ├── TelnetProvider      ── TCP + negotiation ── legacy device (M2)
      └── SerialProvider      ── serial port ── console device (M2)
      │
      ▼
Output reader → bounded buffered chunks → typed terminal-output events
      │
      ▼
React session router → correct xterm instance → render
      │
      ├── optional logging stream
      └── future search/automation hooks
```

### 7.4 Output-stream design

- Do not issue one Tauri event per received byte; use batching (e.g. 8–32 KB or 10–20 ms) with bounded queues.
- Send raw bytes safely (e.g. binary-capable channel or encoding such as base64) and preserve order. Avoid unintended UTF-8 splitting between chunks by using appropriate stream decoder at terminal boundary.
- Sequence events per session and enforce stable association to `sessionId`; after close, ignore stale output for destroyed sessions.
- Backpressure: bounded channel, backoff/drain policy, useful notification. **Never silently drop bytes** while claiming a faithful terminal session.
- Terminal scrollback size is bounded per session; inactive tab output may continue in background without switching focus.
- Concurrency: avoid a single global lock around all session I/O.

### 7.5 State separation

**Persisted state:** saved profiles, folders, tags, settings, snippets, known application workspaces, snapshot definitions, log metadata.

**Ephemeral state:** PTY handle, TCP/socket state, live tab ID, connection state, visible terminal buffer, UI focus, tab working context, progress/event listeners.

Never serialize PTY handles, passwords, full terminal buffers or network connections into JSON snapshot metadata.

---

## 8. Suggested database model

Use SQLite migrations and foreign keys. `id` = UUID/ULID string; secret references point to keyring, not plaintext credentials.

```text
folders(
  id PK, parent_id FK nullable, name, sort_order,
  created_at, updated_at
)

sessions(
  id PK, folder_id FK nullable, name, host, port,
  protocol ENUM['ssh','telnet','serial','local'],
  username nullable, ssh_alias nullable, identity_file nullable,
  tags_json, description, notes,
  settings_override_json, credential_ref nullable,
  is_favorite, sort_order, created_at, updated_at
)

command_groups(id PK, parent_id nullable, name, sort_order)
commands(
  id PK, group_id FK nullable, name, content,
  vendor_profile nullable, execute_mode ENUM['insert','send'],
  risk_level, variables_schema_json, tags_json,
  created_at, updated_at
)

workspaces(id PK, name, description, layout_json, created_at, updated_at)
application_settings(key PK, value_json, schema_version)
log_index(id PK, session_id nullable, file_path, started_at, ended_at, size_bytes)
recent_connections(id PK, session_id nullable, safe_host_label, last_used_at)
```

Additional tables may be introduced only when needed. Timestamps stored in UTC ISO-8601. Avoid storing credentials inside `sessions.settings_override_json` or `application_settings` through generic forms. Migrations must be reversible where practical and accompanied by backup instructions.

### 8.1 Example saved session JSON (export metadata only)

```json
{
  "schemaVersion": 1,
  "sessions": [
    {
      "id": "a7e2e9d3-0000-4000-8000-33c3fd17e32e",
      "name": "CORE-JKT-01",
      "protocol": "ssh",
      "host": "192.0.2.10",
      "port": 22,
      "username": "operator",
      "folderPath": ["Production", "IP Core"],
      "tags": ["cisco", "core", "jakarta"],
      "sshAlias": null,
      "identityFile": null,
      "terminal": {
        "fontSize": 13,
        "scrollback": 5000,
        "colorScheme": "default-dark"
      }
    }
  ]
}
```

`192.0.2.10` is a documentation-only test address (RFC 5737), not a device to attempt in live production.

---

## 9. Proposed Rust/Tauri service interfaces

Interface descriptions are **architecture contracts**, not ready-to-compile exact crate APIs. Choose actual Rust signatures after prototyping.

```rust
// Domain concepts / pseudocode — refine in implementation
struct SessionId(String);
struct ConnectionOptions { /* sanitized protocol-specific typed fields */ }
struct TerminalDimensions { cols: u16, rows: u16 }

enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
    Failed(String),
}

trait ConnectionProvider {
    // async: connect, read output, write bytes, resize if supported, disconnect
}
```

**Tauri commands (typed DTOs; request validated in Rust):**

| Command | Input | Expected behavior |
|---|---|---|
| `sessions_list` | filter, folder | Query saved sessions (no secrets) |
| `session_save` | validated metadata | Upsert atomically |
| `session_delete` | session ID | Metadata removal, explicit UI confirmation |
| `connection_open` | profile ID OR typed ad hoc spec | Spawn provider, return runtime session ID |
| `connection_write` | runtime session ID, bytes | Write only to owned live session |
| `connection_resize` | runtime session ID, cols/rows | PTY resize if applicable |
| `connection_close` | runtime session ID | Graceful cleanup |
| `connection_reconnect` | runtime session ID | New instance, explicit lifetime |
| `terminal_log_start` | runtime session ID + log options | Open secure local log |
| `terminal_log_stop` | runtime session ID | Flush and close file |
| `workspace_save` | layout metadata | Persist layout |
| `workspace_restore` | workspace ID | Return targets and layout; reconnect after confirmation |
| `commands_list` | filter | Return saved snippets |
| `command_dispatch` | explicit target IDs, content, mode | Validate scope, issue audited multi-send (M2) |

**Events:** `connection-state`, `terminal-output`, `terminal-error`, `terminal-exit`, `terminal-log-status`, `serial-device-change` (M2), `transfer-progress` (M3). Attach runtime ID + ordered stream sequence where relevant.

Enforce narrow Tauri capabilities. The untrusted renderer cannot access arbitrary filesystem or execute arbitrary commands. Filesystem dialogs may select import/export/log paths, but all access is validated by Rust. App UI should not silently load remote URLs, execute arbitrary JavaScript from terminal output or permit control-sequence URL injection.

---

## 10. Security requirements / threat model

### Threat actors and risks

1. **Malicious remote host** sending terminal control sequences or huge output to crash/render a misleading UI.
2. **Compromised saved session/import file** injecting process options, file paths, or command strings.
3. **Untrusted or mistaken local operator** sending config commands to the wrong device or all devices by accident.
4. **Lost/stolen laptop** exposing plain text credentials or highly sensitive terminal logs.
5. **SSH MITM** through unverified/changed host keys.
6. **Script/plugin abuse** executing code with local user's permissions.

### Mandatory controls

- Keep cryptographic trust decisions in OpenSSH/verified provider; no silent host-key bypass.
- All spawned processes use direct argv arrays, strict executable selection, validated types, and controlled environment.
- Do not use `sshpass`, plaintext SQLite passwords, or secrets in command line/environment.
- Opt-in Telnet with persistent insecure marker; never call it “secure Telnet” without transport encryption.
- Logs are opt-in, sensitive; file permissions restrictive; no external uploads.
- Clipboard paste of multiple lines or control characters requires review in M2; for M1, at minimum detect and confirm risky multi-line paste before sending.
- Broadcast starts **off**, shows exact targets, confirmation and post-send outcome.
- Disable JavaScript remote content embedding, and avoid unsanitized HTML in any terminal/sidebar tooltip.
- Dependency pinning, automated CVE advisories, release signing/checksum, and permissions review before GA.
- Respect local Linux `~/.ssh` permissions; never chmod user private keys without explicit instruction.
- No background device discovery, automatic command injection, or telemetry collection without consent.

**Security acceptance cases**:
- Session host value `-oProxyCommand=...` is rejected, no process spawned.
- CSV import containing path traversal cannot write outside configured storage.
- Unknown key prompts; changed host key blocks; user can inspect OpenSSH messages.
- An unauthorized attempt by the renderer to call a filesystem/process action is blocked by configured capabilities.
- App startup crashes while logging cannot leave a plaintext password stored in application DB.

---

## 11. Linux target, packaging, distribution

### Primary target

- Ubuntu 24.04 LTS x86_64: development and primary release validation.
- Ubuntu 22.04 LTS x86_64: compatibility release, build baseline if feasible.
- Test GNOME Wayland and X11; test KDE Plasma to cover clipboard, focus, keyboard shortcuts and resize.
- Dependencies include WebKitGTK 4.1, GTK, runtime libraries, and system OpenSSH. Follow current Tauri v2 prerequisite instructions.

### Packaging

- M1: signed or checksum-verified `.deb`; include `.desktop`, application icon (original), desktop categories, packaged license notices.
- M2: AppImage (test runtime dependencies and distribution compatibility), optional RPM.
- Artifact names/versioning conventional and distinct from SecureCRT; don't overwrite other terminal clients.
- On startup, check for `ssh` existence and show actionable install instruction if missing; no silent network package installation.
- Support user storage via XDG paths: `~/.config/<app>`, `~/.local/share/<app>`, `~/.cache/<app>`; honor XDG environment overrides.
- Clean uninstall removes application package but preserves user data by default; provide a separate, explicit user-data-delete flow.

### Build reproducibility

- Pin package manager and lockfiles (`pnpm-lock.yaml`, `Cargo.lock`).
- CI: typecheck/lint, unit tests, Rust tests, integration tests, security dependency checks, `.deb` packaging smoke test.
- Ensure `.deb` built on a compatible baseline to avoid glibc/WebKitGTK mismatches.

---

## 12. Performance and reliability SLOs

Numbers are **quality targets to measure on a documented baseline**, not guaranteed outcomes.

| Metric | Goal | Test |
|---|---|---|
| App cold start | ≤ 5 s | Ubuntu reference laptop, 5-run median |
| Warm open first terminal | ≤ 3 s | 5-run median |
| Session-manager search 1k records | ≤ 200 ms | P95 local UI input-to-results |
| Terminal responsiveness under 1 MB output burst | No visible multi-second UI stall | Benchmark local SSH lab |
| 20 SSH sessions | No cross-session I/O, crashes, orphan processes | 30-minute soak |
| Idle memory | Record baseline; target < 250 MB for 1 tab on reference rig | RSS with WebKit subprocesses included |
| 20 sessions memory | Target < 750 MB with 5k scrollback/session; tune if platform constraints | RSS total process tree |
| Connection cleanup | No orphan SSH process after close | Process-tree assertion |
| Database integrity | Zero corrupted state during controlled app termination | Crash recovery and migration tests |

UI processing latencies must exclude network RTT when reporting terminal responsiveness. Add real observability for memory, output throughput, per-session queue length and error reasons in development mode, without recording passwords.

---

## 13. Tests / QA matrix

### 13.1 Test levels

- **Unit**: parsers, host validation, port validation, session models, layout tree, template interpolation, filename sanitization, protocol safety policy.
- **Component**: sidebar tree, virtualized session list, settings overrides, command composer, connection badges, confirmation dialogs.
- **Rust integration**: spawn PTY shell, read/write/control signal, close/reopen, EOF, resize, cancellation, logging.
- **SSH integration**: local OpenSSH server containers/VM, password/key/agent auth, host key unknown/changed, ProxyJump, IPv6, timeouts, malformed hosts.
- **Telnet/serial integration**: lab server and pseudo-terminal pair plus at least one real USB-serial adapter.
- **End-to-end GUI**: GNOME Wayland/X11; keyboard events, copy/paste, tab focus, split resizing, launch from `.deb`.
- **Security**: fuzz import parsers, terminal control sequences, Unicode and path normalization, process argv injection, multi-line broadcast review.

### 13.2 Acceptance scenarios

**AT-01**: From empty install, Quick Connect -> `user@lab-host` -> interactive SSH -> execute `uname -a` -> receive output -> disconnect -> process exits.

**AT-02**: Save that host to `Lab/Servers`, restart app, open same saved session, metadata and user settings retained.

**AT-03**: Open three SSH tabs concurrently; send unique command to each; assert each output only in its matching xterm, including when one tab is inactive.

**AT-04**: Open local shell -> `top` -> resize window -> keyboard works and terminal receives new dimensions.

**AT-05**: SSH to host with changed key -> show OpenSSH warning and deny unsafe silent continuation.

**AT-06**: Drag 2 tabs into split layout, close and reopen app, restore saved layout with explicit reconnect consent.

**AT-07**: Highlight `down` via regex, then stream 100k lines; UI remains usable; turning off highlight stops decorations.

**AT-08**: Import CSV with valid, duplicate, malformed and suspicious host rows; preview and report, never run malicious host value.

**AT-09**: Broadcast benign command to selected two of three devices, verify third receives no bytes; disable/lock target and verify excluded.

**AT-10**: Serial port unplug while active yields readable connection error; close releases lock and does not crash app.

**AT-11**: Log session to disk, inspect file mode and contents; no file generated for sessions with logging disabled.

**AT-12**: Send multiline paste containing line breaks and potential configuration mode changes; UI shows full preview and target before sending.

**AT-13**: Run in Wayland and X11, switch tab rapidly while remote sends output; focus and clipboard remain correct.

**AT-14**: Twenty sessions disconnect and reconnect repeatedly; check for memory growth and orphan child processes.

**AT-15**: Launch packaged `.deb` from desktop launcher outside development shell; OpenSSH executable resolution, fonts and resources work.

---

## 14. Implementation phases / Codex delivery backlog

### Phase 0 — Repository foundation and architecture ADRs

Deliverables:
- `README.md`, `PRD.md`, `ARCHITECTURE.md`, `SECURITY.md`, `LICENSE` (chosen project license), `CONTRIBUTING.md`, `docs/adr/`.
- Tauri v2 + React TS + Vite setup, ESLint/Prettier/Rustfmt/Clippy, tests and CI.
- ADR-001 connection engine (OpenSSH PTY), ADR-002 persistence (SQLite), ADR-003 frontend terminal isolation, ADR-004 credentials.
- Verify a **real** local PTY echo app before building beautiful UI.

Exit gate: `pnpm dev` starts app, Rust integration test can open shell PTY, read/write, resize and close.

### Phase 1 — Terminal engine, real SSH and session model

Deliverables:
- Reusable `TerminalPane` xterm component.
- Rust `SessionSupervisor` with provider abstraction.
- `LocalShellProvider` and `OpenSSHProcessProvider`.
- Strict structured argument builder, host validation and error handling.
- `Quick Connect` screen, `New Local Shell` and independent tabs.
- Backend connection event routing and cancellation.

Exit gate: AT-01, AT-03, AT-04, AT-05 passing on a real OpenSSH lab.

### Phase 2 — Saved sessions / app shell

Deliverables:
- SQLite migrations, saved sessions CRUD, folder tree, filtering, settings, dark/light theme.
- Basic logging and search; profile display, reconnect, tab status.
- Keyboard shortcuts/clipboard/multi-line paste confirmation.
- `.deb` build and smoke testing.

Exit gate: AT-02, AT-11, AT-12, AT-15 passing. **Ship first usable MVP only now.**

### Phase 3 — Core network engineering productivity

Deliverables:
- Splits, pane focus, workspace layouts/snapshots.
- Command Manager, Button Bar, Composer, per-vendor snippets.
- Broadcast target management/confirmation, regex highlighting.
- CSV/TSV import and metadata export, Active Sessions Manager.

Exit gate: AT-06 through AT-09 plus AT-13 (relevant portions) passing.

### Phase 4 — Protocol expansion

Deliverables:
- Proper Telnet implementation and insecure banner.
- Linux serial console via asynchronous serial adapter.
- Additional device interoperability suite; serial permissions UX.

Exit gate: AT-10 and Telnet negotiation tests passing on lab gear.

### Phase 5 — Advanced features

Deliverables:
- SFTP UI, SSH tunnel management, credential references/keyring, automation API.
- Script recorder with secret-aware exclusions, vendor-specific test lab.
- Evaluate native SSH provider after functional and security equivalence benchmark.

Exit gate: all added functions pass threat-model and integration tests.

### Hard stop: no fake completion

Codex must **never** mark a feature implemented based solely on a visible button, an empty stub, test snapshots, or placeholder static terminal text. Each feature must have a test path that demonstrates actual behavior.

---

## 15. Suggested repository layout

```text
opsterm/
├── README.md
├── PRD.md
├── ARCHITECTURE.md
├── SECURITY.md
├── docs/
│   ├── adr/
│   ├── protocol-testing.md
│   ├── compatibility-matrix.md
│   ├── security-checklist.md
│   └── development-plan.md
├── src/
│   ├── app/
│   ├── components/
│   │   ├── layout/
│   │   ├── terminal/
│   │   ├── sessions/
│   │   ├── commands/
│   │   └── settings/
│   ├── features/
│   │   ├── connections/
│   │   ├── sessions/
│   │   ├── workspaces/
│   │   ├── logging/
│   │   └── imports/
│   ├── state/
│   ├── types/
│   └── tests/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── domain/
│   │   ├── commands/
│   │   ├── session_supervisor/
│   │   ├── providers/
│   │   │   ├── local_pty.rs
│   │   │   ├── openssh_pty.rs
│   │   │   ├── telnet.rs
│   │   │   └── serial.rs
│   │   ├── persistence/
│   │   ├── security/
│   │   └── logging/
│   ├── capabilities/
│   ├── migrations/
│   └── tests/
├── e2e/
├── .github/workflows/
├── package.json
├── pnpm-lock.yaml
└── Cargo.lock
```

The exact file layout can change to match Tauri conventions; avoid a single huge `main.rs`, `App.tsx` or one store owning all sessions.

---

## 16. Dependencies and due diligence

- [Tauri architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri security model and IPC trust boundaries](https://tauri.app/security/)
- [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri Debian packaging guidance](https://v2.tauri.app/distribute/debian/)
- [xterm.js documentation](https://xtermjs.org/docs/guides/using-addons/)
- [xterm.js GitHub and maintained addons](https://github.com/xtermjs/xterm.js)
- [russh native SSH engine candidate](https://github.com/Eugeny/russh) — research only until separate ADR.
- [tokio-serial documentation](https://docs.rs/tokio-serial/latest/tokio_serial/)
- [Freedesktop Secret Service API](https://specifications.freedesktop.org/secret-service/latest/ch01.html)

Before adding dependency: check current maintenance, features, Linux compatibility, transitive licensing, security issues, and minimum Rust/Node version. Pin exact versions in lockfiles; do not invent exact package API methods in implementation.

---

## 17. Risks and mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| SSH CLI/auth prompts behave differently by distro | High | Real lab integration with password/key/agent/ProxyJump, explicit tests |
| Tauri WebKitGTK GPU terminal issues | High | Renderer fallback, Wayland/X11 test plan, default xterm renderer first |
| PTY process and child cleanup on crash | High | Supervisory cleanup, bounded termination, orphan-process tests |
| High output overwhelms webview | High | Chunked events, bounded queues, throttled rendering, soak tests |
| Wrong-device broadcast command | Critical | Explicit target list, arm, review, locked-session exclusion |
| Host-key verification disabled accidentally | Critical | Strict defaults, security regression tests |
| Secrets leaked into logs/imports | Critical | Keyring-only secret storage, opt-in recording, secret scanning |
| Complex script engine executes untrusted code | Critical | Defer until explicit run permissions, isolate subprocess, threat model |
| Vendor device syntax differs by firmware | Medium | Verified examples, per-vendor/version profile metadata |
| Scope creep from full SecureCRT parity | High | Milestone gates and honest feature status |
| Dependency/licensing conflict | High | SBOM/licenses review in CI before distribution |
| Building on new glibc breaks Ubuntu 22.04 | Medium | Oldest supported build baseline |

---

## 18. Definition of Done (per feature)

A feature is **Done** only when:

1. Real behavior works end-to-end on the intended platform, not just UI.
2. Typed API, input validation and error states are implemented.
3. Unit/integration/e2e tests appropriate to risk are included and passing.
4. Keyboard and mouse workflows function, focus is correct, and error feedback is visible.
5. Security assumptions are documented and no new secrets are written to plaintext.
6. Data persists if applicable; migrations/backups considered.
7. There is no hardcoded fake remote output or hidden network dependency.
8. README/user instructions and acceptance evidence are updated.
9. Build succeeds for Ubuntu target; no Clippy/TypeScript errors.
10. Feature is marked in a `FEATURE_STATUS.md` with `Planned / In Progress / Implemented / Tested`.

Global GA gate: real SSH, PTY, 20-session isolation test, host-key test, crash recovery, import injection test, command-broadcast safety (if shipping), `.deb` install/uninstall validation.

---

## 19. Concrete Codex execution instructions

**Use the following as the opening message to Codex after copying this PRD into the repository as `PRD.md`:**

```text
You are the lead engineer implementing OpsTerm: an original Linux-first desktop
terminal client inspired by SecureCRT workflows. Read PRD.md in full.

Constraints:
- Target Ubuntu 24.04 LTS first, then Ubuntu 22.04.
- Stack: Tauri v2 + Rust + Tokio + React/TypeScript + xterm.js + SQLite.
- Implement an ACTUAL terminal and SSH connection, not a static mock.
- M1 SSH backend is the installed OpenSSH binary spawned via a safe PTY
  interface using typed arguments. Respect known_hosts, ssh_config, ssh-agent
  and ProxyJump; never disable host-key checking or use sshpass.
- Never embed passwords in SQLite, process argv, logs, or terminal UI state.
- Clean professional UI, compact, charcoal/light neutral palette, NO neon.
- Ensure local shell uses a real PTY; each tab has isolated input/output.
- Follow security gates and acceptance tests in PRD.md.
- Work sequentially: M0 -> M1 -> M2. Do not implement all features at once.
- Maintain ARCHITECTURE.md, FEATURE_STATUS.md, and docs/adr/*.
- Before coding: inspect repository, make an implementation plan and identify
  risky assumptions. Then implement Phase 0 and Phase 1 only.
- Do not claim a feature is complete unless its real end-to-end test passes.
- When blocked, report exact error and what needs to be changed instead of
  replacing real behavior with a stub.

After Phase 1, deliver:
1. Working app source and the exact Ubuntu setup commands.
2. A demonstrable SSH and local-shell PTY terminal.
3. Tests and logs of their results with secret redaction.
4. Current file tree, architecture decisions, and next-step backlog.
```

### Codex-sized work tickets (prioritized)

**T-001**: Scaffold Tauri v2/React/TS/Rust; verify Ubuntu development environment.\
**T-002**: Rust PTY local shell spike, read/write/resize/cancellation tests.\
**T-003**: xterm.js pane, input/output binding, resize and clipboard.\
**T-004**: SessionSupervisor, typed events, session lifecycle and isolation.\
**T-005**: Secure OpenSSH argv builder, host/port validation, known_hosts tests.\
**T-006**: SSH Quick Connect with actual server + error states.\
**T-007**: Tab management and reconnect/close flow.\
**T-008**: SQLite session/folder models and migrations.\
**T-009**: Session Manager CRUD/tree/filter/search.\
**T-010**: Settings, themes and terminal typography.\
**T-011**: Logging, output search and multiline paste confirmation.\
**T-012**: Ubuntu `.deb` packaging + E2E M1 exit gate.\
**T-013**: Workspace splits, snapshots and Active Sessions Manager.\
**T-014**: Command Manager, Button Bar and reviewed multi-target Composer.\
**T-015**: Telnet adapter and serial adapter with lab tests.\
**T-016**: SFTP, keyring references, scripting following separate security ADRs.

### Suggested issue format

```markdown
## T-00X — Title
Priority: P0/P1/P2
Milestone: M0/M1/M2/M3

### User-facing outcome
[observable behavior]

### Technical deliverables
- [ ] ...

### Edge cases / security
- [ ] ...

### Acceptance tests
- [ ] Real end-to-end test ...

### Evidence
Commands run, test output, screenshots where useful, known limitations.
```

---

## 20. Acceptance checklist for owner

Before considering M1 complete, manually verify on Ubuntu:

- [ ] Installer and desktop launcher work without developer terminal.
- [ ] Local shell opens and responds correctly to key events.
- [ ] SSH connection to a real Ubuntu/Linux lab host works.
- [ ] SSH config alias, agent/key and ProxyJump have passing tests.
- [ ] Host-key mismatch is blocked rather than automatically bypassed.
- [ ] Multiple tabs are independent; closing one leaves others alive.
- [ ] Reconnect produces a genuinely new SSH process/session.
- [ ] Font/theme changes apply and persist after restart.
- [ ] Session Manager saved folders work after restart.
- [ ] Terminal selection/copy/paste and multiline review work.
- [ ] Log output writes to intended local path, with restrictive permissions.
- [ ] Performance and child-process cleanup smoke tests pass.
- [ ] No hardcoded credentials, fake sessions or UI-only implementation.

Before considering M2 complete, additionally verify:

- [ ] Split panes are independent and accurately resized.
- [ ] Snapshot restore recreates layout and requests reconnection consent.
- [ ] Command Manager and Button Bar work on a lab device.
- [ ] Multi-target Composer cannot accidentally target all devices without arming.
- [ ] Telnet displays persistent unencrypted security warning.
- [ ] Serial console works with real USB-to-serial hardware and releases device on close.
- [ ] CSV import has row preview, validation and conflict strategy.

---

## 21. Open product/engineering decisions

These should be written as ADRs, not silently assumed:

1. Confirm final product name, logo and distribution license; no VanDyke branding.
2. Decide whether Ubuntu 22.04 compatibility is mandatory from first release or one milestone later.
3. Decide whether workspaces open saved sessions automatically or only after explicit confirmation (recommended default: confirmation).
4. Determine serial adapter hardware and network lab platforms available for automated E2E tests.
5. Confirm command-broadcast rules for production vs lab targets and whether read-only mode should be per-session.
6. Validate Tauri/WebKitGTK terminal performance on real GNOME/KDE and establish baseline metrics before promising large-scale capacity.
7. Decide when/how a native SSH engine can match agent, ProxyJump, host-key, and algorithm behavior safely.

**Recommended default decisions:** Linux x86_64 / Ubuntu 24.04 first; OpenSSH PTY engine; `.deb` first; SSH and local PTY before Telnet/Serial; no cloud sync; no secrets database; safe broadcast disabled by default; compact dark professional UI.

---

## 22. Future enhancements differentiated for ISP/network operations

These are **new product ideas**, not features attributed to SecureCRT:

- NOC-centric device inventory labels: POP, region, vendor, ASN, device role, environment.
- Session group presets: `Core`, `Aggregation`, `Metro-E`, `BNG`, `OLT`, `Datacenter`.
- Device-context ribbon showing which router is being modified, with a stronger visual warning for Production.
- Opt-in vendor CLI pager helper, verified against per-model/per-firmware tests.
- Named incident workspaces: `BGP Loss Investigation`, `OLT Down`, `Backbone Maintenance`.
- Read-only collection mode to capture `show`/`display` evidence from selected devices with target-specific logs (no universal promise of detecting safe commands).
- Manual config snapshot/diff tools with audit notes and secret masking, following security review.
- Planned VPN/Bastion connection readiness checks (only with user-configured endpoints).
- Per-customer and per-environment workspaces without requiring cloud accounts.

These should never block the baseline terminal and connection stability.

---

## Final principle

**Build a dependable terminal product before expanding the feature list.**
The first success is not “looks like SecureCRT”; it is “an engineer can safely and reliably use it for daily work on real routers, switches, OLTs, and servers under Ubuntu.”
