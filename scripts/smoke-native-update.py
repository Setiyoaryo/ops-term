#!/usr/bin/env python3
"""Signed native updates and a live replaced GUI; private Xvfb/fixtures only."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

from package_egui import MANIFEST_PREFIX, raw_public_key, sign, version

spec = importlib.util.spec_from_file_location('measure', 'scripts/measure-egui.py')
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=15).stdout.strip()


def digest(path):
    with path.open('rb') as stream:
        checksum = hashlib.sha256()
        for chunk in iter(lambda: stream.read(65536), b''):
            checksum.update(chunk)
        return checksum.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--key', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=Path('/tmp/opsterm-native-update'))
    args = parser.parse_args()
    authority = Path(os.environ.get('XAUTHORITY', ''))
    if not any(part.startswith('xvfb-run.') for part in authority.parts):
        raise RuntimeError('Use xvfb-run; refusing the real desktop')
    if raw_public_key(args.key) != Path('egui/update-pubkey.txt').read_text().strip():
        raise ValueError('Key must match the unchanged embedded release trust key')
    args.output.mkdir(parents=True, exist_ok=True)
    release_version = version()
    key = f'linux-{os.uname().machine}'
    with tempfile.TemporaryDirectory(prefix='opsterm-native-update-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        original, replacement, unknown = (root/name for name in ('A', 'B', 'C'))
        for path in (original, replacement, unknown):
            shutil.copy2(args.binary, path)
            with path.open('ab') as stream:
                stream.write(b'\nOpsTerm private update fixture ' + path.name.encode())
            path.chmod(0o755)
        manifest = root/'manifest.json'
        def point_to(path):
            url = path.as_uri()
            size = path.stat().st_size
            sha = digest(path)
            canonical = f'{MANIFEST_PREFIX}\n{release_version}\n{key}\n{size}\n{sha}\n{url}'
            asset = dict(url=url, size=size, sha256=sha, signature=sign(args.key, canonical))
            manifest.write_text(json.dumps(dict(version=release_version, assets={key: asset})))
        env = dict(os.environ, OPSTERM_UPDATE_URL=manifest.as_uri(),
                   XDG_DATA_HOME=str(root/'data'), XDG_CONFIG_HOME=str(root/'config'),
                   DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'no-bus'),
                   GDK_BACKEND='x11', XDG_SESSION_TYPE='x11', WINIT_UNIX_BACKEND='x11',
                   LIBGL_ALWAYS_SOFTWARE='1')
        for name in ('APPIMAGE', 'APPDIR', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET'):
            env.pop(name, None)
        config = root/'config/org.opsterm.app/update.json'
        config.parent.mkdir(parents=True)
        config.write_text(json.dumps(dict(auto_check=False)))
        executable = root/'opsterm'
        shutil.copy2(original, executable)
        def cli(flag, expected=0):
            result = subprocess.run([str(executable), flag], env=env, capture_output=True,
                                    text=True, timeout=30)
            assert result.returncode == expected, (result.stdout, result.stderr)
            return result.stdout+result.stderr
        point_to(replacement)
        assert 'available' in cli('--check-update')
        assert 'installed' in cli('--install-update')
        assert digest(executable) == digest(replacement)
        backup = root/'.opsterm.old'
        assert digest(backup) == digest(original)
        assert 'up to date' in cli('--check-update')
        backup_hash = digest(backup)
        inode = executable.stat().st_ino
        assert 'up to date' in cli('--install-update')
        assert executable.stat().st_ino == inode and digest(backup) == backup_hash
        def stop_detached():
            # Restart uses setsid. Match only our private executable paths.
            for entry in Path('/proc').iterdir():
                if not entry.name.isdigit():
                    continue
                try:
                    target = (entry/'exe').readlink()
                    if target.parent != root or not target.name.startswith('opsterm-'):
                        continue
                    pid = int(entry.name)
                    os.kill(pid, signal.SIGTERM)
                    deadline = time.monotonic() + 5
                    while (entry/'exe').exists() and time.monotonic() < deadline:
                        time.sleep(.05)
                    if (entry/'exe').exists() and (entry/'exe').readlink() == target:
                        os.kill(pid, signal.SIGKILL)
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    pass
        processes = []
        try:
            for case in ('known', 'unknown'):
                executable = root/f'opsterm-{case}'
                shutil.copy2(original, executable)
                point_to(original)
                process, window = measure.launch(executable, env)
                processes.append(process)
                run('xdotool', 'windowraise', window)
                run('xdotool', 'windowfocus', '--sync', window)
                time.sleep(1)
                run('import', '-window', window, str(args.output/f'{case}-initial.png'))
                next_file = root/f'next-{case}'
                shutil.copy2(replacement if case == 'known' else unknown, next_file)
                os.replace(next_file, executable)
                assert str(Path(f'/proc/{process.pid}/exe').readlink()).endswith(' (deleted)')
                point_to(replacement)
                before_hash, before_inode = digest(executable), executable.stat().st_ino
                def click(x, y):
                    run('xdotool', 'windowraise', window)
                    run('xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1')
                    time.sleep(.3)
                click(340, 22)  # Help
                click(400, 53)  # Check for updates
                time.sleep(4)
                run('import', '-window', window, str(args.output/f'{case}-checked.png'))
                click(76, 245 if case == 'known' else 270)  # Restart now / Install update
                if case == 'known':
                    # Restart is the actual action here; an erroneous second install
                    # leaves the old process alive and fails this assertion.
                    process.wait(timeout=8)
                    assert process.returncode == 0
                    time.sleep(1)
                    stop_detached()
                else:
                    time.sleep(1)
                    assert process.poll() is None
                    assert root.joinpath('.opsterm-unknown.update.lock').is_file(), 'Install handler did not run'
                    assert digest(executable) == before_hash
                    assert executable.stat().st_ino == before_inode
                    assert not root.joinpath('.opsterm-unknown.old').exists()
                    run('import', '-window', window, str(args.output/'unknown-refused.png'))
                assert digest(executable) == before_hash
                assert executable.stat().st_ino == before_inode
        finally:
            for process in processes:
                if process.poll() is None:
                    measure.stop(process)
            stop_detached()
        print('PASS: signed native update/rollback; repeated install preserves inode/rollback; live deleted executable restarts known payload and refuses unknown replacement')


if __name__ == '__main__':
    main()
