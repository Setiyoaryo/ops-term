#!/bin/sh
# Per-user install; replace atomically so running executables stay valid.
set -eu
usage() { printf '%s\n' 'Usage: install.sh [--prefix ABSOLUTE_DIR]' >&2; exit 2; }
if [ "$#" -eq 0 ]; then
    : "${HOME:?HOME must name your home directory}"
    prefix="$HOME/.local"
elif [ "$#" -eq 2 ] && [ "$1" = '--prefix' ]; then
    prefix="$2"
else
    usage
fi
case "$prefix" in /*) ;; *) printf '%s\n' 'Install prefix must be an absolute path' >&2; exit 2 ;; esac
case "$prefix" in *'
'*) printf '%s\n' 'Install prefix must not contain newlines' >&2; exit 2 ;; esac
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin_dir="$prefix/bin"
app_dir="$prefix/share/applications"
icon_dir="$prefix/share/icons/hicolor/256x256/apps"
mkdir -p -- "$bin_dir" "$app_dir" "$icon_dir"
staging=$(mktemp -d "$bin_dir/.opsterm-install.XXXXXX")
trap 'rm -rf -- "$staging"' EXIT HUP INT TERM
install -m 755 -- "$source_dir/opsterm-egui" "$staging/opsterm-egui"
mv -f -- "$staging/opsterm-egui" "$bin_dir/opsterm-egui"
install -m 644 -- "$source_dir/icons/128x128@2x.png" "$icon_dir/org.opsterm.app.png"
# Desktop Exec has its own escaping rules, including percent field codes.
exec_path=$(printf '%s' "$bin_dir/opsterm-egui" | sed 's/\\/\\\\\\\\/g; s/"/\\\\\\"/g; s/`/\\\\`/g; s/\$/\\\\$/g; s/%/%%/g')
while IFS= read -r line; do
    case "$line" in Exec=*) printf 'Exec="%s"\n' "$exec_path" ;; *) printf '%s\n' "$line" ;; esac
done < "$source_dir/org.opsterm.app.desktop" > "$staging/desktop"
install -m 644 -- "$staging/desktop" "$app_dir/org.opsterm.app.desktop"
printf 'Installed %s\n' "$bin_dir/opsterm-egui"
printf 'Add %s to your PATH if needed; default-prefix installs appear in your application menu.\n' "$bin_dir"
