#!/usr/bin/env python3
"""Package and verify the two native agentforge release binaries."""

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path


PLATFORMS = {("linux", "amd64"), ("linux", "arm64"), ("macos", "amd64"), ("macos", "arm64"), ("windows", "amd64")}
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.]+)?\Z")
BINARIES = ("agentforge", "agentforge-sidecar")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def package_archive(path, files, names, windows):
    if windows:
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for source, name in zip(files, names):
                info = zipfile.ZipInfo(name)
                info.create_system = 3
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                archive.writestr(info, source.read_bytes())
    else:
        with tarfile.open(path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
            for source, name in zip(files, names):
                info = tarfile.TarInfo(name)
                info.size, info.mode, info.uid, info.gid = source.stat().st_size, 0o755, 0, 0
                info.uname = info.gname = ""
                with source.open("rb") as stream:
                    archive.addfile(info, stream)


def read_archive(path, names, windows):
    if windows:
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
            if [item.filename for item in members] != names:
                raise ValueError("ZIP must contain exactly the two canonical binaries")
            for item in members:
                mode = item.external_attr >> 16
                if stat.S_IFMT(mode) != stat.S_IFREG:
                    raise ValueError("ZIP contains a non-regular member")
            return [(item.filename, archive.read(item)) for item in members]
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        if ([item.name for item in members] != names
                or any(not item.isfile() or item.mode != 0o755 for item in members)):
            raise ValueError("TAR must contain exactly the two regular canonical binaries")
        return [(item.name, archive.extractfile(item).read()) for item in members]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--install-dir", required=True, type=Path)
    parser.add_argument("--os", required=True, choices=("linux", "macos", "windows"))
    parser.add_argument("--arch", required=True, choices=("amd64", "arm64"))
    parser.add_argument("--version", default="")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    version = args.version
    if not version:
        version = json.loads((Path(__file__).resolve().parents[1] / "package.json").read_text(encoding="utf-8"))["version"]
    if not VERSION.fullmatch(version):
        parser.error("version must match the release format 1.2.3 or 1.2.3-suffix, without a leading v")
    if (args.os, args.arch) not in PLATFORMS:
        parser.error("unsupported Tier 1 OS/architecture combination")
    if args.report and (args.report.exists() or args.report.is_symlink()):
        parser.error("report already exists; choose a fresh report path")

    windows = args.os == "windows"
    suffix = ".exe" if windows else ""
    member_names = [name + suffix for name in BINARIES]
    source_files = [args.binary_dir / name for name in member_names]
    ext = ".zip" if windows else ".tar.gz"
    archive_name = f"agentforge-v{version}-{args.os}-{'x86_64' if args.arch == 'amd64' else 'arm64'}{ext}"
    raw_names = [f"{name}-{args.os}-{args.arch}{suffix}" for name in BINARIES]
    destinations = [args.output_dir / name for name in raw_names + [archive_name]]
    report_path = args.report
    if report_path and (report_path.resolve() in {path.resolve() for path in destinations}
                        or report_path.resolve() == args.install_dir.resolve()
                        or args.install_dir.resolve() in report_path.resolve().parents):
        parser.error("report path conflicts with release outputs or installation")

    report = {"status": "FAIL", "version": version, "os": args.os, "arch": args.arch, "archive": archive_name, "checks": []}
    created_install = False
    stage = None
    published = []
    try:
        if args.install_dir.exists() or args.install_dir.is_symlink():
            raise FileExistsError("install directory already exists; choose a fresh path")
        if args.output_dir.exists() and args.output_dir.is_symlink():
            raise ValueError("output directory must not be a symlink")
        if any(path.exists() or path.is_symlink() for path in destinations):
            raise FileExistsError("release output already exists; choose a fresh output directory")
        for path in source_files:
            if path.is_symlink() or not path.is_file() or not stat.S_ISREG(path.stat().st_mode):
                raise FileNotFoundError(f"missing regular source binary: {path.name}")
        source_hashes = {name: digest(path) for name, path in zip(member_names, source_files)}
        report["source_sha256"] = source_hashes
        revision = subprocess.run(["git", "-C", str(Path(__file__).resolve().parents[1]), "rev-parse", "HEAD"], capture_output=True, text=True)
        report["qualifier_revision"] = revision.stdout.strip() if revision.returncode == 0 else None

        args.output_dir.mkdir(parents=True, exist_ok=True)
        stage = Path(tempfile.mkdtemp(prefix=".agentforge-package-", dir=args.output_dir))
        staged_files = [stage / name for name in member_names]
        for source, staged in zip(source_files, staged_files):
            shutil.copyfile(source, staged)
            if not windows:
                staged.chmod(0o755)
        staged_archive = stage / archive_name
        package_archive(staged_archive, staged_files, member_names, windows)
        archived = read_archive(staged_archive, member_names, windows)
        args.install_dir.mkdir(parents=True)
        created_install = True
        installed_hashes = {}
        for (name, data), source in zip(archived, source_files):
            installed = args.install_dir / name
            with installed.open("xb") as stream:
                stream.write(data)
            if not windows:
                installed.chmod(0o755)
                if (installed.stat().st_mode & 0o777) != 0o755 or not os.access(installed, os.X_OK):
                    raise ValueError(f"installed executable mode check failed: {name}")
            installed_hashes[name] = digest(installed)
            if installed_hashes[name] != source_hashes[name]:
                raise ValueError(f"installed hash mismatch: {name}")
        for source, staged in zip(source_files, staged_files):
            if digest(source) != digest(staged):
                raise ValueError(f"staged hash mismatch: {staged.name}")
        for name, staged in zip(raw_names + [archive_name], staged_files + [staged_archive]):
            destination = args.output_dir / name
            with destination.open("xb") as output_file, staged.open("rb") as input_file:
                published.append(destination)
                shutil.copyfileobj(input_file, output_file)
        report.update({"status": "PASS", "archive_sha256": digest(args.output_dir / archive_name), "installed_sha256": installed_hashes})
        report["checks"] = ["archive members and types", "source/staged/installed SHA-256"]
        if not windows:
            report["checks"].append("installed executable mode")
    except Exception as error:
        failure = str(error)
        for path, label in ((stage, "<staging>"), (args.binary_dir, "<binary-dir>"),
                            (args.output_dir, "<output-dir>"), (args.install_dir, "<install-dir>"),
                            (report_path, "<report>")):
            if path is not None:
                failure = failure.replace(str(path), label)
        report["failure"] = failure[:500]
        for path in published:
            path.unlink(missing_ok=True)
        if created_install:
            shutil.rmtree(args.install_dir, ignore_errors=True)
    finally:
        if stage is not None:
            shutil.rmtree(stage, ignore_errors=True)

    output = json.dumps(report, indent=2) + "\n"
    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        with report_path.open("x", encoding="utf-8") as stream:
            stream.write(output)
    print(output, end="")
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
