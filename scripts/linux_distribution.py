#!/usr/bin/env python3
"""Package an already-built egui ELF; do not build Rust or bundle host drivers."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SUPPORT = ROOT / 'packaging/linux'
ARCHITECTURES = {'x86_64': 'Advanced Micro Devices X86-64', 'aarch64': 'AArch64'}
APPIMAGETOOL_SHA256 = {
    'x86_64': 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0',
    'aarch64': 'f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158',
}
RUNTIME_SHA256 = {
    'x86_64': '2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d',
    'aarch64': '00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444',
}


def validate_elf_header(header: str, arch: str) -> None:
    if arch not in ARCHITECTURES:
        raise ValueError(f'Unsupported Linux architecture: {arch}')
    machine = re.search(r'^\s*Machine:\s*(.+)$', header, re.MULTILINE)
    if not re.search(r'Class:\s*ELF64\b', header) or not machine or machine[1].strip() != ARCHITECTURES[arch]:
        raise ValueError(f'Expected a 64-bit {arch} ELF executable')


def validate_elf_metadata(version_info: str, dynamic: str, arch: str, max_glibc: str = '2.35') -> None:
    """Validate readelf output without executing the untrusted ELF (never use ldd)."""
    if arch not in ARCHITECTURES:
        raise ValueError(f'Unsupported Linux architecture: {arch}')
    baseline = tuple(int(part) for part in max_glibc.split('.'))
    for version in set(re.findall(r'\bGLIBC_([0-9]+(?:\.[0-9]+)+)\b', version_info)):
        if tuple(int(part) for part in version.split('.')) > baseline:
            raise ValueError(f'ELF requires GLIBC_{version}; rebuild on glibc <= {max_glibc}')
    # glibc 2.36 introduced DT_RELR under an ABI name rather than GLIBC_2.36.
    for abi in set(re.findall(r'\bGLIBC_(?:ABI_[A-Za-z0-9_]+|PRIVATE)\b', version_info)):
        raise ValueError(f'ELF requires unsupported {abi}; rebuild on glibc <= {max_glibc}')
    allowed = {'libc.so.6', 'libm.so.6', 'libgcc_s.so.1'}
    allowed.add('ld-linux-x86-64.so.2' if arch == 'x86_64' else 'ld-linux-aarch64.so.1')
    needed = set(re.findall(r'\(NEEDED\).*?\[([^\]]+)\]', dynamic))
    unexpected = needed - allowed
    if unexpected:
        raise ValueError('ELF depends on nonportable shared libraries: ' + ', '.join(sorted(unexpected)))
    if re.search(r'\((?:RPATH|RUNPATH)\)', dynamic):
        raise ValueError('ELF contains RPATH/RUNPATH; rebuild without host library search paths')


def validate_elf(binary: Path, arch: str, max_glibc: str = '2.35') -> None:
    binary = binary.resolve(strict=True)
    def readelf(option: str) -> str:
        return subprocess.run(['readelf', option, str(binary)], check=True, capture_output=True, text=True, env={**os.environ, 'LC_ALL': 'C'}).stdout
    validate_elf_header(readelf('-h'), arch)
    validate_elf_metadata(readelf('--version-info'), readelf('-d'), arch, max_glibc)


def download_verified(url: str, expected_sha256: str, destination: Path) -> None:
    """Download into temporary storage and publish only after digest verification."""
    digest = hashlib.sha256()
    temporary = destination.with_name(destination.name + '.download')
    try:
        with urllib.request.urlopen(url, timeout=120) as response, temporary.open('wb') as output:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
                output.write(chunk)
        if digest.hexdigest() != expected_sha256:
            raise ValueError(f'SHA-256 mismatch for {url}')
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def build_appimage(appdir: Path, target: Path, arch: str) -> None:
    """Pinned tool/runtime only, extracted without FUSE and removed on return."""
    if arch not in ARCHITECTURES:
        raise ValueError(f'Unsupported Linux architecture: {arch}')
    with tempfile.TemporaryDirectory(prefix='opsterm-appimage-') as temporary:
        work = Path(temporary)
        tool = work / 'appimagetool.AppImage'
        runtime = work / 'runtime'
        download_verified(f'https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-{arch}.AppImage', APPIMAGETOOL_SHA256[arch], tool)
        download_verified(f'https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-{arch}', RUNTIME_SHA256[arch], runtime)
        tool.chmod(0o755)
        subprocess.run([str(tool), '--appimage-extract'], cwd=work, check=True, stdout=subprocess.DEVNULL)
        subprocess.run([str(work / 'squashfs-root/AppRun'), '--runtime-file', str(runtime), '--no-appstream', str(appdir.resolve()), str(target.resolve())], check=True, env={**os.environ, 'ARCH': arch})
        target.chmod(0o755)


def build_distribution(binary: Path, version: str, arch: str, output: Path) -> list[Path]:
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._+-]*', version):
        raise ValueError('Version must be a safe filename component')
    validate_elf(binary, arch)
    output.mkdir(parents=True, exist_ok=True)
    basename = f'opsterm-egui-{version}-linux-{arch}'
    archive = output / (basename + '.tar.gz')
    image = output / (basename + '.AppImage')
    with tempfile.TemporaryDirectory(prefix='opsterm-package-') as temporary:
        work = Path(temporary)
        package = work / basename
        package.mkdir()
        shutil.copy2(binary, package / 'opsterm-egui')
        (package / 'opsterm-egui').chmod(0o755)
        for name in ('install.sh', 'diagnose.sh', 'org.opsterm.app.desktop', 'README.txt', 'THIRD_PARTY_NOTICES.txt', 'dbus-COPYING'):
            shutil.copy2(SUPPORT / name, package / name)
        shutil.copy2(ROOT / 'LICENSE', package / 'LICENSE')
        shutil.copytree(ROOT / 'src-tauri/icons', package / 'icons')
        for name in ('install.sh', 'diagnose.sh'):
            (package / name).chmod(0o755)
        appdir = work / 'OpsTerm.AppDir'
        (appdir / 'usr/bin').mkdir(parents=True)
        shutil.copy2(package / 'opsterm-egui', appdir / 'usr/bin/opsterm-egui')
        shutil.copy2(SUPPORT / 'AppRun', appdir / 'AppRun')
        (appdir / 'AppRun').chmod(0o755)
        shutil.copy2(SUPPORT / 'org.opsterm.app.desktop', appdir / 'org.opsterm.app.desktop')
        shutil.copy2(ROOT / 'src-tauri/icons/icon.png', appdir / 'org.opsterm.app.png')
        shutil.copy2(ROOT / 'src-tauri/icons/icon.png', appdir / '.DirIcon')
        shutil.copytree(package / 'icons', appdir / 'usr/share/opsterm/icons')
        (appdir / 'usr/share/licenses/opsterm').mkdir(parents=True)
        for name in ('LICENSE', 'THIRD_PARTY_NOTICES.txt', 'dbus-COPYING'):
            shutil.copy2(package / name, appdir / 'usr/share/licenses/opsterm' / name)
        # Stage both artifacts first, so a tooling failure cannot publish a lone tarball.
        staged_image = work / image.name
        build_appimage(appdir, staged_image, arch)
        staged_archive = work / archive.name
        with tarfile.open(staged_archive, 'w:gz') as tar:
            tar.add(package, arcname=basename)
        shutil.copy2(staged_archive, archive)
        shutil.copy2(staged_image, image)
    return [archive, image]
