# Create a project from a git repository

Wisdoverse Forge can create a project and clone a git repository into it for you,
so an agent starts work in a directory that already has the code. You paste an
HTTPS git URL when you create the project; the platform clones the repository in
the background and shows you a clone status badge. When it reaches **Ready**, the
agent's terminal opens in a working copy of the repo.

This guide is for the person creating the project. The operator setup an admin
does once (the egress firewall) is at the end and links the runbook.

## Before you start

- You can create projects in your workspace (the **Create Project** button is
  visible in the team's project list).
- The repository is reachable over **HTTPS**. SSH URLs (`git@…`) are not
  supported in this version.
- For a **private** repository, a matching git credential exists in
  **Settings → Git Credentials** for the repository's host (for example a GitHub
  personal access token for a `github.com` repo). You do **not** put the token in
  the URL — the platform matches a stored credential to the repository's host and
  injects it securely. A URL that contains a username or token
  (`https://user:token@host/…`) is rejected.
- The operator has applied the clone egress firewall if your host can reach an
  internal network (see [Operator setup](#operator-setup-egress-firewall)).

## Create the project

1. Open your team's project list and choose **Create Project**.
2. Enter a **project name**.
3. In **Git repository URL**, paste the HTTPS clone URL, for example:

   ```text
   https://github.com/your-org/your-repo.git
   ```

4. The form shows the **workspace directory** the repo will be cloned into
   (read-only — derived from the project name, e.g. `your-repo`). You cannot type
   a host path; the platform chooses a safe directory name for you.
5. Choose **Create**.

The project is created immediately with a clone status of **Queued**; you do not
have to wait on the form.

## What the clone status means

The project card and the project detail show a clone status badge:

| Status      | Meaning                                                                                       |
| ----------- | --------------------------------------------------------------------------------------------- |
| **Queued**  | The clone is scheduled and will start shortly. Nothing is on disk yet.                        |
| **Cloning** | The repository is being cloned in a short-lived, locked-down container.                       |
| **Ready**   | The repository is cloned into the project's workspace directory. The agent can start work.    |
| **Failed**  | The clone did not finish. The badge shows a short, secret-free reason and a **Retry** button. |

The badge updates live over the realtime connection. If you refresh the page, the
status is re-read from the server, so you never miss the final result.

## What gets cloned (v1 limits)

- The repository's **default branch**, with **full history**.
- HTTPS token auth for **GitHub and GitLab** only.
- **No** branch / tag / commit selection — you get the default branch.
- **No** Git LFS objects, **no** submodules, **no** sparse/partial checkout.
- After the clone reaches **Ready**, the agent owns the directory and its `.git`.
  The platform does **not** re-sync, pull, or re-clone — it is a one-shot copy.
  To point a project at a different repository, create a new project.

## When a clone fails

1. Read the reason on the **Failed** badge. It is deliberately short and never
   contains a token or credential. Common reasons:
   - **Authentication** — no matching credential, or the token lacks access. Add
     or fix the credential in **Settings → Git Credentials**, then **Retry**.
   - **Not found** — the URL is wrong or the repo is private with no credential.
   - **Network** — the host could not be reached. Check the URL and try again.
   - **Repository too large / timed out** — the repo exceeds the configured size
     or time limit (see the escape hatch below).
2. Choose **Retry**. This starts a fresh clone attempt for the same project.
   Retry is available only from a **Failed** state; an in-progress or already
   **Ready** clone cannot be re-queued.

### Escape hatch: clone it yourself in the agent terminal

If a repository is too large or too slow for the platform's clone limits, you do
not have to abandon the project. Create the project **without** a git URL (or let
the clone fail), open the agent terminal, and clone the repository yourself into
the workspace directory:

```bash
cd /workspace
git clone https://github.com/your-org/your-repo.git
```

From the agent terminal you own the network and the disk budget, so a large or
unusual repository (LFS, submodules, a specific branch) works exactly as it would
on your own machine.

## Security model (how your credentials and repos are protected)

The clone runs in a disposable, least-privilege container — never as a server
process and never with access to other projects. Four layers protect against a
hostile or mistyped repository URL reaching your internal network (SSRF):

1. **In-app URL gate.** Only `https://` URLs with a real public host are
   accepted. Loopback, private (RFC1918), link-local, the cloud metadata address
   (`169.254.169.254`), and `.local` hosts are rejected when you create the
   project. A URL containing an embedded credential is rejected.
2. **Dedicated egress network.** The clone container runs on its own Docker
   network that the internal services (database, NATS, the API) are not on.
3. **Deploy-layer firewall.** The operator blocks the clone network from reaching
   private ranges and the metadata endpoint at the packet level — the layer that
   closes DNS-rebinding and stock-bridge routing. See
   [the egress firewall runbook](../runbooks/clone-egress-firewall.md).
4. **In-container pre-resolve.** Before `git clone` runs, the container re-checks
   that the host does not resolve to an internal address and refuses if it does.

Credentials are **host-matched** (the token for the repository's host, not "the
latest token"), **mounted** into the container as a short-lived read-only file
(never passed as an environment variable or on the command line), **never
logged**, and **scrubbed from error messages** before any failure reason is
stored or shown. The raw clone output stays on the server and is never surfaced
to you.

Clones are **tenant-isolated**: the container mounts only a single per-clone
staging directory inside your own workspace, never the projects root or a sibling
project, and a clone only ever lands in the creator's workspace, never across
organizations.

## Operator setup (egress firewall)

If your deployment lets users create projects from a git URL **and** the host can
reach an internal network or a cloud metadata service, you must apply the clone
egress firewall once. This is the deploy-layer half of the SSRF defense above;
the application already does its three in-app/in-container halves.

Follow [`docs/runbooks/clone-egress-firewall.md`](../runbooks/clone-egress-firewall.md).

The clone feature and its env vars are configured in
[`docs/guides/configuration.md`](configuration.md) under
**Project Git Clone Variables**. To disable cloning entirely (for example on an
air-gapped deployment), set `PROJECT_CLONE_WORKER_ENABLED=false`.

## Build the clone image

The clone runs in a dedicated minimal image, `agentforge-clone`, separate from
the agent images. Build it with:

```bash
make build-clone
```

`make build-agent-all` also builds it alongside the agent images. The image
contains only `git`, CA certificates, and the clone entrypoint — no Node, Python,
Docker CLI, sidecar, or agent harness — to keep the attack surface minimal for
cloning untrusted repositories.
