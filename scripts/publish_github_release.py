"""Publish verified draft assets without replacing a published release.

Run as `python3 -m scripts.publish_github_release --repo owner/repo --tag vX.Y.Z`.
GitHub CLI reads its existing credential store; this script accepts no raw token.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

from scripts.package_egui import validate_release_tag, version


def run_gh(args: list[str]) -> str:
    result = subprocess.run(["gh", *args], text=True, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "GitHub CLI request failed")
    return result.stdout


def get_release(repository: str, tag: str | None = None) -> dict | None:
    suffix = f"tags/{tag}" if tag else "latest"
    try:
        return json.loads(run_gh(["api", f"repos/{repository}/releases/{suffix}"]))
    except RuntimeError as error:
        if "(HTTP 404)" in str(error):
            return None
        raise


def should_mark_latest(tag: str, latest_tag: str | None) -> bool:
    def stable_version(value: str) -> tuple[int, ...]:
        validate_release_tag(value, value.removeprefix("v"))
        return tuple(int(part) for part in value[1:].split("."))

    candidate = stable_version(tag)
    return latest_tag is None or candidate >= stable_version(latest_tag)


def publish(repository: str, tag: str, directory: Path) -> None:
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9][A-Za-z0-9_.-]*", repository):
        raise ValueError("repository must use owner/name format")
    release_version = version()
    validate_release_tag(tag, release_version)
    binary = f"opsterm-egui-{release_version}-linux-x86_64"
    files = [directory / name for name in [binary, f"{binary}.sha256", f"{binary}.sig", "manifest.json"]]
    if any(path.is_symlink() or not path.is_file() or path.stat().st_size == 0 for path in files):
        raise RuntimeError("all four nonempty, regular release assets are required")

    existing = get_release(repository, tag)
    if existing is not None and existing.get("draft") is not True:
        raise RuntimeError("this release is already published; bump the version instead of replacing its assets")
    if existing is None:
        notes = Path("docs/releases") / f"{tag}.md"
        note_args = ["--notes-file", str(notes)] if notes.is_file() else ["--generate-notes"]
        run_gh(["release", "create", tag, "--repo", repository, "--verify-tag", "--draft", "--title", f"OpsTerm {tag}", *note_args])
    else:
        print(f"Resuming draft {tag}", flush=True)

    run_gh(["release", "upload", tag, *map(str, files), "--repo", repository, "--clobber"])
    with tempfile.TemporaryDirectory(prefix="opsterm-upload-verify-") as temporary:
        args = ["release", "download", tag, "--repo", repository, "--dir", temporary]
        for path in files:
            args.extend(["--pattern", path.name])
        run_gh(args)
        for path in files:
            downloaded = Path(temporary) / path.name
            if not downloaded.is_file() or hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(downloaded.read_bytes()).digest():
                raise RuntimeError(f"uploaded asset verification failed: {path.name}")

    latest = get_release(repository)
    make_latest = should_mark_latest(tag, latest["tag_name"] if latest else None)
    run_gh(["release", "edit", tag, "--repo", repository, "--draft=false", f"--latest={str(make_latest).lower()}"])
    print(f"Published https://github.com/{repository}/releases/tag/{tag} (latest={make_latest})", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--directory", type=Path, default=Path("release"))
    args = parser.parse_args()
    try:
        publish(args.repo, args.tag, args.directory)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"release: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
