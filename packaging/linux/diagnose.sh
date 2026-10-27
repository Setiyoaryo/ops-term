#!/bin/sh
# Only public system/display capabilities; no environment or process dumps.
set -u
printf '%s\n' 'OpsTerm Linux diagnostics'
if [ -r /etc/os-release ]; then
    sed -n '/^NAME=/p; /^VERSION=/p; /^ID=/p; /^VERSION_ID=/p' /etc/os-release
fi
printf 'Architecture: '; uname -m
printf 'C library: '; getconf GNU_LIBC_VERSION 2>/dev/null || true
if [ -n "${DISPLAY:-}" ]; then printf '%s\n' 'X11 display variable: set'; else printf '%s\n' 'X11 display variable: unset'; fi
if [ -n "${WAYLAND_DISPLAY:-}" ]; then printf '%s\n' 'Wayland display variable: set'; else printf '%s\n' 'Wayland display variable: unset'; fi
printf '%s\n' 'Requires glibc >= 2.35 and a working OpenGL/display stack.'
printf '%s\n' 'Display checks: OPSTERM_DISPLAY_BACKEND=x11 or OPSTERM_DISPLAY_BACKEND=wayland.'
printf '%s\n' 'For missing graphics libraries, install your distribution Mesa/vendor graphics drivers.'
printf '%s\n' 'Without usable FUSE, run the AppImage with --appimage-extract-and-run.'
