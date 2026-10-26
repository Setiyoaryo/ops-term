# ADR-0004 — Credential handling

- Status: Accepted
- Date: 2026-10-14
- Milestone: M1 (no storage) → M3 (keyring)

## Context

Passwords and key passphrases are the highest-value assets in a terminal
client. Storing them in SQLite, argv, environment variables, logs or clipboard
is unacceptable. Prompt matching for auto-enable passwords is fragile and
vendor-specific.

## Decision

- **M1 stores no credentials at all.** Interactive SSH password and
  keyboard-interactive prompts are answered by the user inside the remote PTY.
  Agent and key authentication work through OpenSSH as configured.
- `sshpass` is prohibited.
- **M3** will integrate the Freedesktop Secret Service (GNOME Keyring /
  KWallet) for secret storage only, with `credential_ref` pointers in SQLite
  and no plaintext fallback.
- Password autofill into OpenSSH prompts is explicitly out of scope until a
  reviewed secure architecture exists.

## Consequences

- Positive: the M1 attack surface contains no stored secrets.
- Negative: no centrally stored SSH passwords in M1; users rely on keys/agent
  or type passwords interactively. This is an accepted baseline limitation.
