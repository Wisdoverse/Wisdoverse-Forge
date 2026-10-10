# Started-task reliability report

Use this report to measure whether started tasks reach a terminal state with a
persisted result. It reports counts for one organization. It does not report
whether an accepted outcome was useful.

## Before you start

You need a running Forge API and a valid JWT for a user in the organization
you want to inspect. The JWT must have access to the analytics route. Set the
API base URL and JWT in your shell. Replace both placeholders:

```sh
export API_URL='https://staging.example.com'
export JWT='replace-with-a-valid-jwt'
```

Do not paste the JWT into a shared terminal. Do not save the JWT in a script.
The query returns organization-level counts. It does not return task titles, results,
errors, or user details.

## Request the report

The default window is 720 hours. The API limits `hours` to the range 1 to
8,760. Values below or above this range become the nearest limit. This example
requests the default window:

```sh
curl --fail-with-body --silent --show-error \
  --header "Authorization: Bearer ${JWT}" \
  "${API_URL%/}/api/v1/analytics/task-reliability?hours=720"
```

A successful response has `ok: true` and a `data` object. This shape example
shows field types only. It is not a measurement:

```json
{
  "ok": true,
  "data": {
    "windowHours": 720,
    "windowStartedAt": "<timestamp>",
    "observedAt": "<timestamp>",
    "coverageSince": "<timestamp>",
    "coverageComplete": true,
    "startedTasks": 0,
    "terminalWithPersistedResults": 0,
    "terminalWithoutPersistedResults": 0,
    "unfinishedTasks": 0,
    "deletedTasks": 0,
    "unplacedHistoricalTasks": 0,
    "terminalPersistenceRate": null
  }
}
```

Use the values returned by the API. A `null` `terminalPersistenceRate` means
the window is empty or coverage is incomplete.
Do not read `null` as zero or as a pass. Read `coverageComplete`, the window
timestamps, and the counts before you compare a rate with a target.

## Read the counts

`startedTasks` is the denominator. A new start occurs when the stored task
status becomes `working` or `started_at` becomes non-null. The system stores
the first start time.

Each task with a recorded first start in the window
appears once. A retry does not add another task. Unfinished tasks and deleted
tasks stay in this denominator. Deleting a task clears result eligibility but
keeps its recorded start.

`terminalWithPersistedResults` counts completed tasks with a non-null JSON
result, failed tasks with a non-null JSON error, and canceled tasks with a
stored `canceled_at` timestamp. SQL NULL and JSON `null` do not count as a
persisted result. The `canceled_at` timestamp records explicit cancellation
without a result.

`terminalWithoutPersistedResults` counts terminal tasks without the required
result or cancellation record. `unfinishedTasks` counts started tasks whose
current task row is not terminal. `deletedTasks` counts started tasks whose
task row no longer exists. These counts describe different states in the
current report. Use `startedTasks` as the denominator.

`unplacedHistoricalTasks` counts backfilled starts whose timestamp is unknown.
Their start time cannot place them in a requested window. The system cannot
reconstruct records that it deleted before it began this measurement.
Unknown historical starts predate the coverage marker.

The migration backfills timestamps from surviving task, run, and assignment
records. It does not change those records. It stores unknown start times as
`NULL`.

The migration writes the instance-wide `coverageSince` marker after it
installs the trigger and completes the backfill. It uses `clock_timestamp()`
for this marker. It does not use the SQLx migration timestamp.

The report sets
`coverageComplete` only when the requested window starts at or after this
marker. An earlier window has incomplete coverage. Its rate is `null`, even
when it contains observed tasks.

The terminal persistence rate is
`terminalWithPersistedResults / startedTasks`.
Use a non-null rate from a window with enough real task outcomes.
Compare this rate with the roadmap target. The target is more than 85 percent.
A result of 85 percent does not meet the target. Accepted outcomes remain a
separate product criterion.

The existing agent-reliability report keeps its current response and
denominator. This task report measures persisted terminal outcomes. Neither
report alone proves that a person accepted a result, verified its correctness,
or found it useful.
