# Agent image paste and session-close confirmation

Goal: image paste into CLI agents running inside OpsTerm works, and closing live terminal sessions requires explicit confirmation. Preserve egui styling, text paste and existing inventory.

Evidence: egui-winit0.35 handles Ctrl+V as a native text paste and returns even if the clipboard has no text. The terminal never receives the agent's image-paste control key. OpsTerm's key encoding also lacks Ctrl+V. Tab X and Ctrl+Shift+W call close_tab directly, and native window close is not intercepted.

Design: retain a small documented local patch of the pinned egui-winit crate: consume native paste only when nonempty text was pasted; otherwise preserve the original keyboard event. In the focused terminal, encode Ctrl+V as byte0x16 so the CLI agent can read its own image clipboard. No image conversion, temporary screenshots, auto-submission or dependency on a specific agent. Normal text paste stays native. Verify actual native input and the Codex composer where possible; local clipboard over remote SSH still requires the agent/remote environment to support it.

Close flow: tab X and Ctrl+Shift+W request confirmation for open sessions, including disconnected tabs with output using stable session IDs. A modal provides Cancel (safe default, Escape) and Close session. Native application close with open tabs is cancelled until a separate close-all confirmation is accepted. Restart after installing updates must use that same protection. Settings/help dialog X remains a normal dialog close. Cancel must leave the session/command untouched; confirm closes only the named tab even if tabs reorder.

- [x] Add failing keyboard/close regressions and a native clipboard reproduction.
- [x] Apply and document the minimal backend patch with upstream licenses; implement terminal key encoding and confirmation state/dialogs.
- [x] Verify PNG clipboard Ctrl+V delivers exactly one control key, text paste remains exactly once, and real agent accepts an attachment without submitting a prompt.
- [x] Verify tab X/shortcut, Cancel/Escape, confirm, stable identity, native window close, and update restart protection.
- [ ] Guarded Rust/frontend checks and real native GUI smoke, independent review, signed release/install and cache cleanup.

Final source verification and limitations: [report](../../image-paste-close-2026-10-27.md). Signed release/install and final cleanup remain until publication.
