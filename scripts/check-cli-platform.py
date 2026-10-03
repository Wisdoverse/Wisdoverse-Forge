#!/usr/bin/env python3
"""Run native Platform CLI checks with isolated synthetic state and a loopback fixture."""

import argparse
import base64
import hashlib
import http.server
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path


TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x86_64", "bash"),
    "x86_64-unknown-linux-musl": ("linux", "x86_64", "bash"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64", "bash"),
    "aarch64-unknown-linux-musl": ("linux", "aarch64", "bash"),
    "x86_64-apple-darwin": ("darwin", "x86_64", "bash"),
    "aarch64-apple-darwin": ("darwin", "aarch64", "bash"),
    "x86_64-pc-windows-msvc": ("windows", "x86_64", "powershell"),
}
SYNTHETIC_TOKEN = "cli-platform-check-token"
SPECIAL_VALUE = "quote' dollar$ tick` semi; line\n雪"
PRIVATE_ROOT = ""


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def json_data(result):
    envelope = json.loads(result.stdout)
    if not isinstance(envelope, dict) or envelope.get("ok") is not True or not isinstance(envelope.get("data"), dict):
        raise RuntimeError("CLI returned an invalid JSON success envelope")
    return envelope["data"]


def run(check, argv, env, report, cwd, stdin=None, timeout=20):
    argv = list(argv)
    if os.name == "nt" and Path(argv[0]).name.lower().removesuffix(".exe") in {"agentforge", "agentforge-sidecar"}:
        resolved = shutil.which(argv[0], path=env.get("PATH", ""))
        if not resolved:
            raise RuntimeError(f"{check}: executable not found on isolated PATH")
        argv[0] = resolved
    result = subprocess.run(
        argv, input=stdin, text=True, encoding="utf-8", errors="replace",
        capture_output=True, env=env, cwd=cwd, timeout=timeout,
    )
    if result.returncode:
        detail = result.stderr[-400:]
        for key in ("HOME", "USERPROFILE", "XDG_CONFIG_HOME"):
            if env.get(key):
                detail = detail.replace(env[key], "<isolated-path>")
        if PRIVATE_ROOT:
            detail = detail.replace(PRIVATE_ROOT, "<isolated-path>")
        detail = detail.replace(SYNTHETIC_TOKEN, "<synthetic-token>")
        raise RuntimeError(f"{check} failed ({result.returncode}): {detail.strip()}")
    report["checks"].append(check)
    return result


def isolated_env(home, config_home, path):
    inherited_path = next((value for key, value in os.environ.items() if key.upper() == "PATH"), "")
    env = {
        key: value for key, value in os.environ.items()
        if key.upper() not in {"PATH", "HOME", "USERPROFILE", "XDG_CONFIG_HOME", "BASH_ENV", "ENV"}
        and not key.upper().startswith(("AGENTFORGE_", "OTEL_", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"))
    }
    env.update({"PATH": str(path) + os.pathsep + inherited_path, "NO_PROXY": "*", "no_proxy": "*"})
    if os.name == "nt":
        env["USERPROFILE"] = str(home)
    else:
        env["HOME"] = str(home)
    if config_home is not None:
        env["XDG_CONFIG_HOME"] = str(config_home)
    return env


def check_secure_modes(paths):
    if os.name == "nt":
        return
    for path, expected in paths:
        mode = path.stat().st_mode & 0o777
        if mode != expected:
            raise RuntimeError(f"unexpected Unix mode for {path.name}: {mode:o}, expected {expected:o}")


class EnrollmentHandler(http.server.BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        type(self).requests.append({
            "path": self.path,
            "authorized": self.headers.get("Authorization") == f"Bearer {SYNTHETIC_TOKEN}",
            "idempotency_key": self.headers.get("Idempotency-Key", ""),
            "body": body,
        })
        payload = {"ok": True, "agent": {"id": "fixture-agent", "name": "Fixture"}, "enrollment": {
            "runtimeId": "fixture-runtime", "env": {"AGENTFORGE_TEST_VALUE": SPECIAL_VALUE},
            "sidecarCommand": "agentforge-sidecar --help",
        }}
        encoded = json.dumps(payload).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_args):
        pass


def main():
    global PRIVATE_ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", required=True, type=Path)
    parser.add_argument("--binary-revision", help="source revision for the supplied binaries")
    parser.add_argument("--target", required=True, choices=sorted(TARGETS))
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    expected_os, expected_arch, shell = TARGETS[args.target]
    actual_os = {"Linux": "linux", "Darwin": "darwin", "Windows": "windows"}.get(platform.system(), platform.system().lower())
    actual_arch = {"AMD64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine(), platform.machine().lower())
    report = {
        "target": args.target, "runner_os": actual_os, "runner_arch": actual_arch,
        "binary_revision": args.binary_revision, "checks": [], "status": "FAIL",
    }
    try:
        if (actual_os, actual_arch) != (expected_os, expected_arch):
            raise RuntimeError(f"target requires native {expected_os}/{expected_arch}; runner is {actual_os}/{actual_arch}")
        suffix = ".exe" if os.name == "nt" else ""
        sources = {name: args.binary_dir / f"{name}{suffix}" for name in ("agentforge", "agentforge-sidecar")}
        report["binaries"] = {}
        for name, path in sources.items():
            if not path.is_file():
                raise RuntimeError(f"missing binary: {path.name}")
            report["binaries"][name] = {"source_sha256": sha256(path)}

        shell_exe = shutil.which("bash") if shell == "bash" else shutil.which("pwsh")
        if not shell_exe:
            raise RuntimeError(f"native {shell} executable is unavailable")

        with tempfile.TemporaryDirectory(prefix="agentforge cli platform ") as temporary:
            root = Path(temporary)
            PRIVATE_ROOT = str(root)
            bindir = root / "bin"
            bindir.mkdir()
            for name, source in sources.items():
                destination = bindir / f"{name}{suffix}"
                shutil.copy2(source, destination)
                if sha256(source) != sha256(destination):
                    raise RuntimeError(f"binary copy hash mismatch: {name}")
                report["binaries"][name]["copy_sha256"] = sha256(destination)

            clean_home = root / "smoke-home"
            clean_home.mkdir()
            smoke_env = isolated_env(clean_home, None, bindir)
            if shell == "bash":
                expected_cli = shlex.quote(str(bindir / "agentforge"))
                expected_sidecar = shlex.quote(str(bindir / "agentforge-sidecar"))
                path_smoke = "set -euo pipefail\n"
                path_smoke += f'test "$(command -v agentforge)" = {expected_cli}\n'
                path_smoke += f'test "$(command -v agentforge-sidecar)" = {expected_sidecar}\n'
                path_smoke += "agentforge --version\nagentforge --help\nagentforge agents --help\nagentforge-sidecar --version\nagentforge-sidecar --help\n"
            else:
                ps_smoke = root / "path-smoke.ps1"
                cli_path = str(bindir / "agentforge.exe").replace("'", "''")
                sidecar_path = str(bindir / "agentforge-sidecar.exe").replace("'", "''")
                path_smoke = (
                    "$ErrorActionPreference = 'Stop'\n"
                    + f"if ((Get-Command agentforge -CommandType Application -ErrorAction Stop).Source -ne '{cli_path}') {{ exit 10 }}\n"
                    + f"if ((Get-Command agentforge-sidecar -CommandType Application -ErrorAction Stop).Source -ne '{sidecar_path}') {{ exit 11 }}\n"
                    + "agentforge --version; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
                    + "agentforge --help; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
                    + "agentforge agents --help; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
                    + "agentforge-sidecar --version; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
                    + "agentforge-sidecar --help; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n"
                )
                ps_smoke.write_text(path_smoke, encoding="utf-8-sig", newline="\n")
            if shell == "bash":
                run("native Bash PATH and CLI smoke", [shell_exe, "-c", path_smoke], smoke_env, report, clean_home)
            else:
                run("native PowerShell PATH and CLI smoke", [shell_exe, "-NoProfile", "-NonInteractive", "-File", str(ps_smoke)], smoke_env, report, clean_home)

            for name, argv in (
                ("agentforge version", ["agentforge", "--version"]),
                ("agentforge help", ["agentforge", "--help"]),
                ("agentforge agents help", ["agentforge", "agents", "--help"]),
                ("sidecar version", ["agentforge-sidecar", "--version"]),
                ("sidecar help", ["agentforge-sidecar", "--help"]),
            ):
                result = run(name, argv, smoke_env, report, clean_home)
                if name.endswith("version"):
                    report.setdefault("versions", {})[name] = result.stdout.strip()[:120]

            for mode in ("xdg", "default-home"):
                home = root / f"home-{mode}"
                home.mkdir()
                config_home = root / f"config-{mode}" if mode == "xdg" else None
                if config_home:
                    config_home.mkdir()
                env = isolated_env(home, config_home, bindir)
                work = root / f"work-{mode}"
                work.mkdir()
                config_root = config_home / "agentforge" if config_home else home / ".agentforge"
                config_file = config_root / "config.yaml"
                credentials = config_root / "credentials"

                run(f"{mode} config set/get/list", ["agentforge", "config", "set", "server", "https://forge.example.com"], env, report, home)
                config_get = run(f"{mode} config get", ["agentforge", "--json", "config", "get", "server"], env, report, work)
                if json_data(config_get).get("value") != "https://forge.example.com":
                    raise RuntimeError(f"config get value mismatch in {mode} mode")
                config_list = run(f"{mode} config list", ["agentforge", "--json", "config", "list"], env, report, work)
                if json_data(config_list).get("server") != "https://forge.example.com":
                    raise RuntimeError(f"config list value mismatch in {mode} mode")
                if not config_file.is_file():
                    raise RuntimeError(f"config file missing in {mode} mode")
                check_secure_modes([(config_file, 0o600), (config_file.parent, 0o700)])

                run(f"{mode} auth login", ["agentforge", "auth", "login", "--token", "-"], env, report, home, stdin=SYNTHETIC_TOKEN + "\n")
                check_secure_modes([(credentials, 0o600), (credentials.parent, 0o700)])
                if credentials.read_text(encoding="utf-8").strip() != SYNTHETIC_TOKEN:
                    raise RuntimeError(f"credential file content mismatch in {mode} mode")
                status = run(f"{mode} auth status", ["agentforge", "--json", "auth", "status"], env, report, work)
                auth = json_data(status)
                if auth.get("authenticated") is not True or auth.get("source") != "file":
                    raise RuntimeError(f"file-backed auth status failed in {mode} mode")
                run(f"{mode} auth logout", ["agentforge", "auth", "logout"], env, report, work)
                status = run(f"{mode} auth status after logout", ["agentforge", "--json", "auth", "status"], env, report, work)
                if json_data(status).get("authenticated") is not False or credentials.exists():
                    raise RuntimeError(f"logout did not clear credentials in {mode} mode")

                server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), EnrollmentHandler)
                EnrollmentHandler.requests = []
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    run(f"{mode} fixture server config", ["agentforge", "config", "set", "server", f"http://127.0.0.1:{server.server_port}"], env, report, home)
                    run(f"{mode} fixture auth login", ["agentforge", "auth", "login", "--token", "-"], env, report, home, stdin=SYNTHETIC_TOKEN + "\n")
                    command = ["agentforge", "agents", "enroll-local", "--tool", "codex", "--project", "fixture-project", "--shell", "--shell-format", shell]
                    block = run(f"{mode} enrollment command", command, env, report, work).stdout
                    check_secure_modes([(credentials, 0o600)])
                    check_script = "import os,sys; assert os.environ['AGENTFORGE_TEST_VALUE'] == sys.argv[1]"
                    if shell == "bash":
                        script = "set -euo pipefail\n" + block + "\n" + shlex.quote(sys.executable) + " -c " + shlex.quote(check_script) + " " + shlex.quote(SPECIAL_VALUE)
                        run(f"{mode} emitted Bash block", [shell_exe, "-c", script], env, report, work)
                    else:
                        ps_script = root / f"enrollment-{mode}.ps1"
                        expected = base64.b64encode(SPECIAL_VALUE.encode("utf-8")).decode("ascii")
                        script = (
                            "$ErrorActionPreference = 'Stop'\n" + block
                            + "\nif ($LASTEXITCODE -ne 0) { exit 20 }\n"
                            + f"if ($env:AGENTFORGE_TEST_VALUE -cne [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{expected}'))) {{ exit 19 }}\n"
                        )
                        ps_script.write_text(script, encoding="utf-8-sig", newline="\n")
                        run(f"{mode} emitted PowerShell block", [shell_exe, "-NoProfile", "-NonInteractive", "-File", str(ps_script)], env, report, work)

                    requests = EnrollmentHandler.requests
                    if len(requests) != 1:
                        raise RuntimeError(f"expected one enrollment POST in {mode} mode, got {len(requests)}")
                    request = requests[0]
                    key = request["idempotency_key"]
                    if (request["path"] != "/api/v1/agents/local-enroll" or not request["authorized"]
                            or not key.startswith("cli-enroll-") or len(key) != 43
                            or any(char not in "0123456789abcdef" for char in key[11:])
                            or request["body"].get("cliTool") != "codex"
                            or request["body"].get("projectId") != "fixture-project"):
                        raise RuntimeError(f"enrollment fixture contract failed in {mode} mode")
                    report["checks"].append(f"{mode} synthetic enrollment fixture request")
                    run(f"{mode} fixture auth logout", ["agentforge", "auth", "logout"], env, report, work)
                finally:
                    server.shutdown()
                    server.server_close()
                    thread.join(timeout=5)

        report["status"] = "PASS"
    except Exception as error:
        report["failure"] = str(error)[:800]
    report["qualifier_revision"] = subprocess.run(
        ["git", "-C", str(Path(__file__).resolve().parents[1]), "rev-parse", "HEAD"],
        text=True, encoding="utf-8", errors="replace", capture_output=True, timeout=5,
    ).stdout.strip() or None
    encoded_report = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(encoded_report, encoding="utf-8")
    print(encoded_report, end="")
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
