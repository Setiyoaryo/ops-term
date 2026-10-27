#!/usr/bin/env python3
"""Prove native and no-FUSE AppImage CLI startup in an unmodified distro image."""
import os
from pathlib import Path
import subprocess
import sys

ALLOWED = {'ubuntu:22.04','ubuntu:24.04','debian:12','debian:13','fedora:latest','archlinux:base','cachyos/cachyos:latest'}


def main():
    image, folder = sys.argv[1:]
    if image not in ALLOWED:
        raise ValueError('Use an approved official distro image')
    folder = Path(folder).resolve()
    binary = next(path for path in folder.glob('opsterm-egui-*-linux-x86_64') if path.is_file())
    appimage = next(folder.glob('*.AppImage'))
    for path in (binary, appimage):
        path.chmod(0o755)  # Actions artifact downloads intentionally reset executable bits.
    subprocess.run(['docker','pull',image],check=True)
    # No host HOME/config/display/credentials are mounted; the actual app cannot use network.
    command = ['docker','run','--rm','--network','none','--cap-drop=ALL','--security-opt=no-new-privileges',
        '--read-only','--tmpfs','/tmp:rw,exec,size=128m','--memory=256m','--cpus=1',
        '--mount',f'type=bind,src={folder},dst=/opt/opsterm,readonly', image]
    subprocess.run([*command, '/opt/opsterm/' + binary.name, '--version'],check=True)
    subprocess.run([*command, '/opt/opsterm/' + appimage.name, '--appimage-extract-and-run', '--version'],check=True)
    print('PASS native + AppImage CLI:', image)


if __name__ == '__main__':
    main()
