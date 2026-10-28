# Selection scrollback and intermittent UI stalls

## Verified selection defect

The edge-drag path passed -1 above and +1 below to Alacritty Scroll::Delta. Its display offset uses positive values to reveal older history. Thus dragging upward at offset zero remained clamped at zero. Reverse these two signs; preserve selection behavior and existing UI.

Two integrated egui RawInput regressions failed on the old code and passed after the sign fix. They verify both directions, stationary held-pointer continuation, a stable anchor and nonempty copied text. The native Xvfb regression failed on the installed official v0.1.7: copied rows were 477..493, inside the initial viewport. With the corrected executable it passed and copied rows 420..493, crossing into history.

## Random freeze investigation

The user reports frequent random GNOME “not responding” with no known trigger. The currently running v0.1.7 process was observed waiting in native poll/epoll, with roughly 64–72 MiB RSS. No corresponding kernel GPU/OOM entry was found in the inspected interval. These snapshots do not diagnose the earlier freeze. Same-user gdb attachment was denied by ptrace restrictions, and passwordless elevated attachment was unavailable. No permission settings or active sessions were changed.

Known blocking paths remain candidates, not proven causes: upstream [egui #8664](https://github.com/emilk/egui/issues/8664) describes synchronous clipboard reads; source review also identified stop-logging flush/lock contention when logging is enabled. Neither establishes the cause of this random report. Do not claim that reversing the scroll direction fixes the separate freeze.

## Automatic evidence

A worker samples an atomic last-progress timestamp/stage once per second. After six seconds without progress, it records only process/version/time, last UI phase and the main thread's kernel wait/syscall number. No terminal/session/clipboard payload is available to this worker. The worker never locks egui context or transport state and never terminates sessions. Minimized windows are excluded when known; occlusion/suspend/native dialogs can still produce observations, so a record is not itself proof of deadlock.

Reports are lazy-created mode 0600 under a private diagnostics directory, with unique timestamp/PID filenames and create_new protection. Each process is capped at 64 KiB; retention prunes only recognized regular reports from finished processes, preserving eight plus all live-process reports and unrelated files. Tests cover detection/minimization, a real blocked-progress report, permissions, symlink refusal and retention.

## Delivery checklist

- [x] Reproduce original selection failure in unit and native GUI tests; verify corrected history copy.
- [x] Add content-free automatic stall diagnostics with privacy/lifecycle tests; preserve egui UI.
- [x] Run final checks and publish signed v0.1.8 packages through existing architecture/distro gates.
- [x] Verify official payloads and installed update; consume binaries and clean caches.

## Published and installed evidence

- Commit/tag: 6f77d66 / v0.1.8. Release run [40657328973](https://github.com/Setiyoaryo/ops-term/actions/runs/40657328973) passed both architectures, native X11/Wayland/GNS3/selection smoke, real signed AppImage update/rollback and all seven distribution compatibility jobs. Both architectures passed 59 core + 141 egui tests. CI 40657325018 passed; Pages 40659878028 passed and live website advertises v0.1.8.
- Official GUI copy regression: x86_64 copied rows 413..492 and ARM64 copied rows 423..492, reaching beyond the original viewport.
- All six official downloaded native/AppImage/archive payloads passed size, SHA-256 and Ed25519 verification against the repository public key. Release contains nineteen assets.
- Existing v0.1.7 installed updater successfully installed v0.1.8; --version confirms it and --check-update reports up to date. Installed x86_64 SHA-256 is 56688265dfd6fbe3dc6683e9895f18c26f6e0f96937d193ef3207936a3bac31b, matching the official manifest.
- Existing terminal windows were not killed or restarted. The new executable must be opened to activate its diagnostics; the earlier observed user process had already exited before installation. The random freeze remains unresolved pending actual incident evidence.
- Fresh installed-release Xvfb drag/copy smoke passed (rows 421..493). Final npm run clean -- --dist completed; target/dist are absent.
