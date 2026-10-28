# Close confirmation dialog polish

Goal: make the existing egui close modal clear, calm and elegant without changing confirmation policy or keyboard safety.

Design: centered dark card, 24 px padding, subtle rounded border/shadow and dim backdrop. A small vector warning badge accompanies a stronger heading. Show the session name (or active-session count) in a separate inset panel, then short muted risk text. Right-align generous Cancel and solid muted-red destructive buttons. Keep Cancel's initial focus, Escape/backdrop cancellation, Ctrl+Enter explicit confirmation, stable IDs and first-frame protection. Inherit neutral colors from the selected theme and wrap long session names within the card. No bitmap assets, new dependencies, animations or behavior changes.

- [x] Restyle only close_confirmation.rs; preserve state handling.
- [x] Run existing confirmation and app regression tests; inspect native screenshots for tab/window confirmations.
- [ ] Review, release/install through existing signed pipeline and clean consumed build caches.

Verification before release: all 141 egui tests passed; fmt and clippy all-targets with -D warnings passed. Native Xvfb local-tab/window close smoke passed with screenshots inspected. Real loopback Telnet smoke at 760×500 with a long session name passed active cancellation and disconnected close; name wraps within card and controls remain visible. Independent code and visual review found no blockers. Screenshots remain outside the repository under /tmp/opsterm-modal-polish and /tmp/opsterm-modal-small.
