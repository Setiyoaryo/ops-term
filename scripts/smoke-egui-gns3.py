#!/usr/bin/env python3
"""GNS3-style launch against a real loopback Telnet console in isolated Xvfb."""
import os
from pathlib import Path
import socket
import signal
import subprocess
import sys
import tempfile
import time


def wait_for(predicate, message, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(.05)
    raise AssertionError(message)


def stop(process):
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
    authority = Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Run through xvfb-run; refusing the real desktop')
    binary = str(Path(sys.argv[1]).resolve())
    output = Path(sys.argv[2] if len(sys.argv) > 2 else '/tmp/opsterm-gns3-gui')
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opsterm-gns3-') as folder, socket.socket() as server:
        server.bind(('127.0.0.1', 0))
        server.listen(1)
        server.settimeout(15)
        env = dict(os.environ, XDG_DATA_HOME=folder, XDG_CONFIG_HOME=folder + '/config', TMPDIR=folder,
                   GDK_BACKEND='x11', XDG_SESSION_TYPE='x11', WINIT_UNIX_BACKEND='x11',
                   LIBGL_ALWAYS_SOFTWARE='1')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('WAYLAND_SOCKET', None)
        process = subprocess.Popen([binary, '--telnet', '127.0.0.1', '--port',
                                   str(server.getsockname()[1]), '--title', 'GNS3 R1 lab'],
                                  env=env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            with server.accept()[0] as console:
                console.settimeout(.2)
                console.sendall(b'\r\nGNS3 TELNET ROUNDTRIP READY\r\nRouter# ')
                def find_window():
                    result = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', 'GNS3 R1 lab'],
                                            capture_output=True, text=True)
                    return result.stdout.splitlines()[0] if result.returncode == 0 else None
                window = wait_for(find_window, 'Named GNS3 console window did not appear')
                subprocess.run(['xdotool', 'windowfocus', '--sync', window], check=True)
                time.sleep(1)
                subprocess.run(['xdotool', 'mousemove', '--window', window, '450', '300', 'click', '1'], check=True)
                subprocess.run(['xdotool', 'type', '--clearmodifiers', 'show version'], check=True)
                subprocess.run(['xdotool', 'key', 'Return'], check=True)
                received = bytearray()
                def got_command():
                    try:
                        received.extend(console.recv(4096))
                    except socket.timeout:
                        pass
                    return b'show version\r' in received
                wait_for(got_command, 'Typing did not reach the real Telnet socket')
                console.sendall(b'\r\nGNS3 ROUNDTRIP OK\r\nRouter# ')
                time.sleep(.4)
                subprocess.run(['import', '-window', window, str(output / 'console.png')], check=True)
                # A second simultaneous GNS3 launch must create its own window.
                # A bound, non-listening port deterministically refuses TCP.
                with socket.socket() as refused:
                    refused.bind(('127.0.0.1', 0))
                    offline = subprocess.Popen([binary, '--telnet', '127.0.0.1', '--port',
                        str(refused.getsockname()[1]), '--title', 'GNS3 Offline'],
                        env=env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    try:
                        def find_offline():
                            result = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', 'GNS3 Offline'], capture_output=True, text=True)
                            return result.stdout.splitlines()[0] if result.returncode == 0 else None
                        offline_window = wait_for(find_offline, 'Second GNS3 window did not appear')
                        time.sleep(.7)
                        assert offline.poll() is None and process.poll() is None
                        subprocess.run(['import', '-window', offline_window,
                                        str(output / 'connection-refused.png')], check=True)
                    finally:
                        stop(offline)
                subprocess.run(['xdotool', 'windowfocus', '--sync', window], check=True)
                subprocess.run(['xdotool', 'key', 'ctrl+shift+w'], check=True)
                time.sleep(.25)
                subprocess.run(['import', '-window', window, str(output / 'active-close-confirm.png')], check=True)
                subprocess.run(['xdotool', 'key', 'Escape'], check=True)
                console.sendall(b'\r\nACTIVE SESSION SURVIVED CANCEL\r\nRouter# ')
                time.sleep(.2)
            time.sleep(.5)
            subprocess.run(['xdotool', 'key', 'ctrl+shift+w'], check=True)
            time.sleep(.25)
            subprocess.run(['import', '-window', window, str(output / 'disconnected-tab-closed.png')], check=True)
            assert process.poll() is None, 'Closing a disconnected tab unexpectedly quit the app'
            print('PASS: GNS3 arguments, named window, real Telnet output/input, simultaneous windows, active close cancellation and immediate disconnected close (inspect screenshots)')
        finally:
            stop(process)


if __name__ == '__main__':
    main()
