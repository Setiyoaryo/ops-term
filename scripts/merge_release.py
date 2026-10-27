"""Verify and combine signed architecture artifacts before public publication."""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from scripts.package_egui import MANIFEST_PREFIX, validate_release_tag, version
from scripts.publish_github_release import release_files

ARCHITECTURES = ('x86_64', 'aarch64')
FORMATS = ('', '-appimage', '-tar')


def verify_asset(path: Path, release_version: str, key: str, asset: dict, public_key: str) -> None:
    payload = path.read_bytes()
    if len(payload) != asset['size']:
        raise ValueError('asset size mismatch: ' + path.name)
    if hashlib.sha256(payload).hexdigest() != asset['sha256']:
        raise ValueError('asset hash mismatch: ' + path.name)
    raw_key = base64.b64decode(public_key.strip(), validate=True)
    if len(raw_key) != 32:
        raise ValueError('expected a 32-byte Ed25519 public key')
    canonical = f"{MANIFEST_PREFIX}\n{release_version}\n{key}\n{asset['size']}\n{asset['sha256']}\n{asset['url']}"
    with tempfile.TemporaryDirectory(prefix='opsterm-verify-') as temporary:
        root = Path(temporary)
        (root / 'public.der').write_bytes(bytes.fromhex('302a300506032b6570032100') + raw_key)
        (root / 'message').write_text(canonical)
        (root / 'signature').write_bytes(base64.b64decode(asset['signature'], validate=True))
        result = subprocess.run(['openssl', 'pkeyutl', '-verify', '-rawin', '-pubin', '-keyform', 'DER',
            '-inkey', str(root / 'public.der'), '-in', str(root / 'message'), '-sigfile', str(root / 'signature')],
            capture_output=True, check=False)
        if result.returncode:
            raise ValueError('invalid asset signature: ' + path.name)


def merge(inputs: list[Path], output: Path, release_version: str, public_key: str) -> None:
    validate_release_tag('v' + release_version, release_version)
    expected = {f'linux-{arch}{kind}' for arch in ARCHITECTURES for kind in FORMATS}
    assets = {}
    copies = []
    for directory in inputs:
        manifest = json.loads((directory / 'manifest.json').read_text())
        files = release_files(directory, release_version)
        for key, asset in manifest['assets'].items():
            if key not in expected or key in assets:
                raise ValueError('unexpected or duplicate architecture/format: ' + key)
            suffix = '.AppImage' if key.endswith('-appimage') else '.tar.gz' if key.endswith('-tar') else ''
            arch = key.removeprefix('linux-').removesuffix('-appimage').removesuffix('-tar')
            name = f'opsterm-egui-{release_version}-linux-{arch}{suffix}'
            url = f'https://github.com/Setiyoaryo/ops-term/releases/download/v{release_version}/{name}'
            if asset['url'] != url:
                raise ValueError('unexpected official asset URL')
            verify_asset(directory / name, release_version, key, asset, public_key)
            if (directory / (name + '.sig')).read_text().strip() != asset['signature']:
                raise ValueError('signature sidecar mismatch')
            if (directory / (name + '.sha256')).read_text().strip() != f"{asset['sha256']}  {name}":
                raise ValueError('checksum sidecar mismatch')
            assets[key] = asset
        copies.extend(path for path in files if path.name != 'manifest.json')
    if set(assets) != expected:
        raise ValueError('release requires native, AppImage and archive assets for both architectures')
    output.mkdir(parents=True, exist_ok=True)
    for path in copies:
        shutil.copy2(path, output / path.name)
    (output / 'manifest.json').write_text(json.dumps({'version':release_version,
        'notes':f'OpsTerm {release_version}', 'assets':assets}, indent=2) + '\n')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, default=Path('release'))
    args = parser.parse_args()
    merge(args.input, args.output, version(), Path('egui/update-pubkey.txt').read_text())


if __name__ == '__main__':
    main()
