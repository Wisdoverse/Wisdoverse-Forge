# BOE Deploy Pipeline

This runbook covers the Jenkins pipeline that deploys the `DEV` branch to the BOE
test environment with a cold snapshot and an automatic rollback. Read it in
order the first time. After the first supervised deploy, routine work needs only
[Routine deploy](#routine-deploy).

The pipeline is defined by `Jenkinsfile`, `scripts/deploy-release.mjs` and
`docker/Dockerfile.deploy-runner`.

## What the pipeline does

1. Jenkins reads `Jenkinsfile` from `main`. It fetches `main` and `DEV`, and
   it refuses to continue unless the `DEV` commit is already on `main`.
2. It copies that commit into a release directory with `git archive`. The copy
   holds no secrets. It also places the environment file from a Jenkins
   credential into a private directory.
3. A short-lived runner container builds the server, orchestrator and frontend
   images for that commit. The running stack is not touched yet.
4. The deploy step takes a lock and runs a preflight check. Preflight changes
   nothing. If any check fails, the deploy stops here.

   Among other things, preflight checks that the running containers belong to
   this deployment. For each named volume of the new release it accepts:
   - the running container mounts that same volume at that path, or
   - the running container mounts nothing at that path (the new release adds
     the volume; Compose creates it in this project at start).

   It refuses when the running container mounts a different volume at that
   path, or when a container belongs to another Compose project.

5. The runner stops every service of the stack. It snapshots the four
   databases, the JetStream and uploads volumes, and the web root.
6. It migrates the databases, starts the new release and checks the backend.
   Then it switches the frontend and checks the served page.
7. If every check passes, the new release becomes current. If any step after
   the services stopped fails, the runner restores the previous release from
   the snapshot and checks it again.

All SQL runs in one short-lived PostgreSQL client container per run, named
`<project>-deploy-<attempt>-sql`. It is removed when the run ends, on every
path. Each step is written to a journal before it runs. If the runner, Jenkins or the
host dies halfway, `recover` reads the journal and finishes the job safely.

## Prerequisites

Complete every item before the first deploy.

### Jenkins

- Jenkins runs in a container that has the Docker CLI and the host Docker
  socket. `JENKINS_HOME` must be a bind mount or a named volume. The pipeline
  finds the host path of `JENKINS_HOME` from the container's own mounts.
- The job name may only contain letters, digits, `.`, `_` and `-`.
- Exclude `<jenkins-home>/forge-deploy/` from Jenkins backups and artifact
  archiving. The `private/` directory below it holds the environment file and
  the snapshots.

### Jenkins job settings

| Setting              | Value                                                                                                                                                                      |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Job type             | Pipeline                                                                                                                                                                   |
| Definition           | Pipeline script from SCM, Git, this repository                                                                                                                             |
| Branch               | `main` (not `DEV`). The pipeline deploys `DEV` itself.                                                                                                                     |
| Script path          | `Jenkinsfile`                                                                                                                                                              |
| Lightweight checkout | Allowed. It is used only to read `Jenkinsfile`.                                                                                                                            |
| Build triggers       | None until the first supervised deploy has succeeded. Then enable SCM polling. The pipeline checks out `DEV` with polling, so polling watches `DEV`. Do not add a webhook. |

### Environment file credential

Create a Jenkins credential of type **Secret file** with the ID
`<jenkins-credential-id>`. Pass this ID as `ENV_FILE_CREDENTIAL_ID` when you run
the job. The default ID is `forge-deploy-env`.

The file is a Compose environment file, the same as `docker/.env` for
`make prod-ext`. It must contain everything `docker/compose.yml` and
`docker/compose.external.yml` need, plus these variables:

Every variable that locates state, a network, a lock or a probe target is
REQUIRED. The tool uses no defaults for them: several Compose defaults are
the names of an existing deployment, and on a shared host a missing variable
would point at it. A missing one is a refusal (exit 12) that lists the names.

| Variable                                                                                                                      | Meaning                                                                                                                                                                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `COMPOSE_PROJECT_NAME`, `CONTAINER_NAME_PREFIX`                                                                               | Compose project and container name prefix of this deployment                                                                                                                                                              |
| `DATABASE_URL`                                                                                                                | API database, `postgres://<api-role>:<password>@<db-host>:5432/<api-db>`                                                                                                                                                  |
| `ORCHESTRATOR_DATABASE_URL`                                                                                                   | Orchestrator database, with its own role                                                                                                                                                                                  |
| `DB_HOST`, `DB_PORT`                                                                                                          | Database server for Temporal                                                                                                                                                                                              |
| `TEMPORAL_DB_USER`, `TEMPORAL_DB_PASSWORD`, `TEMPORAL_DB_NAME`                                                                | Temporal role and database; the visibility database is `<name>_visibility`                                                                                                                                                |
| `EXTERNAL_NETWORK`                                                                                                            | Docker network where the database host name resolves                                                                                                                                                                      |
| `CONTAINER_NETWORK`                                                                                                           | Docker network of this deployment's agent containers                                                                                                                                                                      |
| `WEBROOT`                                                                                                                     | Absolute host path the web server serves, for example `/srv/forge-deploy/www`                                                                                                                                             |
| `AGENTFORGE_WORKSPACE_ROOT`, `OAUTH_MOUNT_DIR`                                                                                | Absolute host paths of the agent workspaces and the OAuth staging directory                                                                                                                                               |
| `DEPLOY_LOCK_DIR`                                                                                                             | Absolute lock directory, for example `/var/lock/forge-deploy`                                                                                                                                                             |
| `APP_URL`                                                                                                                     | URL used to check the served `index.html`, for example `https://forge.example.com`                                                                                                                                        |
| `BIND_ADDRESS`, `AGENTFORGE_PORT`, `ORCHESTRATOR_PORT`, `NATS_PORT`, `NATS_MONITOR_PORT`, `TEMPORAL_PORT`, `TEMPORAL_UI_PORT` | Published address and ports; the health probes use them. The compose file publishes NATS and Temporal on every interface; to keep one on loopback, give the port as `127.0.0.1:<port>` (the probes then use that address) |

Optional: `DEPLOY_PG_CLIENT_IMAGE` (PostgreSQL client image, default
`postgres:18-alpine`), `WEBROOT_OWNER_UID`/`WEBROOT_OWNER_GID` (owner of
written frontend files, default the owner of `WEBROOT`).

Rules for this file:

- Do not set `VERSION`. The pipeline appends `VERSION=<commit>`.
- Do not use `${...}` references inside the database URLs. The runner reads
  those values directly.
- Each database must be owned by the role in its URL. That role needs
  `CREATEDB`, because snapshots are database copies.
- A database copy does not carry database-level settings, so preflight refuses
  a database with a non-default ACL (`GRANT CONNECT ...` on the database), a
  database or role setting (`ALTER DATABASE/ROLE ... SET`), a connection limit,
  a non-default tablespace, or replication slots, subscriptions or
  publications. Object-level grants inside the database (schemas, tables) are
  copied and need nothing.

### `DEV` branch rules

`DEV` is a deploy pointer. It only moves to commits that are already on `main`.
In the GitHub repository settings, create two rulesets for `DEV`:

| Ruleset | Rules                               | Bypass                    |
| ------- | ----------------------------------- | ------------------------- |
| A       | Block force pushes. Block deletion. | Nobody                    |
| B       | Restrict updates.                   | Repository administrators |

Do not add pull-request or linear-history rules to `DEV`. Commits are reviewed
when they enter `main`.

Trust boundary: Jenkins runs repository code with the host Docker socket, so
every merged line has host privileges. Review on `main` and the human
promotion to `DEV` are the boundary. The rulesets keep bots and accidents out.
They do not protect against a compromised administrator.

### Database and Temporal checks (read-only)

Every owning role must be able to connect to the maintenance database
`postgres`. The snapshot, the fence and every rename run there. For each of
the three roles, put `PGHOST`, `PGPORT`, `PGUSER` and `PGPASSWORD` in a
mode-600 file, then run:

```bash
sudo docker run --rm --pull never --network <external-network> --env-file <role.env> \
  postgres:18-alpine psql -X -At -d postgres -c 'select current_user'
```

Success: the role name prints. There are two different failures, with two
different remedies:

- `permission denied for database "postgres"`: the role lacks the `CONNECT`
  privilege. A database administrator runs `GRANT CONNECT ON DATABASE postgres
TO <role>` (or to `PUBLIC`).
- `database "postgres" is not currently accepting connections`: connections to
  `postgres` are disabled for everyone, and `GRANT CONNECT` does not change
  that. Check from the role's own database:

  ```bash
  sudo docker run --rm --pull never --network <external-network> --env-file <role.env> \
    postgres:18-alpine psql -X -At -d <own-database> \
    -c "select datallowconn from pg_database where datname = 'postgres'"
  ```

  `f` means disabled. A superuser re-enables it from any other database with
  `ALTER DATABASE postgres WITH ALLOW_CONNECTIONS true`.

Fix either before the first deploy. Without access to `postgres`, preflight
refuses and nothing is changed.

The health probe runs the Temporal CLI inside the running Temporal container.
Check that the old container has it:

```bash
docker exec <container-prefix>-temporal temporal --version
```

Success: a version prints. If the command is not found, the deploy refuses
before anything stops (`Temporal is not SERVING; its healthy state cannot be
recorded`). Before the first deploy, as a supervised step, recreate only the
Temporal container from the image the release pins
(`temporalio/auto-setup:1.26`). Rehearse that step on the rehearsal stack first.

### Host directories and images

Run on the host as an administrator:

```bash
sudo mkdir -p /var/lock/forge-deploy
test -d /srv/forge-deploy/www && echo "web root exists"
docker image inspect postgres:18-alpine nats:2.12.7-alpine temporalio/auto-setup:1.26 --format '{{.RepoTags}}'
```

Replace `/srv/forge-deploy/www` with your `WEBROOT`. The runner never pulls
images. Every image must already be present locally. Success: the three
image tags print and the web root exists.

## First deploy

The first deploy is supervised. The environment owner must give an explicit go.

### 1. Prepare: release root, private root, staged inputs, runner image

Preparation always comes first: `baseline` needs everything this step creates.

Run the Jenkins job with `ACTION=stage`. It only prepares:

- it archives the `DEV` commit into `release/releases/<commit>/`
- it stages the env file and the runner reference in `private/staged/`
- it builds the runner image under a temporary tag, creates
  `forge-deploy-runner:<commit>` only if that tag does not exist yet (otherwise
  it adopts the existing image), and records the runner's image id in
  `private/staged/runner-image`. Every later run starts the runner by that id.

It does not run the deploy script against the stack and changes nothing in the
running stack.

Success: the console shows `Release commit: <commit>` and ends with `Staged
<commit>: release directory, inputs and runner image are ready; nothing was
deployed.` Note the commit. Run `ACTION=stage` again whenever you need to
prepare again. Never use `deploy` to repeat preparation.

How inputs become active: a deploy stages new inputs only. When the script
writes a new attempt's intent, one rename turns `private/staged/` into
`private/inputs/<attempt>/`, and the journal names that generation. The first
`baseline` does the same for its own inputs. "Active inputs" are always the
generation that the journal points to: the unfinished attempt's, otherwise the
current release's. `state.json` also carries them as `activeGeneration` and
`activeRunner`. `ACTION=status` and `ACTION=recover` use only those. A
generation that nothing points to (for example after a crash right after the
rename) is never used, and retention removes it later.

### 2. Record the baseline

The baseline is the release that is running now, deployed by hand. Recording it
lets the first deploy roll back to it.

#### 2a. Find the Compose files the running stack was built from

A version tag does not identify that source. It may even be a commit that was
never pushed. Ask Docker instead (read-only):

```bash
docker inspect <container-prefix>-server --format \
  '{{index .Config.Labels "com.docker.compose.project.working_dir"}} {{index .Config.Labels "com.docker.compose.project.config_files"}}'
```

The first value is the directory Compose ran in. The second lists the exact
files it used, for example `<dir>/compose.yml,<dir>/compose.external.yml`.

#### 2b. Hand that directory to `baseline`

`baseline` reads `release/releases/baseline/docker/compose.yml` and
`release/releases/baseline/docker/compose.external.yml`, and the files they
bind-mount next to them, such as `nats.conf`. Copy the directory from 2a into
that place:

```bash
ROOT=<jenkins-home-host-path>/forge-deploy/<job-name>
sudo mkdir -p "$ROOT/release/releases/baseline/docker"
sudo cp -a <working-dir-from-2a>/. "$ROOT/release/releases/baseline/docker/"
```

If the source is a commit you still have, `git archive <commit> | sudo tar -x
-C "$ROOT/release/releases/baseline"` gives the same layout.

If 2a lists more files than those two (an extra override file), merge what
they add into the baseline copy of `compose.external.yml` (see 2d). That copy
is a scratch file in the release root, not repository code.

#### 2c. Run `baseline`

```bash
REV=<release-commit>                               # "Release commit:" line of step 1
IMG=$(sudo cat "$ROOT/private/staged/runner-image")  # the runner's image id
WEB=/srv/forge-deploy/www                          # your WEBROOT
LOCK=/var/lock/forge-deploy                         # your DEPLOY_LOCK_DIR

sudo docker run --rm --network host \
  --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
  --mount "type=bind,src=$ROOT/release,dst=$ROOT/release" \
  --mount "type=bind,src=$ROOT/private,dst=$ROOT/private" \
  --mount "type=bind,src=$WEB,dst=$WEB" \
  --mount "type=bind,src=$LOCK,dst=$LOCK" \
  -e TMPDIR="$ROOT/release/tmp" \
  -e FORGE_RELEASE_ROOT="$ROOT/release" \
  -e FORGE_PRIVATE_ROOT="$ROOT/private" \
  -e FORGE_RUNNER_IMAGE="$IMG" \
  -e COMPOSE_PARALLEL_LIMIT=1 \
  "$IMG" \
  node "$ROOT/release/releases/$REV/scripts/deploy-release.mjs" baseline
```

What `baseline` does:

1. It tags the running server and orchestrator images as
   `:baseline-<compose-project>`. It never moves a tag that already points at
   another image; it refuses instead.
2. It renders the baseline Compose files with the staged env file.
3. It compares that rendering with the running containers: image ids,
   environment variable names, mounts (named-volume identity, bind source,
   read-only flag), networks, full port bindings and health checks.

Success: `baseline recorded: the next deploy can roll back to the running
release`. `status` then shows `current release: baseline` and an active
generation. A refused baseline changes nothing except the `:baseline-<compose-project>` image
tags, so you can run it as often as you need. Once a baseline is recorded it
refuses to run again.

#### 2d. Reconcile hand edits

The running containers may differ from the historical source, for example
because of an override file that no longer exists. Every refusal line names the
service, the property, what the rendering has and what the running container
has:

```text
refused: nats: published ports: rendered has 0.0.0.0:4222->4222/tcp that the container lacks; running has 127.0.0.1:4222->4222/tcp that the rendering lacks
refused: agentforge-server: environment names: rendered FOO_FLAG; running without them
refused: nats: health check command: rendered 52 chars, sha256 1a2b3c4d5e6f (...); running 61 chars, sha256 6f5e4d3c2b1a (inspect locally: docker inspect ...)
```

Method: edit the baseline's scratch copy of the file that DEFINES the property,
directly. Do not add an override file: Compose merges lists such as `ports`, so
an override keeps the original entry next to the new one. In the first example,
the `nats` ports are defined in `release/releases/baseline/docker/compose.yml`.
Change that entry:

```yaml
ports:
  - '127.0.0.1:${NATS_PORT:-4222}:4222' # was '${NATS_PORT:-4222}:4222'
```

Then run 2c again, and repeat until it prints `baseline recorded`.

The report shows image ids, variable names (never values), mount keys, network
names and port bindings. A command (the health check) is shown only as its
length and a short sha256. It can contain credentials, so compare it locally
with the `docker inspect` command printed in the line.

If `baseline` was interrupted (for example the runner was killed), first find
out whether it recorded anything. Run `status` from a host shell (see
[Emergency path](#emergency-path-host-shell)):

- It shows `current release: baseline`: the baseline is recorded. Do not run
  it again. Continue with step 3.
- It refuses with `no staged or active inputs` (exit 12), or shows
  `current release:  (none - run baseline first)`: nothing was
  recorded. Run the Jenkins job with `ACTION=stage` (it stages the inputs
  again), then run 2c again.

Use the same `docker run` line with `status` instead of `baseline` to read the
state from a host shell. See also [Emergency path](#emergency-path-host-shell).

### 3. Take extra database dumps

For the first deploy only, keep an independent dump of each database. Do this
in addition to the pipeline's snapshot. Repeat for the API, orchestrator,
Temporal and Temporal visibility databases. Each database uses its own role.

```bash
sudo install -m 600 /dev/null "$ROOT/private/pg-dump.env"
sudoedit "$ROOT/private/pg-dump.env"   # PGHOST=, PGUSER=, PGPASSWORD=, PGDATABASE= for one database
sudo mkdir -p -m 700 "$ROOT/private/dumps"
sudo docker run --rm --pull never --network <external-network> \
  --env-file "$ROOT/private/pg-dump.env" \
  -v "$ROOT/private/dumps:/dumps" \
  postgres:18-alpine sh -c 'pg_dump -Fc -f "/dumps/$PGDATABASE.dump"'
sudo rm "$ROOT/private/pg-dump.env"
```

Success: `ls -l "$ROOT/private/dumps"` shows four non-empty `.dump` files.

### 4. Go / no-go checklist

Continue only if every item is true:

- [ ] Rehearsals A to F passed on the isolated rehearsal stack, and their results
      are recorded under [Recorded runs](#recorded-runs).
- [ ] `status` shows `current release: baseline`.
- [ ] Four database dumps exist and are not empty.
- [ ] No agent containers are running. Preflight refuses while any are running.
- [ ] Users know about the maintenance window. The stack is stopped during
      the snapshot, and the new release is reachable before it is verified.
- [ ] The environment owner gave an explicit go.

### 5. Deploy

Promote the commit to `DEV`. Then run the Jenkins job with `ACTION=deploy`.

Point of no return: after the journal reaches `snapshotted`, the runner
migrates the databases in place. From then on, the only way back is the
snapshot. The automatic rollback uses it. A manual restore from the dumps is the
last resort.

Success: the console ends with `deployed <commit>-<env-hash>`, and the job exits 0. Then
do the post-checks: run one Temporal workflow, start one agent per Container
CLI, and watch the stack for 30 minutes. Record the result under
[Recorded runs](#recorded-runs). Enable SCM polling on the job afterwards.

## Routine deploy

1. Make sure the commit is on `main`. Move `DEV` to it.
2. Run the job with `ACTION=deploy`, or wait for SCM polling.

Image tags are global to the Docker host. Two deployments on one host (for
example a rehearsal next to the live stack) share revision images
(`agentforge-server:<commit>` and so on) by design. `build` adopts a
revision tag that already exists instead of rebuilding it, and no command
ever moves an existing tag to another image. Baseline images are scoped per
deployment: `agentforge-server:baseline-<compose-project>`.

Success: exit code 0 and `deployed <commit>-<env-hash>`. If `DEV` already points at the
current release, the job prints `already the current release` and exits 0. A
commit that already has a release record is never rebuilt, so the images that
were verified for it stay exactly the ones its record names. If a tag of a
recorded release points at another image, `build` fails with `tag ... no longer
points at the recorded image`. Re-tag the recorded image id, or investigate who
moved the tag.

## What a rollback restores

A rollback runs automatically when any step fails after the services stop. It
restores:

- the four databases, by renaming the snapshot copy back to the original name.
  The failed databases are kept as `<db>__failed_<attempt>` with connections
  disabled. A later successful deploy removes them.
- the JetStream volume and the uploads volume.
- the previous release's images and rendered Compose configuration.
- the previous web root content, `index.html` last.

Then it checks the previous release with the same checks as a deploy. Success:
exit code 10 and `finished in phase rolled_back`.

It also removes the agent containers that the new release created, running or
stopped. The selection is exact: the product's own container names (`agentforge-agent-<id>`,
`agentforge-devenv-<id>`, `agentforge-clone-<id>`) with the matching label, on
this deployment's agent network, created after the deploy started.

### What a rollback does not restore

- Anything users or agents wrote while the new release was running. The new
  release is reachable from the moment it starts until verification ends. A
  rollback discards everything written in that window. There is no maintenance
  mode yet.
- Effects outside the stack: object storage writes, emails, outbound calls and
  Redis keys.
- The agent workspace root and the OAuth staging directory. These are not part
  of the snapshot.
- The compliance export volume. It is not part of the snapshot.
- Extra frontend asset files. New hashed asset files stay in the web root after
  a rollback. They are harmless, because the restored `index.html` does not
  reference them.

Migrations are forward-only. An old binary refuses a newer schema, which is
why the rollback restores the databases instead of starting the old binary on
the migrated schema.

## Rotating a credential

A release is a revision plus the content of its env file. Changing only the env
file (for example a rotated password) and running `ACTION=deploy` for the same
revision deploys a new release. A deploy is a no-op only when the revision and
the env file are both unchanged.

Rotate with overlap. A rollback starts the previous release from its rendered
configuration, which contains the previous credentials:

1. Add the new credential where the system allows two at once (a second API
   token, a second user). Keep the old one valid.
2. Put the new value into the Jenkins file credential.
3. Run `ACTION=deploy`. Success: `deployed <commit>-<env-hash>`.
4. Verify, and wait until a rollback to the previous release is no longer
   needed.
5. Only then revoke the old credential.

If the system holds only one value (for example a PostgreSQL role password),
there is no overlap. A rollback after the change would start the previous
release with the old value. Change the value immediately before the deploy, and
keep the old value ready.

If a credential was revoked too early, a rollback or `recover` now fails to
connect. The preferred fix is to restore the old credential temporarily, run
`ACTION=recover`, and repeat the rotation from step 1.

If the old credential cannot be restored, put the new value into the two files
that carry it, then recover:

1. the env file of the active input generation:
   `private/inputs/<generation>/deploy.env` (`status`: `active inputs:
generation <generation>`)
2. the saved rendered configuration of the release a rollback starts:
   `private/configs/<previous-release-id>.json` (`status`: `attempt: <attempt>
<previous-release-id> -> <target>`; without an unfinished attempt, the
   `previous release:` line)

Editing only the env file does not help: a rollback starts the previous
release from its saved rendered configuration, which embeds the previous
credential.

Replace the value without printing it, without putting it in argv or in the
shell history (bash). The values are read with echo disabled and reach the
runner only through the environment:

```bash
ROOT=<jenkins-home-host-path>/forge-deploy/<job-name>
GEN=<generation>
PREV=<previous-release-id>
IMG=$(sudo sed -n 's/^  "activeRunnerImage": "\(sha256:[0-9a-f]*\)",\{0,1\}$/\1/p' "$ROOT/private/state.json")
sudo tee "$ROOT/private/.replace-credential.js" >/dev/null <<'SCRIPT'
const fs = require('fs'); const o = process.env.OLD; const n = process.env.NEW
if (!o || !n) { console.error('OLD and NEW must be set'); process.exit(2) }
for (const f of process.argv.slice(2)) {
  const text = fs.readFileSync(f, 'utf8'); let count = 0
  const swap = (v) => { const parts = v.split(o); count += parts.length - 1; return parts.join(n) }
  const walk = (v) => typeof v === 'string' ? swap(v) : Array.isArray(v) ? v.map(walk) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, walk(x)])) : v
  const out = f.endsWith('.json') ? JSON.stringify(walk(JSON.parse(text)), null, 2) : swap(text)
  fs.writeFileSync(f + '.tmp', out, { mode: 0o600 }); fs.renameSync(f + '.tmp', f)
  console.log(`${f.split('/').pop()}: replaced ${count} occurrence(s)`)
}
SCRIPT
read -rs -p 'old value: ' OLD; echo; read -rs -p 'new value: ' NEW; echo
export OLD NEW
sudo --preserve-env=OLD,NEW docker run --rm --network none -e OLD -e NEW \
  --mount "type=bind,src=$ROOT/private,dst=/p" "$IMG" \
  node /p/.replace-credential.js "/p/inputs/$GEN/deploy.env" "/p/configs/$PREV.json"
unset OLD NEW
sudo rm "$ROOT/private/.replace-credential.js"
```

Success: each file reports at least one replacement. If a file reports `0
occurrence(s)`, stop. The value appears there in another form, for example a
`$` written as `$$` in the rendered configuration. Restore the old credential
instead.

Then run `status`. It shows `note: the active env file differs from its
recorded hash (deployment identity unchanged)`. Then run `ACTION=recover`.
Recovery accepts a changed secret. It refuses a changed deployment identity:
the project name, anything that locates state (database hosts, ports, database
or role names, networks, web root, workspace and OAuth roots, storage
provider, path, endpoint, bucket, region), lock directory or published ports.

## Recovery required

`recovery_required` means a rollback step failed. The runner stopped every
service of the stack on purpose. It is safer to stay down than to run on a
half-restored state. Every later deploy refuses until `recover` succeeds.

1. Do not start services by hand.
2. Run the job with `ACTION=status`. If Jenkins itself cannot load the
   pipeline, use the [Emergency path](#emergency-path-host-shell). `status`
   prints:
   - `attempt:` the attempt id and the two releases involved
   - `phase:` and the step it was interrupted at
   - one line per database with the original name and OID, the snapshot name
     and OID, and the name the failed copy gets (`<db>__failed_<attempt>`)
   - the volume and web-root archive paths under `snapshots/<attempt>/`
   - `helper containers: names start with ...`
   - `error:` the step that failed and why
3. Repair the cause. Common causes: the database server is unreachable, a
   session of another role keeps a database open (a rename then fails with
   `is being accessed by other users`), a snapshot database was renamed or
   dropped, the disk is full, or an image is missing locally.
4. Run the job with `ACTION=recover`. Recovery first removes every helper
   container of this project (label `forge.deploy.project=<project>`) and
   confirms that they are gone. If that removal fails, recovery stops. To see
   the helpers yourself, run
   `docker ps -a --filter label=forge.deploy.project=<project>`.

Success: exit code 0, and `status` shows `phase: rolled_back`. Recovery
decides each database rename from the actual database names and ids. Running
it twice is safe.

If `recover` keeps failing with `names and OIDs match no known swap state`, a
database was changed by hand. Stop and escalate. Compare the database names
with the snapshot names that `status` shows. For the first deploy, the dumps
from step 3 are the last resort.

If the runner, Jenkins or the host died during a deploy, the next deploy
refuses with `attempt ... is unfinished`. Run `ACTION=recover`. Recovery either
finishes a committed deploy or rolls it back.

## Finalize failed (exit 14 or 15)

Do not roll back by hand in either case.

Exit 15 means the commit write was attempted, but it is not known whether it
became durable. Nothing was stopped and nothing was rolled back. Repair the
cause, then run `ACTION=recover`. Recovery reads the journal on disk. If the
commit is there, recovery treats the release as committed. If it is not,
recovery rolls back.

Exit 14 means the new release is committed and was left running. Only
finishing work failed, for example a journal write because the disk is full.
Nothing was stopped and nothing was rolled back. Repair the cause, then run
`ACTION=recover`. For a committed attempt, recover makes sure the committed
release is running (it starts it if needed), checks the backend, and closes
the attempt. Success: exit 0 and `phase: done`.

## Emergency path (host shell)

Use this when Jenkins, Git or the registry is unavailable. It runs the same
runner image, release directory and environment file that the last deploy
prepared, with no fetch, build or credential change.

```bash
ROOT=<jenkins-home-host-path>/forge-deploy/<job-name>
# The journal names the active runner (commit and image id) as plain fields:
REV=$(sudo sed -n 's/^  "activeRunner": "\([0-9a-f]*\)",\{0,1\}$/\1/p' "$ROOT/private/state.json")
IMG=$(sudo sed -n 's/^  "activeRunnerImage": "\(sha256:[0-9a-f]*\)",\{0,1\}$/\1/p' "$ROOT/private/state.json")
WEB=/srv/forge-deploy/www                          # your WEBROOT
LOCK=/var/lock/forge-deploy                         # your DEPLOY_LOCK_DIR
ACTION=status                                       # or: recover

sudo docker run --rm --network host \
  --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
  --mount "type=bind,src=$ROOT/release,dst=$ROOT/release" \
  --mount "type=bind,src=$ROOT/private,dst=$ROOT/private" \
  --mount "type=bind,src=$WEB,dst=$WEB" \
  --mount "type=bind,src=$LOCK,dst=$LOCK" \
  -e TMPDIR="$ROOT/release/tmp" \
  -e FORGE_RELEASE_ROOT="$ROOT/release" \
  -e FORGE_PRIVATE_ROOT="$ROOT/private" \
  -e FORGE_RUNNER_IMAGE="$IMG" \
  "$IMG" \
  node "$ROOT/release/releases/$REV/scripts/deploy-release.mjs" "$ACTION" --revision "$REV"
```

Success: the same output and exit code as the Jenkins job. If the state file
cannot be read or is missing a field its phase needs, both commands exit 11
with `RECOVERY REQUIRED: the deploy state file cannot be read`. If the active
env file is not the one the attempt started with, recover exits 11 and says
so; restore that file. Do not deploy then. Inspect `$ROOT/private/state.json`
and `state.json.tmp`, repair the file, then run `status` again.

## Preparing from a host shell

This does what the Jenkins `Prepare release` stage does, for a host without
Jenkins (for example a rehearsal). Prerequisites: a clone of this repository
with `origin/main` fetched, and the env file for this deployment.

```bash
ROOT=<deploy-root>                                 # holds release/ and private/
REPO=<repository-clone>
REV=$(git -C "$REPO" rev-parse --verify '<commit>^{commit}')
git -C "$REPO" merge-base --is-ancestor "$REV" origin/main || echo "NOT on main: stop"
mkdir -p "$ROOT/release/releases" "$ROOT/release/tmp" "$ROOT/private" && chmod 700 "$ROOT/private"
DEST="$ROOT/release/releases/$REV"
if [ ! -d "$DEST" ]; then
  mkdir -p "$DEST.partial" && git -C "$REPO" archive --format=tar "$REV" | tar -x -C "$DEST.partial" \
    && mv "$DEST.partial" "$DEST"
fi
# Runner image: temporary tag; the shared tag only if absent; record the image id.
SHARED="forge-deploy-runner:$REV"
WANT=$(docker image inspect --format '{{.Id}}' "$SHARED" 2>/dev/null || true)
if [ -z "$WANT" ]; then
  TEMP="$SHARED-build-$(date +%s)-$$"
  docker build -t "$TEMP" - < "$DEST/docker/Dockerfile.deploy-runner"
  WANT=$(docker image inspect --format '{{.Id}}' "$SHARED" 2>/dev/null || true)
  if [ -z "$WANT" ]; then WANT=$(docker image inspect --format '{{.Id}}' "$TEMP"); docker tag "$WANT" "$SHARED"; fi
  docker image rm "$TEMP" >/dev/null
fi
[ "$(docker image inspect --format '{{.Id}}' "$SHARED")" = "$WANT" ] || echo "runner tag changed: stop"
# Staged inputs: one directory, swapped in with a rename.
(umask 077 && rm -rf "$ROOT/private/staged.partial" && mkdir "$ROOT/private/staged.partial" \
  && cp <env-file> "$ROOT/private/staged.partial/deploy.env" \
  && printf '\nVERSION=%s\n' "$REV" >> "$ROOT/private/staged.partial/deploy.env" \
  && printf '%s\n' "$REV" > "$ROOT/private/staged.partial/runner" \
  && printf '%s\n' "$WANT" > "$ROOT/private/staged.partial/runner-image" \
  && rm -rf "$ROOT/private/staged" && mv "$ROOT/private/staged.partial" "$ROOT/private/staged")
```

Success: `private/staged/` holds `deploy.env`, `runner` and `runner-image`, and
nothing printed `stop`. Then run the runner commands of this runbook with
`IMG=$(cat "$ROOT/private/staged/runner-image")`.

## Exit codes

| Code | Meaning                                                                                                            | What to do next                                                                                        |
| ---- | ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| 0    | Success, already current, or nothing to recover                                                                    | Nothing                                                                                                |
| 1    | Usage or unexpected error before any change, or the run finished but its SQL helper container could not be removed | Read the error. For a helper, remove it with `docker rm -f -v <name>` (`status` shows the name prefix) |
| 10   | Deploy failed and the previous release is restored and verified                                                    | Fix the cause on `main`, promote again                                                                 |
| 11   | Recovery required (services stopped), or the state file cannot be read                                             | Follow [Recovery required](#recovery-required)                                                         |
| 12   | Preflight refused, nothing changed                                                                                 | Read each `refused:` line and fix it                                                                   |
| 13   | Another deploy, recover or baseline holds the lock                                                                 | Wait for it to finish                                                                                  |
| 14   | The result was reached (a committed release keeps running) but finishing work failed                               | Follow [Finalize failed](#finalize-failed-exit-14-or-15)                                               |
| 15   | The commit write was attempted and may not be durable; nothing was stopped or rolled back                          | Follow [Finalize failed](#finalize-failed-exit-14-or-15)                                               |

The Jenkins job maps these codes:

| Exit code | Build result | Message                                                                                                                       |
| --------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| 0         | SUCCESS      | `<action>: success`, or `Release <commit> was already current; nothing was deployed.`                                         |
| 14        | UNSTABLE     | `Release is committed and running; run the recover action to finish housekeeping; do NOT roll back.`                          |
| 15        | UNSTABLE     | `Commit outcome unknown; services were left running. Run the recover action (it decides from the journal); do NOT roll back.` |
| 10        | FAILURE      | `Deploy failed: the previous release was restored and verified. ...`                                                          |
| 11        | FAILURE      | `RECOVERY REQUIRED: read the error above, repair the cause, then run this job with ACTION=recover ...`                        |
| 12        | FAILURE      | `Preflight refused; nothing was changed. ...`                                                                                 |
| 13        | FAILURE      | `Another deploy, recover or baseline holds the lock. ...`                                                                     |
| other     | FAILURE      | `<action> exited with code <n>; see the output above ...`                                                                     |

## Rehearsals

Before the snapshot, preflight records what the running release answers: each
health endpoint as `healthy`, `answers` or `unsupported` (HTTP 404 from an
older build), and its streams and consumers. A rollback must reach that state
again. If an endpoint times out or errors, or the stream listing fails, the
deploy refuses before anything stops. Fix the running release first.

Run every rehearsal on an isolated second stack before the first live deploy.
Record each result under [Recorded runs](#recorded-runs).

### Rehearsal stack

Prerequisites, all different from the live stack:

- a second Jenkins job, `<rehearsal-job>`, with its own file credential
- in that env file: `COMPOSE_PROJECT_NAME`, `CONTAINER_NAME_PREFIX`, every host
  port, `EXTERNAL_NETWORK`, `CONTAINER_NETWORK`, `WEBROOT`, `DEPLOY_LOCK_DIR`,
  `AGENTFORGE_WORKSPACE_ROOT`, `OAUTH_MOUNT_DIR`, `LLM_ENCRYPTION_KEY` and
  `JWT_SECRET`, plus `STORAGE_PROVIDER=local`, mail off,
  `CLI_IMAGE_AUTO_UPDATE_ENABLED=false`
- a scratch PostgreSQL 18 on the rehearsal `EXTERNAL_NETWORK`. Its three roles
  must have the live attributes: not superuser, `CREATEDB`, owner of their
  databases. Load it from fresh `pg_dump -Fc` dumps of the four live databases
  (see [step 3](#3-take-extra-database-dumps)), with `pg_restore --no-owner
--role=<owning-role> -d <db>`.
- a scratch Redis on the same network

Bring the rehearsal stack up from the baseline images and config (the
live-stack steps 1 and 2 with the rehearsal job). Then seed state that must
survive a rollback, and keep the values:

```bash
export P=<rehearsal-project>
# a running Temporal workflow, an unacknowledged JetStream message and sequence values
curl -fsS "http://127.0.0.1:<rehearsal-nats-monitor-port>/jsz?streams=true" > before-jsz.json
docker run --rm --network <rehearsal-external-network> --env-file <rehearsal-pg.env> postgres:18-alpine \
  psql -At -c "select sequencename, last_value from pg_sequences order by 1" > before-seq.txt
```

`<rehearsal-pg.env>` is a mode-600 file with `PGHOST`, `PGUSER`, `PGPASSWORD`
and `PGDATABASE` for the scratch server.

Run `status` from a host shell while a rehearsal runs. Use the
[Emergency path](#emergency-path-host-shell) line with the rehearsal `ROOT`.

### Fresh roots for every rehearsal, and teardown

Reloading the dumps does not reset the deploy state: `state.json` still names
a current release, so `baseline` refuses. Give every rehearsal its own release
root, private root and lock. A new rehearsal job name gives new roots
(`<jenkins-home>/forge-deploy/<rehearsal-job-N>/`). Also set a new
`DEPLOY_LOCK_DIR` (or `COMPOSE_PROJECT_NAME`) in its env file.

Tear a rehearsal down, on the rehearsal stack only. Use the saved rendered
configuration of its current release: it needs no env interpolation. `status`
shows the release id after `current release:`. Remove the service containers
before their networks. Delete the rehearsal roots only after everything else
succeeded:

```bash
REH=<jenkins-home-host-path>/forge-deploy/<rehearsal-job-N>
P=<rehearsal-project>
CFG="$REH/private/configs/<current-release-id>.json"
docker compose --project-name "$P" -f "$CFG" --profile external down -v \
  && docker ps -aq --filter label=forge.deploy.project="$P" | xargs -r docker rm -f -v \
  && docker rm -f -v <rehearsal-prefix>-pg <rehearsal-prefix>-redis \
  && docker network rm <rehearsal-external-network> <rehearsal-agent-network> \
  && sudo rm -rf "$REH" "<rehearsal-lock-dir>/$P.lock" \
  && echo "rehearsal removed"
```

Success: `rehearsal removed`. If any command fails, the chain stops and the
roots stay. They hold the journal and the rendered configurations you need to
finish. Look at what is left (`docker ps -a --filter
label=com.docker.compose.project=$P`, `docker network ls`), fix the cause, and
run the chain again. Never delete the roots first.

Then create the scratch database and the rehearsal stack again, and start at
[step 1](#1-prepare-release-root-private-root-staged-inputs-runner-image).

### A: baseline to main, success

Run `<rehearsal-job>` with `ACTION=deploy`. Success: exit 0, `deployed
<commit>`, `status` shows `phase: done`. Record the build and migration times
from the console.

### B: forced verification failure

Reset the rehearsal stack to the baseline (reload the dumps, run `baseline`).
Run with `ACTION=deploy` and `SIMULATE_VERIFY_FAILURE=true`. Success: exit 10,
`status` shows `phase: rolled_back` and `current release: baseline`. The
ledgers are back at their baseline length:

```bash
docker run --rm --network <rehearsal-external-network> --env-file <rehearsal-pg.env> postgres:18-alpine \
  psql -At -c "select count(*), max(version) from _sqlx_migrations"
```

The sequence values and the JetStream message from the seed step are unchanged.

### C: failure during the migration

Reset to the baseline. Start the deploy. When `status` shows
`phase: migrating`, kill the migration container:

```bash
docker kill "$(docker ps -q --filter label=forge.deploy.project=$P --filter name=-migrate)"
```

Success: the same result as B.

### D: failure during the rollback, then recover

Reset to the baseline. Run with `SIMULATE_VERIFY_FAILURE=true`. When `status`
shows `phase: snapshotted`, hold a superuser session open on the API database
so that its rename fails:

```bash
docker run -d --name <rehearsal-prefix>-blocker --network <rehearsal-external-network> \
  --env-file <rehearsal-superuser.env> -e PGDATABASE=<api-db> -e PGAPPNAME=blocker \
  postgres:18-alpine psql -c 'select pg_sleep(3600)'
```

Success: exit 11, `status` shows `phase: recovery_required` and the `error:`
line names the rename. A new `ACTION=deploy` refuses with
`attempt ... is unfinished`. End the session:

```bash
docker run --rm --network <rehearsal-external-network> --env-file <rehearsal-superuser.env> postgres:18-alpine \
  psql -c "select pg_terminate_backend(pid) from pg_stat_activity where application_name = 'blocker'"
docker rm -f <rehearsal-prefix>-blocker
```

Then run `ACTION=recover`. Success: exit 0 and `phase: rolled_back`.

Also kill the runner itself, once at each snapshot step and once between the
two renames. Use `docker kill <runner-container>` when `status` shows the step.
Then run `ACTION=recover`. Success every time: exit 0, `phase: rolled_back`,
and every database is either the original or its snapshot under its own name.

### E: a second release without migrations, then pruning

From the release of A, promote `main` plus one commit without migrations and
deploy. Success: exit 0. Then list the snapshot databases:

```bash
docker run --rm --network <rehearsal-external-network> --env-file <rehearsal-pg.env> postgres:18-alpine \
  psql -At -c "select datname from pg_database where datname like '%\\_\\_snap\\_%' or datname like '%\\_\\_failed\\_%'"
```

Success: only the snapshots of the last good attempt and of the baseline
attempt remain (`status` shows both ids under `protected snapshots`).

### F: once through the real Jenkins job

Run B through `<rehearsal-job>` with its own credential. This proves path
discovery, the mounts and the credential handling. Then search the console log
for every secret value of the rehearsal env file:

```bash
curl -fsS -u <jenkins-user>:<api-token> "https://jenkins.example.com/job/<rehearsal-job>/lastBuild/consoleText" > console.txt
grep -c -F -f <file-with-one-secret-per-line> console.txt   # must print 0
```

Not covered by the rehearsals, checked after the live deploy: external S3, the
external proxy serving the web root, and starting a real agent with image
signature verification.

## Recorded runs

None recorded yet. Evidence from the rehearsals and the first deploy is added
by a later pull request.
