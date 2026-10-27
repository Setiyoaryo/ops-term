from pathlib import Path
import json
import tempfile
import unittest
from scripts import publish_github_release as publisher

class AssetTests(unittest.TestCase):
    def test_release_uses_all_manifest_assets_and_sidecars(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            names=['opsterm-egui-0.1.5-linux-x86_64', 'opsterm-egui-0.1.5-linux-x86_64.AppImage', 'opsterm-egui-0.1.5-linux-aarch64.tar.gz']
            manifest={'version':'0.1.5','assets':{str(i):{'url':'https://example.test/'+name} for i,name in enumerate(names)}}
            (root/'manifest.json').write_text(json.dumps(manifest))
            for name in names:
                for suffix in ('','.sha256','.sig'):
                    (root/(name+suffix)).write_text('fixture')
            self.assertEqual({p.name for p in publisher.release_files(root,'0.1.5')}, {'manifest.json'}|{n+s for n in names for s in ('','.sha256','.sig')})

    def test_rejects_manifest_path_traversal(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            (root/'manifest.json').write_text(json.dumps({'version':'0.1.5','assets':{'linux-x86_64':{'url':'https://example.test/../../private'}}}))
            with self.assertRaises(ValueError): publisher.release_files(root,'0.1.5')
