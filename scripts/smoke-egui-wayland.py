#!/usr/bin/env python3
"""Check real Wayland buffer presentation on a private headless Weston socket."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def stop(process):
    if process is None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()


def main():
    binary = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix='opsterm-wayland-') as temporary:
        root = Path(temporary)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), WAYLAND_DISPLAY='wayland-opsterm', TMPDIR=str(root),
            XDG_DATA_HOME=str(root / 'data'), XDG_CONFIG_HOME=str(root / 'config'),
            LIBGL_ALWAYS_SOFTWARE='1', OPSTERM_DISPLAY_BACKEND='wayland', SHELL='/bin/sh')
        env.pop('DISPLAY', None)
        env.pop('WAYLAND_SOCKET', None)
        help_text = subprocess.run(['weston', '--backend=headless-backend.so', '--help'], env=env, text=True, capture_output=True).stdout
        renderer = '--renderer=pixman' if '--renderer' in help_text else '--use-pixman'
        compositor = subprocess.Popen(['weston','--backend=headless-backend.so',renderer,
            '--socket=wayland-opsterm','--idle-time=0','--log='+str(root/'weston.log')], env=env,
            start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        app = None
        try:
            deadline = time.monotonic() + 12
            while not (runtime / 'wayland-opsterm').exists():
                if compositor.poll() is not None or time.monotonic() > deadline:
                    raise AssertionError('Private Weston did not start: '+(root/'weston.log').read_text())
                time.sleep(.1)
            with (root/'client.log').open('w') as log:
                app = subprocess.Popen([str(binary)], env=dict(env,WAYLAND_DEBUG='client'),
                    start_new_session=True, stdout=subprocess.DEVNULL, stderr=log)
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    text = (root/'client.log').read_text(errors='replace')
                    if app.poll() is not None:
                        raise AssertionError('Wayland client exited: '+text[-3000:])
                    if 'wl_surface' in text and '.attach(' in text and '.commit(' in text:
                        print('PASS: real Wayland buffer attached and committed')
                        return
                    time.sleep(.1)
                raise AssertionError('No Wayland buffer presentation observed: '+text[-3000:])
        finally:
            stop(app)
            stop(compositor)


if __name__ == '__main__':
    main()
