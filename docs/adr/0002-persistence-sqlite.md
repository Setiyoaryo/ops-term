# ADR-0002 — Persistence: local SQLite

- Status: Accepted (implementation begins in Phase 2)
- Date: 2026-10-14
- Milestone: M1/M2

## Context

The app must manage hundreds or thousands of saved sessions with nested
folders, tags, snippets and settings, and search them quickly — all locally,
without a cloud account.

## Decision

Use a single local SQLite database (via `rusqlite`, to be finalised in Phase 2)
with:

- UUID/ULID primary keys;
- WAL mode;
- versioned, reversible migrations;
- UTC ISO-8601 timestamps.

Credentials are **never** stored in SQLite. The schema stores secret
*references* only (resolved against the Linux Secret Service in M3).

## Consequences

- Positive: reliable local queries, transactional edits, offline operation.
- Negative: schema migrations require care; must provide backup instructions
  and crash-recovery tests.
