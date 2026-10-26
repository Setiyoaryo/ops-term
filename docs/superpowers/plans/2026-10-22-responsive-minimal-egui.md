# Responsive minimal egui workspace

**Goal:** A modern minimal dark UI, immediate device-tab activation, and lower measured runtime memory/idle CPU. Preserve the safe single-worker build path.

**Design:** Flat terminal surface, restrained blue accent, compact unified connection toolbar, rectangular tabs, virtualized device rows, collapsible command composer, and secondary tools in menus. A click creates/focuses its tab synchronously; connection setup remains cancellable background work. Reopening an already active device focuses it without spawning another connection. Pending completion never steals focus. No change can eliminate server/network authentication time; the UI must show progress and errors immediately.

**Performance:** Cache the device library until edits/import/manual refresh; draw visible rows only. Wake UI on terminal output/background completion, with a one-second idle clock/lifecycle fallback. Apply styles only when they change. Bound terminal scrollback to 2,000 lines and output queue to 128 KiB per session, and batch terminal text by color run instead of one shape per character.

**Verification:** Keep the previous release in `/tmp/opsterm-pass2/opsterm-before`. Compare both binaries under the same Xvfb/software-rendering fixture, 500 synthetic devices, one local shell, five-second idle CPU sampling and `/proc/PID/smaps_rollup` RSS/PSS. Test queued tab creation, duplicate-device focus, completion after selection changes, cancellation, failures, cached library refresh, virtualized rows, output wakeups, bounded history, ANSI/Unicode selection/rendering, and existing workflows. Use `scripts/build_guard.py` for all compiles; no production device connections.

- [x] Record baseline CPU/RSS/PSS and preserve source/binary.
- [x] Add failing connection/idle/rendering regressions; implement immediate pending tabs with stable identity, reuse, close/retry, and worker notifications.
- [x] Cache/virtualize devices; reduce idle polling, style work, output queues/history and per-character drawing.
- [x] Replace the workspace chrome with the selected modern minimal dark design and update GUI automation to locate its controls.
- [x] Run Rust tests, Clippy, formatting, guarded release build, real GUI smoke, local click-action bound (<50 ms) and actual saved-device GUI click, and before/after memory/CPU benchmark.
- [x] Review changes, record results/limits, and launch the latest verified app for the user.

Results: `docs/performance-2026-10-22.md`. Rust 47 core + 52 egui tests pass;
release build peaks at 543.1 MiB. After-output fixture RSS 145.55 -> 122.93 MiB,
PSS 100.86 -> 78.39 MiB; idle CPU 297.16% -> 7.20% with software GL.
Real-device network latency is not measured; no production connection is opened.
