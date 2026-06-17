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

| Tier | Platform | CPU           | Target Triple                                               | Notes                                                   |
| ---- | -------- | ------------- | ----------------------------------------------------------- | ------------------------------------------------------- |
| 1    | Linux    | x86_64        | `x86_64-unknown-linux-gnu` or `x86_64-unknown-linux-musl`   | Primary server and workstation path.                    |
| 1    | Linux    | ARM64         | `aarch64-unknown-linux-gnu` or `aarch64-unknown-linux-musl` | Required for ARM servers and single-board hosts.        |
| 1    | macOS    | Apple Silicon | `aarch64-apple-darwin`                                      | Primary local operator workstation path.                |
| 1    | macOS    | Intel         | `x86_64-apple-darwin`                                       | Required while Intel Macs remain common in enterprises. |
| 1    | Windows  | x86_64        | `x86_64-pc-windows-msvc`                                    | Required for PowerShell-based operators.                |
| 2    | Windows  | ARM64         | `aarch64-pc-windows-msvc`                                   | Supported when CI and signer capacity are available.    |

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

- A shell with `curl`, `grep`, `tar`, `shasum`, and permission to write to
  `/usr/local/bin`.
- The release version and target name from the release page.

Replace `v1.2.3` and the target archive with the current release artifact for
your computer.

```bash
VERSION=v1.2.3
TARGET=linux-x86_64

curl -LO "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/${VERSION}/agentforge-${VERSION}-${TARGET}.tar.gz"
curl -LO "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/${VERSION}/SHA256SUMS"
grep " agentforge-${VERSION}-${TARGET}.tar.gz$" SHA256SUMS | shasum -a 256 -c -
tar -xzf "agentforge-${VERSION}-${TARGET}.tar.gz"
sudo install -m 0755 agentforge /usr/local/bin/agentforge
sudo install -m 0755 agentforge-sidecar /usr/local/bin/agentforge-sidecar

agentforge --version
agentforge --help
agentforge-sidecar --help
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
$Version = "v1.2.3"
$Target = "windows-x86_64"
$InstallDir = "$env:LOCALAPPDATA\Programs\AgentForge"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Invoke-WebRequest -Uri "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/$Version/agentforge-$Version-$Target.zip" -OutFile "agentforge.zip"
Invoke-WebRequest -Uri "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/download/$Version/SHA256SUMS" -OutFile "SHA256SUMS"
Get-FileHash .\agentforge.zip -Algorithm SHA256
Expand-Archive .\agentforge.zip -DestinationPath $InstallDir -Force

setx PATH "$env:PATH;$InstallDir"
& "$InstallDir\agentforge.exe" --version
& "$InstallDir\agentforge.exe" --help
& "$InstallDir\agentforge-sidecar.exe" --help
```

Compare the printed SHA-256 value with the matching line in `SHA256SUMS` before
using the binaries.

Success looks like:

- `agentforge.exe --version` prints the CLI version and exits with code `0`.
- `agentforge.exe --help` lists commands such as `auth`, `config`, and
  `agents`.
- `agentforge-sidecar.exe --help` prints the join-command guidance instead of
  trying to connect to the server.

Next, open a new PowerShell window so `PATH` is refreshed, then connect:

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
