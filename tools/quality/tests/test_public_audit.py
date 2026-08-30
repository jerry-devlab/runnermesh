import tempfile
import unittest
from pathlib import Path

from tools.quality.public_audit import check_markdown_links, scan_sensitive_text


class SensitiveTextTests(unittest.TestCase):
    def test_detects_private_path_and_token_shapes(self) -> None:
        windows_path = "C:" + "\\Users\\realname\\private\\receipt.json"
        token = "gh" + "p_" + ("A" * 24)
        reasons = scan_sensitive_text(f"{windows_path} {token}")
        self.assertIn("personal Windows user path", reasons)
        self.assertIn("GitHub token shape", reasons)

    def test_allows_documented_user_placeholder(self) -> None:
        placeholder = "C:" + "\\Users\\<user>\\AppData"
        self.assertEqual(scan_sensitive_text(placeholder), [])

    def test_detects_runner_service_identity(self) -> None:
        service = "actions" + ".runner.orgname.private-host"
        self.assertIn("personal runner service identity", scan_sensitive_text(service))

    def test_does_not_treat_slash_delimited_prose_as_a_home_path(self) -> None:
        self.assertEqual(scan_sensitive_text("identity/home/work-root evidence"), [])


class MarkdownLinkTests(unittest.TestCase):
    def test_accepts_existing_relative_target_and_external_link(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "target.md").write_text("ok\n", encoding="utf-8")
            source = root / "source.md"
            source.write_text(
                "[local](target.md) [external](https://example.com)\n",
                encoding="utf-8",
            )
            self.assertEqual(check_markdown_links(source, root), [])

    def test_reports_missing_relative_target_without_echoing_target(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source.md"
            source.write_text("[missing](not-present.md)\n", encoding="utf-8")
            issues = check_markdown_links(source, root)
            self.assertEqual(len(issues), 1)
            self.assertEqual(issues[0].reason, "relative Markdown link target is missing")
            self.assertNotIn("not-present", issues[0].receipt())


if __name__ == "__main__":
    unittest.main()
