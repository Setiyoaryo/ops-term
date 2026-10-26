#!/usr/bin/env python3
"""Build, sign and package the egui client for auto-update."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_KEY = Path.home() / ".config/opsterm/update-signing.key"
MANIFEST_PREFIX = "opsterm-update-v1"


def run(
    args: list[str],
    *,
    cwd: Path = ROOT,
    capture: bool = False,
    text: bool = True,
) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, cwd=cwd, check=True, text=text, capture_output=capture
    )


def version() -> str:
    with (ROOT / "egui/Cargo.toml").open("rb") as handle:
        return tomllib.load(handle)["package"]["version"]


def validate_release_tag(tag: str, release_version: str) -> None:
    stable = r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    if tag != f"v{release_version}" or not re.fullmatch(stable, tag):
        raise ValueError(f"release tag must match stable egui version v{release_version}")


def raw_public_key(key: Path) -> str:
    der = run(
        ["openssl", "pkey", "-in", str(key), "-pubout", "-outform", "DER"],
        capture=True,
        text=False,
    ).stdout
    return base64.b64encode(der[-32:]).decode()


def sign(key: Path, canonical: str) -> str:
    with tempfile.TemporaryDirectory() as directory:
        message = Path(directory) / "manifest.txt"
        signature = Path(directory) / "manifest.sig"
        public_pem = Path(directory) / "public.pem"
        message.write_text(canonical, encoding="utf-8")
        run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(key), "-in", str(message), "-out", str(signature)])
        run(["openssl", "pkey", "-in", str(key), "-pubout", "-out", str(public_pem)])
        run(
            [
                "openssl",
                "pkeyutl",
                "-verify",
                "-rawin",
                "-pubin",
                "-inkey",
                str(public_pem),
                "-in",
                str(message),
                "-sigfile",
                str(signature),
            ],
            capture=True,
        )
        return base64.b64encode(signature.read_bytes()).decode()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--asset-base-url", default=os.environ.get("OPSTERM_ASSET_BASE_URL", ""))
    parser.add_argument("--local", action="store_true", help="write a manifest that points at release/ on this machine")
    parser.add_argument("--notes", default=os.environ.get("OPSTERM_RELEASE_NOTES", ""))
    parser.add_argument("--key", type=Path, default=Path(os.environ.get("OPSTERM_UPDATE_SIGNING_KEY", DEFAULT_KEY)))
    parser.add_argument("--keep-target", action="store_true")
    parser.add_argument("--tag", help="require a matching stable GitHub release tag")
    args = parser.parse_args()
    release_version = version()
    if args.tag:
        try:
            validate_release_tag(args.tag, release_version)
        except ValueError as error:
            print(f"package: {error}", file=sys.stderr)
            return 2

    if args.local and args.asset_base_url:
        print("package: use either --local or --asset-base-url, not both", file=sys.stderr)
        return 2
    release = ROOT / "release"
    release.mkdir(exist_ok=True)
    if args.local:
        base = release.resolve().as_uri()
    else:
        base = args.asset_base_url.strip().rstrip("/")
        if base.startswith("/"):
            base = Path(base).resolve().as_uri()
        if not base.startswith(("https://", "http://", "file://")):
            print(
                "package: use --local, or pass an http(s)/file URL with --asset-base-url",
                file=sys.stderr,
            )
            return 2
    if not args.key.is_file():
        print(f"package: signing key not found: {args.key}", file=sys.stderr)
        print("package: run scripts/update-keygen.sh first", file=sys.stderr)
        return 2

    public_key = raw_public_key(args.key)
    committed = (ROOT / "egui/update-pubkey.txt").read_text().strip()
    if public_key != committed:
        print("package: signing key does not match egui/update-pubkey.txt", file=sys.stderr)
        print("package: rebuild with the matching public key or run scripts/update-keygen.sh", file=sys.stderr)
        return 2

    if sys.platform != "linux":
        print("package: only Linux packages are supported", file=sys.stderr)
        return 2
    machine = os.uname().machine
    machine = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    asset_key = f"linux-{machine}"
    name = f"opsterm-egui-{release_version}-{asset_key}"

    print(f"building {name} ...")
    try:
        run([sys.executable, "scripts/build_guard.py", "--", "cargo", "build", "-p", "opsterm-egui", "--release", "--locked"])
        binary = ROOT / "target/release/opsterm-egui"
        if not binary.is_file():
            print("package: cargo did not produce target/release/opsterm-egui", file=sys.stderr)
            return 2
        package = release / name
        shutil.copy2(binary, package)
        package.chmod(0o755)
    finally:
        if not args.keep_target:
            subprocess.run(["bash", "scripts/clean.sh"], cwd=ROOT, check=False)

    payload = package.read_bytes()
    digest = hashlib.sha256(payload).hexdigest()
    asset_url = f"{base}/{name}"
    canonical = (
        f"{MANIFEST_PREFIX}\n{release_version}\n{asset_key}\n"
        f"{len(payload)}\n{digest}\n{asset_url}"
    )
    signature = sign(args.key, canonical)
    notes = args.notes or (
        f"Local OpsTerm build {release_version}"
        if args.local
        else f"OpsTerm {release_version}"
    )

    manifest = {
        "version": release_version,
        "notes": notes,
        "assets": {
            asset_key: {
                "url": asset_url,
                "size": len(payload),
                "sha256": digest,
                "signature": signature,
            }
        },
    }
    (release / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (release / f"{name}.sha256").write_text(f"{digest}  {name}\n", encoding="utf-8")
    (release / f"{name}.sig").write_text(signature + "\n", encoding="utf-8")

    print(f"binary:   {package}")
    print(f"sha256:   {digest}")
    print(f"manifest: {release / 'manifest.json'}")
    print(f"asset:    {asset_url}")
    if args.local:
        print(f"local update URL: {(release / 'manifest.json').resolve().as_uri()}")
        print("Set that URL in Help > Check for updates, or in OPSTERM_UPDATE_URL.")
    else:
        print("Upload the binary and manifest.json, then set OPSTERM_UPDATE_URL to the manifest URL.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
