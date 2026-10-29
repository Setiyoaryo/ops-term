# Telnet credential and pager startup verification

## Cause and correction

The old egui path spawned a detached pager-write thread immediately after transport creation and slept 1500 ms. A slow login therefore received the pager command as username/password input. Huawei's preset already mapped to `screen-length 0 temporary`; it was an ordering defect, not a need to replace the command with Cisco `terminal length 0`.

The Telnet observer compared raw string suffixes, so trailing ANSI resets and spaces before `:` could hide prompts. It required a sent username before answering a password-only login and could mistake `New password:`/banner suffixes for authentication prompts. Empty saved usernames also prevented fallback to credential metadata. The existing device-click path was already idempotent; real double-click testing verifies that behavior rather than adding a timing workaround.

A bounded stateful raw-byte observer now handles ANSI and split UTF-8, ignores OSC payloads, recognizes exact normalized aliases and sends each configured answer at most once. Password-change prompts remain manual. The initial Enter nudge stops after either credential has been attempted.

Pager setup belongs to its Session and has no detached timer. It requires a complete live vendor CLI prompt, completed configured answers and no active startup script. A decoded-output byte watermark published before credential completion must first be consumed by the UI, preventing an old rendered banner from being mistaken for post-login readiness while output is queued. The responsive worker samples completion before the watermark, caches the watermark before publishing completion, and UI reads completion before checking its processed-byte count. Negotiation bytes are excluded consistently; replacement sessions reset counters. Script writes also require fresh output before pager release. Invalid/failed startup scripts, disconnect, write error and 60-second timeout cancel the one-shot action.

## Regression evidence

- Old core: six prompt regressions failed (ANSI/spaces/password-only/change rejection/erase/UTF-8 split), plus password-only nudge and pending-phase regressions. Old pager/empty-username tests failed too.
- The installed official v0.1.9 failed the native saved-login fixture by sending application input before its allowed authentication stage.
- Independent review caught a stale-UI banner race and missing cancellation for invalid startup scripts. Deterministic tests failed before the output watermark/cancellation fixes. A cache test verifies the watermark is published before completion; the network watermark test uses actual reader chunk boundaries rather than assuming TCP write/read equivalence.
- Full guarded suite after these fixes: 68 core + 147 egui tests passed. Final versioned native/fmt/clippy and public release results are recorded below after completion.
- Real native GUI fixture uses its own Xvfb desktop, temporary XDG directories, private D-Bus and foreground unlocked Secret Service. Synthetic secrets are piped/stored only inside that environment; real user keyrings/vaults and HOME remain untouched. The delayed server tests prompt-shaped pre-auth banner, >1500 ms delay, fragmented colored username/password, credential-username fallback, double click/one TCP connection, delayed genuine CLI, exact one-shot Huawei pager and direct keyboard roundtrip.

No real switch was accessed; physical-device coverage depends on actual prompt variants. Unknown CLI prompts and password-change flows remain conservative rather than receiving speculative commands.

## Dependency audit — existing findings

Official cargo-audit v0.22.2 was verified against its GitHub asset SHA-256 and run against Cargo.lock. The audit returned nonzero: these existing third-party versions are unchanged by this fix (vte 0.15 was already locked and used by egui; core now declares it directly):

- [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html): crypto-bigint 0.5.5 timing variability.
- [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html): rsa 0.9.10 timing side channel; the advisory lists no patched version.
- [RUSTSEC-2026-0154](https://rustsec.org/advisories/RUSTSEC-2026-0154.html): russh 0.54.5 unbounded SSH-agent frame allocation.
- [RUSTSEC-2026-0153](https://rustsec.org/advisories/RUSTSEC-2026-0153.html): russh-cryptovec 0.52.0 unchecked buffer growth, including remote transport/compression paths in this older native SSH stack.

OpenSSH remains the default SSH engine, but the optional native engine uses the affected stack and its agent client, so these findings are not described as universally unreachable. A separate native SSH/crypto dependency upgrade is required; no advisory suppression or clean-audit claim is made. Unmaintained/unsound/yanked dependency warnings are also retained in the audit report under /tmp/opsterm-huawei-cargo-audit.json. This Telnet hotfix does not diagnose the separate random-freeze report.

## Final local validation before publication

Versioned v0.1.10 source passed 68 core + 147 egui tests with private XDG state and no access to the user's Secret Service. Formatting and clippy all-targets with -D warnings passed; twenty packaging/publishing tests passed. Final rebuilt native GUI saved-login smoke passed after the output-watermark and parser short-circuit changes, with screenshots inspected under /tmp/opsterm-huawei-login-final. Native close confirmation regression also passed. An independent final source/security review approved the fixes, while explicitly retaining the pre-existing dependency audit findings. The synthetic secure-store helper is temporarily retained outside build caches only for verifying the eventual installed signed release.
