"""Exercise the publication gate without contacting GitHub or waiting."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify-release.sh"
SHA = "a" * 40


class ReleaseGateTests(unittest.TestCase):
    def run_gate(self, *, tag_sha=SHA, ci="success", security="success", tag="v1.2.3", api_error=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gh = root / "gh"
            gh.write_text(f"#!{sys.executable}\n" + '''
import json, os, sys
args = sys.argv[1:]
if os.environ["MOCK_API_ERROR"] == "true":
    sys.exit(1)
if any("/commits/" in arg for arg in args):
    print(os.environ["MOCK_TAG_SHA"])
else:
    assert "head_sha=" + os.environ["GITHUB_SHA"] in args
    assert "event=push" in args and "branch=main" in args
    workflow = "ci" if any("/ci.yml/" in arg for arg in args) else "security"
    print(json.loads(os.environ["MOCK_RESULTS"])[workflow])
''')
            gh.chmod(0o755)
            sleep = root / "sleep"
            sleep.write_text("#!/bin/sh\nexit 0\n")
            sleep.chmod(0o755)
            environment = os.environ | {
                "PATH": f"{directory}:{os.environ['PATH']}",
                "GITHUB_REPOSITORY": "example/project",
                "GITHUB_SHA": SHA,
                "RELEASE_TAG": tag,
                "MOCK_TAG_SHA": tag_sha,
                "MOCK_RESULTS": json.dumps({"ci": ci, "security": security}),
                "MOCK_API_ERROR": str(api_error).lower(),
            }
            return subprocess.run(
                ["bash", str(SCRIPT)], env=environment, capture_output=True, timeout=30
            ).returncode

    def test_matching_source_and_successful_checks_pass(self):
        self.assertEqual(self.run_gate(), 0)

    def test_different_tag_source_fails(self):
        self.assertNotEqual(self.run_gate(tag_sha="b" * 40), 0)

    def test_failed_ci_fails(self):
        self.assertNotEqual(self.run_gate(ci="failure"), 0)

    def test_cancelled_security_fails(self):
        self.assertNotEqual(self.run_gate(security="cancelled"), 0)

    def test_missing_or_pending_checks_time_out_closed(self):
        self.assertNotEqual(self.run_gate(ci="pending"), 0)

    def test_api_error_fails_closed(self):
        self.assertNotEqual(self.run_gate(api_error=True), 0)

    def test_unexpected_tag_fails(self):
        self.assertNotEqual(self.run_gate(tag="main"), 0)


if __name__ == "__main__":
    unittest.main()
