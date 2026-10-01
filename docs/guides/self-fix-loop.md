# Self-fix loop (human-gated)

The self-fix loop lets a Wisdoverse Forge agent propose a code change to **the
configured repository** as a GitHub **draft pull request**, which an operator
reviews and merges from inside the app. The agent never pushes to your default branch and
never merges anything itself — every change lands only after a person clicks
**Approve & merge**, and the server independently re-checks the change before it
merges.

This guide is for an operator setting it up for the first time.

**Current repository boundary.** This path uses one deployment-level
`GITHUB_APP_REPO` configuration. The server discovers that repository's default
branch from GitHub and uses its observed revision for the rebuild and PR base;
it does not assume `main`. General repository selection remains planned work
in the [Product Roadmap](../../ROADMAP.md). For a pilot, follow the
[Product Validation Guide](product-validation.md) within this existing boundary.

## What you need first

Before any self-fix task can open a pull request, the deployment needs a GitHub
App that is allowed to open and merge pull requests on your repository:

- A **GitHub App** installed on the target repository with these repository
  permissions: **Contents: Read and write** and **Pull requests: Read and
  write**. In-platform review also needs **Checks: Read-only** and squash merging
  enabled on the repository. Branch protection and required reviews still apply.
- The App's **App ID** and the **Installation ID** for the install on your repo.
- The App's **private key** (a `.pem` file you download from the App settings).
- `LLM_ENCRYPTION_KEY` set for production deployments as an independent
  production requirement. It is not used to encrypt this GitHub App key.

You do **not** give the agent a GitHub token. The server holds the App
credentials in memory and uses the private key to sign App requests and mint
short-lived installation tokens. The operator must manage the key through the
deployment's secret store and keep it out of logs.

## Configure the server

Set these four environment variables on the Rust API service (see
`docs/guides/configuration.md` for the full table). All four are required
together — if only some are set, the server refuses to start so the loop can
never boot half-wired:

```dotenv
GITHUB_APP_ID=123456
GITHUB_APP_INSTALLATION_ID=987654
GITHUB_APP_PRIVATE_KEY=<single-line-base64-encoded-private-key-pem>
GITHUB_APP_REPO=your-org/your-repo
```

Set the App ID, Installation ID, private key, and `GITHUB_APP_REPO` together.
The private key may be raw PEM or a single-line base64 value. For Linux, create
a base64 value without writing it to the terminal with
`base64 -w0 your-app.private-key.pem > app-key.base64`. On macOS, use
`base64 < your-app.private-key.pem | tr -d '\n' > app-key.base64`. In Windows
PowerShell, use
`[Convert]::ToBase64String([IO.File]::ReadAllBytes('your-app.private-key.pem')) | Set-Content -NoNewline app-key.base64`.
Store the file securely, enter its single-line value through your deployment's
secret management process, then remove the temporary copy. Do not enable shell
tracing or log secret values.

Optionally override where the server does its private clone work (a server-owned
scratch directory, never inside an agent's `/workspace`):

```bash
# Default: /tmp/agentforge-selffix
SELF_FIX_WORK_DIR=/var/lib/agentforge/selffix
```

Restart the API service after setting these.

For Compose, put all four `GITHUB_APP_*` values in `docker/.env`; set the
`GITHUB_APP_PRIVATE_KEY` value to the single-line base64 literal itself, not a
shell `$(...)` expression. Protect `docker/.env` as a secret. For an external
profile deployment, `make deploy-server` rebuilds and recreates only the API
service. For other profiles, recreate `agentforge-server` with the same Compose
files and profile used by the running stack, for example:

```bash
docker compose --env-file docker/.env -f docker/compose.yml -f docker/compose.prod.yml --profile prod up -d --no-deps --force-recreate agentforge-server
```

## Check the repository before creating work

Sign in as a **platform administrator**, open **Settings**, choose **Show team
and project setup**, then **Maintenance repository** in **People and projects**
(`/settings/maintenance-repository`).
The page checks the connection when opened. Success shows the approved
repository, its actual default branch, full starting version and last-check
time. Preparation access and review/merge prerequisites appear separately.
Next, confirm the agent setup and agree the required checks and reviewer before
assigning a task.

Being an organization owner does not grant platform-administrator access.
Other users see guidance to ask an administrator, without fetching repository
details. This page checks the server's existing configuration; the person
running Forge still connects the GitHub App using the setup steps above.

Choose **Check connection** after changing repository settings. A check has a
30-second deadline and actionable recovery guidance for missing configuration,
permissions, unavailable repositories or API/network failures. Refresh removes
the previous result immediately. Leaving the page, changing accounts or an
updated sign-in state revoking administrator access cancels pending display
work. A server permission refusal also clears the result. Access is checked on
each request; this screen does not poll for external permission changes. A
result records a point in time and does not run CI or reserve a branch revision.

For the same read-only check from a terminal, obtain your existing login token,
set `FORGE_TOKEN` locally to that token, then run:

```bash
curl --fail-with-body \
  -H "Authorization: Bearer ${FORGE_TOKEN}" \
  http://localhost:4003/api/v1/self-fix/repository
```

For a production deployment, replace `http://localhost:4003` with its API URL.
The response identifies the configured repository, its current default branch
and starting revision, and the installation's verification prerequisites:

```json
{
  "ok": true,
  "data": {
    "repository": "your-org/your-repo",
    "defaultBranch": "develop",
    "baseSha": "0123456789abcdef0123456789abcdef0123456789",
    "contentsWrite": true,
    "pullRequestsWrite": true,
    "checksRead": true,
    "squashMergeAllowed": true
  }
}
```

Success means repository access, write permissions and a usable default-branch
revision were observed. `checksRead` and `squashMergeAllowed` must also be true
for the current in-platform verification/merge path. This check does not run CI,
approve a change, test branch-protection compatibility or prove task acceptance.
Agree the task's required checks and reviewer before assigning it. The endpoint
opens no PR and changes no repository files; it only reads repository facts and
obtains a short-lived installation token held by the server.

Archived/disabled repositories, missing write permissions, inaccessible
repositories and missing default-branch revisions produce actionable errors.
After changing installation permissions, restart the API or wait for its cached
installation token to expire before checking again. Repository metadata is read
again for every PR attempt; the preflight response does not reserve a revision.
If a retry finds an existing PR targeting a different base branch, it refuses to
reuse that PR. Inspect the PR on GitHub and create a new task for the current
default branch; branch/head changes need another human review.

## The happy path

1. **Create a self-fix task.** A task marked as a self-fix task targets the
   configured repository's code. An agent works it like any other task, editing
   files in its `/workspace`.
2. **The server opens a draft PR.** When the work is done, the server freezes the
   agent's container, copies the changed files onto a clean clone of the
   configured repository's observed default-branch revision in its own scratch
   directory, validates them, force-pushes a
   deterministic `agent/<task-id>` branch, and opens a **draft** pull request.
   Nothing is merged.
3. **Review it.** Open the task and switch to the **Review** tab. You see the PR
   link, a one-shot CI-check status, and an **Approve & merge** button. Use the
   diff link to read the change in GitHub.
4. **Approve.** When CI is green and the change is not sensitive, **Approve &
   merge** is enabled. Clicking it asks the server to merge. The server re-checks
   — non-sensitive, CI still green, head unmoved — and squash-merges at the exact
   reviewed commit, then posts an audit comment naming you as the approver.

## What success looks like

After a successful approve, the PR is merged on GitHub, the task's review status
reads **Merged**, and the audit comment records who approved it and that no
safety check was bypassed.

## Status and troubleshooting

| What you see                                          | What it means                                                                      | What to do                                                                                  |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| **Review** tab missing                                | The task is not a self-fix task                                                    | Only self-fix tasks expose the Review tab                                                   |
| "No pull request has been opened yet"                 | The Bridge has not opened a PR                                                     | Confirm the task finished and the GitHub App is configured                                  |
| **Approve** disabled, "CI checks not confirmed green" | CI has not reported success on the PR head                                         | Wait for checks to finish, then press **Refresh**                                           |
| **Approve** disabled, "Touches a sensitive path"      | The change edits a protected area (auth, migrations, CI, the self-fix code itself) | A maintainer must review and merge it manually on GitHub; in-platform merge is hard-refused |
| Approve fails with "GitHub not configured"            | The four `GITHUB_APP_*` variables are not all set                                  | Set them (see above) and restart the API                                                    |
| Approve fails after CI went red or the head moved     | The server re-verified at merge time and refused                                   | Re-review the PR; nothing was merged                                                        |

## Safety model

- The agent never runs `git` against your repository and never holds a GitHub
  token. All privileged git runs server-side on a clean clone.
- **Sensitive paths are server-side hard-refused** and can never be merged from
  inside the app, regardless of what a client sends. They are routed to a human
  maintainer on GitHub instead. See `docs/security/self-fix-loop.md`.
- Merges are **expected-head**: the server merges only the exact commit it
  re-verified, so a push between review and approval cannot sneak in.
- Auto-dispatch, auto-deploy, and auto-merge are intentionally **not** part of
  this loop — every merge is a deliberate human action.

## Validation boundary

Automated repository-setup coverage uses a local GitHub HTTP mock, a local Git
origin and a disposable PostgreSQL database. It covers non-`main` branches,
branch-name encoding, permission/configuration failures, default-branch changes,
clone cleanup and the existing merge guards. The HTTP test checks unauthenticated
access and a revoked platform admin against the real router.

The UI tests cover malformed responses, safe error messages, bounded requests,
partial verification permissions, role revocation and late responses from a
previous account. With Node.js dependencies installed, run from the repo root:

```bash
npm run test:unit -- tests/unit/shared/selfFixRepositoryApi.test.ts \
  tests/unit/app/MaintenanceRepositorySection.test.tsx \
  tests/unit/app/SettingsLayout.test.tsx
```

The browser spec uses real login and the running Rust API. It checks keyboard
refresh, exact returned repository facts and permissions, guided failure or
non-admin access, and mobile navigation/layout. Start a local API and browser
app, configure the GitHub App if testing the connected path, set `E2E_PASSWORD`
locally for `dev@example.com`, and use the canonical browser runner:

```bash
npm run test:e2e -- maintenance-repository.spec.ts
```

Set `BASE_URL`, `E2E_API_BASE_URL` or `PLAYWRIGHT_CHROMIUM_PATH` locally when
using non-default ports or an installed system Chromium. The spec does not
mock application authentication or create repository changes. A local GitHub
test service proves only that integration, not a real installation.

With Rust, Git and the protobuf compiler available, run the checks from `rust/`:

```bash
SELF_FIX_IT=1 cargo test -p agentforge-api \
  --test github_repository_setup_test --test github_app_client_test \
  --test self_fix_bridge_test --test self_fix_merge_test
# DATABASE_URL must point to a disposable PostgreSQL instance whose user can create databases.
cargo test -p agentforge-api \
  --test self_fix_repository_route_test --test self_fix_routes_test
```

These tests do not establish readiness for a real GitHub installation or a
production deployment. Before pilot use, run the preflight on the selected
repository, exercise one reviewed task with its actual CI and branch protection,
and record the revision, environment and human verdict using
[Product Validation](product-validation.md).
