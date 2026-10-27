#!/usr/bin/env python3
"""Exercise signed outer-image update/rollback with private temporary local manifests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from linux_distribution import build_appimage
from package_egui import MANIFEST_PREFIX, raw_public_key, sign, version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--key', type=Path, required=True)
    args = parser.parse_args()
    if raw_public_key(args.key) != Path('egui/update-pubkey.txt').read_text().strip():
        raise ValueError('Test key must match the unchanged embedded release trust key')
    arch = os.uname().machine
    release_version = version()
    with tempfile.TemporaryDirectory(prefix='opsterm-image-update-') as temporary:
        root = Path(temporary)
        appdir = root / 'OpsTerm.AppDir'
        (appdir/'usr/bin').mkdir(parents=True)
        shutil.copy2(args.binary, appdir/'usr/bin/opsterm-egui')
        shutil.copy2('packaging/linux/AppRun', appdir/'AppRun')
        (appdir/'AppRun').chmod(0o755)
        shutil.copy2('packaging/linux/org.opsterm.app.desktop',appdir/'org.opsterm.app.desktop')
        shutil.copy2('src-tauri/icons/icon.png',appdir/'org.opsterm.app.png')
        image = root/'A.AppImage'
        replacement = root/'B.AppImage'
        (appdir/'test-marker').write_text('original fixture')
        build_appimage(appdir,image,arch)
        (appdir/'test-marker').write_text('replacement fixture')
        build_appimage(appdir,replacement,arch)
        old = hashlib.sha256(image.read_bytes()).hexdigest()
        new = hashlib.sha256(replacement.read_bytes()).hexdigest()
        assert old != new
        key = f'linux-{arch}-appimage'
        manifest = root/'manifest.json'
        def point_to(path):
            payload=path.read_bytes()
            digest=hashlib.sha256(payload).hexdigest()
            url=path.resolve().as_uri()
            canonical=f'{MANIFEST_PREFIX}\n{release_version}\n{key}\n{len(payload)}\n{digest}\n{url}'
            asset={'url':url,'size':len(payload),'sha256':digest,'signature':sign(args.key,canonical)}
            manifest.write_text(json.dumps({'version':release_version,'assets':{key:asset}}))
        runtime_tmp=root/'runtime';runtime_tmp.mkdir()
        env=dict(os.environ, OPSTERM_UPDATE_URL=manifest.as_uri(), XDG_CONFIG_HOME=str(root/'config'), TMPDIR=str(runtime_tmp))
        env.pop('APPIMAGE',None)
        env.pop('APPDIR',None)
        def cli(flag, expected):
            result=subprocess.run([str(image),'--appimage-extract-and-run',flag],env=env,
                text=True,capture_output=True,timeout=45)
            assert result.returncode==expected,(flag,result.returncode,result.stdout,result.stderr)
            return result.stdout+result.stderr
        # Even a valid signature must not let a native payload replace the image.
        point_to(args.binary)
        assert 'format' in cli('--install-update',2)
        assert hashlib.sha256(image.read_bytes()).hexdigest()==old
        point_to(replacement)
        assert 'available' in cli('--check-update',0)
        assert 'installed' in cli('--install-update',0)
        assert hashlib.sha256(image.read_bytes()).hexdigest()==new
        assert hashlib.sha256((root/'.A.AppImage.old').read_bytes()).hexdigest()==old
        assert 'up to date' in cli('--check-update',0)
        print('PASS: real no-FUSE AppImage environment, signed outer update, rollback, format refusal, outer hash check')


if __name__=='__main__':
    main()
