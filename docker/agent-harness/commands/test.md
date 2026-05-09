Run the project's test suite and report results.

Steps:

1. Detect the project type by checking for:
   - `package.json` → `npm test`
   - `Makefile` with test target → `make test`
   - `go.mod` → `go test ./...`
   - `Cargo.toml` → `cargo test`
   - `pytest.ini` / `pyproject.toml` → `pytest`
2. Run the appropriate test command
3. Report: total tests, passed, failed, skipped
4. If tests fail, show the first 3 failures with context

$ARGUMENTS can override the test command or specify a subset of tests to run.
