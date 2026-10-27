# Image paste and close confirmation verification — 27 October 2026

## Image input

The native egui-winit 0.35 text clipboard handler consumed Ctrl+V even when the clipboard held only an image, so the CLI received no key. OpsTerm also lacked Ctrl+V encoding. Both a keyboard regression and an isolated native PNG clipboard test reproduced the failure before the fix.

The pinned backend patch consumes the native shortcut only when nonempty text was pasted; otherwise it preserves the original key. The focused terminal encodes Ctrl+V as byte0x16, allowing the CLI agent to read its own image clipboard. No image decoding, image file creation or clipboard upload is added to OpsTerm. Ctrl+V and Ctrl+Shift+V native text paste remain exactly once.

The small egui-winit source patch and upstream licenses/provenance are in `vendor/egui-winit/OPSTERM-PATCH.md`. Other locked dependency versions were preserved.

A real Codex CLI 0.164.0 was launched inside the native OpsTerm terminal on an isolated Xvfb desktop. Pasting an 8×8 PNG clipboard fixture produced `[Image #1]` in its composer. The screenshot was inspected; the test ended at attachment acceptance, with no prompt submitted. Screenshot: `/tmp/opsterm-codex-image-check/image-pasted.png`.

[Official Codex image-input documentation](https://learn.chatgpt.com/docs/image-inputs?surface=cli) describes clipboard image input in the interactive composer.

## Close flow

Tab X and Ctrl+Shift+W now request confirmation for every tab, including disconnected tabs with retained terminal output. Native window close is cancelled while a confirmation is pending if any tabs remain. Update restart shares the confirmation flow. Settings/help dialog close buttons remain ordinary dialog close actions.

Cancel gets initial focus; Escape cancels. Ctrl+Enter confirms only after that specific prompt has been shown in an earlier native frame, so a close-one approval cannot approve a newly expanded close-all request. Stable session IDs prevent reordering from closing the wrong tab.

The modal handles keyboard events before background forms and removes leftover keyboard/IME input. Input remains blocked throughout dismissal/layout replay passes. Regressions reproduced and fixed background composer submission, unwanted Quick Connect, premature scope approval, native-close replay and lost editor/terminal focus. A discard layout pass clears egui's previous-frame modal layer before the next native keyboard input; cancellation restores the previous widget rather than forcing terminal focus.

## Verification

- Guarded Rust tests: 55 core + 105 egui passed.
- Clippy core/egui all targets with warnings denied passed; formatting passed.
- Frontend unit tests: 18 passed; TypeScript/Vite build passed.
- `smoke-egui-clipboard.py`: independently verifies the PNG/text owner, Ctrl+V and Ctrl+Shift+V image key delivery exactly once, and native text paste exactly once.
- `smoke-egui-close.py`: real tab X and WM_DELETE_WINDOW, Escape/default Enter cancellation, typing after cancellation, explicit confirmation, and empty-window close passed.
- Existing real local-shell GUI smoke passed, including Ctrl+C, composer Enter, saved-device opening, tab isolation, continuous-output responsiveness and minimum window.
- Actual Codex composer accepted the PNG attachment. No model response or other agent product was claimed as tested.
- Independent read-only reviews checked the backend patch/licenses, close identity, first-frame input guards, scope approval and focus handling; findings were corrected with regressions.

The clipboard fixture initially inherited the desktop's Wayland GTK backend during development and touched its clipboard. This was acknowledged and corrected. Final native clipboard results use an independently probed X11 owner, explicit X11 GTK/session settings, and no Wayland display/socket. GUI fixtures now refuse to run outside xvfb-run to avoid repeating that mistake.

## Limits

Image input depends on the agent supporting its image-paste key and accessing the same clipboard. There is no local-to-remote image transfer over SSH. Dual-format clipboards retain native text priority. Wayland uses the shared keyboard fallback and existing clipboard backend; the automated native desktop tests were run under X11/Xvfb. The isolated OpsTerm tests use temporary databases; existing inventory, vault and user windows were preserved.

CI browser setup now matches the proven Pages setup: install the Playwright headless browser using hosted Ubuntu runtime libraries rather than re-downloading unrelated system fonts through a slow apt mirror. Browser checks remain enabled.

## Published and installed

The signed [v0.1.3 release](https://github.com/Setiyoaryo/ops-term/releases/tag/v0.1.3) is public. [CI](https://github.com/Setiyoaryo/ops-term/actions/runs/40424276087), [release verification](https://github.com/Setiyoaryo/ops-term/actions/runs/40423407488), and [automatic Pages refresh](https://github.com/Setiyoaryo/ops-term/actions/runs/40425092554) succeeded.

The previously trusted updater installed the public v0.1.3 asset after signature/hash verification. The new executable reported `OpsTerm v0.1.3 is up to date`. Its independently checked SHA-256 matches the public manifest:

`ba196ac796e7255fae615cfc6607a39a56a364c616d18e7796d3a69ffc35fc72`

Both native PNG/text clipboard and tab/native-window confirmation GUI smokes passed again against the installed release binary. The public website download points to v0.1.3. Existing user windows were preserved and require a user-controlled relaunch from the installed launcher to load the update. Generated workspace caches were removed after verification.
