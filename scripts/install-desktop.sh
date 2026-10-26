#!/usr/bin/env bash
#
# Install the egui client for the current user (no sudo):
#   ~/.local/bin/opsterm-egui
#   application-menu entry + desktop shortcut
#   hicolor icons
#
set -euo pipefail

cd "$(dirname "$0")/.."

binary="${1:-}"
if [[ -z "$binary" ]]; then
  binary="$(find release -maxdepth 1 -type f -name 'opsterm-egui-*-linux-*' \
    ! -name '*.sha256' ! -name '*.sig' -printf '%T@ %p\n' \
    | sort -nr | head -n 1 | cut -d' ' -f2-)"
fi
if [[ -z "$binary" || ! -f "$binary" ]]; then
  echo "install: no release binary found; run npm run package:egui:local first" >&2
  exit 1
fi

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_dir="$HOME/.local/bin"
applications="$data_home/applications"
icons="$data_home/icons/hicolor"
executable="$bin_dir/opsterm-egui"

install -d -m 755 "$bin_dir"
install -m 755 "$binary" "$executable"

install -Dm644 src-tauri/icons/32x32.png "$icons/32x32/apps/org.opsterm.app.png"
install -Dm644 src-tauri/icons/128x128.png "$icons/128x128/apps/org.opsterm.app.png"
install -Dm644 src-tauri/icons/128x128@2x.png "$icons/256x256/apps/org.opsterm.app.png"
install -Dm644 src-tauri/icons/icon.png "$icons/512x512/apps/org.opsterm.app.png"

install -d -m 755 "$applications"
cat > "$applications/OpsTerm.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=OpsTerm
GenericName=Terminal Client
Comment=Linux-first secure terminal client for network operations
Exec=$executable
TryExec=$executable
Icon=org.opsterm.app
Terminal=false
Categories=Network;
Keywords=ssh;telnet;serial;terminal;switch;router;olt;
StartupNotify=true
StartupWMClass=OpsTerm
EOF
chmod 644 "$applications/OpsTerm.desktop"

desktop_dir="$(xdg-user-dir DESKTOP 2>/dev/null || true)"
if [[ -z "$desktop_dir" || "$desktop_dir" == "$HOME" ]]; then
  desktop_dir="$HOME/Desktop"
fi
if [[ -d "$desktop_dir" ]]; then
  install -m 755 "$applications/OpsTerm.desktop" "$desktop_dir/OpsTerm.desktop"
  gio set "$desktop_dir/OpsTerm.desktop" metadata::trusted true 2>/dev/null || true
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$applications/OpsTerm.desktop"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$applications" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$icons" >/dev/null 2>&1 || true
fi

echo "installed: $executable"
echo "menu:      $applications/OpsTerm.desktop"
if [[ -f "$desktop_dir/OpsTerm.desktop" ]]; then
  echo "desktop:   $desktop_dir/OpsTerm.desktop"
fi
