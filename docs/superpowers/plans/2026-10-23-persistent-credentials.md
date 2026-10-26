# Persistent-credential implementation notes

Requirement: saved Linux credentials must survive reboot and be resolved
automatically for inventory connections.

The kernel-only keyring backend persists until reboot, while the corresponding
SQLite inventory record can persist longer. Use permanent desktop Secret
Service storage, migrate available legacy kernel entries without overwriting
permanent values, and report inaccessible storage explicitly.

Verification must include the backend persistence contract, fixture-isolated
cross-process reads and deletes, and a loopback login using production
credential resolution. Do not infer recoverability of an erased secret from a
surviving inventory record. Do not use production credentials in tests.

See [the technical report](../../credential-persistence-2026-10-23.md).
