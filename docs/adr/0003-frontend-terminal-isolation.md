# ADR-0003 — Frontend terminal isolation and output routing

- Status: Accepted
- Date: 2026-10-14
- Milestone: M1

## Context

Each tab must behave like an independent terminal: switching or closing one tab
must never pause, reorder, or mix another session's input and output.

## Decision

- Every tab owns one backend `session_id` and one xterm.js instance.
- Terminal instances are kept alive for the lifetime of their tab and hidden
  with `visibility` (not unmounted), so buffers survive tab switches.
- A module-level `terminalRegistry` maps `sessionId → Terminal` and routes
  `terminal-output` events; bytes arriving before the pane mounts are buffered
  and flushed on registration.
- Output is delivered as ordered, base64-encoded chunks with a per-session
  sequence number. No single-byte events.

## Consequences

- Positive: independent buffers, no cross-session leakage, resize correctness,
  no early-byte loss.
- Negative: all tabs consume memory while open; scrollback is bounded per
  session to keep this predictable.
