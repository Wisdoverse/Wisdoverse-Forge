#!/usr/bin/env bash
# Called by npm version hook. Syncs version from package.json to derived files.
set -euo pipefail

VERSION="${npm_package_version:?npm_package_version not set}"

# ── 1. Generate changelog (most failure-prone — run first, before file writes)
npx conventional-changelog -p angular -i CHANGELOG.md -s --release-count 1

if ! git diff --quiet CHANGELOG.md 2>/dev/null; then
  echo "Changelog updated for version $VERSION."
else
  echo "WARNING: conventional-changelog produced no changes. Commits may not follow conventional format."
fi

# ── 2. public/version.json (git-tracked, consumed by frontend VersionChecker)
node -e "
  const fs = require('fs'), p = 'public/version.json';
  let d;
  try { d = JSON.parse(fs.readFileSync(p, 'utf8')); }
  catch (e) { console.error('ERROR: public/version.json is not valid JSON:', e.message); process.exit(1); }
  d.latest = process.argv[1];
  fs.writeFileSync(p, JSON.stringify(d, null, 2) + '\n');
" "$VERSION"

# ── 3. docker/.env (local only, not git-tracked — for make prod-ext)
if [ -f docker/.env ]; then
  if grep -q "^VERSION=" docker/.env; then
    sed -i "s|^VERSION=.*|VERSION=${VERSION}|" docker/.env
  else
    echo "VERSION=${VERSION}" >> docker/.env
  fi
fi

# Stage tracked files for the version commit
git add public/version.json CHANGELOG.md
