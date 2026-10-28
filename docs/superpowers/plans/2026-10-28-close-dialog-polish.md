# Close confirmation dialog polish

Goal: make the existing egui close modal clear, calm and elegant without changing confirmation policy or keyboard safety.

Design: centered dark card, 24 px padding, subtle rounded border/shadow and dim backdrop. A small vector warning badge accompanies a stronger heading. Show the session name (or active-session count) in a separate inset panel, then short muted risk text. Right-align generous Cancel and solid muted-red destructive buttons. Keep Cancel's initial focus, Escape/backdrop cancellation, Ctrl+Enter explicit confirmation, stable IDs and first-frame protection. Inherit neutral colors from the selected theme and wrap long session names within the card. No bitmap assets, new dependencies, animations or behavior changes.

- [x] Restyle only close_confirmation.rs; preserve state handling.
- [x] Run existing confirmation and app regression tests; inspect native screenshots for tab/window confirmations.
- [x] Review, release/install through existing signed pipeline and clean consumed build caches.

Verification before release: all 141 egui tests passed; fmt and clippy all-targets with -D warnings passed. Native Xvfb local-tab/window close smoke passed with screenshots inspected. Real loopback Telnet smoke at 760×500 with a long session name passed active cancellation and disconnected close; name wraps within card and controls remain visible. Independent code and visual review found no blockers. Screenshots remain outside the repository under /tmp/opsterm-modal-polish and /tmp/opsterm-modal-small.

Published v0.1.9 from ad2b5d2. Release run https://github.com/Setiyoaryo/ops-term/actions/runs/40662775615 passed both architecture builds, 59 core + 141 egui tests per architecture, native GUI checks, signed AppImage update/rollback and all seven distro jobs. Main CI 40662771671 and Pages 40665426694 passed; live site advertises v0.1.9. All six downloaded payloads passed size/hash/Ed25519 verification. Installed updater moved v0.1.8 to v0.1.9 and --check-update reports current; installed SHA-256 9638d2d1056a1edd265c33e1f1a20cf8e1ecb95a52eb9b93c1217645f49b9de9 matches official x86_64 manifest. Fresh installed-release real Telnet GUI smoke passed at 760×500 with long session name and screenshots inspected under /tmp/opsterm-modal-installed-019. Existing user sessions were not terminated or restarted.
Final npm run clean -- --dist completed; target/dist are absent. Verified temporary downloaded payloads were consumed and removed; manifest/sidecars and screenshots remain as review evidence.
