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

Signed release installation and artifact verification are recorded after publication below.
