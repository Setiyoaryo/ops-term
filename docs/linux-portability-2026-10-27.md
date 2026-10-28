# Linux portability and active-session close confirmation — 2026-10-27

## Scope and evidence

The previous v0.1.4/v0.1.5 executable was built on Ubuntu 24.04, required GLIBC_2.39, and linked the host libdbus/libsystemd dependency chain. This establishes a genuine compatibility limit for older distributions. The user did not provide CachyOS error text/steps; it does not establish the cause of that particular report.

The v0.1.7 release workflow builds on Ubuntu 22.04 for x86_64 and aarch64, with glibc <=2.35 and standard shared dependencies enforced by readelf. An explicit Linux-only vendored D-Bus feature removes the host libdbus/libsystemd chain. AppImage and archive formats accompany the original native binary. Graphics/display drivers, OpenSSH and curl remain host tools. The defined target is mainstream glibc>=2.35 desktops, not every Linux ABI, architecture, GPU or non-FHS system.

AppImage tooling/runtime use immutable upstream release URLs and pinned SHA-256 checks before execution. No drivers or global LD_LIBRARY_PATH override are bundled. Archive installation is atomic and unprivileged; optional absolute --prefix permits safe testing without changing HOME. Packages include full D-Bus AFL-2.1 licensing/source notices.

The updater distinguishes native/AppImage signed asset keys, validates the actual AppImage runtime environment/outer file, hashes and replaces that outer file, rejects format mismatches and keeps the previous artifact. Restart removes only AppImage launcher variables and uses extraction mode so FUSE is optional. Existing native update behavior/trust key is retained.

## Close behavior

Connected SSH/Telnet/serial sessions and detected local foreground/background/exec jobs prompt. Opening placeholders, exited tabs and recognized idle local prompts close immediately. Native window close and restart count only risky sessions. Keyboard input is blocked for the closing frame even if no modal is necessary; Cancel/Escape/default focus/stable IDs/scope escalation protections remain.

Linux checks foreground process group, shell children, executable identity and termios editing state off the UI thread. Common readline/ZLE idle states may close directly. Canonical/unknown editing states conservatively prompt. Prompt-like editing builtins (e.g. read -e) are not universally distinguishable without shell integration; that narrower heuristic is documented rather than adding intrusive profile hooks.

## Verification before publication

- Rebased onto the already released v0.1.5 input/mouse/theme fixes (6bea200), preserving them; version is v0.1.7.
- Guarded merged full Rust suite: 59 core + 134 egui tests passed. Formatting and clippy all-targets with -D warnings passed.
- TDD: original disconnected/opening close regressions failed 2/2; fixed versions passed. Builtin read/loop false-idle regressions failed 2/2 before the termios refinement and passed afterwards. Distribution resolver stub failed four tests before implementation. Explicit-backend stub failed both tests before implementation.
- Twenty Python packaging/publishing tests passed, including six-asset cryptographic merge with cross-key rejection, hash tampering, filename traversal, baseline ELF checks, pinned download verification, installer path escaping and preserved argv.
- actionlint passed for CI and the new matrix release workflow. Signing module CLI invocation was verified; an independent review caught and corrected its initial direct-script import issue.
- Real no-FUSE AppImage update smoke passed on the merged v0.1.7 binary using private temporary local manifests and the existing signing key through openssl (never printed/read into logs). It proved wrong-format refusal, signed outer-image replacement, rollback backup and outer hash equality/up-to-date detection. These test images/manifests were temporary, never published.
- Real extracted AppImage GUI smoke passed with active Telnet cancellation, immediate disconnected close, and descendant-process cleanup; temporary runtime files are scoped to each fixture.
- Native close GUI smoke passed: idle X immediate, foreground sleep popup, Escape/Enter cancellation, native close popup, Ctrl+C return to idle, idle native close immediate. Screenshots were visually inspected.
- Real loopback Telnet GUI smoke passed: named GNS3 launches, command roundtrip, two simultaneous windows, active-close modal/cancel, disconnected immediate close.
- Ubuntu 24.04 disposable container: real X11 Telnet GUI and private headless Weston Wayland buffer attach/commit passed.
- Official cachyos/cachyos disposable container: real X11 Telnet GUI and private headless Weston Wayland buffer attach/commit passed. Package-manager hooks emitted nonfatal network-namespace isolation warnings inside Docker; GUI tests themselves passed. This is software graphics/container coverage, not a reproduction of the unspecified KDE/GPU issue on the user's machine.
- Independent read-only review approved the defined scope after fixes, noting the documented shell-heuristic/ABI limits.

Logs/screenshots remain outside the repository under /tmp/opsterm-portable-* for review. Before final completion, consumed build/test caches and task-created container images are removed with the mandated guarded cleanup. Public release results and installation are recorded after publication.

The unpublished v0.1.6 matrix found a pre-existing ARM64-only test compile error: serial PTY name buffers used i8 rather than libc::c_char (unsigned on ARM64). The buffer now follows the platform C ABI. The failed release run 40483648510 was cancelled before publication; its tag remains as history, and the corrected release uses v0.1.7.

The first v0.1.7 release matrix compiled and passed Rust/clippy and created both portable formats, but both native GUI startup probes exited before a visible window while stderr was discarded. Separate ARM diagnostics then passed all GUI probes in both debug (40488919366) and optimized (40490059284) builds, including before the extra runtime packages were installed. The original startup failure was not reproduced in these diagnostic runs, so no speculative application fix was claimed.

Release infrastructure now captures startup errors/exit status, declares complete software X11/EGL dependencies, and also checks the original /dev/null stdio mode. A manual release_tag input rebuilds an existing unpublished immutable tag using current infrastructure; checkout equality and trusted-main ancestry are checked before repository code runs. Application sources/public key remain exactly from that tag; only the trusted-main test diagnostic helper is overlaid. All product, signature, architecture and distro gates remain required. Independent focused review approved this path after validation-order hardening. No public tag or published asset was rewritten.

## Published release verification — 2026-10-28

- Immutable source tag v0.1.7 (a6a096a), release workflow 40492894438: both native architectures, all GUI/signature/update gates, all seven distribution jobs, and publication passed. Main CI 40492891843 and Pages 40495378540 passed. Latest stable release and live website both report v0.1.7.
- All six downloaded official payloads passed SHA-256, size and Ed25519 manifest verification using the repository public key. Nineteen public assets include the manifest and signature/checksum sidecars.
- Installed ~/.local/bin/opsterm-egui reports v0.1.7 and matches official x86_64 SHA-256 057cfd609277bb2b1f9081b1f82c5faeaa829d7357bf71d3c74d2329a11c2b61. Its --check-update reports up to date. Existing application windows were left running.
- Fresh installed-release Xvfb close smoke passed: idle local tab closes immediately, foreground sleep prompts, Escape/Enter cancel, Ctrl+C returns to idle.
- Public release: https://github.com/Setiyoaryo/ops-term/releases/tag/v0.1.7 ; pipeline: https://github.com/Setiyoaryo/ops-term/actions/runs/40492894438 .
- The unspecified original CachyOS symptom remains unconfirmed; passing container startup is compatibility evidence rather than proof about the user's GPU/desktop.
- Fresh installed-release GNS3/Telnet smoke passed real command roundtrip, simultaneous windows, active-close cancellation and immediate disconnected-tab close; active/disconnected screenshots were inspected.
- Both repository worktrees were cleaned with npm run clean -- --dist; target/dist caches are absent. Only task-created Ubuntu 22.04 and CachyOS Docker images were removed; pre-existing images were preserved.
