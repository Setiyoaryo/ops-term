#!/usr/bin/env python3
"""Native saved-login regression. Use build_guard + xvfb-run; loopback only.

Requires xdotool, ImageMagick, dbus-run-session, gdbus and gnome-keyring.
Creates its own Secret Service and XDG directories; never uses a user vault.
"""
import json
import os
import pathlib
import select
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import uuid


def run(*args, env=None):
    result = subprocess.run(args, env=env, check=True, capture_output=True,
                            text=True, timeout=15)
    return result.stdout.strip()


def stop_process(process):
    if process is None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def wait_for(check, message, seconds=12, errors=None):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if errors:
            raise AssertionError(errors[0])
        value = check()
        if value:
            return value
        time.sleep(.05)
    if errors:
        raise AssertionError(errors[0])
    raise AssertionError(message)


class Wire:
    """Strip Telnet negotiation, preserving every application byte and CR line."""
    def __init__(self, peer, listener):
        self.peer, self.listener = peer, listener
        self.state = 'data'
        self.pending = bytearray()
        self.lines = []

    def poll(self, seconds):
        ready, _, _ = select.select([self.peer, self.listener], [], [], seconds)
        if self.listener in ready:
            extra, _ = self.listener.accept()
            extra.close()
            raise AssertionError('Double click opened more than one TCP connection')
        if self.peer not in ready:
            return
        data = self.peer.recv(4096)
        if not data:
            raise AssertionError('Fixture disconnected before verification finished')
        for value in data:
            if self.state == 'iac':
                self.state = ('sub' if value == 250 else 'option'
                              if value in (251, 252, 253, 254) else 'data')
                if value == 255:
                    self.pending.append(value)
            elif self.state == 'option':
                self.state = 'data'
            elif self.state == 'sub':
                if value == 255:
                    self.state = 'sub-iac'
            elif self.state == 'sub-iac':
                self.state = 'data' if value == 240 else 'sub'
            elif value == 255:
                self.state = 'iac'
            elif value == 13:
                self.lines.append(bytes(self.pending))
                self.pending.clear()
            elif value not in (0, 10):
                self.pending.append(value)

    def expect(self, expected, label, seconds=8):
        deadline = time.monotonic() + seconds
        while not self.lines and time.monotonic() < deadline:
            self.poll(min(.1, max(0, deadline - time.monotonic())))
        # Never include received contents in failures: a failed test may contain a secret.
        if not self.lines or self.lines.pop(0) != expected:
            raise AssertionError(label)

    def quiet(self, seconds, allow_nudge=False):
        deadline = time.monotonic() + seconds
        nudges = 0
        while True:
            while self.lines:
                value = self.lines.pop(0)
                if allow_nudge and value == b'' and nudges == 0:
                    nudges += 1
                else:
                    raise AssertionError('Unexpected application command before its allowed stage')
            if self.pending:
                raise AssertionError('Unexpected partial application command before its allowed stage')
            if time.monotonic() >= deadline:
                return
            self.poll(min(.1, deadline - time.monotonic()))


def fixture(listener, pager_ready, roundtrip, stop, errors):
    try:
        listener.settimeout(.2)
        deadline = time.monotonic() + 12
        while not stop.is_set() and time.monotonic() < deadline:
            try:
                peer, _ = listener.accept()
                break
            except socket.timeout:
                continue
        else:
            if stop.is_set():
                return
            raise AssertionError('No loopback connection after device double click')
        with peer:
            wire = Wire(peer, listener)
            # A prompt-shaped banner is not evidence that authentication finished.
            peer.sendall(b'Welcome to Huawei console\r\n<BR-APJI>\r\n')
            # Exceeds the former 1500 ms pager timer; only the legacy Enter is allowed.
            wire.quiet(2.2, allow_nudge=True)
            for chunk in (b'\r\n\x1b[3', b'2mUser', b'name ', b':', b'\x1b[0m '):
                peer.sendall(chunk)
                time.sleep(.09)
            wire.expect(b'test-user', 'Saved blank username did not fall back to credential username')
            for chunk in (b'\r\n\x1b[3', b'3mPass', b'word ', b':', b'\x1b[0m '):
                peer.sendall(chunk)
                time.sleep(.09)
            wire.expect(b'fixture-password', 'Saved password was not answered exactly once')
            # Correct credentials alone must not release the pager before a CLI prompt.
            wire.quiet(1.8)
            peer.sendall(b'\r\nAuthentication successful\r\n\x1b[32m<BR-')
            wire.quiet(.25)
            peer.sendall(b'APJI>\x1b[0m ')
            wire.expect(b'screen-length 0 temporary', 'Huawei pager missing or wrong after CLI prompt')
            wire.quiet(.3)
            peer.sendall(b'\r\n<BR-APJI> ')
            pager_ready.set()
            wire.expect(b'display version', 'Direct keyboard command missing or pager sent more than once')
            peer.sendall(b'display version\r\nROUNDTRIP_OK Huawei fixture\r\n<BR-APJI> ')
            roundtrip.set()
            while not stop.is_set():
                wire.quiet(.1)
    except Exception as error:
        errors.append(str(error))


def private_session(binary, output, folder):
    def interrupted(_signum, _frame):
        raise SystemExit('Isolated GUI smoke interrupted')
    signal.signal(signal.SIGTERM, interrupted)
    env = dict(os.environ)
    reference = f'opsterm-smoke-login-{uuid.uuid4()}'
    helper = pathlib.Path(os.environ.get(
        'OPSTERM_LOGIN_HELPER', 'target/debug/examples/credential_store_smoke')).resolve()
    daemon = process = None
    window = None
    stop = threading.Event()
    server = None
    errors = []
    with (output / 'gui.log').open('w') as log:
        try:
            control = folder / 'runtime/keyring'
            control.mkdir(mode=0o700)
            env['GNOME_KEYRING_CONTROL'] = str(control)
            daemon = subprocess.Popen(
                ['gnome-keyring-daemon', '--foreground', '--unlock', '--components=secrets',
                 '--control-directory', str(control)], env=env, stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            # The temporary vault master is delivered only through a pipe.
            daemon.stdin.write((uuid.uuid4().hex + '\n').encode())
            daemon.stdin.close()
            def secret_service_ready():
                if daemon.poll() is not None:
                    raise AssertionError('Private Secret Service exited')
                return 'true' in run('gdbus', 'call', '--session', '--dest',
                    'org.freedesktop.DBus', '--object-path', '/org/freedesktop/DBus',
                    '--method', 'org.freedesktop.DBus.NameHasOwner',
                    'org.freedesktop.secrets', env=env)
            wait_for(secret_service_ready, 'Private Secret Service did not start')

            def launch():
                app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log,
                                       start_new_session=True)
                def find():
                    if app.poll() is not None:
                        raise AssertionError('App exited before window appeared')
                    found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--pid',
                        str(app.pid), '--name', '^OpsTerm$'], env=env, capture_output=True,
                        text=True, timeout=5)
                    return found.stdout.splitlines()[0] if found.returncode == 0 else None
                try:
                    return app, wait_for(find, 'No app window', errors=errors)
                except BaseException:
                    stop_process(app)
                    raise

            process, window = launch()
            stop_process(process)
            process = None
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0))
                listener.listen(4)
                data = pathlib.Path(env['XDG_DATA_HOME'])
                with sqlite3.connect(data / 'org.opsterm.app/opsterm.db') as db:
                    db.execute('INSERT INTO credentials(name,username,kind,tags_json,created_at,updated_at) VALUES(?,?,?,?,?,?)',
                        (reference, 'test-user', 'password', '[]', '2026-10-29', '2026-10-29'))
                    db.execute('INSERT INTO sessions(id,name,host,port,protocol,username,credential_ref,settings_override_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)',
                        ('login-fixture', 'Lab Huawei', '127.0.0.1', listener.getsockname()[1],
                         'telnet', '', reference, json.dumps({'pager': 'huawei'}),
                         '2026-10-29', '2026-10-29'))
                run(str(helper), 'set', reference, env=env)
                run(str(helper), 'kernel-delete', reference, env=env)
                run(str(helper), 'durable-get', reference, env=env)
                pager_ready, roundtrip = threading.Event(), threading.Event()
                server = threading.Thread(target=fixture,
                    args=(listener, pager_ready, roundtrip, stop, errors), daemon=True)
                server.start()
                try:
                    process, window = launch()
                    run('xdotool', 'windowfocus', '--sync', window, env=env)
                    time.sleep(.4)
                    run('xdotool', 'mousemove', '--window', window, '45', '208',
                        'click', '--repeat', '2', '--delay', '100', '1', env=env)
                    wait_for(pager_ready.is_set, 'Huawei login/pager ordering did not finish',
                             seconds=18, errors=errors)
                    run('xdotool', 'type', '--clearmodifiers', 'display version', env=env)
                    run('xdotool', 'key', 'Return', env=env)
                    wait_for(roundtrip.is_set, 'Keyboard input required an extra terminal click', errors=errors)
                    time.sleep(.4)
                    run('import', '-window', window, str(output / 'logged-in.png'), env=env)
                    run('xdotool', 'windowsize', window, '760', '500', env=env)
                    time.sleep(.3)
                    run('import', '-window', window, str(output / 'minimum.png'), env=env)
                    if errors:
                        raise AssertionError(errors[0])
                finally:
                    stop.set()
                    server.join(timeout=2)
                    if server.is_alive():
                        raise AssertionError('Fixture thread did not stop')
                if errors:
                    raise AssertionError(errors[0])
            print('GUI saved-login passed: private durable secret, delayed ANSI prompts, credential username fallback, one double-click connection, exact Huawei pager ordering, immediate keyboard roundtrip.')
        except BaseException:
            if window and process and process.poll() is None:
                subprocess.run(['import', '-window', window, str(output / 'failure.png')],
                               env=env, check=False, timeout=5)
            raise
        finally:
            stop.set()
            stop_process(process)
            # All durable records disappear with the private vault. Remove the unique
            # kernel cache too while the isolated Secret Service is still available.
            if daemon and daemon.poll() is None:
                try:
                    run(str(helper), 'delete', reference, env=env)
                finally:
                    stop_process(daemon)
            else:
                stop_process(daemon)


def main():
    authority = pathlib.Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Run through xvfb-run; refusing the real desktop')
    if len(sys.argv) > 1 and sys.argv[1] == '--private-session':
        private_session(pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]), pathlib.Path(sys.argv[4]))
        return
    binary = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'target/release/opsterm-egui').resolve()
    output = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else 'test-results/egui-login').resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opsterm-login-') as temporary:
        folder = pathlib.Path(temporary)
        env = dict(os.environ, LIBGL_ALWAYS_SOFTWARE='1', GDK_BACKEND='x11',
                   XDG_SESSION_TYPE='x11', WINIT_UNIX_BACKEND='x11')
        for name in ('DBUS_SESSION_BUS_ADDRESS', 'DBUS_SESSION_BUS_PID',
                     'DBUS_SESSION_BUS_WINDOWID', 'GNOME_KEYRING_CONTROL',
                     'GNOME_KEYRING_PID', 'SSH_AUTH_SOCK', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET'):
            env.pop(name, None)
        for name, directory in (('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config'),
                                ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')):
            path = folder / directory
            path.mkdir(mode=0o700)
            env[name] = str(path)
        # Keep HOME unchanged. Only descendants receive this private bus and vault.
        wrapper = subprocess.Popen(['dbus-run-session', '--', sys.executable,
            str(pathlib.Path(__file__).resolve()), '--private-session', str(binary),
            str(output), str(folder)], env=env, start_new_session=True)
        try:
            if wrapper.wait(timeout=70) != 0:
                raise RuntimeError('Isolated GUI saved-login regression failed')
        finally:
            stop_process(wrapper)


if __name__ == '__main__':
    main()
