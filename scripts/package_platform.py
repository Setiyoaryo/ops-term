#!/usr/bin/env python3
"""Package an already-built macOS/Windows egui binary for release.

Linux keeps its ELF/AppImage pipeline in `linux_distribution.py`. This module
only assembles portable artifacts: the raw updatable binary plus an archive a
person can download and unpack.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile
import zipfile

from scripts.release_layout import asset_key, asset_name, split_key

ROOT = Path(__file__).resolve().parents[1]

README = """OpsTerm portable build

This archive contains the OpsTerm desktop client. The raw binary is the
updatable artifact; the archive is only for convenience.

The build is not code-signed yet. On macOS, Gatekeeper may ask you to confirm
the first launch (System Settings > Privacy & Security, or
`xattr -dr com.apple.quarantine OpsTerm.app`). On Windows, SmartScreen may warn
about an unknown publisher.
"""

INFO_PLIST = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>opsterm-egui</string>
  <key>CFBundleIdentifier</key><string>org.opsterm.app</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>OpsTerm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"""


def _add_file(archive: zipfile.ZipFile, path: Path, name: str, mode: int = 0o644) -> None:
    info = zipfile.ZipInfo(name)
    info.external_attr = (mode & 0xFFFF) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(info, path.read_bytes())


def _app_archive(binary: Path, version: str, archive_path: Path) -> None:
    with tempfile.TemporaryDirectory(prefix='opsterm-app-') as temporary:
        app = Path(temporary) / 'OpsTerm.app'
        contents = app / 'Contents'
        (contents / 'MacOS').mkdir(parents=True)
        (contents / 'Resources').mkdir(parents=True)
        shutil.copy2(binary, contents / 'MacOS/opsterm-egui')
        (contents / 'MacOS/opsterm-egui').chmod(0o755)
        (contents / 'Info.plist').write_text(INFO_PLIST.format(version=version))
        icon = ROOT / 'src-tauri/icons/icon.png'
        if icon.is_file():
            shutil.copy2(icon, contents / 'Resources/icon.png')
        (contents / 'Resources/README.txt').write_text(README)
        with zipfile.ZipFile(archive_path, 'w', zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(app.rglob('*')):
                relative = path.relative_to(app.parent)
                if path.is_dir():
                    info = zipfile.ZipInfo(str(relative) + '/')
                    info.external_attr = (0o755 << 16) | 0x10
                    archive.writestr(info, b'')
                else:
                    mode = 0o755 if os.access(path, os.X_OK) else 0o644
                    _add_file(archive, path, str(relative), mode)


def _portable_archive(binary: Path, archive_path: Path, executable_name: str) -> None:
    with zipfile.ZipFile(archive_path, 'w', zipfile.ZIP_DEFLATED) as archive:
        _add_file(archive, binary, executable_name, 0o755)
        _add_file(archive, ROOT / 'LICENSE', 'LICENSE')
        info = zipfile.ZipInfo('README.txt')
        info.external_attr = (0o644 << 16)
        archive.writestr(info, README)


def package(
    binary: Path,
    version: str,
    platform: str,
    arch: str,
    output: Path,
) -> dict[str, Path]:
    if platform == 'linux':
        raise ValueError('use linux_distribution.build_distribution for Linux')
    split_key(asset_key(platform, arch))
    if not binary.is_file():
        raise ValueError(f'binary not found: {binary}')
    output.mkdir(parents=True, exist_ok=True)
    raw_key = asset_key(platform, arch)
    raw = output / asset_name(version, raw_key)
    shutil.copy2(binary, raw)
    raw.chmod(0o755)

    packages = {raw_key: raw}
    if platform == 'macos':
        app_key = asset_key(platform, arch, '-app')
        app = output / asset_name(version, app_key)
        _app_archive(binary, version, app)
        packages[app_key] = app
    elif platform == 'windows':
        zip_key = asset_key(platform, arch, '-zip')
        archive = output / asset_name(version, zip_key)
        _portable_archive(binary, archive, 'opsterm-egui.exe')
        packages[zip_key] = archive

    (output / 'platform.json').write_text(
        json.dumps({'version': version, 'platform': platform, 'arch': arch}, indent=2) + '\n'
    )
    return packages


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--platform', required=True, choices=('macos', 'windows'))
    parser.add_argument('--arch', required=True)
    parser.add_argument('--output', type=Path, default=Path('release'))
    args = parser.parse_args()
    packages = package(args.binary, args.version, args.platform, args.arch, args.output)
    for key, path in packages.items():
        print(f'{key}: {path}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
