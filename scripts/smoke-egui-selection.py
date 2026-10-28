#!/usr/bin/env python3
"""Drag into scrollback and verify copied history in a private Xvfb desktop."""
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

spec = importlib.util.spec_from_file_location('measure', 'scripts/measure-egui.py')
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)


def run(*args):
    subprocess.run(list(args), check=True)


def main():
    authority = Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Run through xvfb-run; refusing the real desktop/clipboard')
    output = Path(sys.argv[2] if len(sys.argv) > 2 else '/tmp/opsterm-selection-gui')
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opsterm-selection-') as folder:
        env = dict(os.environ, XDG_DATA_HOME=folder, XDG_CONFIG_HOME=folder + '/config',
                   OPSTERM_SELECTION_SMOKE_DIR=folder, LIBGL_ALWAYS_SOFTWARE='1',
                   OPSTERM_DISPLAY_BACKEND='x11', GDK_BACKEND='x11', XDG_SESSION_TYPE='x11')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('WAYLAND_SOCKET', None)
        process, window = measure.launch(Path(sys.argv[1]).resolve(), env)
        try:
            run('xdotool', 'windowfocus', '--sync', window)
            time.sleep(1)
            run('xdotool', 'mousemove', '--window', window, '450', '300', 'click', '1')
            command = 'seq -f "selection-row-%03g" 1 500; touch "$OPSTERM_SELECTION_SMOKE_DIR/ready"'
            run('xdotool', 'type', '--clearmodifiers', command)
            run('xdotool', 'key', 'Return')
            deadline = time.monotonic() + 10
            while not (Path(folder) / 'ready').exists():
                if time.monotonic() > deadline:
                    raise AssertionError('History workload did not finish')
                time.sleep(.05)
            time.sleep(.7)
            run('xdotool', 'mousemove', '--window', window, '500', '500', 'mousedown', '1')
            run('xdotool', 'mousemove', '--window', window, '400', '75')
            time.sleep(3)
            run('xdotool', 'mouseup', '1')
            run('xdotool', 'key', 'ctrl+shift+c')
            time.sleep(.3)
            probe = "import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk,Gdk; print(Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text() or '')"
            text = subprocess.check_output([sys.executable, '-c', probe], env=env, text=True, timeout=5)
            (output / 'copied.txt').write_text(text)
            run('import', '-window', window, str(output / 'selection.png'))
            rows = [int(n) for n in re.findall(r'selection-row-(\d+)', text)]
            assert rows and min(rows) < 450, ('Selection stuck in visible viewport', rows)
            print(f'PASS: held drag reached scrollback; copied rows {min(rows)}..{max(rows)}')
        finally:
            measure.stop(process)


if __name__ == '__main__':
    main()
