# GNS3 Telnet console integration — 2026-10-27

OpsTerm now accepts `--telnet HOST [--port PORT] [--title NAME]`. The GNS3 desktop custom-console command is documented in README and the v0.1.4 release notes. This uses the existing background connection queue and native Telnet provider, retaining the current egui UI. Each invocation creates a separate window with one named tab. No shared-window IPC, VNC implementation, saved-device insertion, password arguments or shell command execution was added. Existing no-argument startup remains Local Shell.

Host, port and title are validated before database/GUI initialization. Help/version work without a display. The external uncredentialed console suppresses only the automatic startup vault dialog; vault state and protected inventory access are unchanged.

## Verification

- TDD: the initial parser stub produced five failing tests and one passing default-shell test; implementing parsing made all six pass.
- Guarded complete Rust suite: 55 core + 112 egui tests passed. Includes exact GNS3 arguments, spaces/metacharacters in names, IPv6, malformed/duplicate/unknown options, no extra local tab/inventory writes, and existing close/clipboard regressions.
- Guarded formatting and clippy (`--all-targets -- -D warnings`) passed. Existing dependency future-compatibility notice for russh remains unrelated to this change.
- Six packaging/publishing tests passed.
- Display-free CLI probes: help/version return 0; invalid port/unknown argument return 2; no database/config initialization.
- `scripts/smoke-egui-gns3.py` passed against the development binary under an isolated Xvfb server. A real loopback TCP console sent output and received `show version\r` typed into OpsTerm. A second simultaneous launch created its own named window and showed a connection-refused error against a bound, non-listening port.
- Screenshots visually inspected: `console.png` shows exactly one named tab and server response; `connection-refused.png` shows the offline node and error; `close-confirm.png` shows the expected close modal; `cancelled.png` shows the disconnected tab and output surviving Escape. Images/logs are in `/tmp/opsterm-gns3-gui` and `/tmp/opsterm-gns3-*.log`, outside the repository.
- Read-only independent review found no blockers. Its warning that process survival alone does not prove close cancellation was addressed through screenshot inspection, plus simultaneous-window and error-path coverage.

The actual user GNS3 project/router console was not operated; verification used the same command arguments with a local Telnet server. The user's existing OpsTerm/GNS3 windows and GNS3 preferences were left untouched. This is a console-command integration, not a GNS3 server-management plugin. Software-GL debug screenshots are functional tests, not RAM benchmarks.

## Published release and installation

- Public latest release: https://github.com/Setiyoaryo/ops-term/releases/tag/v0.1.4 — complete, non-draft, four signed/versioned assets.
- CI: https://github.com/Setiyoaryo/ops-term/actions/runs/40429838937 — success.
- Release: https://github.com/Setiyoaryo/ops-term/actions/runs/40429841461 — success, including release-profile Rust tests, clippy, signed packaging and manifest verification.
- Installed through the existing trusted v0.1.3 updater using the versioned HTTPS manifest. `/home/portego/.local/bin/opsterm-egui --version` reports `OpsTerm v0.1.4`; the normal stable feed check reports up to date.
- Installed SHA-256 independently matches the signed public manifest: `e8769a83e921ff6ddc170d3a52ec78927544ea2d96c0bd6b43ab6af06ec60fc0`.
- Re-ran the native GNS3 console smoke against the **installed release binary**, successfully. Screenshots visually inspected again; artifacts are `/tmp/opsterm-release-0.1.4-gns3/` and its `.log` sibling.
- Also inspected the installed native GNS3 console launcher: it substitutes these same placeholders and launches argv directly on Linux. Its settings and projects were not modified.
- All local build/test caches from this task were cleaned after consuming the development binary, before publication. After the final installed-binary smoke, an independent session started its own guarded Rust build and edited keyboard/clipboard files in the shared workspace; those changes and its active caches were left untouched, respecting the build lock. No generated artifacts were committed.
