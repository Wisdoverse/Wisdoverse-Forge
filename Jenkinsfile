// Wisdoverse Forge deploy pipeline (issue #1319). Runbook: docs/runbooks/boe-deploy.md
//
// Job configuration (required, design A7): "Pipeline script from SCM" reading this Jenkinsfile
// from `main`. The pipeline checks out `DEV` itself (registered for SCM polling) and deploys it
// only after proving that the commit is already on `main`. Keep SCM polling disabled until the
// first supervised deploy has succeeded; enable it afterwards.
//
// ACTION=status and ACTION=recover never fetch, build or replace the env file: they use the
// ACTIVE inputs, which the journal names (state.json: activeGeneration, activeRunner), so a Git
// or registry outage cannot block a recovery. A deploy only stages new inputs in private/staged;
// the script turns them into an attempt's generation when it writes the intent.
//
// Nothing host-specific lives here: host paths are derived at run time from the Jenkins
// container's own mounts, the web root and lock directory come from the env-file credential.
// Every sh step starts with `#!/bin/sh` and `set -eu`: without a shebang Jenkins runs `sh -xe`,
// which would trace commands into the console.

pipeline {
  agent any

  options {
    skipDefaultCheckout()
    disableConcurrentBuilds()
    timeout(time: 120, unit: 'MINUTES')
    buildDiscarder(logRotator(numToKeepStr: '30'))
  }

  parameters {
    string(name: 'ENV_FILE_CREDENTIAL_ID', defaultValue: 'forge-deploy-env',
      description: 'Jenkins "Secret file" credential holding the deployment env file (deploy only)')
    choice(name: 'ACTION', choices: ['deploy', 'stage', 'recover', 'status'],
      description: 'deploy: release DEV; stage: only prepare release, inputs and runner (nothing deployed); recover: finish or roll back an interrupted attempt; status: print state')
    booleanParam(name: 'SIMULATE_VERIFY_FAILURE', defaultValue: false,
      description: 'Rehearsal only: fail verification after the new backend starts, forcing a rollback')
  }

  stages {
    stage('Locate') {
      steps {
        script {
          if (!(env.JOB_BASE_NAME ==~ /[A-Za-z0-9._-]+/)) {
            error('The job name may only contain letters, digits, dot, underscore and dash.')
          }
          env.FORGE_JOB_DIR = "forge-deploy/${env.JOB_BASE_NAME}"
          // The runner mounts directories at their HOST paths, so find where JENKINS_HOME lives on the host.
          env.FORGE_HOST_HOME = sh(returnStdout: true, script: '''#!/bin/sh
            set -eu
            docker inspect --format '{{range .Mounts}}{{if eq .Destination "'"$JENKINS_HOME"'"}}{{.Source}}{{end}}{{end}}' "$(hostname)"
          ''').trim()
          if (!(env.FORGE_HOST_HOME ==~ /\/[A-Za-z0-9._\/-]+/)) {
            error('Could not find the host path of JENKINS_HOME (absolute, no spaces or commas) from this container\'s mounts.')
          }
        }
      }
    }

    stage('Prepare release') {
      when { expression { params.ACTION == 'deploy' || params.ACTION == 'stage' } }
      steps {
        script {
          // An explicit checkout registers DEV for polling. Checking out runs nothing from the tree.
          def dev = checkout(poll: true, changelog: true, scm: [
            $class: 'GitSCM',
            branches: [[name: 'refs/heads/DEV']],
            userRemoteConfigs: scm.userRemoteConfigs,
            extensions: [[$class: 'CloneOption', shallow: false, noTags: true]],
          ])
          env.FORGE_REV = dev.GIT_COMMIT
        }
        sh '''#!/bin/sh
          set -eu
          git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main
          # Refuse before any file from the DEV tree is used (A7).
          if ! git merge-base --is-ancestor "$FORGE_REV" refs/remotes/origin/main; then
            echo "DEV ($FORGE_REV) is not on main; promote only commits that are already on main." >&2
            exit 1
          fi
          echo "Release commit: $FORGE_REV"
        '''
        withCredentials([file(credentialsId: params.ENV_FILE_CREDENTIAL_ID, variable: 'FORGE_ENV_SRC')]) {
          sh '''#!/bin/sh
            set -eu
            umask 077
            LOCAL="$JENKINS_HOME/$FORGE_JOB_DIR"
            mkdir -p "$LOCAL/release/releases" "$LOCAL/release/tmp" "$LOCAL/private"
            chmod 700 "$LOCAL/private"
            DEST="$LOCAL/release/releases/$FORGE_REV"
            if [ ! -d "$DEST" ]; then
              rm -rf "$DEST.partial"
              mkdir -p "$DEST.partial"
              git archive --format=tar -o "$DEST.tar" "$FORGE_REV"
              tar -x -C "$DEST.partial" -f "$DEST.tar"
              rm -f "$DEST.tar"
              mv "$DEST.partial" "$DEST"
            fi
            # Staged only: one directory with the env file and the runner reference. The script turns
            # it into an attempt's generation with one rename when it writes the intent, so an
            # unfinished attempt keeps the inputs it started with.
            rm -rf "$LOCAL/private/staged.partial"
            mkdir -m 700 "$LOCAL/private/staged.partial"
            cp "$FORGE_ENV_SRC" "$LOCAL/private/staged.partial/deploy.env"
            chmod 600 "$LOCAL/private/staged.partial/deploy.env"
            printf '\\nVERSION=%s\\n' "$FORGE_REV" >> "$LOCAL/private/staged.partial/deploy.env"
            printf '%s\\n' "$FORGE_REV" > "$LOCAL/private/staged.partial/runner"
            rm -rf "$LOCAL/private/staged"
            mv "$LOCAL/private/staged.partial" "$LOCAL/private/staged"
          '''
        }
        script { readPaths('staged/deploy.env') }
        // Round 7/8 reviews: tags are global on the host and immutable: first writer wins, with
        // provenance. Every check of the shared tag (its revision label and its id) and its creation
        // happen inside one hold of the host-wide image lock DEPLOY_LOCK_DIR/images.lock, the lock
        // the script takes for every tag. The lock is held by a helper container from the
        // PostgreSQL client image the deploy already requires (busybox flock), with the lock
        // directory mounted; it is released when the helper is removed. A failed inspection is a
        // refusal, never "absent". The runner is recorded by the id read back under the lock.
        sh '''#!/bin/sh
          set -eu
          LOCAL="$JENKINS_HOME/$FORGE_JOB_DIR"
          SHARED="forge-deploy-runner:$FORGE_REV"
          LABEL="org.opencontainers.image.revision"
          HOLDER="forge-image-lock-$BUILD_NUMBER-$$"
          refuse() { echo "$*; nothing was deployed." >&2; exit 12; }
          unlock() { docker rm -f "$HOLDER" >/dev/null 2>&1 || true; }
          trap unlock EXIT
          lock() {
            docker run -d --rm --pull never --network none --name "$HOLDER" \\
              --mount "type=bind,src=$FORGE_LOCK_DIR,dst=/lock" \\
              "$FORGE_PG_CLIENT_IMAGE" flock -x /lock/images.lock sh -c 'echo locked; exec sleep 900' >/dev/null
            i=0
            until docker logs "$HOLDER" 2>/dev/null | grep -qx locked; do
              i=$((i + 1))
              [ "$i" -lt 900 ] || { unlock; refuse "The image lock $FORGE_LOCK_DIR/images.lock was not taken within 900 s"; }
              sleep 1
            done
          }
          # 0: present (prints its id after checking the label), 1: absent; any other failure refuses.
          present() {
            if OUT=$(docker image inspect --format '{{.Id}}' "$SHARED" 2>&1); then
              [ "$(docker image inspect --format "{{index .Config.Labels \\"$LABEL\\"}}" "$SHARED")" = "$FORGE_REV" ] \\
                || refuse "$SHARED exists with another revision label; a release tag is immutable: investigate, never retag"
              WANT=$OUT
              return 0
            fi
            case "$OUT" in *"No such image"*|*"No such object"*) return 1 ;; esac
            refuse "Inspection of $SHARED failed"
          }
          lock
          if present; then
            unlock
            echo "Adopted the existing runner image $WANT for $FORGE_REV (provenance matches)"
          else
            unlock
            TEMP="$SHARED-build-$BUILD_NUMBER-$$"
            docker build --label "$LABEL=$FORGE_REV" -t "$TEMP" - < "$LOCAL/release/releases/$FORGE_REV/docker/Dockerfile.deploy-runner"
            BUILT=$(docker image inspect --format '{{.Id}}' "$TEMP")
            lock
            if present; then
              echo "Another build published $SHARED first; adopted $WANT"
            else
              docker tag "$BUILT" "$SHARED"
              present || refuse "$SHARED could not be read back after tagging"
            fi
            unlock
            docker image rm "$TEMP" >/dev/null
          fi
          printf '%s\\n' "$WANT" > "$LOCAL/private/staged/runner-image"
        '''
        script {
          env.FORGE_RUNNER_ID = sh(returnStdout: true, script: '''#!/bin/sh
            set -eu
            cat "$JENKINS_HOME/$FORGE_JOB_DIR/private/staged/runner-image"
          ''').trim()
          if (!(env.FORGE_RUNNER_ID ==~ /sha256:[0-9a-f]{64}/)) {
            error('The runner image id was not recorded; nothing was deployed.')
          }
          // A recorded revision is never rebuilt: rebuilding could move the tags its record pins.
          def probe = runner('status', " | grep -qx \"revision \$FORGE_REV: recorded\"")
          env.FORGE_RECORDED = sh(returnStatus: true, script: probe) == 0 ? 'true' : 'false'
          def current = runner('status', " | grep -qx \"release \$FORGE_REV with the staged env: current\"")
          env.FORGE_IS_CURRENT = sh(returnStatus: true, script: current) == 0 ? 'true' : 'false'
        }
      }
    }

    stage('Staged only') {
      when { expression { params.ACTION == 'stage' } }
      steps { echo("Staged ${env.FORGE_REV}: release directory, inputs and runner image are ready; nothing was deployed.") }
    }

    stage('Use active inputs') {
      when { expression { params.ACTION == 'recover' || params.ACTION == 'status' } }
      steps {
        script {
          // The journal is the pointer (round 4, C2): state.json names the active generation and its
          // runner as plain top-level fields, written in the same atomic write as the journal.
          def pointer = { String key ->
            sh(returnStdout: true, script: """#!/bin/sh
              set -eu
              sed -n 's/^  "${key}": "\\([0-9a-z:]*\\)",\\{0,1\\}\$/\\1/p' "\$JENKINS_HOME/\$FORGE_JOB_DIR/private/state.json"
            """).trim()
          }
          env.FORGE_REV = pointer('activeRunner')
          env.FORGE_RUNNER_ID = pointer('activeRunnerImage')
          env.FORGE_GENERATION = pointer('activeGeneration')
          if (!(env.FORGE_REV ==~ /[0-9a-f]{7,40}/) || !(env.FORGE_GENERATION ==~ /[0-9a-z]{6,24}/) || !(env.FORGE_RUNNER_ID ==~ /sha256:[0-9a-f]{64}/)) {
            error('No active inputs in the journal yet. Run ACTION=status to confirm; if no baseline is recorded, run ACTION=stage, then baseline from a host shell (runbook: First deploy). Never use deploy to repeat preparation.')
          }
          readPaths("inputs/${env.FORGE_GENERATION}/deploy.env")
        }
      }
    }

    stage('Build') {
      when { expression { params.ACTION == 'deploy' && env.FORGE_RECORDED != 'true' } }
      steps { script { runStep('build') } }
    }

    stage('Deploy') {
      when { expression { params.ACTION == 'deploy' } }
      steps { script { runStep('deploy') } }
    }

    stage('Recover') {
      when { expression { params.ACTION == 'recover' } }
      steps { script { runStep('recover') } }
    }

    stage('Status') {
      when { expression { params.ACTION == 'status' } }
      steps { script { runStep('status') } }
    }
  }
}

// Paths only (no secret values): read single keys from a private env file, never print it.
def readPaths(String file) {
  def envValue = { String key ->
    sh(returnStdout: true, script: """#!/bin/sh
      set -eu
      sed -n 's/^${key}=//p' "\$JENKINS_HOME/\$FORGE_JOB_DIR/private/${file}" | tail -n 1 | sed -e 's/^"\\(.*\\)"\$/\\1/' -e "s/^'\\(.*\\)'\$/\\1/"
    """).trim()
  }
  env.FORGE_WEB_ROOT = envValue('WEBROOT')
  // A locator: required, never defaulted (round 7, T1; review: the Jenkins fallback was a default).
  env.FORGE_LOCK_DIR = envValue('DEPLOY_LOCK_DIR') ?: ''
  if (!env.FORGE_LOCK_DIR) {
    error('DEPLOY_LOCK_DIR must be set in the env file; nothing was deployed.')
  }
  // Deliberate default (runbook): the client image holds no state; it also holds the image lock.
  env.FORGE_PG_CLIENT_IMAGE = envValue('DEPLOY_PG_CLIENT_IMAGE') ?: 'postgres:18-alpine'
  [env.FORGE_WEB_ROOT, env.FORGE_LOCK_DIR].each { p ->
    if (!(p ==~ /\/[A-Za-z0-9._\/-]+/)) {
      error('WEBROOT and DEPLOY_LOCK_DIR must be absolute paths without spaces or commas.')
    }
  }
}

// Round 4, C4: map the script's exit code to a build result and a clear next action. A committed
// release is never presented as a failure that invites a rollback.
def runStep(String command) {
  int code = sh(returnStatus: true, script: runner(command))
  if (code == 0) {
    boolean noop = command == 'deploy' && env.FORGE_IS_CURRENT == 'true'
    echo(noop ? "Release ${env.FORGE_REV} was already current; nothing was deployed." : "${command}: success")
  } else if (code == 14) {
    unstable('Release is committed and running; run the recover action to finish housekeeping; do NOT roll back.')
  } else if (code == 15) {
    unstable('Commit outcome unknown; services were left running. Run the recover action (it decides from the journal); do NOT roll back.')
  } else if (code == 10) {
    error('Deploy failed: the previous release was restored and verified. Fix the cause on main, then promote again.')
  } else if (code == 11) {
    error('RECOVERY REQUIRED: read the error above, repair the cause, then run this job with ACTION=recover (runbook: Recovery required).')
  } else if (code == 12) {
    error('Preflight refused; nothing was changed. Fix each "refused:" line above, then run again.')
  } else if (code == 13) {
    error('Another deploy, recover or baseline holds the lock. Wait for it to finish, then run again.')
  } else {
    error("${command} exited with code ${code} (137: the runner was killed); see the output above. Run ACTION=status: if it shows an unfinished attempt, run ACTION=recover. Never roll back by hand (runbook: Exit codes).")
  }
}

// One runner invocation; every directory is mounted at its host path (design D2).
def runner(String command, String suffix = '') {
  return """#!/bin/sh
    set -eu
    ROOT="\$FORGE_HOST_HOME/\$FORGE_JOB_DIR"
    SIMULATE=0
    if [ "\$SIMULATE_VERIFY_FAILURE" = "true" ]; then SIMULATE=1; fi
    docker run --rm --network host \\
      --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \\
      --mount "type=bind,src=\$ROOT/release,dst=\$ROOT/release" \\
      --mount "type=bind,src=\$ROOT/private,dst=\$ROOT/private" \\
      --mount "type=bind,src=\$FORGE_WEB_ROOT,dst=\$FORGE_WEB_ROOT" \\
      --mount "type=bind,src=\$FORGE_LOCK_DIR,dst=\$FORGE_LOCK_DIR" \\
      -e TMPDIR="\$ROOT/release/tmp" \\
      -e FORGE_RELEASE_ROOT="\$ROOT/release" \\
      -e FORGE_PRIVATE_ROOT="\$ROOT/private" \\
      -e FORGE_RUNNER_IMAGE="\$FORGE_RUNNER_ID" \\
      -e SIMULATE_VERIFY_FAILURE="\$SIMULATE" \\
      -e COMPOSE_PARALLEL_LIMIT=1 \\
      "\$FORGE_RUNNER_ID" \\
      node "\$ROOT/release/releases/\$FORGE_REV/scripts/deploy-release.mjs" ${command} --revision "\$FORGE_REV"${suffix}
  """
}
