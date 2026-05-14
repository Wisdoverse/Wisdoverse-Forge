# Getting Started

This guide gets a clean local machine onto the current Rust-first runtime path.
It is the recommended path for a first self-host trial and for developers who
need the full platform running locally.

## Choose a Local Mode

| Mode          | Command Path                           | Includes                                                                      | Best For                                                    |
| ------------- | -------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Full platform | `make quickstart-local`, `npm run dev` | Rust API, Rust orchestrator, Temporal, PostgreSQL, Redis, NATS, Vite frontend | First local trial, daily development, workflow verification |
| API-only loop | `npm run server`, `npm run dev`        | Rust API and Vite frontend only                                               | UI or API work that does not need orchestrator or Temporal  |

The full platform mode is the default recommendation.

## Prerequisites

- Node.js 24+
- Docker and Docker Compose v2
- Make
- Git

## 1. Clone the Repository

```bash
git clone https://github.com/Wisdoverse/Wisdoverse-Forge.git wisdoverse-forge
cd wisdoverse-forge
npm install
```

## 2. Start the Backend Stack

```bash
make quickstart-local
```

This checks the required tools, creates `docker/.env` from
`docker/.env.example` when it is missing, fills local development secrets,
starts the backend stack in detached mode, and waits for the runtime health
checks to pass.

The generated local values include:

- `POSTGRES_PASSWORD`
- `REDIS_PASSWORD`
- `JWT_SECRET`
- `MCP_TOKEN`
- `API_KEY_SALT`
- `LLM_ENCRYPTION_KEY`
- NATS callout passwords, private seeds, and public keys

This starts the default backend services for local development:

- Rust API on `:4003`
- Rust orchestrator on `:4010`
- Temporal on `:7233` with UI on `:8233`
- PostgreSQL, Redis, and NATS

The bootstrap script does not overwrite non-empty values. If an existing
`docker/.env` looks like an external or production deployment, it reports the
missing local values and leaves the file unchanged.

Use these separate commands when you want to control each step yourself:

```bash
make bootstrap-local
make dev-d
make local-health
```

Use `make dev-logs` to follow backend logs. Use `make dev` instead of
`make dev-d` when you want Compose logs attached to the current terminal.

## 3. Start the Frontend

Open a second terminal in the repository root and run:

```bash
npm run dev
```

The Vite frontend will be available on `http://localhost:4002`.

On a fresh database, open the frontend and complete the web setup/register flow
to create the initial owner account. The example environment files do not
define a default application administrator.

## 4. First Use Path

After registering, open `/start` in the app. It shows the same minimum path as
an in-product checklist. Use the direct provider path first; it avoids Container
CLI image and OAuth setup while still proving the application can run useful AI
work.

1. Open Settings -> Teams and create a team.
2. Open Settings -> Projects and create a project under that team.
3. Open Settings -> Providers, choose Add Provider, select your LLM provider,
   enter a model and API key, save, then use Test to verify the key.
   For Ollama, configure `OLLAMA_BASE_URL` on the backend and leave API Key
   empty.
4. Select the project in the sidebar.
5. Open Agents -> New Agent, choose Provider + Prompt, select the provider and
   model, then create the agent.
6. Open the agent and use the History chat to send a short prompt.

To route tasks through the board, create or select a task group for the selected
project. The New Agent dialog can create a default task group when a project is
selected. Then open Tasks and use Add Task on the board.

Use Container CLI agents only after the base UI path works. Pull public CLI
images with `make update-agents`, or build Claude locally with
`make build-agent CLI_TOOL=claude` after accepting the vendor terms. Container
CLI agents also need matching user OAuth credentials or deployment-level
fallback keys such as `CONTAINER_OPENAI_API_KEY`, `CONTAINER_ANTHROPIC_API_KEY`,
or `CONTAINER_GOOGLE_API_KEY`.

## 5. Verify the Environment

```bash
make local-health
```

Expected results:

- Rust API returns a healthy liveness response.
- Rust API readiness returns `status:"ready"`.
- Rust orchestrator returns `status:"healthy"`.
- Temporal cluster health passes.
- NATS monitoring health returns `status:"ok"`.

Then open:

- `http://localhost:4002` for the frontend
- `http://localhost:8233` for Temporal UI

You can rerun the bootstrap checker at any time without changing secrets:

```bash
scripts/bootstrap-local.sh --check
```

You can include the frontend in the check after starting Vite:

```bash
scripts/check-local-runtime.sh --frontend
```

## 6. Optional Lightweight API Loop

If you only need the Rust API and frontend, you can skip Compose and run:

```bash
npm run server
npm run dev
```

This loop does not provide the orchestrator, Temporal, or the full backend integration stack. Use it only when those dependencies are not part of your change.

## Common Issues

- Missing passwords or tokens in `docker/.env` will cause Compose startup failures.
- If ports `4002`, `4003`, `4010`, `5432`, `6379`, `7233`, or `8233` are already in use, stop the conflicting process or change the mapped ports.
- If Docker is unavailable, internal MCP and container-backed agent features will not work.
- If NATS key generation fails, install the `nk` CLI from the NATS toolchain or
  allow Docker to pull `natsio/nats-box:latest`, then rerun `make bootstrap-local`.

## Next Reads

- [Configuration Guide](configuration.md) for runtime variables.
- [Architecture Overview](../architecture/overview.md) for service boundaries and flow diagrams.
- [Deployment Guide](deployment.md) for production-oriented Compose usage.
