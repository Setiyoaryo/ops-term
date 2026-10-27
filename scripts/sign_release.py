"""Sign already-tested portable artifacts; no build or publishing credentials."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path
import tarfile
from scripts.linux_distribution import validate_elf
from scripts.package_egui import MANIFEST_PREFIX, raw_public_key, sign, validate_release_tag, version


def sign_artifacts(directory: Path, release_version: str, arch: str, key: Path) -> None:
    if arch not in ('x86_64', 'aarch64'):
        raise ValueError('unsupported release architecture')
    if raw_public_key(key) != Path('egui/update-pubkey.txt').read_text().strip():
        raise ValueError('signing key does not match embedded update key')
    base = f'opsterm-egui-{release_version}-linux-{arch}'
    raw = directory / base
    validate_elf(raw, arch)
    native_version = subprocess.run([str(raw.resolve()), '--version'], check=True, text=True, capture_output=True).stdout.strip()
    if native_version != f'OpsTerm v{release_version}':
        raise ValueError('native binary version differs from release metadata')
    raw_bytes = raw.read_bytes()
    image = directory / (base + '.AppImage')
    header = image.read_bytes()[:11]
    if not header.startswith(b'\x7fELF') or header[8:11] != b'AI\x02':
        raise ValueError('not a type-2 AppImage')
    archive = directory / (base + '.tar.gz')
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            parts = Path(member.name).parts
            if member.name.startswith('/') or '..' in parts or not parts or parts[0] != base or member.issym() or member.islnk():
                raise ValueError('unsafe portable archive member')
        binary = tar.extractfile(base + '/opsterm-egui')
        if binary is None or binary.read() != raw_bytes:
            raise ValueError('archive binary differs from verified native binary')
    assets = {}
    for suffix, kind in (('', ''), ('.AppImage', '-appimage'), ('.tar.gz', '-tar')):
        path = directory / (base + suffix)
        if path.is_symlink() or not path.is_file():
            raise ValueError('artifact must be a regular file')
        payload = path.read_bytes()
        digest = hashlib.sha256(payload).hexdigest()
        asset_key = f'linux-{arch}{kind}'
        url = f'https://github.com/Setiyoaryo/ops-term/releases/download/v{release_version}/{path.name}'
        canonical = f'{MANIFEST_PREFIX}\n{release_version}\n{asset_key}\n{len(payload)}\n{digest}\n{url}'
        signature = sign(key, canonical)
        assets[asset_key] = {'url':url, 'size':len(payload), 'sha256':digest, 'signature':signature}
        (directory / (path.name + '.sha256')).write_text(f'{digest}  {path.name}\n')
        (directory / (path.name + '.sig')).write_text(signature + '\n')
    (directory / 'manifest.json').write_text(json.dumps({'version':release_version,
        'notes':f'OpsTerm {release_version}', 'assets':assets}, indent=2) + '\n')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--key', type=Path, required=True)
    parser.add_argument('--directory', type=Path, default=Path('release'))
    args = parser.parse_args()
    release_version = version()
    validate_release_tag(args.tag, release_version)
    sign_artifacts(args.directory, release_version, os.uname().machine, args.key)


if __name__ == '__main__':
    main()
