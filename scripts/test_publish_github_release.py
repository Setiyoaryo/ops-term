from contextlib import redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts import publish_github_release as publisher


class PublishReleaseTests(unittest.TestCase):
    def test_latest_never_moves_backwards(self):
        self.assertTrue(publisher.should_mark_latest("v0.1.2", "v0.1.1"))
        self.assertFalse(publisher.should_mark_latest("v0.1.1", "v0.1.2"))
        self.assertTrue(publisher.should_mark_latest("v0.1.1", None))
        with self.assertRaises(ValueError):
            publisher.should_mark_latest("v0.1.1", "unrecognized-tag")

    def fixtures(self, directory):
        files = []
        for name in ["opsterm-egui-0.1.1-linux-x86_64", "opsterm-egui-0.1.1-linux-x86_64.sha256", "opsterm-egui-0.1.1-linux-x86_64.sig", "manifest.json"]:
            path = Path(directory) / name
            path.write_bytes(name.encode())
            files.append(path)
        return files

    def test_draft_is_resumed_and_older_release_does_not_replace_latest(self):
        with tempfile.TemporaryDirectory() as directory:
            files = self.fixtures(directory)
            calls = []
            def fake_gh(args):
                calls.append(args)
                if args[:2] == ["release", "download"]:
                    output = Path(args[args.index("--dir") + 1])
                    for source in files:
                        (output / source.name).write_bytes(source.read_bytes())
                return ""
            with patch.object(publisher, "version", return_value="0.1.1"), patch.object(publisher, "get_release", side_effect=[{"draft": True}, {"tag_name": "v0.1.2"}]), patch.object(publisher, "run_gh", side_effect=fake_gh):
                with redirect_stdout(io.StringIO()):
                    publisher.publish("example/app", "v0.1.1", Path(directory))
            self.assertFalse(any(args[:2] == ["release", "create"] for args in calls))
            self.assertIn("--clobber", next(args for args in calls if args[:2] == ["release", "upload"]))
            self.assertIn("--latest=false", next(args for args in calls if args[:2] == ["release", "edit"]))

    def test_published_assets_are_immutable(self):
        with tempfile.TemporaryDirectory() as directory:
            self.fixtures(directory)
            with patch.object(publisher, "version", return_value="0.1.1"), patch.object(publisher, "get_release", return_value={"draft": False}), patch.object(publisher, "run_gh") as gh:
                with self.assertRaises(RuntimeError):
                    publisher.publish("example/app", "v0.1.1", Path(directory))
                gh.assert_not_called()


if __name__ == "__main__":
    unittest.main()
