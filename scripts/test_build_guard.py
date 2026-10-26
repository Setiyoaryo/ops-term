import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

GUARD = Path(__file__).with_name("build_guard.py")


class BuildGuardTests(unittest.TestCase):
    def run_guard(self, code, **limits):
        env = {**os.environ, "OPSTERM_BUILD_RESERVE_MB": "1", **limits}
        return subprocess.run(
            [sys.executable, str(GUARD), "--", sys.executable, "-c", code],
            env=env, capture_output=True, text=True, timeout=15,
        )

    def test_propagates_failure(self):
        result = self.run_guard("raise SystemExit(7)")
        self.assertEqual(result.returncode, 7, result.stderr)

    def test_refuses_to_start_with_insufficient_headroom(self):
        result = self.run_guard("print('SHOULD_NOT_RUN')", OPSTERM_BUILD_RESERVE_MB="999999999")
        self.assertEqual(result.returncode, 75, result.stderr)
        self.assertNotIn("SHOULD_NOT_RUN", result.stdout)

    def test_stops_entire_process_group_before_memory_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            finished = Path(directory) / "should-not-exist"
            child = f"import time; x = bytearray(80*1024*1024); time.sleep(3); open({str(finished)!r}, 'w').close()"
            code = f"import subprocess,sys,time; subprocess.Popen([sys.executable, '-c', {child!r}]); time.sleep(10)"
            result = self.run_guard(code, OPSTERM_BUILD_MAX_MB="48")
            self.assertEqual(result.returncode, 75, result.stderr)
            self.assertIn("Stopping", result.stderr)
            time.sleep(3.2)
            self.assertFalse(finished.exists(), "a descendant survived guard shutdown")

    def test_rejects_invalid_limits(self):
        result = self.run_guard("pass", OPSTERM_BUILD_MAX_MB="nope")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_only_one_guard_runs_at_a_time(self):
        with tempfile.TemporaryDirectory() as directory:
            ready = Path(directory) / "ready"
            env = {**os.environ, "OPSTERM_BUILD_RESERVE_MB": "1"}
            code = f"import time; open({str(ready)!r}, 'w').close(); time.sleep(4)"
            first = subprocess.Popen([sys.executable, str(GUARD), "--", sys.executable, "-c", code], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                deadline = time.monotonic() + 5
                while not ready.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(ready.exists())
                result = self.run_guard("pass")
                self.assertEqual(result.returncode, 75, result.stderr)
                self.assertIn("already running", result.stderr)
            finally:
                first.terminate()
                first.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
