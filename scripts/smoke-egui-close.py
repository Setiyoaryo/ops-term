#!/usr/bin/env python3
"""Confirm real tab X and native window X behavior in an isolated Xvfb desktop."""
import ctypes
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

spec = importlib.util.spec_from_file_location('measure', 'scripts/measure-egui.py')
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)


def run(*args):
    subprocess.run(list(args), check=True)


def wait_for(predicate, message, seconds=4):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate(): return
        time.sleep(.04)
    raise AssertionError(message)


def window_close(window):
    # Send WM_DELETE_WINDOW to our own test window; no window manager required.
    class Data(ctypes.Union):
        _fields_ = [('bytes', ctypes.c_char * 20), ('shorts', ctypes.c_short * 10), ('longs', ctypes.c_long * 5)]
    class Message(ctypes.Structure):
        _fields_ = [('type', ctypes.c_int), ('serial', ctypes.c_ulong), ('send_event', ctypes.c_int),
                    ('display', ctypes.c_void_p), ('window', ctypes.c_ulong), ('message_type', ctypes.c_ulong),
                    ('format', ctypes.c_int), ('data', Data)]
    class Event(ctypes.Union):
        _fields_ = [('message', Message), ('padding', ctypes.c_long * 24)]
    x11 = ctypes.CDLL('libX11.so.6')
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(None)
    assert display
    try:
        event = Event()
        event.message.type = 33
        event.message.display = display
        event.message.window = int(window)
        event.message.message_type = x11.XInternAtom(display, b'WM_PROTOCOLS', 0)
        event.message.format = 32
        event.message.data.longs[0] = x11.XInternAtom(display, b'WM_DELETE_WINDOW', 0)
        assert x11.XSendEvent(display, int(window), 0, 0, ctypes.byref(event))
        x11.XFlush(display)
    finally:
        x11.XCloseDisplay(display)


def main():
    # Prevent accidental use of the real desktop/clipboard when run directly.
    authority = Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Run this GUI test through xvfb-run; refusing the real desktop')
    output = Path(sys.argv[2] if len(sys.argv) > 2 else '/tmp/opsterm-close-gui')
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opsterm-close-') as folder:
        data = Path(folder)
        env = dict(os.environ, XDG_DATA_HOME=folder, XDG_CONFIG_HOME=str(data / 'config'),
                   OPSTERM_CLOSE_SMOKE_DIR=folder, GDK_BACKEND='x11', XDG_SESSION_TYPE='x11',
                   WINIT_UNIX_BACKEND='x11', LIBGL_ALWAYS_SOFTWARE='1')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('WAYLAND_SOCKET', None)
        process, window = measure.launch(Path(sys.argv[1]).resolve(), env)
        def click(x, y):
            run('xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1')
            time.sleep(.15)
        def command(marker):
            run('xdotool', 'type', '--clearmodifiers', 'touch "$OPSTERM_CLOSE_SMOKE_DIR/' + marker + '"')
            run('xdotool', 'key', 'Return')
            wait_for(lambda: (data / marker).exists(), 'Session did not survive ' + marker)
        try:
            run('xdotool', 'windowfocus', '--sync', window)
            time.sleep(1)
            click(450, 300)
            command('ready')
            click(141, 65)
            run('import', '-window', window, str(output / 'tab-confirm.png'))
            run('xdotool', 'key', 'Escape')
            command('tab-escape')
            click(141, 65)
            run('xdotool', 'key', 'Return')
            command('tab-enter-cancel')
            click(141, 65)
            run('xdotool', 'key', 'ctrl+Return')
            time.sleep(.3)
            run('import', '-window', window, str(output / 'tab-closed.png'))
            # With no live sessions the native close needs no extra prompt.
            window_close(window)
            process.wait(timeout=4)
            process, window = measure.launch(Path(sys.argv[1]).resolve(), env)
            run('xdotool', 'windowfocus', '--sync', window)
            time.sleep(1)
            click(450, 300)
            command('window-ready')
            window_close(window)
            time.sleep(.3)
            assert process.poll() is None, 'Native X quit without confirmation'
            run('import', '-window', window, str(output / 'app-confirm.png'))
            run('xdotool', 'key', 'Escape')
            command('window-escape')
            window_close(window)
            time.sleep(.2)
            run('xdotool', 'key', 'Return')
            command('window-enter-cancel')
            window_close(window)
            time.sleep(.2)
            run('xdotool', 'key', 'ctrl+Return')
            process.wait(timeout=4)
            assert process.returncode == 0
            print('Close GUI passed: tab X and native X require confirmation; Escape/Enter cancel; explicit confirm closes.')
        except BaseException:
            if process.poll() is None:
                run('import', '-window', window, str(output / 'failure.png'))
            raise
        finally:
            measure.stop(process)


if __name__ == '__main__':
    main()
