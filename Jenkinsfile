// Wisdoverse Forge deploy pipeline (issue #1319). Runbook: docs/runbooks/boe-deploy.md
//
// Job configuration (required, design A7): "Pipeline script from SCM" reading this Jenkinsfile
// from `main`. The pipeline then deploys the head of `DEV`, and only after proving that this
// commit is already on `main`. Keep SCM polling disabled until the first supervised deploy has
// succeeded; enable it afterwards.
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
      description: 'Jenkins "Secret file" credential holding the deployment env file')
    choice(name: 'ACTION', choices: ['deploy', 'recover', 'status'],
      description: 'deploy: release DEV; recover: finish or roll back an interrupted attempt; status: print state')
    booleanParam(name: 'SIMULATE_VERIFY_FAILURE', defaultValue: false,
      description: 'Rehearsal only: fail verification after the new backend starts, forcing a rollback')
  }

  stages {
    stage('Prepare') {
      steps {
        checkout scm
        sh '''#!/bin/sh
          set -eu
          if [ -f "$(git rev-parse --git-dir)/shallow" ]; then git fetch --no-tags --unshallow origin; fi
          git fetch --no-tags origin \
            +refs/heads/main:refs/remotes/origin/main \
            +refs/heads/DEV:refs/remotes/origin/DEV
          REV="$(git rev-parse --verify 'refs/remotes/origin/DEV^{commit}')"
          # Refuse before any file from the DEV tree is used (A7).
          if ! git merge-base --is-ancestor "$REV" refs/remotes/origin/main; then
            echo "DEV ($REV) is not on main; promote only commits that are already on main." >&2
            exit 1
          fi
          printf '%s' "$REV" > .forge-rev
          echo "Release commit: $REV"
        '''
        script {
          env.FORGE_REV = readFile('.forge-rev').trim()
          env.FORGE_JOB_DIR = "forge-deploy/${env.JOB_BASE_NAME}"
          if (!(env.JOB_BASE_NAME ==~ /[A-Za-z0-9._-]+/)) {
            error('The job name may only contain letters, digits, dot, underscore and dash.')
          }
          // The runner mounts directories at their HOST paths, so find where JENKINS_HOME lives on the host.
          env.FORGE_HOST_HOME = sh(returnStdout: true, script: '''#!/bin/sh
            set -eu
            docker inspect --format '{{range .Mounts}}{{if eq .Destination "'"$JENKINS_HOME"'"}}{{.Source}}{{end}}{{end}}' "$(hostname)"
          ''').trim()
          if (!env.FORGE_HOST_HOME.startsWith('/')) {
            error('Could not find the host path of JENKINS_HOME from this container\'s mounts.')
          }
        }
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
        script {
          // Paths only (no secret values): read single keys, never print the file.
          def envValue = { String key ->
            sh(returnStdout: true, script: """#!/bin/sh
              set -eu
              sed -n 's/^${key}=//p' "\$JENKINS_HOME/\$FORGE_JOB_DIR/private/deploy.env" | tail -n 1 | sed -e 's/^"\\(.*\\)"\$/\\1/' -e "s/^'\\(.*\\)'\$/\\1/"
            """).trim()
          }
          env.FORGE_WEB_ROOT = envValue('WEBROOT')
          env.FORGE_LOCK_DIR = envValue('DEPLOY_LOCK_DIR') ?: '/var/lock/forge-deploy'
          [env.FORGE_WEB_ROOT, env.FORGE_LOCK_DIR, env.FORGE_HOST_HOME].each { p ->
            if (!(p ==~ /\/[A-Za-z0-9._\/-]+/)) {
              error('WEBROOT, DEPLOY_LOCK_DIR and the Jenkins home must be absolute paths without spaces or commas.')
            }
          }
        }
        sh '''#!/bin/sh
          set -eu
          DOCKERFILE="$JENKINS_HOME/$FORGE_JOB_DIR/release/releases/$FORGE_REV/docker/Dockerfile.deploy-runner"
          docker build -t "forge-deploy-runner:$FORGE_REV" - < "$DOCKERFILE"
        '''
      }
    }

    stage('Build') {
      when { expression { params.ACTION == 'deploy' } }
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

// One runner invocation; every directory is mounted at its host path (design D2).
def runner(String command) {
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
      node "\$ROOT/release/releases/\$FORGE_REV/scripts/deploy-release.mjs" ${command} --revision "\$FORGE_REV"
  """
}
