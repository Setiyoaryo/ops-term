import unittest
from scripts import build_pages as pages


def release(tag="v0.1.1"):
    return {
        "tag_name": tag, "draft": False, "prerelease": False,
        "assets": [{
            "name": f"opsterm-egui-{tag[1:]}-linux-x86_64",
            "browser_download_url": f"https://github.com/Setiyoaryo/ops-term/releases/download/{tag}/opsterm-egui-{tag[1:]}-linux-x86_64",
        }],
    }


class PagesReleaseTests(unittest.TestCase):
    def test_versioned_download_uses_a_real_public_release_asset(self):
        data = pages.normalize_release(release())
        self.assertEqual(data["VERSION"], "0.1.1")
        self.assertEqual(data["ASSET_NAME"], "opsterm-egui-0.1.1-linux-x86_64")
        self.assertIn("/releases/download/v0.1.1/", data["DOWNLOAD_URL"])

    def test_untrusted_urls_and_nonstable_releases_are_rejected(self):
        for url in ["javascript:alert(1)", "https://evil.example/binary", "https://github.com/other/repo/binary"]:
            data = release()
            data["assets"][0]["browser_download_url"] = url
            with self.subTest(url=url), self.assertRaises(ValueError):
                pages.normalize_release(data)
        for tag in ["v1.2.3-rc.1", "v1.2.3;echo unsafe", "v01.2.3"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                pages.normalize_release(release(tag))
        data = release(); data["draft"] = True
        with self.assertRaises(ValueError):
            pages.normalize_release(data)

    def test_rendered_content_has_no_unresolved_metadata(self):
        html = pages.render_template('<a href="{{DOWNLOAD_URL}}">{{VERSION}}</a>', pages.normalize_release(release()))
        self.assertNotIn("{{", html)
        self.assertIn('>0.1.1</a>', html)
        with self.assertRaises(ValueError):
            pages.render_template("{{UNKNOWN_TOKEN}}", pages.normalize_release(release()))


if __name__ == "__main__":
    unittest.main()
