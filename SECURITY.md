# Security

Current desktop credential storage uses the Linux desktop Secret Service with
encrypted D-Bus sessions. Secrets persist beyond reboot; SQLite, exports and
workspace layouts hold references only. The previous kernel-only backend is
read for automatic migration when no durable entry exists. Writes succeed only
after durable storage succeeds. A compatibility kernel copy is optional and is
removed before deleting the durable secret, preventing stale-secret revival.
Named inventory credentials are resolved in connection workers without requiring
password entry for each connection. See [the storage correction](docs/credential-persistence-2026-10-23.md).

The milestone descriptions below retain their original M1 scope.

OpsTerm connects to production network infrastructure, so security properties
are enforced in code, not advertised in prose. This document describes the
threat model and the controls that are actually implemented today.

## Threat model

| # | Threat | Control |
|---|---|---|
| 1 | Malicious remote host sending control sequences / huge output | xterm.js parser handles sequences; bounded per-session scrollback; output delivered in bounded chunks; no HTML injection from output |
| 2 | Compromised saved session / import file injecting options | all spawns use validated argv; host/usernames starting with `-` are rejected; no shell strings |
| 3 | Mistaken operator sending config to the wrong device | broadcast is disabled by default (M2, arm + review); M1 has no broadcast |
| 4 | Lost/stolen laptop exposing secrets | no credentials are stored at all in M1; logs are opt-in (M2) |
| 5 | SSH MITM via unverified/changed host keys | host-key checking is left to OpenSSH; no bypass flags are ever added |
| 6 | Script/plugin abuse | no script execution engine exists yet |

## Enforced controls (M1)

- **No host-key bypass.** The argv builder never emits
  `StrictHostKeyChecking=no`, `UserKnownHostsFile=/dev/null`,
  `StrictHostKeyChecking=accept-new`, or any `-o` that weakens verification.
  Unknown keys prompt via OpenSSH; changed keys block. This is asserted by
  `security::tests::argv_contains_no_host_key_bypass`.
- **Direct argv only.** `security::build_ssh_argv` produces a
  `Vec<String>` of distinct, validated arguments. There is no `sh -c` anywhere
  in the codebase.
- **Trusted binary resolution.** Only `/usr/bin/ssh`, `/usr/local/bin/ssh` and
  `/bin/ssh` are considered; there is no implicit `PATH` search that a hostile
  `PATH` could redirect.
- **No secrets in argv or storage.** M1 stores no credentials. No `sshpass`.
  Interactive password prompts happen inside the remote PTY, not in app memory.
- **Input validation at the boundary.** Hosts, usernames, ports, identity
  paths and `ProxyJump` values are validated in `security` before any spawn.
- **Restrictive Tauri capabilities.** `src-tauri/capabilities/default.json`
  grants only `core:default`; the renderer cannot touch the filesystem or spawn
  processes directly.
- **CSP.** The window CSP forbids remote content and restricts scripts to
  `self`, preventing remote-code injection from terminal output.

## Security acceptance cases (from PRD §10)

- [x] Session host `-oProxyCommand=...` is rejected, no process spawned
      (`rejects_host_that_looks_like_an_option`).
- [ ] CSV import path traversal cannot write outside configured storage (M2).
- [x] Unknown host key prompts; changed key blocks; bypass flags absent
      (`argv_contains_no_host_key_bypass`).
- [x] Renderer filesystem/process actions are blocked by capabilities.
- [ ] Crash while logging cannot leave plaintext secrets in the DB (M2).

## Reporting

Report vulnerabilities privately to the maintainers rather than opening a
public issue. Include a minimal reproduction and the affected version.
