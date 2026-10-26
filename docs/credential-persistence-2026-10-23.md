# Persistent credentials on Linux

The Linux kernel keyring backend alone advertises persistence until reboot.
SQLite inventory records can outlive those kernel secrets, so a surviving
credential name does not prove that the password is still available.

OpsTerm uses the desktop Secret Service backend (`sync-secret-service` with
`crypto-rust`) for permanent storage and an encrypted D-Bus session. Saves only
succeed after permanent storage succeeds. Existing kernel-backed credentials
are migrated when still available; a missing secret cannot be reconstructed
from its inventory record.

Reads prefer the permanent entry. If it is absent, a still-available legacy
kernel secret may be migrated without replacing an existing permanent entry.
Locked/access-denied errors do not authorize falling back to a stale cache.
Deletion clears the legacy cache before removing the permanent entry.

SSH/Telnet connections load saved credentials inside the worker. Missing or
locked storage is reported as an error, rather than silently treating a saved
credential as unset. Passwords are not put in argv, process environment, logs
or SQLite.

## Verification

The backend persistence contract is covered by Rust tests. The optional
`scripts/smoke-credential-store.py` fixture tests cross-process saves/reads,
legacy migration and deletion with unique `opsterm-smoke-` credential names.
It uses fixed example values and does not accept secrets through argv, env or
files. Run such fixtures with an isolated test keyring/session.

`scripts/smoke-login.py` exercises saved-credential Telnet login against a
loopback fixture; it does not prove connectivity to production devices.
