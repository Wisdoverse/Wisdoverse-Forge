#!/usr/bin/env python3
"""Exercise release version resolution and installed Platform CLI version checks."""

import contextlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseVersionTests(unittest.TestCase):
    def test_resolver_preserves_explicit_and_default_versions(self):
        package = load_script("package-cli-release")
        for version in ("1.2.3", "1.2.3-rc.1"):
            with self.subTest(version=version):
                self.assertEqual(package.resolve_version(version), version)
        self.assertEqual(package.resolve_version(), json.loads((ROOT / "package.json").read_text())["version"])

    def test_resolver_rejects_malformed_versions(self):
        package = load_script("package-cli-release")
        for version in ("v1.2.3", "1.2", "1.2.3\nother=1", "1.2.3;echo unexpected", "1.2.3 ", 123):
            with self.subTest(version=version), self.assertRaises(ValueError):
                package.resolve_version(version)

    def test_workflow_resolver_outputs_only_a_validated_version(self):
        workflow = (ROOT / ".github/workflows/cli-platforms.yml").read_text()
        step = workflow.split("      - name: Resolve CLI release version\n", 1)[1].split("\n      - ", 1)[0]
        lines = step.split("        run: |\n", 1)[1].splitlines()
        self.assertEqual(lines[0].strip(), "python - <<'PY'")
        self.assertEqual(lines[-1].strip(), "PY")
        script = "\n".join(line[10:] for line in lines[1:-1])
        cases = (("", "2.3.4"), ("1.2.3", "1.2.3"), ("1.2.3-rc.1", "1.2.3-rc.1"),
                 ("v1.2.3", None), ("1.2.3\nother=1", None))
        with tempfile.TemporaryDirectory(prefix="cli-version-resolver-") as temporary:
            directory = Path(temporary)
            (directory / "scripts").mkdir()
            shutil.copyfile(ROOT / "scripts/package-cli-release.py", directory / "scripts/package-cli-release.py")
            (directory / "package.json").write_text('{"version":"2.3.4"}', encoding="utf-8")
            output = directory / "output"
            for requested, expected in cases:
                with self.subTest(requested=requested):
                    output.write_text("prior=value\n", encoding="utf-8")
                    env = {**os.environ, "CLI_RELEASE_VERSION": requested, "GITHUB_OUTPUT": str(output)}
                    result = subprocess.run([sys.executable, "-c", script], cwd=directory, env=env, capture_output=True, text=True)
                    self.assertEqual(result.returncode == 0, expected is not None, result.stderr)
                    self.assertEqual(output.read_text(), "prior=value\n" + (f"version={expected}\n" if expected else ""))

    def test_package_preserves_release_version_in_report_and_archive(self):
        # Unix installation permissions require a Unix runner.
        platforms = (("windows", ".exe"),) if os.name == "nt" else (("linux", ""), ("windows", ".exe"))
        with tempfile.TemporaryDirectory(prefix="cli-version-package-") as temporary:
            directory = Path(temporary)
            for os_label, suffix in platforms:
                with self.subTest(os=os_label):
                    binaries = directory / os_label
                    binaries.mkdir()
                    for name in ("agentforge", "agentforge-sidecar"):
                        (binaries / (name + suffix)).write_bytes(b"fixture binary")
                    argv = [sys.executable, str(ROOT / "scripts/package-cli-release.py"), "--binary-dir", str(binaries),
                            "--output-dir", str(directory / (os_label + "-archives")),
                            "--install-dir", str(directory / (os_label + "-installed")),
                            "--os", os_label, "--arch", "amd64", "--version", "1.2.3-rc.1"]
                    result = subprocess.run(argv, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                    report = json.loads(result.stdout)
                    self.assertEqual(report["version"], "1.2.3-rc.1")
                    self.assertTrue(report["archive"].startswith(f"agentforge-v1.2.3-rc.1-{os_label}-x86_64."))
                    self.assertEqual(report["source_sha256"], report["installed_sha256"])

    def check_installed_version(self, version_output, expected):
        checker = load_script("check-cli-platform")
        runner_os = {"Linux": "linux", "Darwin": "darwin", "Windows": "windows"}[checker.platform.system()]
        runner_arch = {"AMD64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(checker.platform.machine(), checker.platform.machine())
        target = next(target for target, values in checker.TARGETS.items() if values[:2] == (runner_os, runner_arch))
        seen = []

        def fixture_run(name, _argv, _env, report, _cwd, **_kwargs):
            seen.append(name)
            if name == "xdg config set/get/list":
                raise RuntimeError("fixture stops before configuration")
            report["checks"].append(name)
            return SimpleNamespace(stdout=version_output if name == "agentforge version" else "fixture help\n")

        with tempfile.TemporaryDirectory(prefix="cli-version-installed-") as temporary:
            directory = Path(temporary)
            suffix = ".exe" if os.name == "nt" else ""
            for name in ("agentforge", "agentforge-sidecar"):
                (directory / (name + suffix)).write_bytes(b"fixture binary")
            report_path = directory / "report.json"
            argv = ["check-cli-platform.py", "--binary-dir", str(directory), "--target", target, "--report", str(report_path)]
            if expected is not None:
                argv += ["--expected-version", expected]
            with patch.object(sys, "argv", argv), patch.object(checker, "run", fixture_run), \
                    patch.object(checker.shutil, "which", return_value="fixture-shell"), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(checker.main(), 1)
            return json.loads(report_path.read_text()), seen

    def test_installed_version_mismatch_stops_before_configuration(self):
        for version in ("agentforge dev\n", "agentforge 1.2.4\n", "agentforge 1.2.3\nextra output\n", ""):
            with self.subTest(version=version):
                report, seen = self.check_installed_version(version, "1.2.3")
                self.assertIn("Platform CLI version differs from release 1.2.3", report["failure"])
                self.assertIn("AGENTFORGE_CLI_VERSION", report["failure"])
                self.assertNotIn("xdg config set/get/list", seen)
                self.assertNotIn("installed Platform CLI release version", report["checks"])

    def test_exact_installed_version_reaches_next_check(self):
        for version in ("1.2.3", "1.2.3-rc.1"):
            with self.subTest(version=version):
                report, seen = self.check_installed_version(f"agentforge {version}\r\n", version)
                self.assertIn("installed Platform CLI release version", report["checks"])
                self.assertEqual(report["expected_version"], version)
                self.assertIn("xdg config set/get/list", seen)
                self.assertEqual(report["failure"], "fixture stops before configuration")

    def test_manual_source_check_can_omit_release_version(self):
        report, seen = self.check_installed_version("agentforge dev\n", None)
        self.assertNotIn("installed Platform CLI release version", report["checks"])
        self.assertIn("xdg config set/get/list", seen)
        self.assertEqual(report["failure"], "fixture stops before configuration")


if __name__ == "__main__":
    unittest.main()
