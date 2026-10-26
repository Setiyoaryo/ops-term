"""Build the standalone OpsTerm landing page for GitHub Pages."""
from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import re
import shutil
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "website"
OUTPUT = ROOT / "dist/pages"
REPOSITORY = "Setiyoaryo/ops-term"
RELEASES = f"https://github.com/{REPOSITORY}/releases"


def normalize_release(payload: dict) -> dict[str, str]:
    tag = payload.get("tag_name", "")
    if not re.fullmatch(r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", tag):
        raise ValueError("landing page requires a stable release tag")
    if payload.get("draft") or payload.get("prerelease"):
        raise ValueError("draft/prerelease downloads cannot be advertised as stable")
    version = tag[1:]
    name = f"opsterm-egui-{version}-linux-x86_64"
    expected_url = f"{RELEASES}/download/{tag}/{name}"
    asset = next((item for item in payload.get("assets", []) if item.get("name") == name), None)
    if asset is None or asset.get("browser_download_url") != expected_url:
        raise ValueError("download must be the expected official Linux release asset")
    return {
        "VERSION": version, "TAG": tag, "ASSET_NAME": name,
        "DOWNLOAD_URL": expected_url, "RELEASE_URL": f"{RELEASES}/tag/{tag}",
        "UPDATE_URL": f"{RELEASES}/latest/download/manifest.json",
    }


def render_template(template: str, values: dict[str, str]) -> str:
    for key, value in values.items():
        template = template.replace("{{" + key + "}}", html.escape(value, quote=True))
    if re.search(r"\{\{[A-Z_]+\}\}", template):
        raise ValueError("unresolved landing page metadata")
    return template


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live-release", action="store_true")
    args = parser.parse_args()
    payload = json.loads((SOURCE / "release.json").read_text())
    if args.live_release:
        request = urllib.request.Request(
            f"https://api.github.com/repos/{REPOSITORY}/releases/latest",
            headers={"Accept": "application/vnd.github+json", "User-Agent": "OpsTerm-Pages"},
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = json.load(response)
        except (urllib.error.URLError, TimeoutError) as error:
            print(f"Live release unavailable; using the checked-in release snapshot: {type(error).__name__}")
    metadata = normalize_release(payload)
    for path in SOURCE.rglob("*"):
        if path.is_symlink():
            raise ValueError("symlinks are not permitted in public site source")
    if OUTPUT.exists():
        shutil.rmtree(OUTPUT)
    OUTPUT.mkdir(parents=True)
    for name in ["index.html", "styles.css", "script.js"]:
        text = (SOURCE / name).read_text()
        (OUTPUT / name).write_text(render_template(text, metadata))
    shutil.copytree(SOURCE / "assets", OUTPUT / "assets")
    (OUTPUT / ".nojekyll").write_text("")
    print(f"Built {metadata['TAG']} landing page → {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
