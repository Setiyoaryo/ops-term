# ADR-0001 — Connection engine: OpenSSH subprocess over a PTY

- Status: Accepted
- Date: 2026-10-14
- Milestone: M0/M1

## Context

OpsTerm must open real SSH sessions to production network devices. We can
either (a) spawn the installed OpenSSH client under a pseudo-terminal, or
(b) embed a native Rust SSH library (e.g. `russh`).

The user base depends on `~/.ssh/config`, `known_hosts`, `ssh-agent`,
`ProxyJump` bastions, certificates, and occasional legacy algorithms. Getting
any of these subtly wrong will silently break real workflows.

## Decision

For M1, spawn the system OpenSSH client (`/usr/bin/ssh` and known absolute
locations) through `portable-pty`, using a structured, individually validated
argv array. Never build a shell command string. Leave host-key verification to
OpenSSH; never add bypass flags.

All SSH behaviour sits behind a `ConnectionProvider` trait so a native engine
can be introduced later without UI changes.

## Consequences

- Positive: immediate compatibility with existing SSH configuration and
  authentication material; strong host-key security by default.
- Negative: one process per connection; limited ability to script
  credentials; OpenSSH handshake phases are not finely observable, so the
  reported state machine stays coarse and honest.
- Follow-up: a separate spike will evaluate a native provider against a
  compliance suite (agent, ProxyJump, host-key, algorithm) before adoption.
