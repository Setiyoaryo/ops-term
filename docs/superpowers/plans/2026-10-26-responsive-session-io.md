# Responsive session I/O and removal of legacy product integration

Goal: keep egui responsive when a transport stops accepting input and remove the requested third-party importer, UI and documentation references. Keep saved inventory, generic CSV import/export, security checks and the existing visual design.

Evidence: Session::write, dispatch_command and runner writes call blocking provider.write on the UI thread; Session::drop calls provider.kill synchronously; resizing and lifecycle queries also call transport code directly. PTY write_all and serial poll can wait indefinitely. With bounded reader backpressure, a large echoing paste can prevent the UI from draining output, creating a circular wait. The currently running process is idle rather than hung, so this does not prove every reported intermittent freeze shares that cause.

Design: wrap real opened transports at the connection-worker boundary. A bounded byte budget and bounded operation queue accept ordered writes/resizes without waiting. A dedicated worker performs transport operations and lifecycle polling. UI lifecycle reads use atomics. Close marks the session closed immediately and starts transport shutdown independently, so queued writes do not delay close. Return explicit queue/size errors rather than growing memory without limit, and surface asynchronous failures in the existing UI status. Repaint implementation remains unchanged; the reproduced failures are transport waits.

- [x] Add regression tests with deliberately blocked writes/resize/shutdown, ordered input, bounded queue, cancellation and real PTY backpressure.
- [x] Implement worker adapter, error reporting and bind it to real connection setup; preserve test seams.
- [x] Remove the importer from egui and React, delete its dedicated tests and remove product-comparison references from current documentation. Preserve generic CSV/TSV and saved data.
- [x] Run guarded Rust tests/clippy, frontend checks and relevant smoke tests; review; build/install verified binary without terminating active user sessions; clean generated caches.

Additional evidence and final validation: [responsiveness report](../../responsiveness-2026-10-26.md). Native quick Ctrl+C handling was also reproduced and fixed; Shift+Arrow shortcuts are unchanged.

Signed v0.1.2 published, installed through the trusted updater, and verified with a real GUI smoke. Public website download automatically refreshed. Existing user window remains open and needs a user-controlled restart.
