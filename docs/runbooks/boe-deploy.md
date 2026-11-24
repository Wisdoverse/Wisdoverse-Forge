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

| Setting | Value |
| --- | --- |
| Job type | Pipeline |
| Definition | Pipeline script from SCM, Git, this repository |
| Branch | `main` (not `DEV`). The pipeline deploys `DEV` itself. |
| Script path | `Jenkinsfile` |
| Lightweight checkout | Allowed. It is used only to read `Jenkinsfile`. |
| Build triggers | None until the first supervised deploy has succeeded. Then enable SCM polling. The pipeline checks out `DEV` with polling, so polling watches `DEV`. Do not add a webhook. |

### Environment file credential

Create a Jenkins credential of type **Secret file** with the ID
`<jenkins-credential-id>`. Pass this ID as `ENV_FILE_CREDENTIAL_ID` when you run
the job. The default ID is `forge-deploy-env`.

The file is a Compose environment file, the same as `docker/.env` for
`make prod-ext`. It must contain everything `docker/compose.yml` and
`docker/compose.external.yml` need, plus these variables:

| Variable | Required | Meaning |
| --- | --- | --- |
| `COMPOSE_PROJECT_NAME` | yes | Compose project of the stack, for example `wisdoverse-forge` |
| `DATABASE_URL` | yes | API database, `postgres://<api-role>:<password>@<db-host>:5432/<api-db>` |
| `ORCHESTRATOR_DATABASE_URL` | yes | Orchestrator database, with its own role |
| `DB_HOST`, `DB_PORT` | yes | Database server for Temporal |
| `TEMPORAL_DB_USER`, `TEMPORAL_DB_PASSWORD` | yes | Temporal role. The databases are `TEMPORAL_DB_NAME` (default `temporal`) and `<name>_visibility`. |
| `EXTERNAL_NETWORK` | yes | Docker network where the database host name resolves |
| `WEBROOT` | yes | Absolute host path the web server serves, for example `/srv/forge-deploy/www` |
| `APP_URL` | yes | Public URL used to check the served `index.html`, for example `https://forge.example.com` |
| `DEPLOY_LOCK_DIR` | no | Lock directory. The default is `/var/lock/forge-deploy`. |
| `DEPLOY_PG_CLIENT_IMAGE` | no | PostgreSQL client image. The default is `postgres:18-alpine`. |
| `WEBROOT_OWNER_UID`, `WEBROOT_OWNER_GID` | no | Owner of written frontend files. The default is the owner of `WEBROOT`. |

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

| Ruleset | Rules | Bypass |
| --- | --- | --- |
| A | Block force pushes. Block deletion. | Nobody |
| B | Restrict updates. | Repository administrators |

Do not add pull-request or linear-history rules to `DEV`. Commits are reviewed
when they enter `main`.

Trust boundary: Jenkins runs repository code with the host Docker socket, so
every merged line has host privileges. Review on `main` and the human
promotion to `DEV` are the boundary. The rulesets keep bots and accidents out.
They do not protect against a compromised administrator.

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

### 1. Prepare the release directories

Run the Jenkins job once with `ACTION=deploy`. Before the baseline exists, a
deploy only prepares: it archives the `DEV` commit, places the environment
file, builds the runner image and the release images, and then refuses. It
changes nothing in the running stack. Success: the console shows
`Release commit: <commit>` and ends with `refused: no current release is
recorded: run baseline once before the first deploy` (exit code 12, so the
build shows as failed). Note the commit.

`ACTION=status` and `ACTION=recover` never prepare anything. They only reuse
the active inputs. A deploy writes the new env file and runner reference to a
staging place (`private/staged.env`, `release/staged-runner`). The script
makes them active (`private/deploy.env`, `release/last-runner`) only when it
starts a new attempt, or, the first time, when `baseline` runs. If an attempt
is unfinished, a deploy refuses and the active inputs stay exactly the ones
that attempt started with.

### 2. Record the baseline

The baseline is the release that is running now, deployed by hand. Recording it
lets the first deploy roll back to it.

Prerequisite: you know `<baseline-commit>`, the commit the running stack was
built from, and you have a clone of this repository on the host.

```bash
ROOT=<jenkins-home-host-path>/forge-deploy/<job-name>
REV=<release-commit>                               # "Release commit:" line of step 1
WEB=/srv/forge-deploy/www                          # your WEBROOT
LOCK=/var/lock/forge-deploy                         # your DEPLOY_LOCK_DIR

sudo mkdir -p "$ROOT/release/releases/baseline"
git -C <repository-clone> archive <baseline-commit> | sudo tar -x -C "$ROOT/release/releases/baseline"

sudo docker run --rm --network host \
  --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
  --mount "type=bind,src=$ROOT/release,dst=$ROOT/release" \
  --mount "type=bind,src=$ROOT/private,dst=$ROOT/private" \
  --mount "type=bind,src=$WEB,dst=$WEB" \
  --mount "type=bind,src=$LOCK,dst=$LOCK" \
  -e TMPDIR="$ROOT/release/tmp" \
  -e FORGE_RELEASE_ROOT="$ROOT/release" \
  -e FORGE_PRIVATE_ROOT="$ROOT/private" \
  -e FORGE_RUNNER_IMAGE="forge-deploy-runner:$REV" \
  "forge-deploy-runner:$REV" \
  node "$ROOT/release/releases/$REV/scripts/deploy-release.mjs" baseline
```

What `baseline` does: it tags the running server and orchestrator images as
`:baseline`. It renders the baseline commit's Compose files with the live
environment file. Then it compares that rendering with the running containers:
image ids, environment variable names, mounts, networks, published ports and
health checks.

Success: `baseline recorded: the next deploy can roll back to the running
release`. The first `baseline` also makes the staged env file and runner
reference active. If it prints `refused: ...`, the rendering does not describe the
running stack. Fix the baseline commit or the environment file, then run it
again. `baseline` runs only once.

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

Success: the console ends with `deployed <commit>`, and the job exits 0. Then
do the post-checks: run one Temporal workflow, start one agent per Container
CLI, and watch the stack for 30 minutes. Record the result under
[Recorded runs](#recorded-runs). Enable SCM polling on the job afterwards.

## Routine deploy

1. Make sure the commit is on `main`. Move `DEV` to it.
2. Run the job with `ACTION=deploy`, or wait for SCM polling.

Success: exit code 0 and `deployed <commit>`. If `DEV` already points at the
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

## Finalize failed (exit 14)

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
REV=$(cat "$ROOT/release/last-runner")
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
  -e FORGE_RUNNER_IMAGE="forge-deploy-runner:$REV" \
  "forge-deploy-runner:$REV" \
  node "$ROOT/release/releases/$REV/scripts/deploy-release.mjs" "$ACTION" --revision "$REV"
```

Success: the same output and exit code as the Jenkins job. If the state file
cannot be read or is missing a field its phase needs, both commands exit 11
with `RECOVERY REQUIRED: the deploy state file cannot be read`. If the active
env file is not the one the attempt started with, recover exits 11 and says
so; restore that file. Do not deploy then. Inspect `$ROOT/private/state.json`
and `state.json.tmp`, repair the file, then run `status` again.

## Exit codes

| Code | Meaning | What to do next |
| --- | --- | --- |
| 0 | Success, already current, or nothing to recover | Nothing |
| 1 | Usage or unexpected error before any change, or the run finished but its SQL helper container could not be removed | Read the error. For a helper, remove it with `docker rm -f -v <name>` (`status` shows the name prefix) |
| 10 | Deploy failed and the previous release is restored and verified | Fix the cause on `main`, promote again |
| 11 | Recovery required (services stopped), or the state file cannot be read | Follow [Recovery required](#recovery-required) |
| 12 | Preflight refused, nothing changed | Read each `refused:` line and fix it |
| 13 | Another deploy, recover or baseline holds the lock | Wait for it to finish |
| 14 | The result was reached (a committed release keeps running) but finishing work failed | Follow [Finalize failed](#finalize-failed-exit-14) |

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
