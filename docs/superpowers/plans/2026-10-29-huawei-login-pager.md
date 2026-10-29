# Huawei Telnet startup ordering and reliable saved login

Goal: saved Telnet credentials are answered once at recognized prompts; pager commands never race a fixed delay against login. Repeated/double device clicks keep one opening connection. Preserve existing UI and secret-storage boundaries.

Evidence: egui apply_pager unconditionally starts a 1500 ms write thread on transport creation, before authentication. Telnet LoginAutomation compares raw string tails (ANSI/trailing control codes and spaces before ':' fail); password-only prompts require sent_user and cannot be answered. Some saved empty username values bypass the credential username fallback. Device row clicks already use idempotent open_saved; do not change click semantics without a failing regression.

Architecture: replace the pager timer with a per-session, one-shot pending startup action and bounded timeout. Read the LIVE terminal cursor row (not scrolled history), require a complete vendor CLI prompt, and serialize after any existing expect script. Cancel pending pager on disconnect, reconnect replacement, timeout or failing startup script; never retry blindly. Huawei retains screen-length 0 temporary; other presets retain their command. Telnet prompt matching uses a bounded, stateful ANSI/UTF-8 observer via the already locked vte crate; recognize explicit username/password prompts, including spacing/color/chunk splits and password-only login. Never fill password-change prompts; at most one username/password attempt. Existing initial Enter nudge must not fire after password-only authentication. Ignore blank saved usernames when using credential metadata. No credentials in logs/argv/environment or new storage.

- [x] Write and observe RED core prompt and egui early-pager/username-fallback regressions.
- [x] Implement core prompt normalization (owned core/providers/telnet.rs, core/Cargo.toml only), and session pager sequencing + username fallback (root owns egui sources).
- [x] Extend real native GUI login smoke for delayed/fragmented colored Huawei prompts, double click, one TCP connection, exact username/password/pager order and no pre-auth pager. Use an isolated D-Bus/keyring and synthetic credentials; preserve real user vault/keyring. GUI implementer owns scripts/smoke-login.py only.
- [x] Run guarded Rust tests/fmt/clippy, native smoke and independent security/correctness review. All builds/tests serialized by root; no cleanup during active work.
- [ ] Publish and install signed v0.1.10 through existing multi-arch/distro gates; verify public assets, actual installed login flow and clean caches.

Parallel work follows subagent-driven-development/dispatching-parallel-agents: separate files, root coordinates all Cargo commands and lock/cleanup. Implementers stage failing tests first and await root RED evidence before changing production logic.

Review refinements: decoded reader-byte completion watermark prevents stale UI banner release; cache sampling/publishing order is covered by deterministic regression. Invalid script validation cancels pending pager. Existing dependency advisories are recorded without suppression in docs/telnet-startup-2026-10-29.md.
