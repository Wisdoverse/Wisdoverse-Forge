Prepare and ship changes: lint, test, commit, push, and create MR/PR.

Steps:

1. Run linting if available (eslint, golint, ruff, etc.)
2. Run the test suite — abort if critical tests fail
3. Show `git diff --stat` for review
4. Stage relevant files (avoid .env, credentials, secrets, build artifacts)
5. Commit with a conventional commit message
6. Push to origin with `-u` flag
7. Create MR/PR via `glab mr create` or `gh pr create` (detect which is configured)
8. Report the MR/PR URL
