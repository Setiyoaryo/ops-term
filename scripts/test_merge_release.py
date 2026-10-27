import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from scripts import merge_release

class MergeTests(unittest.TestCase):
    def test_tampered_signed_asset_is_rejected_before_signature_check(self):
        with tempfile.TemporaryDirectory() as folder:
            artifact=Path(folder)/'opsterm-egui-0.1.5-linux-x86_64'
            artifact.write_bytes(b'changed')
            asset={'size':8,'sha256':hashlib.sha256(b'original').hexdigest(),'signature':'invalid','url':'https://example.test/'+artifact.name}
            with self.assertRaisesRegex(ValueError,'size|hash'):
                merge_release.verify_asset(artifact,'0.1.5','linux-x86_64',asset,'invalid')

    def test_valid_multi_arch_signatures_merge_and_cross_key_fails(self):
        import subprocess
        from scripts.package_egui import sign, raw_public_key, MANIFEST_PREFIX
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder); private=root/'key.pem'
            subprocess.run(['openssl','genpkey','-algorithm','Ed25519','-out',str(private)],check=True,capture_output=True)
            public=raw_public_key(private)
            inputs=[]
            for arch in ('x86_64','aarch64'):
                directory=root/arch;directory.mkdir();inputs.append(directory);assets={}
                for suffix, kind in (('',''),('.AppImage','-appimage'),('.tar.gz','-tar')):
                    name=f'opsterm-egui-0.1.6-linux-{arch}{suffix}'
                    payload=name.encode();path=directory/name;path.write_bytes(payload)
                    digest=hashlib.sha256(payload).hexdigest();key=f'linux-{arch}{kind}'
                    url=f'https://github.com/Setiyoaryo/ops-term/releases/download/v0.1.6/{name}'
                    canonical=f'{MANIFEST_PREFIX}\n0.1.6\n{key}\n{len(payload)}\n{digest}\n{url}'
                    signature=sign(private,canonical)
                    asset={'url':url,'size':len(payload),'sha256':digest,'signature':signature};assets[key]=asset
                    (directory/(name+'.sha256')).write_text(f'{digest}  {name}\n')
                    (directory/(name+'.sig')).write_text(signature+'\n')
                    if not suffix:
                        with self.assertRaisesRegex(ValueError,'signature'):
                            merge_release.verify_asset(path,'0.1.6',key+'-appimage',asset,public)
                (directory/'manifest.json').write_text(json.dumps({'version':'0.1.6','assets':assets}))
            output=root/'out';merge_release.merge(inputs,output,'0.1.6',public)
            self.assertEqual(len(json.loads((output/'manifest.json').read_text())['assets']),6)
            self.assertEqual(len(list(output.iterdir())),19)

    def test_manifest_requires_complete_architecture_and_format_set(self):
        with tempfile.TemporaryDirectory() as folder:
            source=Path(folder)/'input';source.mkdir()
            (source/'manifest.json').write_text(json.dumps({'version':'0.1.5','assets':{}}))
            with self.assertRaises(ValueError):
                merge_release.merge([source],Path(folder)/'out','0.1.5','invalid')
