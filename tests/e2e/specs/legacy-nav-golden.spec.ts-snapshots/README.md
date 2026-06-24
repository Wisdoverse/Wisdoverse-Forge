# Legacy Nav Golden Snapshots

These snapshot files capture the rendered sidebar org/team/project labels
under the deterministic mock-API regime in `legacy-nav-golden.spec.ts`.

## First run

The placeholder files (`nav-team-list-linux.txt`, `nav-project-list-linux.txt`)
are intentionally empty on first commit. Playwright's `toMatchSnapshot()`
treats an empty snapshot as a baseline of empty string — the first CI run
will fail and emit the actual rendered text in the test report.

## Refreshing snapshots

After observing the first failing CI output, regenerate the snapshots locally:

```bash
BASE_URL=http://<staging-or-local> \
  npx playwright test tests/e2e/specs/legacy-nav-golden.spec.ts \
    --config tests/e2e/playwright.config.ts \
    --update-snapshots
```

Then commit the updated `*-linux.txt` files. The mock data is deterministic,
so once the baseline is captured, any future diff in this snapshot represents
a real change to the legacy nav contract or sidebar rendering.

## File naming

Playwright's default snapshot naming is `<spec>-snapshots/<name>-<platform>.txt`.
Linux is the CI platform, so only `*-linux.txt` files are checked in here.
