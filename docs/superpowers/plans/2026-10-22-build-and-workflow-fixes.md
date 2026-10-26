# Build and workflow repairs

**Goal:** Produce a verified egui release, keep build memory bounded, and repair existing terminal workflows across egui and Tauri.

**Approach:** Preserve the existing UI and providers. Use stable session identities for broadcast and tab lifecycle. Bound frontend terminal buffers and preserve encoding across hibernation. Build one crate/process at a time with reduced compiler memory and a Linux process-group memory watchdog that stops work before memory pressure reaches the desktop.

**Constraints:** This directory has no Git metadata. Work inline without commits/worktrees. Do not compile multiple clients concurrently. No production device connections. A watchdog stop is a failed/incomplete build, never a successful verification. Memory sampling is a precaution, not an absolute guarantee against system-wide OOM.

- [x] Add a standard-library Python build guard; test command exit propagation, low-memory refusal, process-group termination, and exclusive build locking. Configure Cargo for one job, dev debug info off, release without full LTO; wire documented build commands through the guard.
- [x] Reproduce egui broadcast misrouting on reorder/close and command-library dispatch with test providers. Keep selection on Session, centralize dispatch, preserve active session on close/reorder, and verify tests.
- [x] Repair egui command Enter handling, quick-connect validation/IPv6 parsing, editor error preservation, and tab overflow within existing controls. Add focused regressions and headless frame checks where feasible.
- [x] Reproduce frontend encoding loss on terminal unmount, stale buffers after close, and oversized buffered chunks. Separate pane detach from session disposal, cap pending buffers and discard closed-session snapshots, and test round trips.
- [x] Run frontend unit tests/build and Playwright workflows with one worker. Run Rust core/egui tests, formatting, and an egui release build sequentially through the watchdog. Attempt other client checks only while memory headroom permits; report limits honestly.
- [x] Review changed code, update README build instructions and write actual verification results.

**Verification:** 47 core + 42 egui tests, 21 frontend tests, 30 Playwright journeys, 5 watchdog tests, Clippy, formatting and real GUI smoke passed. Release full dependency build peaked at 854.5 MiB sampled RSS; final incremental release peaked at 543.3 MiB. See `docs/verification-2026-10-22.md`. Full GTK/Tauri builds and production devices were outside the selected egui validation scope.
