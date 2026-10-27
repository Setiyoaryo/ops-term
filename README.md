# OpsTerm

A Linux-first, open-source terminal workspace for network operations: connect to
routers, switches, OLTs, firewalls and servers at scale with real SSH, local
shells, Telnet and serial sessions, an organised device library, and safe
multi-session workflows.

> **Status:** the feature set spans M1–M3 and beyond. See
> [`FEATURE_STATUS.md`](FEATURE_STATUS.md) for exactly what is real vs planned.
> Nothing in this repository simulates remote output.

OpsTerm is an **independent** terminal workspace for network operations,
built with original code, assets and branding.

## Portable Linux packages

[Download the latest signed release](https://github.com/Setiyoaryo/ops-term/releases/latest).
Both **x86_64** and **aarch64/ARM64** have a native binary, `.AppImage`, and portable
`.tar.gz` archive. Releases target **glibc 2.35 or newer** (Ubuntu 22.04+, Debian
12+, and current Fedora/Arch/CachyOS), with the host graphics/display stack.
This is not a promise for musl/Alpine, 32-bit Linux, older glibc or an untested GPU.
The current egui interface, local shell, SSH tools and GNS3 integration are retained.

For example, on an x86_64 machine after downloading the v0.1.7 AppImage:

```bash
chmod +x opsterm-egui-0.1.7-linux-x86_64.AppImage
./opsterm-egui-0.1.7-linux-x86_64.AppImage
```

If FUSE is unavailable, add `--appimage-extract-and-run`. AppImage updates verify a
separate signed image asset and replace the outer file with a rollback backup;
they never write to the mounted executable or replace an image with a raw binary.
Use a regular image file; updating through symlink paths is refused.

The archive includes `install.sh` for an atomic per-user install, desktop/icon
files, `diagnose.sh`, and third-party notices. After extracting it, run
`./install.sh` (default `~/.local`) or `./install.sh --prefix /absolute/prefix`.
The default preserves the existing `~/.local/bin/opsterm-egui` GNS3 command path.
Host tools such as OpenSSH and `curl`, and your Mesa/vendor graphics drivers,
remain host prerequisites. No Flatpak host-shell sandbox changes are required.

For display diagnosis, `OPSTERM_DISPLAY_BACKEND=x11` or `=wayland` explicitly
selects that backend; the default is `auto`. The option preserves the environment
inherited by terminal programs. X11 requires an available X11/XWayland server;
Wayland requires a running compositor. `diagnose.sh` reports only public system
capabilities and whether display variables are set, without dumping credentials.

## Download and updates

Download the Linux x86_64 egui binary and checksum from
[GitHub Releases](https://github.com/Setiyoaryo/ops-term/releases/latest).
Verify the checksum before installing into a user-writable location such as
`~/.local/bin/opsterm-egui`; detailed instructions accompany each release.

The binary checks the signed GitHub update feed automatically. **Help → Check
for updates** checks immediately and offers installation. Existing sessions
continue until you choose to restart. Custom update feeds and local development
builds remain supported. See [auto-update setup](docs/auto-update.md).

**Settings → Appearance** provides terminal font family/weight, four text
palettes, brightness and live preview. See [appearance settings](docs/terminal-appearance-2026-10-26.md).

CI and tagged releases run on standard GitHub-hosted runners. No laptop runner
or paid distribution service is required. The existing GTK/Tauri clients remain
available as source; published binaries and automatic updates cover egui only.

## Repository layout (Cargo workspace)

| Path | What it is |
|---|---|
| `core/` | `opsterm-core` — all UI-agnostic logic: domain types, validated process spawning, connection providers (OpenSSH, russh, local PTY, Telnet, serial), SQLite storage, logging, credentials, tunnels, SFTP. |
| `egui/` | `opsterm-egui` — **custom-rendered modern desktop client** (egui + `alacritty_terminal`). No GTK, no webview. Recommended client. |
| `native/` | `opsterm-native` — a GTK4 + VTE desktop client (its own window, not GNOME Terminal). |
| `src-tauri/` | The original Tauri desktop app (React web UI). |

All clients share the same database: `~/.local/share/org.opsterm.app/opsterm.db`.

## The egui client (recommended)

A single native binary with a custom desktop UI. It has no GTK or webview UI
dependency. Binary size and runtime footprint depend on the build profile,
graphics driver, terminal dimensions and open sessions.

```bash
npm run build:egui
./target/release/opsterm-egui
```

Features: SSH (OpenSSH **and** native engine) · local shell · Telnet · serial ·
tabs with drag-reorder · split (drag-resize) · 2×2 grid monitor · device editor
with **vendor profiles** (Huawei/Cisco/Juniper/MikroTik/ZTE/Linux) · import CSV · command library + button bar · **broadcast** to selected targets ·
runbooks (expect) · auto pager-disable & auto login/enable · logging per session
(`0600`) · keyword highlight (editable) + find · scratchpad · double-click
copy · SFTP browser · SSH tunnels (L/R/D) · subnet calculator · ping ·
traceroute · DNS · workspace save/restore · auto-reconnect · 4 UI themes · selectable terminal fonts and text palettes · font
zoom · output export.

Keyboard: `Ctrl+=`/`Ctrl+-`/`Ctrl+0` zoom · `Ctrl+Shift+C` or `Ctrl+Insert`
copy · `Ctrl+C` copy (with a selection) or ETX · `Ctrl+Shift+A` select all ·
`Ctrl+Shift+V` or `Shift+Insert` paste · `Ctrl+V` is sent to the application as
literal-next (`0x16`), matching xterm/Tabby. Double-click selects a word; mouse
wheel or `Shift+PageUp`/`Shift+PageDown` scrolls history. In the command editor,
`Enter` sends and `Shift+Enter` inserts a new line. Scratchpad is available
under View.

GNS3 desktop console: in **Edit → Preferences → General → Console applications**,
edit **Console application command for Telnet** and enter the installed executable's
absolute path followed by:

```text
/home/portego/.local/bin/opsterm-egui --telnet "{host}" --port {port} --title "{name}"
```

Replace `/home/portego` with your home directory on other machines. GNS3 replaces
these placeholders when you open a node's console. Each launch opens a separate
OpsTerm window with one named Telnet tab and no extra local shell; it does not
reuse an existing window or add devices to the saved inventory. The GNS3 server
address and console port must be reachable from this computer. This uses OpsTerm's
built-in Telnet transport; no external `telnet` executable is required. VNC console
settings are separate and continue using your VNC viewer. Without command arguments,
OpsTerm still opens Local Shell. Use `opsterm-egui --help` for console options.

Clipboard images: with a CLI agent such as Codex in the terminal, `Ctrl+V`
forwards its image-paste shortcut (`0x16`) when the clipboard has no text.
Text paste is `Ctrl+Shift+V` or `Shift+Insert`. The agent reads the clipboard
itself;
OpsTerm does not create image files or upload the local clipboard over SSH.
The clipboard must be available where the agent runs.

Tab X and `Ctrl+Shift+W` ask before disconnecting active SSH/Telnet/serial sessions
or interrupting a detected local command/job. Idle local prompts, disconnected
sessions and connection-opening placeholders close directly. Window X and update
restart ask only when risky sessions remain. **Cancel** receives initial focus;
**Escape** cancels; **Ctrl+Enter** confirms after the prompt has been shown.
Cancelling restores the previously focused field or terminal. Linux local activity
uses foreground/background processes, executable identity and terminal editing
state; unknown/canonical shell states conservatively ask. Builtins that deliberately
use prompt-like raw editing (such as `read -e`) cannot always be distinguished
without shell integration. Settings/help dialogs keep their normal close behavior.

The default workspace uses a minimal dark theme with a flat terminal surface.
Click a device row once to open its tab immediately; connection setup runs in
the background. Clicking an open device focuses its existing tab. Right-click
and choose **Open in new tab** for a separate connection. Closing a pending tab
discards its eventual connection result. Network/authentication time still
depends on the server.

Saved inventory credentials are used automatically for SSH/Telnet. Linux secrets
are persisted in the desktop Secret Service (GNOME Keyring/KWallet), using an
encrypted D-Bus session. Available credentials from the previous kernel-only
backend migrate automatically. A missing or locked credential produces a storage
error; the app does not redirect saved-device connections into a password form.
Passwords are never saved in SQLite. Device/tab selection also focuses the
terminal keyboard automatically. See [the persistence correction](docs/credential-persistence-2026-10-23.md).

The command composer starts collapsed: use **Command** in the status bar or
`Ctrl+Shift+P`. `Ctrl+Shift+T` opens a local terminal and `Ctrl+Shift+W` closes
the current tab. The device library refreshes after edits/imports; **Refresh**
also reloads changes made by another client.

Idle rendering sleeps between events, with a one-second status/lifecycle clock.
Device rows are virtualized, terminal text is drawn in color runs, scrollback
is limited to 2,000 lines, and each session's output queue is capped at 128 KiB.
The status bar shows Linux process RSS. For a repeatable before/after benchmark,
see [the performance report](docs/performance-2026-10-22.md).

Broadcast selection follows the device across tab moves and closes. Connections
that finish opening while broadcast is enabled are initially unselected; include
them explicitly under Targets.

## The GTK4 + VTE client

```bash
npm run build:native
./target/release/opsterm-native
```

Binary ≈ 1.6 MB; GTK4 baseline memory ≈ 65 MB PSS.

## The Tauri client (web UI)

```bash
npm install
npm run tauri:dev          # develop
npm run tauri:build        # .deb
```

## Requirements

- Ubuntu 22.04 / 24.04 LTS x86_64
- Rust (stable), Python 3.10+ for guarded builds, and Node.js 20+ with npm for the guarded build scripts
- System libraries:
  - Tauri client: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev`
  - GTK4 client: `libgtk-4-dev libvte-2.91-gtk4-dev libadwaita-1-dev`
  - egui build: `libdbus-1-dev libxkbcommon-dev libx11-dev libxcb1-dev libgl1-mesa-dev libwayland-dev`; runtime needs a working GPU/GL stack
- `openssh-client` (SSH; absent → the app shows an install hint)

```bash
sudo apt update
sudo apt install -y build-essential pkg-config curl wget file openssh-client \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev \
  libgtk-4-dev libvte-2.91-gtk4-dev libadwaita-1-dev \
  libdbus-1-dev libxkbcommon-dev libx11-dev libxcb1-dev libgl1-mesa-dev libwayland-dev
```

## Test & lint

```bash
python3 scripts/build_guard.py -- cargo test --locked -- --test-threads=1
python3 scripts/build_guard.py -- cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check

npm test                                                # frontend unit tests
npm run e2e                                             # Playwright E2E
npm run build                                           # tsc + vite build
npm run test:build-guard                                 # watchdog regressions
```

CI runs all of the above on Ubuntu 24.04
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

`cargo build` / `cargo test` default to core + egui. The GTK and Tauri clients
remain available with `-p opsterm-native` / `-p opsterm`; `--workspace` includes
all clients and requires all system libraries.

### Builds on laptops with limited RAM

Cargo uses one job by default. Development builds omit debug information;
release builds disable full LTO and use 16 codegen units to reduce compiler and
linker memory. All npm build entry points run the Linux memory watchdog. Without
Node/npm, the equivalent egui command is:

```bash
python3 scripts/build_guard.py -- cargo build -p opsterm-egui --release --locked
```

The watchdog permits one guarded build at a time, samples the whole process
group every 100 ms, and stops it with exit code **75** if sampled RSS reaches
2 GiB or available memory falls below 1.5 GiB. It also respects enclosing cgroup
v2 memory limits. A stopped build is incomplete: close other memory-heavy apps
and rerun the same command; Cargo reuses completed artifacts. The watchdog does
not kill unrelated apps or clear build caches. Sampling cannot guarantee against
all sudden memory spikes or other processes exhausting RAM.

Limits can be adjusted explicitly using `OPSTERM_BUILD_MAX_MB` and
`OPSTERM_BUILD_RESERVE_MB`. Raw `cargo` commands still use one worker but do not
run the watchdog; use the guarded commands above for memory-sensitive builds.

An optional GUI smoke test uses a temporary database and a real local shell. It
checks Ctrl+C and command-editor Enter, then captures normal/minimum screenshots
under `test-results/egui/`. Requires `xvfb`, `xdotool`, and ImageMagick:

```bash
python3 scripts/build_guard.py -- timeout 30s xvfb-run -a -s '-screen 0 1280x850x24' bash scripts/smoke-egui.sh
```

## Binary and auto-update

Build a signed, self-updating binary for the egui client:

```bash
npm run update:keygen          # once; keeps the private key outside the repo
npm run package:egui:local     # binary + local file:// manifest in release/
```

Install for the current user (binary, menu entry, icons, desktop shortcut):

```bash
npm run install:desktop
```

For a local-only workflow, set the update URL to
`file:///home/<user>/…/release/manifest.json`; no server or publication is
needed. For hosted releases use
`OPSTERM_ASSET_BASE_URL=https://downloads.example.com/opsterm/v0.1.0 npm run package:egui`.

The updater verifies an Ed25519 signature, SHA-256, size and ELF header before
replacing the executable, and keeps a rollback copy. Configure the manifest URL
with `OPSTERM_UPDATE_URL` or **Help → Check for updates…**. A rebuild with the
same version but a different binary hash is also offered as an update. See
[`docs/auto-update.md`](docs/auto-update.md).

## Disk usage / build cache

The Cargo `target/` directory is a build cache and can grow to tens of GB
(dependencies for egui/Tauri/GTK). It is safe to delete at any time — the next
build regenerates it (slower). Reclaim space with:

```bash
npm run clean             # or: scripts/clean.sh
scripts/clean.sh --dist   # also removes dist/
npm run clean:all         # also removes node_modules and dist
```

Clean up after every build or test run instead of keeping the cache; the rule
is documented in [`AGENTS.md`](AGENTS.md). `target/` is git-ignored, so it is
never committed.

## Security

Every process is spawned with a structured, validated argv — never a shell
string. Host-key verification is never disabled; there is no
`StrictHostKeyChecking=no`, no `UserKnownHostsFile=/dev/null`, and no `sshpass`.
Secrets live only in the OS keyring. See [`SECURITY.md`](SECURITY.md).

## Documentation

- [`PRD.md`](PRD.md) — product requirements
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — system design and data flow
- [`SECURITY.md`](SECURITY.md) — threat model and controls
- [`FEATURE_STATUS.md`](FEATURE_STATUS.md) — honest feature tracking
- [`docs/`](docs/) — development plan, ADRs, compatibility matrix
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to work on the project

## License

MIT — see [`LICENSE`](LICENSE).

## Product website

The public landing page is [setiyoaryo.github.io/ops-term](https://setiyoaryo.github.io/ops-term/).
Its standalone source lives in `website/`, separate from the desktop frontend.

```bash
npm run pages:build                  # checked-in stable release snapshot
npm run pages:build -- --live-release # latest public GitHub release
npm run pages:test                   # desktop + mobile browser checks
npm run clean -- --dist              # after inspecting the output
```

GitHub Pages publishes after site changes on `main`, manual dispatch, or a
successful `Release egui` workflow. Builds validate the official download asset
and run browser checks before deployment. If the GitHub API is temporarily
unavailable, the build uses `website/release.json`; refresh that snapshot when
publishing a new stable release. Configure the repository's Pages source as
**GitHub Actions**. The website serves its font locally; the font's OFL license
is included in `website/assets/`.
