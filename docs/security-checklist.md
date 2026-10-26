# Security checklist (per release)

Cross-check before tagging a release.

## Host-key / SSH trust

- [ ] `build_ssh_argv` emits no `StrictHostKeyChecking=no` /
      `UserKnownHostsFile=/dev/null` / `accept-new` bypass.
- [ ] Changed-key and unknown-key behaviour follows OpenSSH defaults.

## Process spawning

- [ ] No `sh -c` / shell string construction anywhere.
- [ ] Only trusted absolute executables are spawned (`ssh`, `$SHELL`).
- [ ] Hosts/usernames/paths/jumps validated before any spawn.

## Secrets

- [ ] No credentials in SQLite, argv, environment, logs or clipboard.
- [ ] No `sshpass`.
- [ ] Logs (M2+) are opt-in with restrictive permissions.

## Renderer boundary

- [ ] Tauri capabilities remain minimal (`core:default`).
- [ ] CSP forbids remote content; no unsanitized HTML from terminal output.

## Dependencies & supply chain

- [ ] `Cargo.lock` and `package-lock.json` committed and pinned.
- [ ] `cargo audit` / `npm audit` reviewed.
- [ ] Release `.deb` built with checksum and (eventually) signing.
