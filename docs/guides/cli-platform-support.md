# CLI Platform Support

Wisdoverse Forge ships two operator-facing command-line binaries:

- `agentforge` - the Platform CLI used to log in, inspect work, and enroll
  local Host CLI agents.
- `agentforge-sidecar` - the local process that connects a Host CLI agent to a
  remote Wisdoverse Forge control plane.

The CLI experience is a product surface, not an internal developer shortcut.
Every command, error message, install step, and troubleshooting path must be
usable by an operator who has never built this repository from source.

## Beginner-First Standard

Document and design every CLI feature for a first-time user by default.

- Start with the shortest safe path that works for a new user.
- State prerequisites before commands.
- Use copy-pasteable commands with placeholder values such as
  `<project-id>` and `https://forge.example.com`.
- Explain what the user should see after the command succeeds.
- Put advanced build, debug, and verification details after the basic path.
- Avoid assuming the reader knows Rust, Cargo, Docker networking, NATS, Temporal,
  shell quoting, or CI internals.
- Error messages must say what failed, why it matters, and the next action.
- UI text and docs should use product terms from
  [Architecture Glossary](../architecture/glossary.md), not implementation
  shortcuts.

For any new CLI command, the acceptance checklist is:

1. A new user can find where to start from `README.md` or `docs/README.md`.
2. The command has `--help` text for every option.
3. The happy path includes one short example.
4. The failure path has actionable error text.
5. The docs say how to verify success.
6. The feature works without reading source code.

## Supported Platform Policy

Release artifacts for `agentforge` and `agentforge-sidecar` must cover the
mainstream operator platforms below.

| Tier | Platform | CPU           | Target Triple                                               | Notes                                                                           |
| ---- | -------- | ------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 1    | Linux    | x86_64        | `x86_64-unknown-linux-gnu` or `x86_64-unknown-linux-musl`   | Primary server and workstation path.                                            |
| 1    | Linux    | ARM64         | `aarch64-unknown-linux-gnu` or `aarch64-unknown-linux-musl` | Required for ARM servers and single-board hosts.                                |
| 1    | macOS    | Apple Silicon | `aarch64-apple-darwin`                                      | Primary local operator workstation path.                                        |
| 1    | macOS    | Intel         | `x86_64-apple-darwin`                                       | Required while Intel Macs remain common in enterprises.                         |
| 1    | Windows  | x86_64        | `x86_64-pc-windows-msvc`                                    | PowerShell commands; native relay and recovery checks are pending CI execution. |
| 2    | Windows  | ARM64         | `aarch64-pc-windows-msvc`                                   | Supported when CI and signer capacity are available.                            |

Tier 1 means each public release should provide a downloadable artifact, install
instructions, checksum, and a smoke test. Tier 2 means the code should avoid
known blockers, but release artifacts may lag until the project has a validated
CI runner and signing path for that target.

Container agent images remain Linux container artifacts. Multi-platform CLI
support applies to the local operator binaries used outside the containers.

## Release Artifact Standard

Each public CLI release should include:

- A compressed archive per platform, named with product, version, operating
  system, and CPU architecture.
- `SHA256SUMS` for all archives.
- Signature or provenance verification instructions.
- SBOM or auditable build metadata for release binaries where the release
  pipeline supports it.
- A changelog entry that calls out CLI changes, breaking flags, and migration
  notes.
- A smoke-test command for each Tier 1 platform:

```bash
agentforge --version
agentforge --help
agentforge agents --help
agentforge-sidecar --help
```

Windows docs must include PowerShell examples when environment variables or
path edits are required.

## Install Experience Requirements

The first install path should not require cloning the repository.

Recommended order for operators:

1. Download a release artifact for the user's OS and CPU.
2. Verify the checksum.
3. Place the binary on `PATH`.
4. Run `agentforge --version`.
5. Log in or configure the API URL.
6. Run the specific workflow command, such as `agentforge agents enroll-local`.

Commands that print follow-up shell blocks must support both POSIX shells and
Windows PowerShell. For Host CLI enrollment, use `--shell-format bash` on macOS
or Linux and `--shell-format powershell` on Windows so the returned
`agentforge-sidecar` launch block can be pasted into the same shell.

Source builds are still supported for contributors, but they are not the
primary operator path.

### Linux or macOS install example

Prerequisites:

- A shell with `curl`, `awk`, `tar`, `shasum`, and permission to write to
  `/usr/local/bin`.
- The release version and target name from the release page.

Replace `v1.2.3` and the target archive with the current release artifact for
your computer.

| Computer            | Target archive name |
| ------------------- | ------------------- |
| Linux x86_64        | `linux-x86_64`      |
| Linux ARM64         | `linux-arm64`       |
| macOS Intel         | `macos-x86_64`      |
| macOS Apple Silicon | `macos-arm64`       |

The archive contains `agentforge` and `agentforge-sidecar` at its root. The
commands stop before installation if a download or checksum check fails.

```bash
(
  set -eu
  VERSION=v1.2.3
  TARGET=linux-x86_64
  ARCHIVE="agentforge-${VERSION}-${TARGET}.tar.gz"

  curl -fLO "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/${VERSION}/${ARCHIVE}"
  curl -fLO "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/${VERSION}/SHA256SUMS"
  awk -v archive="$ARCHIVE" '$2 == archive { print }' SHA256SUMS | shasum -a 256 -c -
  tar -xzf "$ARCHIVE"
  sudo install -m 0755 agentforge /usr/local/bin/agentforge
  sudo install -m 0755 agentforge-sidecar /usr/local/bin/agentforge-sidecar

  agentforge --version
  agentforge --help
  agentforge-sidecar --help
)
```

Success looks like:

- `agentforge --version` prints the CLI version and exits with code `0`.
- `agentforge --help` lists commands such as `auth`, `config`, and `agents`.
- `agentforge-sidecar --help` explains that most users should copy the join
  command from the Agents page.

Next, point the CLI at your Forge server and store a token:

```bash
agentforge config set server https://forge.example.com
agentforge auth login --token <platform-token>
agentforge auth status
```

### Windows PowerShell install example

Prerequisites:

- PowerShell 7 or Windows PowerShell with permission to write to a folder on
  `PATH`.
- The release version and target archive from the release page.

Replace `v1.2.3` with the current release version.

```powershell
$ErrorActionPreference = "Stop"
$Version = "v1.2.3"
$Target = "windows-x86_64"
$Archive = "agentforge-$Version-$Target.zip"
$InstallDir = "$env:LOCALAPPDATA\Programs\AgentForge"

Invoke-WebRequest -Uri "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/$Version/$Archive" -OutFile $Archive
Invoke-WebRequest -Uri "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/$Version/SHA256SUMS" -OutFile "SHA256SUMS"
$ChecksumLines = @(Get-Content .\SHA256SUMS | Where-Object { ($_ -split '\s+', 2)[1] -ceq $Archive })
if ($ChecksumLines.Count -ne 1) { throw "Expected exactly one checksum for $Archive" }
$ExpectedHash = ($ChecksumLines[0] -split '\s+', 2)[0]
if ($ExpectedHash -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash $Archive -Algorithm SHA256).Hash -ne $ExpectedHash) {
  throw "Checksum mismatch for $Archive; do not install this download"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Expand-Archive $Archive -DestinationPath $InstallDir -Force

$UserPaths = @([Environment]::GetEnvironmentVariable("Path", "User") -split ';' | Where-Object { $_ })
if ($UserPaths -notcontains $InstallDir) {
  [Environment]::SetEnvironmentVariable("Path", (($UserPaths + $InstallDir) -join ';'), "User")
}
$env:PATH = "$InstallDir;$env:PATH"
& "$InstallDir\agentforge.exe" --version
& "$InstallDir\agentforge.exe" --help
& "$InstallDir\agentforge-sidecar.exe" --help
```

The checksum must match before extraction. The commands update your user-level
`PATH` and the current PowerShell window without changing the machine-level
setting.

Success looks like:

- `agentforge.exe --version` prints the CLI version and exits with code `0`.
- `agentforge.exe --help` lists commands such as `auth`, `config`, and
  `agents`.
- `agentforge-sidecar.exe --help` prints the join-command guidance instead of
  trying to connect to the server.

Next, connect from the current PowerShell window:

```powershell
agentforge config set server https://forge.example.com
agentforge auth login --token <platform-token>
agentforge auth status
```

## Host CLI Agent Enrollment

Host CLI enrollment is the main reason operators need local binaries. The local
machine runs `agentforge-sidecar` and the selected CLI tool, while the remote
platform manages identity, task assignment, heartbeats, and result evidence.

Start with [Host CLI Agent Enrollment](../runbooks/host-cli-agent-enrollment.md)
for the operational flow.

For Host CLI relay events, install Node.js 24 and configure a supported vendor
Container CLI with the AgentForge relay hook. Start the enrolled sidecar and
wait until it reports healthy before launching the vendor CLI. The hook uses a
local Unix-domain socket on Linux and macOS. On Windows it connects to
`\\.\pipe\agentforge-relay-<uuid>` and verifies the sidecar's HMAC server proof
before sending any event data. The Windows pipe accepts local clients only and
uses an endpoint DACL limited to the current user and SYSTEM.

On Windows, the sidecar's default WAL folder is
`%LOCALAPPDATA%\AgentForge\agents\<uuid>\wal`; if `LOCALAPPDATA` is absent, it
uses `%USERPROFILE%\AppData\Local\AgentForge\agents\<uuid>\wal`. A custom
`WAL_PATH` must be absolute and pass the sidecar's private-directory and
reparse-point checks before use. During recovery, a completed tombstone is
processed before cleanup of pending or result files so an acknowledged event
does not return to the queue.

## Contributor Requirements

Any PR that changes the Platform CLI, sidecar CLI, installer, release packaging,
or Host CLI enrollment must update this document or the linked runbook when the
user-facing behavior changes.

The PR description should include:

- Which platforms were tested.
- Which platforms are expected to work but were not tested locally.
- Install or upgrade notes.
- A copy of the CLI help or example command when flags changed.
- Validation output for the smallest relevant CLI smoke test.

### Native platform checks

PRs touching Rust, the operator release workflow, or this guide run **CLI
Platforms** on native Linux x86_64/ARM64, macOS Intel/Apple Silicon, and Windows
x86_64 runners. The workflow runs the built-in Node relay transport tests and
`cargo test --locked -p agentforge-sidecar` on each native runner alongside the
CLI library tests. On Windows, these checks cover named-pipe server
authentication, private pipe/WAL boundaries, and recovery behavior. The
release workflow reuses these checks before attaching the operator binaries to
a release. Open the workflow's per-platform jobs to see the result; a
successful build alone does not pass the check. Windows runtime qualification
remains pending until the native Windows job completes successfully.

Each job runs the CLI library tests, creates the documented release archive,
installs both binaries from it, checks their SHA-256 hashes and `PATH`, and runs help/version,
configuration, token storage/status/logout, and enrollment shell checks.
Configuration and synthetic credentials stay inside temporary directories;
the token never comes from a real account. The enrollment response comes from
a loopback HTTP fixture, and the printed launch block runs the real sidecar
with `--help` in Bash or PowerShell 7. The
`cli-platform-report-<os>-<arch>` artifact contains the package and command
reports with the tested revision, archive and binary hashes, checks, and
outcome. Native Node and sidecar test results are in the per-platform job log.

Configuration and credentials use `XDG_CONFIG_HOME/agentforge` when configured,
otherwise `HOME/.agentforge`. Native Windows shells without a `HOME` environment
variable use `USERPROFILE/.agentforge`, so changing the current directory does
not move or expose the stored token.

To repeat the bounded check on your own computer, prerequisites are Python 3.10+,
Bash on Linux/macOS or PowerShell 7 on Windows, and both binaries built for your
native target. Run from the repository root and replace the target below with
the matching triple from the platform table:

```bash
python scripts/check-cli-platform.py \
  --binary-dir rust/target/x86_64-unknown-linux-gnu/release \
  --target x86_64-unknown-linux-gnu \
  --binary-revision <source-revision> --report cli-platform-report.json
```

Success is a `PASS` report and exit code `0`. A failure names the failed check;
fix it before relying on that platform's artifact. The archive smoke checks and
the native transport/filesystem tests are separate evidence: the former checks
the installed binaries and local command flow, while the latter exercises
bounded local transport and recovery behavior. NATS-dependent tests can skip
when no NATS server is available. Neither check set qualifies a live Forge
server, real NATS delivery, a real vendor task/session, Windows operator
credential ACLs, release signatures, or archive installation from a public
release. Those areas remain unqualified. The Unix socket relay and its
owner-only permissions remain active on Linux and macOS.
