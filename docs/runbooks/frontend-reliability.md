# Frontend session reliability

Use this report to review observed browser sessions for one organization.
It does not prove the 99.5% crash-free target for all users. The report keeps
`populationCoverageVerified` false.

## Before you start

Run the current frontend version. Sign in with an existing organization account
in a normal browser. Use the app. Sign out to end the observed session.
The browser sends observations after authenticated identity becomes available.

The report requires an existing valid bearer token for the same organization.
Use the token from your normal authenticated operator workflow. Replace
`https://staging.example.com` with your API root in the examples. Enter the
token at the hidden prompt. Do not save a real token in commands or documents.

## Read the report

Request the 30-day report with `hours=720`.

Linux or macOS with Bash:

```bash
API_URL='https://staging.example.com'
read -r -s -p 'Bearer token: ' AUTH_TOKEN
printf '\n'
curl --fail --silent --show-error \
  -H "Authorization: Bearer ${AUTH_TOKEN}" \
  "${API_URL%/}/api/v1/analytics/frontend-reliability?hours=720"
unset AUTH_TOKEN
```

Windows PowerShell:

```powershell
$apiUrl = 'https://staging.example.com'
$token = Read-Host 'Bearer token' -AsSecureString
$headers = @{ Authorization = 'Bearer ' + [System.Net.NetworkCredential]::new('', $token).Password }
$uri = "$($apiUrl.TrimEnd('/'))/api/v1/analytics/frontend-reliability?hours=720"
Invoke-RestMethod -Method Get -Uri $uri -Headers $headers
Remove-Variable headers, token
```

The success response has `ok: true` and a `data` object. The object contains
counts and `observedCrashFreeRate`. The rate is a fraction, not a percentage.
Multiply it by 100 to read it as a percentage. `windowStartedAt` uses server
time for the requested window.

## Understand the counts

- `startedSessions` counts sessions whose first observed start falls in the window.
- `crashedSessions` counts sessions with at least one observed frontend error.
- `endedWithoutObservedCrash` counts ended sessions with no observed error.
- `unfinishedSessions` counts sessions with no observed end or error.
- `orphanSessions` counts valid end or error observations without a start.
- `invalidObservations` counts malformed stored observations in the window.

The report counts each session once for its authenticated organization and
user. A late crash remains a crash after an end event. An unfinished session
stays in the denominator and does not enter the numerator.

The server uses the first observed start time to place a session in the window.
This time is not the browser's actual start time.

Telemetry retention keeps history while any matching start remains within the
retention period. Older individual observations can therefore outlive their
row retention age. This preserves earlier errors and duplicate starts. When
all matching starts expire, normal cleanup can remove the history. A report
cannot reconstruct removed observations or prove historical population coverage.

The rate is `endedWithoutObservedCrash / startedSessions`. An empty cohort,
any orphan session, or any invalid observation sets the rate to `null`. A
non-null rate describes observed sessions only. It does not prove full
population coverage.

## Read coverage limits

The browser cannot report a start before the observer runs. Provider bootstrap
errors and errors before authenticated identity remain outside the cohort.
Browser process termination can lose an end event. A lost start or request can
create missing or orphan observations.

The observer ends a session on `pagehide` only when `persisted` is false. A
BFCache pagehide does not end the session. The browser can omit `pagehide` when
it kills a process. See the [HTML Standard page transition events](https://html.spec.whatwg.org/multipage/nav-history-apis.html).

The client sends events with Fetch `keepalive`. This option can allow a request
to outlive its page. It does not guarantee delivery. The observer does not use
an offline queue or retry. See the [Fetch Standard](https://fetch.spec.whatwg.org/).

The client keeps one session ID across React StrictMode remounts and token
refresh. It creates a new ID when the user or organization changes. Each
request captures the current token before it sends. The client does not retry
an event under another identity. A failed event remains missing. A lost crash
event cannot be followed by a healthy end event.

## Privacy and acceptance

The client sends events only when JWT subject and organization match the
current identity, and Rust still verifies authorization.

Each browser event includes one random browser session UUID in its properties.
The API stores the event type and authenticated organization and user. The
client does not send raw errors, stack traces, URLs, DOM content, tokens, email
addresses, or document titles as event properties. Authentication tokens
authorize requests and are not stored in analytics event properties.

The implementation uses the existing `analytics_events` table. It adds no
database schema or monitoring SDK. Use the report to inspect observed
outcomes. Do not use it to claim the 99.5% all-user target is met.
`populationCoverageVerified` is always false.
