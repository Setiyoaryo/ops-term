import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("package_egui", Path(__file__).with_name("package_egui.py"))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ReleaseTagTests(unittest.TestCase):
    def test_matching_public_release_tag(self):
        package.validate_release_tag("v0.1.1", "0.1.1")

    def test_mismatched_or_unsafe_tag_is_rejected_before_packaging(self):
        for tag in ["v0.1.2", "0.1.1", "main", "v0.1.1/other", "v0.1.1;echo unsafe"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                package.validate_release_tag(tag, "0.1.1")

    def test_public_latest_channel_uses_stable_versions_only(self):
        for version in ["0.1.1-rc.1", "0.1.1+build.1", "01.1.1"]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                package.validate_release_tag(f"v{version}", version)


if __name__ == "__main__":
    unittest.main()
