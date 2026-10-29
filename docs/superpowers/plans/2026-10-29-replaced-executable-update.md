# Updating a still-running executable replaced by another instance

Goal: handle an already-installed update as pending restart instead of failing against a Linux `(deleted)` executable path. Preserve active sessions and signed distribution safeguards.

Evidence: user's GUI reports v0.1.9, while ~/.local/bin/opsterm-egui --version reports v0.1.10. The GUI PID's /proc/exe symlink is the original path with `(deleted)` and a different inode from the current installed file. distribution currently resolves std::env::current_exe anew on each call; install_bytes then lstats a nonexistent suffixed path. No running user process will be killed or restarted automatically.

Design: cache the validated distribution path/format and original file identity at process startup; do not guess argv/PATH or strip `(deleted)`. Keep AppImage environment/outer-file validation and symlink rejection. A verified manifest whose asset already matches the target on disk produces pending-restart state. Existing UI presents Restart now (with active-session confirmation), not another download. Recheck hashes inside install worker; if another installation changed the target to an unknown payload, refuse to overwrite it. Serialize new installers with a private per-target file lock and validate original identity while holding the lock. Signed hashes, format binding, rollback and stable-version policy remain enforced.

- [x] Reproduce deleted-executable resolution with a Linux child-process test and pending-restart UI/state tests before production edits.
- [x] Capture native/AppImage launch context early and expose unchanged-target validation (distribution.rs agent); root owns update.rs/main.rs installer state and early initialization.
- [x] Verify missing/symlink/stale-environment refusal, known installed asset, unknown replacement refusal, lock contention and live deleted executable; run existing signature/AppImage/active-close regressions.
- [ ] Review security/correctness, publish/install signed v0.1.11 through existing architecture/distro gates, verify installed behavior, and clean build caches after use.

Testing/builds are serialized by root using build_guard. Implementers own separate files and wait for root RED evidence before production changes. User's old GUI must be reopened manually to load repaired code; an installed binary cannot change an already-running old executable.
