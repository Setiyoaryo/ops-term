# Operational fixes implementation plan

**Goal:** Resolve all seven findings in the operational audit, with regression coverage and bounded verification resource use.

**Design:** Keep the existing providers and UI. Give egui sessions stable runtime IDs, bind each runner to its original session, prevalidate regexes and abort on failed waits. Use a bounded output channel, pump every session with a per-frame budget, and run connection/SFTP operations on background workers. Preserve saved device IDs across open/reconnect. Make serial reads cancellable with poll and a wakeup pipe. Reuse the UTF-8 tail helper in Tauri.

**Constraints:** No Git metadata is present, so no branch/worktree/commit operations are possible. Work inline, sequentially. Rust checks use `systemd-run --user --wait --pipe --working-directory=/path/to/opsterm -p MemoryMax=1G -p MemorySwapMax=0 -p CPUQuota=100%`, cargo `--offline -j 1`, and single-thread tests. Never run multiple builds together. A combined core/egui/Tauri test triggered dependency feature unification and GTK recompilation; its cgroup hit 1 GiB and stopped that job, while the laptop retained approximately 7.8 GiB available RAM. Verify each component separately to retain existing caches rather than raising the job limit. No production network or serial hardware tests.

## Tasks

- [x] Add regression tests to `core/src/providers/serial.rs`: open a pseudo-terminal, block its reader, kill provider, require EOF without writing new data. Confirm failure, then introduce a nonblocking poll reader with cancellation pipe shared by provider shutdown and Drop. Test normal I/O and repeated kill.
- [x] Add regression tests to `egui/src/main.rs`: run a delayed script on A, select/reorder B, and verify only A receives bytes; wait timeout and invalid regex must prevent later sends. Confirm failure, then bind runner to stable session identity and return visible errors. Stop on target closure/reconnect.
- [x] Add output backlog test with a finite fast reader: an undrained session must backpressure before all chunks are read. Confirm failure, use a bounded channel, and pump all sessions independently of pane visibility with per-session chunk and time budgets. Verify hidden-tab processing and exit detection.
- [x] Add a workspace regression with a saved loopback SSH device; opening/saving must retain its ID. Confirm failure, then propagate saved metadata through pending connection completion and reconnect.
- [x] Queue SSH, Telnet, serial and reconnect work outside UI callbacks. Pending requests retain their own origin, dimensions, saved metadata and login script. Poll completion each frame, display failures, and keep automatic actions attached to the completed target. Test pending completion after tab changes, failed opening, and closed reconnect targets.
- [x] Move SFTP mkdir/remove to the same asynchronous result path as listing. Capture device and path per request, prevent overlapping mutations, and reject stale display results when selection/path changes. Test a blocked worker does not block submission/UI polling and failures propagate.
- [x] Add a Unicode timeout regression in the Tauri supervisor. Confirm panic, then use the existing core UTF-8 helper; add helper coverage for the exact 4096-byte tail case.
- [x] Run core and egui tests, Tauri checks/tests as available, frontend tests/TypeScript, and formatting. Review the final diff against the audit and update the audit with fix/verification status. Closure: 47 cached core tests, 23 cached egui tests, 17 frontend tests, TypeScript, formatting, and 3 isolated Unicode tests passed. Full final Tauri verification remains incomplete after two memory-limit kills; no further GTK builds were attempted. See the audit's closing verification section for exact limits and exclusions.

## Acceptance

Each audited behavior has a concrete fix and regression evidence. No unbounded terminal output backlog, no UI-thread connection/SFTP wait, no runner dispatch to a newly selected tab, failed waits stop scripts, serial idle shutdown completes, workspace SSH references survive, and timeout tails handle multibyte text.
