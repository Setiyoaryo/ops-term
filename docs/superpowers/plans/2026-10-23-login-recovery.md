# Recover missing saved passwords and terminal keyboard focus

Evidence: user screenshot reaches Telnet Password prompt after username injection.
Read-only metadata checks show linked credentials; a temporary diagnostic using
the actual core keyring API returns No matching entry. The worker currently
silences that error with `.ok()` and opens a passwordless connection. Native
credential Store also ignores keyring errors. Terminal input requires another
click after a device/tab is selected.

Implement a typed credential failure before transport setup. Show a masked,
per-tab password form with Connect, Cancel, and an explicit optional system
keyring save. Pass the submitted secret only to the worker; do not retain it in
the tab's origin, SQLite, logs, export, or workspace. Do not silently fall back
when save fails. Preserve the worker limit, tab identity, and focus of other tabs.
Transfer keyboard focus when selecting a device/tab or opening its terminal;
do not steal focus on unrelated background completion. Fix ignored native Store
errors so metadata is only created after successful secret storage.

Verify regressions fail first for silent missing-credential fallback and direct
typing after device selection. Test a loopback Telnet server through actual
pending-worker recovery, masked form, one-shot password submission, and prompt
exchange. Run full core/egui tests, Clippy, guarded release build, and real GUI
smoke with missing-credential recovery. No production login attempts or password
printing. Open the verified new app without terminating existing user sessions.

Implemented and verified: typed credential recovery, masked per-tab form,
one-shot/optional keyring handoff, device/tab keyboard focus, and GTK Store error
handling. Both initial regressions failed before the fix. Core 47 + egui 56
tests, Clippy, native check, release build, and both real GUI smokes pass.
See `docs/login-recovery-2026-10-23.md` for evidence and limits.
