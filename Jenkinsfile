// Wisdoverse Forge deploy pipeline (issue #1319). Runbook: docs/runbooks/boe-deploy.md
//
// Job configuration (required, design A7): "Pipeline script from SCM" reading this Jenkinsfile
// from `main`. The pipeline checks out `DEV` itself (registered for SCM polling) and deploys it
// only after proving that the commit is already on `main`. Keep SCM polling disabled until the
// first supervised deploy has succeeded; enable it afterwards.
//
// ACTION=status and ACTION=recover never fetch, build or replace the env file: they reuse the
// runner image, release directory and env file prepared by the last deploy, so a Git or
// registry outage cannot block a recovery.
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
    choice(name: 'ACTION', choices: ['deploy', 'recover', 'status'],
      description: 'deploy: release DEV; recover: finish or roll back an interrupted attempt; status: print state')
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
      when { expression { params.ACTION == 'deploy' } }
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
            cp "$FORGE_ENV_SRC" "$LOCAL/private/deploy.env.partial"
            chmod 600 "$LOCAL/private/deploy.env.partial"
            printf '\\nVERSION=%s\\n' "$FORGE_REV" >> "$LOCAL/private/deploy.env.partial"
            mv "$LOCAL/private/deploy.env.partial" "$LOCAL/private/deploy.env"
          '''
        }
        script { readPaths() }
        sh '''#!/bin/sh
          set -eu
          LOCAL="$JENKINS_HOME/$FORGE_JOB_DIR"
          docker build -t "forge-deploy-runner:$FORGE_REV" - < "$LOCAL/release/releases/$FORGE_REV/docker/Dockerfile.deploy-runner"
          printf '%s\\n' "$FORGE_REV" > "$LOCAL/release/last-runner"
        '''
        script {
          // A recorded revision is never rebuilt: rebuilding could move the tags its record pins.
          def probe = runner('status', " | grep -qx \"revision \$FORGE_REV: recorded\"")
          env.FORGE_RECORDED = sh(returnStatus: true, script: probe) == 0 ? 'true' : 'false'
        }
      }
    }

    stage('Use recorded runner') {
      when { expression { params.ACTION != 'deploy' } }
      steps {
        script {
          env.FORGE_REV = sh(returnStdout: true, script: '''#!/bin/sh
            set -eu
            cat "$JENKINS_HOME/$FORGE_JOB_DIR/release/last-runner"
          ''').trim()
          if (!(env.FORGE_REV ==~ /[0-9a-f]{40}/)) {
            error('No deploy has prepared this job yet; there is nothing to recover or report.')
          }
          readPaths()
        }
      }
    }

    stage('Build') {
      when { expression { params.ACTION == 'deploy' && env.FORGE_RECORDED != 'true' } }
      steps { sh(runner('build')) }
    }

    stage('Deploy') {
      when { expression { params.ACTION == 'deploy' } }
      steps { sh(runner('deploy')) }
    }

    stage('Recover') {
      when { expression { params.ACTION == 'recover' } }
      steps { sh(runner('recover')) }
    }

    stage('Status') {
      when { expression { params.ACTION == 'status' } }
      steps { sh(runner('status')) }
    }
  }
}

// Paths only (no secret values): read single keys from the private env file, never print it.
def readPaths() {
  def envValue = { String key ->
    sh(returnStdout: true, script: """#!/bin/sh
      set -eu
      sed -n 's/^${key}=//p' "\$JENKINS_HOME/\$FORGE_JOB_DIR/private/deploy.env" | tail -n 1 | sed -e 's/^"\\(.*\\)"\$/\\1/' -e "s/^'\\(.*\\)'\$/\\1/"
    """).trim()
  }
  env.FORGE_WEB_ROOT = envValue('WEBROOT')
  env.FORGE_LOCK_DIR = envValue('DEPLOY_LOCK_DIR') ?: '/var/lock/forge-deploy'
  [env.FORGE_WEB_ROOT, env.FORGE_LOCK_DIR].each { p ->
    if (!(p ==~ /\/[A-Za-z0-9._\/-]+/)) {
      error('WEBROOT and DEPLOY_LOCK_DIR must be absolute paths without spaces or commas.')
    }
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
      -e FORGE_RUNNER_IMAGE="forge-deploy-runner:\$FORGE_REV" \\
      -e SIMULATE_VERIFY_FAILURE="\$SIMULATE" \\
      "forge-deploy-runner:\$FORGE_REV" \\
      node "\$ROOT/release/releases/\$FORGE_REV/scripts/deploy-release.mjs" ${command} --revision "\$FORGE_REV"${suffix}
  """
}
