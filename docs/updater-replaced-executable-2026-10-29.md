# Updating a still-running replaced executable — v0.1.11

## Reproduction and fix

The user's window was still running v0.1.9 while the installed executable was already v0.1.10. Its Linux /proc/PID/exe link ended in `(deleted)` because the updater atomically replaced the old inode. Re-resolving current_exe during installation produced a nonexistent suffixed path and `cannot inspect the running executable`.

The distribution resolver now captures the validated native/AppImage path, format and file identity at startup. It never guesses argv/PATH or strips a filename suffix. A signed manifest matching the current disk artifact produces pending-restart state; the existing update dialog shows Restart now and keeps the existing active-session confirmation. Unknown replacements are refused with instructions to reopen/check again. Download, hashing and installation stay on workers. Hash checks stream through a bounded buffer and reject symlink entries or observable file changes.

Installers use a private persistent per-artifact lock file. A repeated identical installation preserves both inode and rollback. Identity is rechecked before writing and before replacement. New cooperating installers serialize; an arbitrary external writer that ignores this lock can still race the final check/rename. No claim of a filesystem-wide transaction against noncooperating processes is made.

The running old v0.1.9 process cannot load this repair without reopening. No existing user window or session was forcibly stopped.

## Validation

Five regressions failed at runtime before the fix: copied live executable lost its launch path after atomic replacement, installed-result state lacked restart, hash detection followed a symlink, identical bytes replaced the inode, and parallel installation ignored the lock.

After the fix, 68 core and 159 egui tests passed with private XDG state and a nonexistent Secret Service bus. Additional coverage includes unknown/in-place/same-size replacement, missing target, symlink swap, AppImage context after mount removal, changed target during installation, signed format binding and lock-file symlink refusal. All-target clippy with warnings denied passed. Cargo.lock changes only the application version; the dependency graph and the previously documented [native SSH/crypto audit findings](telnet-startup-2026-10-29.md#dependency-audit--existing-findings) are unchanged. This updater fix does not diagnose the separate random-freeze report.

The real signed native smoke uses private copies, appended fixture markers, temporary local signed manifests, an isolated Xvfb desktop and private config/data without access to the user keyring. It verifies CLI installation/rollback, repeated-install inode/rollback preservation, a live `(deleted)` GUI switching to Restart now and actually restarting the installed payload, and an unknown external payload refusing Install without changing its hash/inode or creating rollback. The refusal check requires the installer lock to exist so a missed click cannot pass. Detached test restart processes are matched only by private executable path and terminated before fixture cleanup.

The existing signed no-FUSE AppImage smoke passed outer-image replacement/rollback, raw-payload format refusal and outer hash checks. Screenshots are retained outside the repository under /tmp/opsterm-native-update-011. Both architecture release builds now run the native updater regression alongside the existing AppImage gate.

The native close-confirmation GUI regression, twenty packaging/publishing tests, formatting and release-workflow actionlint checks also passed. Local build/test caches were cleaned after consuming the binary; target and dist are absent. Independent source review found no required changes; the additional smoke-review findings (missed-click detection and detached process cleanup) were addressed before its final passing run.

Publication and installed-release verification will be recorded after the signed release gates complete.
