# App-shell first-load measurement

This measurement records verified app-shell readiness on `/tasks`. Readiness
requires both the shell commit and server authentication to complete. It uses a fixed
desktop profile. It does not measure task-data completion, Largest Contentful
Paint (LCP), or real-user crash-free sessions.

## Before you start

Meet each prerequisite before you run the measurement:

- Serve the current production frontend at the target origin.
- Install Node.js 24.15.0 or later and npm.
- Install dependencies with `npm ci` from the repository root.
- Install Chromium with `npx playwright install chromium`.
- Use Playwright's installed Chromium by default. See the [Playwright configuration](../../tests/e2e/playwright.config.ts).
- Use a valid private auth state for the existing `dev@example.com` account.
- Match the auth-state origin to `BASE_URL`, including scheme, host, and port.
- Match every auth cookie domain to the target hostname.

On Unix, set the auth-state file mode to `0600`. The auth state must not expire
or redirect to login during the run. Do not create a throwaway account. Do not
copy or edit the auth-state contents. Do not print auth state, tokens,
passwords, or browser storage.

Build the production frontend locally. Start the preview server in the same terminal:

```sh
npm run build
npm run preview -- --host 127.0.0.1
```

Vite serves the build at `http://127.0.0.1:4173` by default. Use a valid auth
state for this exact origin. You can use another production target if its
origin matches the auth state.

Set the target origin and private auth-state path. Replace both placeholders:

```sh
export BASE_URL='https://staging.example.com'
export E2E_STORAGE_STATE='/path/to/private-auth-state.json'
```

In Windows PowerShell, replace both placeholders with paths and the exact
target origin:

```powershell
$env:BASE_URL = 'https://staging.example.com'
$env:E2E_STORAGE_STATE = 'C:\path\to\private-auth-state.json'
npm run measure:app-shell
```

Set `CHROMIUM_PATH` only to override the Playwright browser binary. It does not
select a config file:

```sh
export CHROMIUM_PATH='/path/to/chromium'
```

## Run the measurement

For command help, run `npm run measure:app-shell -- --help`.

Run the command from the repository root:

```sh
npm run measure:app-shell
```

The command opens 20 fresh browser contexts. It disables cache and service
workers. It uses a 1440 by 900 viewport, 10 Mbps download, 1 Mbps upload, 40 ms
minimum request-to-header latency, and 4x CPU slowdown. It uses the Chrome
DevTools Protocol. See the official [Network](https://chromedevtools.github.io/devtools-protocol/tot/Network/)
and [Emulation](https://chromedevtools.github.io/devtools-protocol/tot/Emulation/)
domain references.

Each sample starts at `Performance.timeOrigin` for the `/tasks` document
navigation. The first shell mark is `forge:app-shell-commit`. The sample ends
at the later of that mark's `startTime` and the authenticated `/api/v1/me`
resource timing entry's `responseEnd`. This measures verified app-shell
readiness. Context creation, browser configuration, and auth preparation occur
before navigation and outside the timed interval.

Each sample must also receive an authenticated identity response. The harness
accepts only `GET /api/v1/me` from the exact target origin, including scheme,
hostname, and port. The response must have a successful HTTP status and JSON
`ok: true`. A missing response or Resource Timing entry makes the sample fail.
The harness does not log the response or credentials.

The output records the host platform, CPU model, browser version, profile,
samples, failed attempts, and p75. Keep the output private. The command must
not print auth state or credentials.

## Read the result

The command reports `PASS`, `FAIL`, or `NOT_RUN`.

- `PASS` requires exactly 20 valid samples, zero failed attempts, and p75 below
  3,500 ms.
- `FAIL` means a sample failed, the cohort is incomplete, or p75 is 3,500 ms or
  higher.
- `NOT_RUN` means required input, the private auth-state file, or the browser
  prerequisite is missing or invalid before sampling starts.

After sampling starts, an unreachable target, a rejected `/api/v1/me` response,
a login redirect, a timeout, a missing shell mark, or a missing Resource Timing
entry causes `FAIL`. The harness reports `expected` and `attempted` counts.
Attempted can be less than 20. Do not
claim 20 completed samples unless the output has 20 valid samples. Keep failed
attempts in the count. Do not remove failed samples to calculate p75. Empty or
incomplete cohorts cannot pass.

After `PASS`, record the output with the tested revision and reference profile.
Use this result only for the documented browser benchmark. It does not prove
production latency or physical-device performance. It does not measure the
99.5% real-user crash-free objective. Measure that objective separately.

For `NOT_RUN`, restore a valid auth state for the exact target origin or fix the
missing prerequisite. For `FAIL`, use the output's fixed `issue` code to find
the failed stage. Fix that issue. Run the full 20-context cohort again.

## Local validation (2026-10-10)

This result used source commit `238c172f217f4576c5a572c08603117aee53a5e0`.
It used a production client build, a real Rust HTTP API, and PostgreSQL.
The API used a synthetic authentication identity. This was not a production deployment.

The host ran Linux x64 on an AMD EPYC 7763 CPU. Chromium was `153.0.8010.12`.
The run used 20 new browser contexts at 1440 by 900 pixels. It limited
download to 10 Mbps and upload to 1 Mbps. It set the minimum RTT to 40 ms and
the CPU slowdown to 4x. It disabled the cache and blocked service workers.

All 20 samples were valid. The run had zero failed attempts. The p75 was
2,921 ms, below the 3,500 ms limit. This result passes the fixed local profile.
It does not prove production latency or the all-user crash-free target.
