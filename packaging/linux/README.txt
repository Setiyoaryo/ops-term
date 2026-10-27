OpsTerm for Linux
=================
Supports 64-bit x86_64 or aarch64 glibc-based systems with glibc >= 2.35,
a working display server and OpenGL graphics drivers. musl/Alpine, 32-bit
and older glibc systems are not supported by these binaries.

Run ./opsterm-egui, or run ./install.sh to install into ~/.local/bin and
create a per-user desktop launcher. No root privileges are required.
Use ./install.sh --prefix /absolute/path for another user-owned prefix.
The installer replaces the executable atomically; running sessions may
continue using the previous executable. Re-run the installer to upgrade.

AppImage: chmod +x the downloaded .AppImage, then run it. Without usable
FUSE, use ./OpsTerm.AppImage --appimage-extract-and-run. Do not run as root.
AppImage and archive use host graphics drivers; no driver paths are forced.

Run ./diagnose.sh for limited system/display information. To investigate
window creation, explicitly try OPSTERM_DISPLAY_BACKEND=x11 or wayland.
The default automatically selects the available backend.

MIT license: see LICENSE. Statically linked D-Bus: see
THIRD_PARTY_NOTICES.txt and dbus-COPYING (AFL-2.1). Original application icon assets are in icons/.
