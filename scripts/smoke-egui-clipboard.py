#!/usr/bin/env python3
"""Native image/text clipboard regression in an isolated Xvfb desktop.

Requires GTK3/PyGObject, xdotool and ImageMagick. Run with build_guard and
xvfb-run; clipboard ownership and the app database stay inside that desktop.
No prompt is submitted to any external service.
"""
import importlib.util
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time

spec = importlib.util.spec_from_file_location("measure", "scripts/measure-egui.py")
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)

OWNER = '''
import sys
import gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk, GdkPixbuf
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if sys.argv[1] == 'image':
    pixels = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, False, 8, 8, 8)
    pixels.fill(0xB17852FF)
    clipboard.set_image(pixels)
else:
    clipboard.set_text('opsterm clipboard text', -1)
print('READY', flush=True)
Gtk.main()
'''
COLLECTOR = '''
import os, pathlib, select, termios, time, tty
folder = pathlib.Path(os.environ['OPSTERM_CLIPBOARD_SMOKE_DIR'])
old = termios.tcgetattr(0)
try:
    tty.setraw(0)
    (folder / 'ready').touch()
    received = bytearray()
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if select.select([0], [], [], .1)[0]:
            data = os.read(0, 4096)
            if not data: break
            received.extend(data)
            (folder / 'received').write_bytes(received)
finally:
    termios.tcsetattr(0, termios.TCSANOW, old)
'''


def run(*args):
    subprocess.run(list(args), check=True)


def wait_for(predicate, message, seconds=4):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.03)
    raise AssertionError(message)


def main():
    # Prevent accidental use of the real desktop/clipboard when run directly.
    authority = Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Run this GUI test through xvfb-run; refusing the real desktop')
    output = Path(sys.argv[2] if len(sys.argv) > 2 else '/tmp/opsterm-clipboard-gui')
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opsterm-clipboard-') as folder:
        data = Path(folder)
        (data / 'collector.py').write_text(COLLECTOR)
        env = dict(os.environ, XDG_DATA_HOME=folder, XDG_CONFIG_HOME=str(data / 'config'),
                   OPSTERM_CLIPBOARD_SMOKE_DIR=folder, LIBGL_ALWAYS_SOFTWARE='1',
                   GDK_BACKEND='x11', XDG_SESSION_TYPE='x11', WINIT_UNIX_BACKEND='x11')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('WAYLAND_SOCKET', None)
        process, window = measure.launch(Path(sys.argv[1]).resolve(), env)
        owner = None
        def own(kind):
            nonlocal owner
            if owner:
                measure.stop(owner)
            owner = subprocess.Popen([sys.executable, '-c', OWNER, kind], env=env,
                                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            assert select.select([owner.stdout], [], [], 3)[0], 'Clipboard owner failed to start'
            assert owner.stdout.readline().strip() == 'READY'
            time.sleep(.1)
            probe = "import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk,Gdk; c=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD); print(c.wait_for_text() if '" + kind + "' == 'text' else bool(c.wait_for_image()))"
            result = subprocess.check_output([sys.executable, '-c', probe], env=env, text=True).strip()
            assert result == ('opsterm clipboard text' if kind == 'text' else 'True'), ('Clipboard fixture did not provide requested content', kind, result)
        try:
            run('xdotool', 'windowfocus', '--sync', window)
            time.sleep(1)
            run('xdotool', 'mousemove', '--window', window, '450', '300', 'click', '1')
            run('xdotool', 'type', '--clearmodifiers', 'python3 "$OPSTERM_CLIPBOARD_SMOKE_DIR/collector.py"')
            run('xdotool', 'key', 'Return')
            wait_for(lambda: (data / 'ready').is_file(), 'Local collector did not start')
            own('image')
            run('xdotool', 'key', 'ctrl+v')
            received = data / 'received'
            wait_for(lambda: received.is_file(), 'Image Ctrl+V was swallowed; terminal received nothing')
            time.sleep(.15)
            assert received.read_bytes() == b'\x16', received.read_bytes().hex()
            run('xdotool', 'key', 'ctrl+shift+v')
            wait_for(lambda: received.read_bytes() == b'\x16\x16', 'Image paste did not arrive exactly once')
            own('text')
            run('xdotool', 'key', 'ctrl+v')
            expected = b'\x16\x16opsterm clipboard text'
            wait_for(lambda: received.read_bytes() == expected, 'Native text paste changed or duplicated')
            time.sleep(.15)
            assert received.read_bytes() == expected
            run('xdotool', 'key', 'ctrl+shift+v')
            expected += b'opsterm clipboard text'
            wait_for(lambda: received.read_bytes() == expected, 'Ctrl+Shift+V text paste changed or duplicated')
            run('import', '-window', window, str(output / 'clipboard.png'))
            print('Clipboard GUI passed: PNG Ctrl+V/Ctrl+Shift+V reached the PTY once; text remained native and exactly once.')
        except BaseException:
            if (data / 'received').is_file():
                print('Synthetic clipboard bytes:', (data / 'received').read_bytes().hex(), flush=True)
            run('import', '-window', window, str(output / 'failure.png'))
            raise
        finally:
            if owner:
                measure.stop(owner)
            measure.stop(process)


if __name__ == '__main__':
    main()
