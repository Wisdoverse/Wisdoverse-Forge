# Self-fix loop (human-gated)

The self-fix loop lets a Wisdoverse Forge agent propose a code change to **this
repository** as a GitHub **draft pull request**, which an operator reviews and
merges from inside the app. The agent never pushes to your default branch and
never merges anything itself — every change lands only after a person clicks
**Approve & merge**, and the server independently re-checks the change before it
merges.

This guide is for an operator setting it up for the first time.

## What you need first

Before any self-fix task can open a pull request, the deployment needs a GitHub
App that is allowed to open and merge pull requests on your repository:

- A **GitHub App** installed on the target repository with these repository
  permissions: **Contents: Read and write** and **Pull requests: Read and
  write**.
- The App's **App ID** and the **Installation ID** for the install on your repo.
- The App's **private key** (a `.pem` file you download from the App settings).
- `LLM_ENCRYPTION_KEY` set (already required in production) — the private key is
  stored encrypted at rest.

You do **not** give the agent a GitHub token. The server holds the App
credentials and mints a short-lived installation token only when it opens or
merges a PR.

## Configure the server

Set these four environment variables on the Rust API service (see
`docs/guides/configuration.md` for the full table). All four are required
together — if only some are set, the server refuses to start so the loop can
never boot half-wired:

```bash
GITHUB_APP_ID=123456
GITHUB_APP_INSTALLATION_ID=987654
# Base64-encoded contents of the .pem (env-safe single line). Raw PEM also works.
GITHUB_APP_PRIVATE_KEY=$(base64 -w0 your-app.private-key.pem)
GITHUB_APP_REPO=your-org/your-repo
```

Optionally override where the server does its private clone work (a server-owned
scratch directory, never inside an agent's `/workspace`):

```bash
# Default: /tmp/agentforge-selffix
SELF_FIX_WORK_DIR=/var/lib/agentforge/selffix
```

Restart the API service after setting these.

## The happy path

1. **Create a self-fix task.** A task marked as a self-fix task targets this
   repository's code. An agent works it like any other task, editing files in
   its `/workspace`.
2. **The server opens a draft PR.** When the work is done, the server freezes the
   agent's container, copies the changed files onto a clean clone of your default
   branch in its own scratch directory, validates them, force-pushes a
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
