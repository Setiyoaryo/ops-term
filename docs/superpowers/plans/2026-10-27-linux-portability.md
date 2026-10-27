# Linux portability implementation plan

Goal: distribute the existing egui application across mainstream Linux distributions, with signed updates preserved and honest compatibility limits. Diagnose the reported CachyOS issue separately when the user provides its symptoms.

Evidence: v0.1.4 ELF requires GLIBC_2.39 and dynamically links libdbus/libsystemd and their transitive libraries. This prevents older glibc distributions from running it; it does not establish the cause of the CachyOS report.

Design: build on Ubuntu 22.04 (glibc 2.35) for x86_64 and aarch64. Enable existing keyring vendored D-Bus support to remove distro-dependent D-Bus/systemd shared-library chains. Publish the current signed native binary plus a portable tar.gz and AppImage per architecture. AppImage tooling/runtime are pinned official release downloads with verified SHA-256. Never override LD_LIBRARY_PATH or bundle graphics drivers. Keep egui appearance and terminal behavior unchanged. Explicit X11/Wayland selection through OPSTERM_DISPLAY_BACKEND is a diagnostic option; auto remains default. AppImage updates select a separately signed AppImage asset and replace the outer image, never the mounted executable. Raw-binary updates remain compatible.

Compatibility: mainstream glibc-based Linux >=2.35 on x86_64/aarch64, with a working graphics/display stack. No blanket claim for musl/Alpine, 32-bit or older glibc. AppImage extraction mode is documented for systems without usable FUSE. Portable archive includes the executable, desktop/icon assets, per-user installer and diagnostic helper; no host-shell sandbox.

- [x] Add failing Python tests for ELF glibc/dependency validation, pinned tool downloads, archive contents/permissions, extra signed assets, merge/publish of multi-arch manifests.
- [x] Implement distribution builder and package guardrails. Keep Python 3.10 compatibility for Ubuntu 22.04 via tomli fallback.
- [x] Add failing Rust tests for package path/format selection and backend parsing; implement validated AppImage update target and explicit display backend hook without changing child environment.
- [ ] Update release matrix, build/test on baseline, run native GUI smoke and container CLI compatibility checks on Ubuntu, Debian, Fedora, Arch and CachyOS where official images are available.
- [ ] Review exact committed sources in isolated worktree, publish a new signed version after checks, verify assets/install/update and document known unverified CachyOS symptoms.
- [ ] Consume produced binaries/reports then clean generated caches, respecting each worktree's build lock and independent work in the original workspace.
