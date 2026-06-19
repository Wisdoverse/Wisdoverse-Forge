# Getting Started

This guide gets Wisdoverse Forge running in a browser on your machine. It is the
recommended path for a first self-host trial and for contributors who need the
full product running locally.

## Choose a Local Path

| Path                       | Commands                               | Best for                                                         |
| -------------------------- | -------------------------------------- | ---------------------------------------------------------------- |
| Full local product         | `make quickstart-local`, `npm run dev` | First trial, daily development, and checking the browser flows   |
| Lightweight developer loop | `npm run server`, `npm run dev`        | UI or API work that does not need the full background work stack |

Start with the full local product unless you already know you only need the
lightweight developer loop.

## What you need first

- Node.js 24+
- Docker and Docker Compose v2
- Make
- Git

You do not need to build the Platform CLI for the first browser-based trial. Use
the CLI later when you want this computer to join Forge as a managed agent or
when you want to operate a remote Forge instance from a terminal. See
[CLI Platform Support](cli-platform-support.md) for Linux, macOS, and Windows
operator paths.

## 1. Clone the Repository

```bash
git clone https://github.com/Wisdoverse/Wisdoverse-Forge.git wisdoverse-forge
cd wisdoverse-forge
npm install
```

## 2. Start the Backend Stack

Run this in the repository root:

```bash
make quickstart-local
```

Success looks like this:

- The command finishes without an error.
- The final health check says the local stack is ready.
- Docker is running the backend services in the background.

What this command does for you:

- Checks required tools.
- Creates `docker/.env` from `docker/.env.example` when it is missing.
- Fills safe local development secrets.
- Starts the local backend services.
- Waits until the local health check passes.

The generated local values include database passwords, app signing secrets,
encryption keys, and local messaging credentials. The bootstrap script does not
overwrite non-empty values. If an existing `docker/.env` looks like an external
or production deployment, it reports the missing local values and leaves the
file unchanged.

Use these separate commands only when you want to control each step yourself:

```bash
make bootstrap-local
make dev-d
make local-health
```

Use `make dev-logs` to follow backend logs. Use `make dev` instead of
`make dev-d` when you want Compose logs attached to the current terminal.

## 3. Start the Browser App

Open a second terminal in the repository root and run:

```bash
npm run dev
```

The Vite frontend will be available on `http://localhost:4002`.

Open that address in your browser. On a fresh database, register the first
account. That first account becomes the owner account for this local Forge
workspace. The example environment files do not create a default administrator.

## 4. First Use Path

After registering, use the in-product setup checklist. It appears on first use
and can be shown again later from Settings.

1. Create a team and project.
   - Open **Settings -> Teams** to create a team if one is missing.
   - Open **Settings -> Projects** to create a project inside that team.
2. Add an AI service.
   - Open **Settings -> AI services**.
   - Choose **Add AI service**.
   - Pick the service you use, paste the service access key, save it, then
     choose **Check connection**.
   - Success looks like: the service shows **Ready**.
3. Create the first agent.
   - Open **Agents -> New Agent**.
   - Choose **Simple chat agent** for the fastest first proof.
   - Use **Project files** later when you want an agent to edit shared files.
   - Use **This computer** later when work must stay on your local machine.
4. Create one small task.
   - Select the project in the left menu.
   - Open **Tasks** and choose **Add Task**.
   - Start with one clear request, such as "summarize this project setup" or
     "review the README for confusing steps."
5. Check the result.
   - Open the task detail panel.
   - Use **Work**, **Result**, **Context**, and **Updates** to understand what
     happened.
   - When the result is useful, save reusable instructions from the review flow.

The New Agent dialog can create a starter place where tasks wait when a project
is selected.

### Add file-working agents later

Use **Project files** agents after **Settings -> Where agents work** shows the
work location and tool setup are ready. Public work tools can be prepared with:

```bash
make update-agents
```

Claude must be built locally after accepting the vendor terms:

```bash
make build-agent CLI_TOOL=claude
```

File-working agents also need matching tool account access or deployment-level
fallback keys such as `CONTAINER_OPENAI_API_KEY`,
`CONTAINER_ANTHROPIC_API_KEY`, or `CONTAINER_GOOGLE_API_KEY`.

To use this computer as a managed agent, install the Platform CLI and
`agentforge-sidecar`, then follow
[Host CLI Agent Enrollment](../runbooks/host-cli-agent-enrollment.md). This path
is for operators who want a remote Forge instance to assign tasks to a command
line tool running on their own workstation.

## 5. Verify the Environment

```bash
make local-health
```

Expected results:

- The local app health check passes.
- The browser app is reachable.
- Background work services are reachable.
- Local messaging and workflow checks pass.

Then open:

- `http://localhost:4002` for the frontend
- `http://localhost:8233` for the workflow admin UI

You can rerun the bootstrap checker at any time without changing secrets:

```bash
scripts/bootstrap-local.sh --check
```

You can include the frontend in the check after starting Vite:

```bash
scripts/check-local-runtime.sh --frontend
```

## 6. Optional Lightweight API Loop

If you only need the API server and browser app, you can skip the full Docker
stack and run:

```bash
npm run server
npm run dev
```

This loop does not provide the full background work stack. Use it only when
those services are not part of your change.

## Common Issues

- **The stack does not start:** rerun `make quickstart-local` and read the first
  failed check. Missing values in `docker/.env` are the most common cause.
- **A port is already in use:** stop the conflicting process or change the mapped
  port. The local path commonly uses `4002`, `4003`, `4010`, `5432`, `6379`,
  `7233`, and `8233`.
- **Docker is not running:** start Docker, then rerun `make quickstart-local`.
  Agents that edit project files need Docker.
- **Local messaging key generation fails:** install the `nk` CLI from the NATS
  toolchain or allow Docker to pull `natsio/nats-box:latest`, then rerun
  `make bootstrap-local`.

## Next Reads

- [Configuration Guide](configuration.md) for settings and environment values.
- [Task Workflow Guide](task-workflow.md) for the browser task lifecycle.
- [CLI Platform Support](cli-platform-support.md) for Platform CLI and local
  sidecar installation expectations.
- [Architecture Overview](../architecture/overview.md) for service boundaries
  and flow diagrams.
- [Deployment Guide](deployment.md) for production-oriented Compose usage.
