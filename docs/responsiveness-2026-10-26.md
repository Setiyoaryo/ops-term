# Responsiveness verification — 26 October 2026

## Reproduced failures

The old egui session path called synchronous transport writes, resize, liveness and shutdown from the UI thread. A deliberately stalled writer blocked UI submission in the regression test. PTY output backpressure and input backpressure can form a circular wait when the UI cannot drain output while submitting a large paste.

Additional failing regressions reproduced a lost final resize after a full operation queue and Telnet shutdown waiting for a mutex held by a blocked write to a non-reading loopback peer. These passed after the fixes.

The native GUI smoke also found a pre-existing Ctrl+C issue in both the installed v0.1.1 binary and the new build. The native event batch contained Copy followed by ControlLeft release with Ctrl still recorded on that event, while the frame's final modifiers were empty. The handler now uses the event modifiers. Regression coverage includes quick Ctrl+C and copy-only Ctrl+Shift+C. Temporary diagnostics were removed.

## Changes in v0.1.2

Real connections are wrapped at the setup-worker boundary by the session I/O adapter. It preserves operation order, bounds pending input to 1 MiB and 64 operations, and returns queue errors promptly. A worker runs transport calls; UI liveness reads are atomic. Closing bypasses queued writes. Telnet uses a separate cloned shutdown socket to interrupt a blocked write without acquiring its mutex. Rejected resizes leave the cached size unchanged so a later frame retries.

Asynchronous transport failures and rejected input use existing status/error surfaces. The workspace styling, fonts and egui rendering remain unchanged. The legacy third-party importer and product-comparison references were removed from current source/docs; generic import/export and existing inventory data are preserved. Repository history was not rewritten.

## Evidence

- The stalled-write, resize-retry, real Telnet shutdown and native Copy modifier regression tests failed before their corresponding fixes.
- Guarded Rust tests: 55 core + 92 egui passed.
- Guarded clippy: core/egui all targets, warnings denied, passed.
- Frontend: 18 unit tests passed; TypeScript/Vite build and 30 Playwright E2E checks passed.
- Native Xvfb GUI smoke: real local shell, Ctrl+C, composer Enter, saved-device row, isolated tab targets, opening/closing a new tab during continuous output, return to the busy first tab and interrupt it, normal/minimum window passed. Screenshots inspected.
- Independent review findings on resize retry, Telnet cancellation and smoke targeting were corrected.

Commands use the guarded entry points described in AGENTS.md. Generated build/test output is removed after use.

## Limits

The user's intermittent freeze was not observed in the already-running process, which was idle during inspection. These fixes address reproduced blocking-I/O paths and do not establish that every possible stall shares the same cause. Vault/keyring operations, filesystem latency and graphics-driver stalls need separate reproduction if they persist. This change does not claim a new total-RSS target.

## Published and installed

The signed [v0.1.2 release](https://github.com/Setiyoaryo/ops-term/releases/tag/v0.1.2) is public. [CI](https://github.com/Setiyoaryo/ops-term/actions/runs/40346710259), [release verification](https://github.com/Setiyoaryo/ops-term/actions/runs/40346732847), and [automatic website refresh](https://github.com/Setiyoaryo/ops-term/actions/runs/40349751716) passed.

The previously trusted v0.1.1 updater detected v0.1.2 and installed it after signature/hash verification. The installed binary subsequently reported `OpsTerm v0.1.2 is up to date`; its SHA-256 independently matches the public manifest:

`edd504574b60d9781f82383f04b4c75799bc538ae239f0c9c33a6b30c122eabe`

The full native GUI smoke also passed against that installed release binary, including continuous output and interruption on the busy first tab. The existing user window was left running; it must be restarted by the user to load the new executable. No user inventory, vault or active sessions were deleted. Generated workspace caches were cleaned after verification.
