# Saved-credential login behavior

This historical note describes an earlier password-recovery UI. That form was
superseded by the persistent-credential implementation described in
[Persistent credentials on Linux](credential-persistence-2026-10-23.md).

Saved-device SSH/Telnet sessions resolve their credential in a worker. A
credential that is missing, locked or inaccessible produces an explicit
storage error and supports retry after the underlying storage is available.
The app does not replace a saved credential with an unrelated password form.

Tests and optional local GUI fixtures cover credential handoff, input focus
and Telnet login on loopback. They use synthetic credentials and make no claim
about recovering secrets that have already been lost from their backing store.
