#!/usr/bin/env python3
"""Check `make setup` network selection; needs make, Python 3, Docker Compose."""

import json
import os
import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT = "agentforge-agents"


def main():
    real_docker = shutil.which("docker")
    if not real_docker or subprocess.run([real_docker, "compose", "version"], capture_output=True).returncode:
        raise SystemExit("Requires make, Python 3, and Docker Compose")

    with tempfile.TemporaryDirectory() as stub_dir:
        stub = pathlib.Path(stub_dir) / "docker"
        stub.write_text('''#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["DOCKER_LOG"], "a") as f: f.write(json.dumps(a) + "\\n")
if a[:3] == ["compose", "--project-name", "agentforge-setup"]:
    c = a[3:]
    if c[:1] == ["--env-file"]: c = c[2:]
    if c != ["-f", "-", "config", "--environment"]: sys.exit("blocked compose command")
    os.execv(os.environ["REAL_DOCKER"], [os.environ["REAL_DOCKER"], *a])
if a[:2] == ["network", "inspect"]: sys.exit(1)
if a[:2] == ["network", "create"]: sys.exit(int(os.getenv("CREATE_EXIT", "0")))
if a[:1] == ["run"]: sys.exit(0)
sys.exit("blocked Docker command: " + repr(a))
''', encoding="utf-8")
        stub.chmod(0o755)
        cases = [
            ("default", None, DEFAULT, {}, None, 0, 0),
            ("double quotes", 'CONTAINER_NETWORK="double"\n', "double", {}, None, 0, 0),
            ("single quotes", "CONTAINER_NETWORK='single'\n", "single", {}, None, 0, 0),
            ("inline comment", "CONTAINER_NETWORK=comment # ignored\n", "comment", {}, None, 0, 0),
            ("empty dotenv", "CONTAINER_NETWORK=\n", DEFAULT, {}, None, 0, 0),
            ("interpolation", "SUFFIX=expanded\nCONTAINER_NETWORK=team-${SUFFIX}\n", "team-expanded", {}, None, 0, 0),
            ("environment override", "CONTAINER_NETWORK=file\n", "environment", {"CONTAINER_NETWORK": "environment"}, None, 0, 0),
            ("empty environment", "CONTAINER_NETWORK=file\n", DEFAULT, {"CONTAINER_NETWORK": ""}, None, 0, 0),
            ("make CLI override", "CONTAINER_NETWORK=file\n", "make-value", {"CONTAINER_NETWORK": "environment"}, "make-value", 0, 0),
            ("dotenv parse failure", "\x00\n", None, {}, None, 1, 0),
            ("create failure", "CONTAINER_NETWORK=selected\n", "selected", {}, None, 1, 37),
        ]
        for name, dotenv, expected, overrides, make_value, want_failure, create_exit in cases:
            with tempfile.TemporaryDirectory() as temp:
                tmp = pathlib.Path(temp)
                env_file, log = tmp / ".env", tmp / "docker.log"
                env_file.write_text(dotenv, encoding="utf-8") if dotenv is not None else None
                workspace = tmp / "workspaces"
                workspace.mkdir()
                env = {k: v for k, v in os.environ.items() if k not in ("CONTAINER_NETWORK", "AGENTFORGE_WORKSPACE_ROOT", "CLAUDE_GID")}
                env.update(overrides, PATH=f"{stub_dir}{os.pathsep}{env['PATH']}", REAL_DOCKER=real_docker,
                           DOCKER_LOG=str(log), CREATE_EXIT=str(create_exit))
                cmd = ["make", "--no-print-directory", "-C", str(ROOT), "setup",
                       f"COMPOSE_ENV_FILE={env_file if dotenv is not None else tmp / 'missing.env'}",
                       f"AGENTFORGE_WORKSPACE_ROOT={workspace}", f"OAUTH_MOUNT_DIR_NAME={tmp / 'oauth'}", "CLAUDE_GID=1012"]
                if make_value is not None: cmd.append(f"CONTAINER_NETWORK={make_value}")
                result = subprocess.run(cmd, env=env, text=True, capture_output=True)
                events = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
                creates = [a[2] for a in events if a[:2] == ["network", "create"]]
                runs = [a for a in events if a[:1] == ["run"]]
                assert (result.returncode != 0) == bool(want_failure), f"{name}: {result.stderr}"
                assert creates == ([expected] if expected is not None else []), f"{name}: {creates}"
                assert len(runs) == (0 if want_failure else 2), f"{name}: unexpected docker run"
                if want_failure: assert any(a[:1] == ["compose"] for a in events), name

    print("PASS: 9 selection cases; Compose parse and network-create failures fail closed")


if __name__ == "__main__":
    main()
