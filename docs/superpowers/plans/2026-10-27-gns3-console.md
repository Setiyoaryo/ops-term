# GNS3 console integration

Goal: let GNS3's custom Telnet console command launch OpsTerm directly into the selected node, retaining the current egui interface.

Design: `opsterm-egui --telnet HOST --port PORT --title NAME` opens one window with one Telnet tab. No implicit local shell, saved-device writes, password arguments, shell interpolation, IPC, or GNS3 configuration changes. Reuse the existing asynchronous connection queue and Telnet provider; errors remain visible in the terminal. Normal no-argument startup remains Local Shell. Separate windows are explicit initial scope; reusing a window requires a separate IPC feature. VNC stays in the existing external viewer.

Trust boundary: command arguments from GNS3. Validate host with core security checks plus IPv6 parsing, require numeric 1–65535 ports, reject duplicate/unknown flags and control characters in display titles. Titles are plain egui text and never executable input. `--help` and `--version` work without a display/database.

Plan (executed inline with writing-plans and TDD):
- [x] Add failing parser tests in `egui/src/startup.rs`: default shell, GNS3 arguments, IPv6, title with spaces/metacharacters, malformed/duplicate/unknown arguments, display-free help/version.
- [x] Implement parser and hook `egui/src/main.rs` startup into the existing queue. Test the resulting tab origin/title and absence of a local shell.
- [x] Run guarded Rust suite, formatting, clippy and a real local TCP Telnet/Xvfb roundtrip. Verify failed connections remain closable and display errors.
- [ ] Document the exact GNS3 Preferences command and separate-window behavior in README. Review, build/install a usable binary, and clean all generated caches after consuming outputs.
