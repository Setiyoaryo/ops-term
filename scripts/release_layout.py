"""Asset keys, filenames and platform groups for release artifacts."""
from __future__ import annotations

REPOSITORY = 'Setiyoaryo/ops-term'

# Asset-key suffix -> filename suffix. Keys without a suffix point at the
# updatable native binary for that platform.
FORMATS = {
    'linux': {'': '', '-appimage': '.AppImage', '-tar': '.tar.gz'},
    'macos': {'': '', '-app': '-app.zip'},
    'windows': {'': '.exe', '-zip': '.zip'},
}

ARCHITECTURES = {
    'linux': ('x86_64', 'aarch64'),
    'macos': ('x86_64', 'aarch64'),
    'windows': ('x86_64',),
}

_KINDS = ('-appimage', '-app', '-tar', '-zip')


def split_key(key: str) -> tuple[str, str, str]:
    platform = key.split('-', 1)[0]
    if platform not in FORMATS:
        raise ValueError(f'unsupported release platform: {key}')
    kind = next((candidate for candidate in _KINDS if key.endswith(candidate)), '')
    rest = key[len(platform) + 1 :]
    arch = rest[: -len(kind)] if kind else rest
    if kind not in FORMATS[platform]:
        raise ValueError(f'unsupported release format: {key}')
    if arch not in ARCHITECTURES[platform]:
        raise ValueError(f'unsupported release architecture: {key}')
    return platform, arch, kind


def asset_key(platform: str, arch: str, kind: str = '') -> str:
    if platform not in FORMATS or arch not in ARCHITECTURES[platform]:
        raise ValueError(f'unsupported platform/architecture: {platform}/{arch}')
    if kind not in FORMATS[platform]:
        raise ValueError(f'unsupported release format: {kind}')
    return f'{platform}-{arch}{kind}'


def asset_name(version: str, key: str) -> str:
    platform, arch, kind = split_key(key)
    return f'opsterm-egui-{version}-{platform}-{arch}{FORMATS[platform][kind]}'


def asset_url(version: str, key: str) -> str:
    return (
        f'https://github.com/{REPOSITORY}/releases/download/v{version}/'
        f'{asset_name(version, key)}'
    )


def expected_keys(platform: str) -> set[str]:
    if platform not in FORMATS:
        raise ValueError(f'unsupported release platform: {platform}')
    return {
        asset_key(platform, arch, kind)
        for arch in ARCHITECTURES[platform]
        for kind in FORMATS[platform]
    }
