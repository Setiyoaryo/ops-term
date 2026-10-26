#!/usr/bin/env bash
# Optional real GUI check, using a temporary database and local shell only.
# Run: xvfb-run -a -s '-screen 0 1280x850x24' bash scripts/smoke-egui.sh
set -euo pipefail
cd "$(dirname "$0")/.."
smoke_binary="${1:-target/release/opsterm-egui}"
smoke_output="${2:-test-results/egui}"
smoke_data=$(mktemp -d /tmp/opsterm-smoke.XXXXXX)
smoke_pid=""
cleanup() {
  smoke_status=$?
  if [[ "$smoke_status" -ne 0 && -n "${smoke_window:-}" ]]; then
    import -window "$smoke_window" "$smoke_output/failure.png" 2>/dev/null || true
  fi
  if [[ -n "$smoke_pid" ]]; then
    kill "$smoke_pid" 2>/dev/null || true
    wait "$smoke_pid" 2>/dev/null || true
  fi
  rm -rf "$smoke_data"
}
trap cleanup EXIT
export XDG_DATA_HOME="$smoke_data"
export OPSTERM_SMOKE_DIR="$smoke_data"
unset WAYLAND_DISPLAY
export LIBGL_ALWAYS_SOFTWARE=1
mkdir -p "$smoke_output"
"$smoke_binary" > "$smoke_output/gui.log" 2>&1 &
smoke_pid=$!

smoke_window=""
for _ in {1..80}; do
  smoke_window=$(xdotool search --onlyvisible --pid "$smoke_pid" --name '^OpsTerm$' 2>/dev/null | head -1 || true)
  [[ -n "$smoke_window" ]] && break
  kill -0 "$smoke_pid" || { cat "$smoke_output/gui.log"; exit 1; }
  sleep 0.1
done
[[ -n "$smoke_window" ]] || { echo 'GUI window did not appear' >&2; exit 1; }
xdotool windowfocus --sync "$smoke_window"
sleep 2
xdotool mousemove --window "$smoke_window" 450 300 click 1
xdotool type --clearmodifiers 'touch "$OPSTERM_SMOKE_DIR/first-ready"'
xdotool key Return
for _ in {1..50}; do
  [[ -f "$smoke_data/first-ready" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/first-ready" ]] || { echo 'Local shell did not finish startup' >&2; exit 1; }
xdotool type --clearmodifiers 'sleep 20'
xdotool key Return
sleep 0.3
xdotool key ctrl+c
xdotool type --clearmodifiers 'touch "$OPSTERM_SMOKE_DIR/ctrlc-works"'
xdotool key Return
for _ in {1..30}; do
  [[ -f "$smoke_data/ctrlc-works" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/ctrlc-works" ]] || { import -window "$smoke_window" "$smoke_output/failure.png"; echo 'Ctrl+C did not interrupt the shell command' >&2; exit 1; }

# The command composer must execute with Enter, without adding an extra line.
xdotool key ctrl+shift+p
sleep 0.2
xdotool mousemove --window "$smoke_window" 450 690 click 1
xdotool type --clearmodifiers 'touch "$OPSTERM_SMOKE_DIR/composer-works"'
xdotool key Return
for _ in {1..30}; do
  [[ -f "$smoke_data/composer-works" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/composer-works" ]] || { echo 'Command composer did not send on Enter' >&2; exit 1; }

# Click a saved device once to open a second real shell, without network access.
xdotool key ctrl+shift+p
python3 - <<'PY'
import os, pathlib, sqlite3
with sqlite3.connect(pathlib.Path(os.environ['XDG_DATA_HOME']) / 'org.opsterm.app/opsterm.db') as db:
    db.execute('INSERT INTO sessions(id,name,host,protocol,created_at,updated_at) VALUES(?,?,?,?,?,?)',
               ('smoke-local', 'Smoke local device', 'local', 'local', '2026-10-22', '2026-10-22'))
PY
xdotool mousemove --window "$smoke_window" 200 170 click 1
sleep 0.2
xdotool mousemove --window "$smoke_window" 45 208 click 1
sleep 1
xdotool mousemove --window "$smoke_window" 450 300 click 1
xdotool type --clearmodifiers 'export OPSTERM_SMOKE_TAB=SECOND; touch "$OPSTERM_SMOKE_DIR/second-ready"'
xdotool key Return
for _ in {1..30}; do
  [[ -f "$smoke_data/second-ready" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/second-ready" ]] || { echo 'Second shell did not open' >&2; exit 1; }
xdotool mousemove --window "$smoke_window" 45 63 click 1
xdotool key ctrl+shift+p
sleep 0.2
xdotool mousemove --window "$smoke_window" 450 690 click 1
xdotool type --clearmodifiers 'test "${OPSTERM_SMOKE_TAB:-FIRST}" = FIRST && touch "$OPSTERM_SMOKE_DIR/tab-switch-works"'
xdotool key Return
for _ in {1..30}; do
  [[ -f "$smoke_data/tab-switch-works" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/tab-switch-works" ]] || { echo 'Tab selection did not change the command target' >&2; exit 1; }
# A busy output stream must not monopolize event handling or prevent a new tab.
xdotool key ctrl+shift+p
xdotool mousemove --window "$smoke_window" 450 300 click 1
xdotool type --clearmodifiers 'yes OPSTERM-STRESS'
xdotool key Return
sleep 0.2
xdotool key ctrl+shift+t
sleep 0.5
xdotool mousemove --window "$smoke_window" 450 300 click 1
xdotool type --clearmodifiers 'touch "$OPSTERM_SMOKE_DIR/responsive-under-output"'
xdotool key Return
for _ in {1..50}; do
  [[ -f "$smoke_data/responsive-under-output" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/responsive-under-output" ]] || { echo 'UI stalled during continuous output' >&2; exit 1; }
xdotool key ctrl+shift+w
sleep 0.2
xdotool key ctrl+Return
sleep 0.2
# Closing the last tab selects SECOND, so explicitly return to busy FIRST.
xdotool mousemove --window "$smoke_window" 45 63 click 1
xdotool mousemove --window "$smoke_window" 450 300 click 1
xdotool key ctrl+c
xdotool type --clearmodifiers 'test "${OPSTERM_SMOKE_TAB:-FIRST}" = FIRST && touch "$OPSTERM_SMOKE_DIR/output-interrupted"'
xdotool key Return
for _ in {1..50}; do
  [[ -f "$smoke_data/output-interrupted" ]] && break
  sleep 0.1
done
[[ -f "$smoke_data/output-interrupted" ]] || { echo 'Could not interrupt continuous output' >&2; exit 1; }
import -window "$smoke_window" "$smoke_output/desktop.png"
xdotool windowsize "$smoke_window" 760 500
sleep 0.5
import -window "$smoke_window" "$smoke_output/minimum.png"
echo 'GUI smoke passed: local PTY, Ctrl+C, composer Enter, saved-device row click, isolated tab targets, continuous-output responsiveness, normal/minimum window.'
