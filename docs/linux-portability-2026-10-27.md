# Linux portability and active-session close confirmation — 2026-10-27

## Scope and evidence

The previous v0.1.4/v0.1.5 executable was built on Ubuntu 24.04, required GLIBC_2.39, and linked the host libdbus/libsystemd dependency chain. This establishes a genuine compatibility limit for older distributions. The user did not provide CachyOS error text/steps; it does not establish the cause of that particular report.

The v0.1.6 release workflow builds on Ubuntu 22.04 for x86_64 and aarch64, with glibc <=2.35 and standard shared dependencies enforced by readelf. An explicit Linux-only vendored D-Bus feature removes the host libdbus/libsystemd chain. AppImage and archive formats accompany the original native binary. Graphics/display drivers, OpenSSH and curl remain host tools. The defined target is mainstream glibc>=2.35 desktops, not every Linux ABI, architecture, GPU or non-FHS system.

AppImage tooling/runtime use immutable upstream release URLs and pinned SHA-256 checks before execution. No drivers or global LD_LIBRARY_PATH override are bundled. Archive installation is atomic and unprivileged; optional absolute --prefix permits safe testing without changing HOME. Packages include full D-Bus AFL-2.1 licensing/source notices.

The updater distinguishes native/AppImage signed asset keys, validates the actual AppImage runtime environment/outer file, hashes and replaces that outer file, rejects format mismatches and keeps the previous artifact. Restart removes only AppImage launcher variables and uses extraction mode so FUSE is optional. Existing native update behavior/trust key is retained.

## Close behavior

Connected SSH/Telnet/serial sessions and detected local foreground/background/exec jobs prompt. Opening placeholders, exited tabs and recognized idle local prompts close immediately. Native window close and restart count only risky sessions. Keyboard input is blocked for the closing frame even if no modal is necessary; Cancel/Escape/default focus/stable IDs/scope escalation protections remain.

Linux checks foreground process group, shell children, executable identity and termios editing state off the UI thread. Common readline/ZLE idle states may close directly. Canonical/unknown editing states conservatively prompt. Prompt-like editing builtins (e.g. read -e) are not universally distinguishable without shell integration; that narrower heuristic is documented rather than adding intrusive profile hooks.

## Verification before publication

- Rebased onto the already released v0.1.5 input/mouse/theme fixes (6bea200), preserving them; version is v0.1.6.
- Guarded merged full Rust suite: 59 core + 134 egui tests passed. Formatting and clippy all-targets with -D warnings passed.
- TDD: original disconnected/opening close regressions failed 2/2; fixed versions passed. Builtin read/loop false-idle regressions failed 2/2 before the termios refinement and passed afterwards. Distribution resolver stub failed four tests before implementation. Explicit-backend stub failed both tests before implementation.
- Twenty Python packaging/publishing tests passed, including six-asset cryptographic merge with cross-key rejection, hash tampering, filename traversal, baseline ELF checks, pinned download verification, installer path escaping and preserved argv.
- actionlint passed for CI and the new matrix release workflow. Signing module CLI invocation was verified; an independent review caught and corrected its initial direct-script import issue.
- Real no-FUSE AppImage update smoke passed on the merged v0.1.6 binary using private temporary local manifests and the existing signing key through openssl (never printed/read into logs). It proved wrong-format refusal, signed outer-image replacement, rollback backup and outer hash equality/up-to-date detection. These test images/manifests were temporary, never published.
- Real extracted AppImage GUI smoke passed with active Telnet cancellation, immediate disconnected close, and descendant-process cleanup; temporary runtime files are scoped to each fixture.
- Native close GUI smoke passed: idle X immediate, foreground sleep popup, Escape/Enter cancellation, native close popup, Ctrl+C return to idle, idle native close immediate. Screenshots were visually inspected.
- Real loopback Telnet GUI smoke passed: named GNS3 launches, command roundtrip, two simultaneous windows, active-close modal/cancel, disconnected immediate close.
- Ubuntu 24.04 disposable container: real X11 Telnet GUI and private headless Weston Wayland buffer attach/commit passed.
- Official cachyos/cachyos disposable container: real X11 Telnet GUI and private headless Weston Wayland buffer attach/commit passed. Package-manager hooks emitted nonfatal network-namespace isolation warnings inside Docker; GUI tests themselves passed. This is software graphics/container coverage, not a reproduction of the unspecified KDE/GPU issue on the user's machine.
- Independent read-only review approved the defined scope after fixes, noting the documented shell-heuristic/ABI limits.

Logs/screenshots remain outside the repository under /tmp/opsterm-portable-* for review. Before final completion, consumed build/test caches and task-created container images are removed with the mandated guarded cleanup. Public release results and installation are recorded after publication.
