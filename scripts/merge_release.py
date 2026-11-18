"""Verify and combine signed and unsigned platform artifacts before publication."""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile
from scripts.package_egui import MANIFEST_PREFIX, sign, validate_release_tag, version
from scripts.publish_github_release import release_files
from scripts.release_layout import (
    FORMATS,
    asset_key,
    asset_name,
    asset_url,
    expected_keys,
    split_key,
)

PLATFORMS = ('linux', 'macos', 'windows')


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


def validate_native_format(path: Path, platform: str) -> None:
    header = path.read_bytes()[:4]
    if platform == 'windows':
        if not header.startswith(b'MZ'):
            raise ValueError('Windows asset is not a PE executable: ' + path.name)
    elif platform == 'macos':
        if header not in (b'\xcf\xfa\xed\xfe', b'\xce\xfa\xed\xfe', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca'):
            raise ValueError('macOS asset is not a Mach-O executable: ' + path.name)
    else:
        raise ValueError('no native-format check for ' + platform)


def _archive_member(archive: Path, member: str, platform: str) -> bytes:
    with zipfile.ZipFile(archive) as bundle:
        for info in bundle.infolist():
            parts = Path(info.filename).parts
            if info.filename.startswith('/') or '..' in parts or info.is_dir():
                continue
            if info.filename == member:
                return bundle.read(info)
    raise ValueError(f'{platform} archive is missing {member}: ' + archive.name)


def validate_archive_format(archive: Path, platform: str, kind: str, raw: Path) -> None:
    if platform == 'macos' and kind == '-app':
        executable = _archive_member(archive, 'OpsTerm.app/Contents/MacOS/opsterm-egui', platform)
    elif platform == 'windows' and kind == '-zip':
        executable = _archive_member(archive, 'opsterm-egui.exe', platform)
    else:
        raise ValueError('unsupported archive format: ' + archive.name)
    if executable != raw.read_bytes():
        raise ValueError('archive binary differs from the signed native binary: ' + archive.name)


def sign_platform_directory(
    directory: Path,
    platform: str,
    arch: str,
    release_version: str,
    key: Path,
    assets: dict,
    copies: list[Path],
) -> None:
    raw_path = directory / asset_name(release_version, asset_key(platform, arch))
    if raw_path.is_symlink() or not raw_path.is_file():
        raise ValueError('missing native artifact: ' + raw_path.name)
    validate_native_format(raw_path, platform)
    for kind in FORMATS[platform]:
        key_name = asset_key(platform, arch, kind)
        if key_name in assets:
            raise ValueError('duplicate architecture/format: ' + key_name)
        path = directory / asset_name(release_version, key_name)
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError('artifact must be a regular file: ' + path.name)
        if kind:
            validate_archive_format(path, platform, kind, raw_path)
        payload = path.read_bytes()
        digest = hashlib.sha256(payload).hexdigest()
        url = asset_url(release_version, key_name)
        canonical = f'{MANIFEST_PREFIX}\n{release_version}\n{key_name}\n{len(payload)}\n{digest}\n{url}'
        signature = sign(key, canonical)
        assets[key_name] = {'url': url, 'size': len(payload), 'sha256': digest, 'signature': signature}
        sidecars = []
        for suffix, content in (('.sha256', f'{digest}  {path.name}\n'), ('.sig', signature + '\n')):
            sidecar = directory / (path.name + suffix)
            sidecar.write_text(content)
            sidecars.append(sidecar)
        copies.extend([path, *sidecars])


def merge(
    inputs: list[Path],
    output: Path,
    release_version: str,
    public_key: str,
    key: Path | None = None,
) -> None:
    validate_release_tag('v' + release_version, release_version)
    assets = {}
    copies = []
    present = set()
    for directory in inputs:
        manifest_path = directory / 'manifest.json'
        platform_path = directory / 'platform.json'
        if manifest_path.is_file():
            manifest = json.loads(manifest_path.read_text())
            files = release_files(directory, release_version)
            for asset_key_name, asset in manifest['assets'].items():
                platform, _arch, _kind = split_key(asset_key_name)
                present.add(platform)
                if asset_key_name in assets:
                    raise ValueError('unexpected or duplicate architecture/format: ' + asset_key_name)
                name = asset_name(release_version, asset_key_name)
                if asset['url'] != asset_url(release_version, asset_key_name):
                    raise ValueError('unexpected official asset URL')
                verify_asset(directory / name, release_version, asset_key_name, asset, public_key)
                if (directory / (name + '.sig')).read_text().strip() != asset['signature']:
                    raise ValueError('signature sidecar mismatch')
                if (directory / (name + '.sha256')).read_text().strip() != f"{asset['sha256']}  {name}":
                    raise ValueError('checksum sidecar mismatch')
                assets[asset_key_name] = asset
            copies.extend(path for path in files if path.name != 'manifest.json')
        elif platform_path.is_file():
            if key is None:
                raise ValueError('a signing key is required for ' + directory.name)
            metadata = json.loads(platform_path.read_text())
            platform, arch = metadata['platform'], metadata['arch']
            if metadata.get('version') != release_version:
                raise ValueError('platform metadata version mismatch: ' + directory.name)
            split_key(asset_key(platform, arch))
            present.add(platform)
            sign_platform_directory(directory, platform, arch, release_version, key, assets, copies)
        else:
            raise ValueError('artifact directory has no manifest: ' + str(directory))
    for platform in present:
        missing = expected_keys(platform) - set(assets)
        if missing:
            raise ValueError('release requires every format for ' + platform + ': ' + ', '.join(sorted(missing)))
    if not present:
        raise ValueError('release has no platform assets')
    output.mkdir(parents=True, exist_ok=True)
    for path in copies:
        shutil.copy2(path, output / path.name)
    (output / 'manifest.json').write_text(json.dumps({'version':release_version,
        'notes':f'OpsTerm {release_version}', 'assets':assets}, indent=2) + '\n')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, default=Path('release'))
    parser.add_argument('--key', type=Path)
    args = parser.parse_args()
    merge(args.input, args.output, version(), Path('egui/update-pubkey.txt').read_text(), args.key)


if __name__ == '__main__':
    main()
