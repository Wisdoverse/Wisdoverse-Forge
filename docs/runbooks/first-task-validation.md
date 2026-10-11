# First reviewed task validation

Use this procedure to record one local first-task review. It does not prove
cohort adoption or production readiness.

The target is one first task with a complete human review within 900 seconds.
Start the timer just before `make product`, after dependency, image, and external
authorization preparation. Count product startup, health checks, registration,
routing, Agent and task setup, sidecar creation, execution, retries, approval
waits, and human review in the measured time. Record acceptance or rejection as
a separate quality outcome.

## Before you start

Use an isolated local installation with a disposable database. Do not reset a
shared database. Prepare these items before you start the timer:

- Install Node.js 24.15 or later, Docker Compose v2, Make, and Git.
- Install Python 3 for the Linux and macOS timer example.
- Install app dependencies and the required Container CLI image.
- Prepare a valid Container CLI account authorization.
- Prepare the GitHub App and repository access if you will use a maintenance task.
- Select a small task with clear acceptance criteria and a real result or artifact.

Use the existing `dev@example.com` test account. On a fresh isolated database,
register this same account through the normal form. Do not create a debug account.

Do not create an account, team, project, Agent, task place, or task before the
timer starts. Start product services and run their health checks after timing
starts. Create the sidecar during the measured activation path.

## Start the timer and product

Record the preparation start and end time in UTC. Keep preparation time separate
from the 900-second activation result.

Run the timer commands from the repository root.

The Linux and macOS example uses Python's monotonic clock. Use the same host,
boot session, and receipt path in both terminals. Choose a new receipt path for each run.

In terminal one, run this command before `make product`:

```bash
umask 077
receipt="$HOME/.cache/first-task-validation/run-01.json"
mkdir -p "$(dirname "$receipt")"
if [ -e "$receipt" ]; then
  echo "Choose a new receipt path."
  exit 1
fi
source_revision=$(git rev-parse HEAD)
python3 - "$receipt" "$source_revision" <<'PY'
from datetime import datetime, timezone
import json
import sys
import time

receipt = {
    "timer_state": "RUNNING",
    "source_revision": sys.argv[2],
    "started_at_utc": datetime.now(timezone.utc).isoformat(),
    "clock_source": "time.monotonic_ns",
    "start_monotonic_ns": time.monotonic_ns(),
}
with open(sys.argv[1], "x", encoding="utf-8") as output:
    json.dump(receipt, output, indent=2)
    output.write("\n")
PY
make product
```

Keep `make product` in the foreground. Continue the browser flow while that
terminal runs. Do not add `&` or use a background process.

The PowerShell 5 example uses the system performance counter. It covers two
windows on the same Windows host. Keep the host running and do not restart it
between timer start and stop. Follow the existing Windows environment
requirements when you run `make product`. This timer example does not test a Windows deployment. Use the same host and receipt path in both windows. Choose a
new file for each run.

In PowerShell window one, run this command before `make product`:

```powershell
$receiptPath = Join-Path $env:LOCALAPPDATA 'FirstTaskValidation\run-01.json'
$receiptDir = Split-Path $receiptPath
New-Item -ItemType Directory -Path $receiptDir -Force | Out-Null
if (Test-Path $receiptPath) { throw 'Choose a new receipt path.' }
$sourceRevision = (git rev-parse HEAD).Trim()
$receipt = [ordered]@{
    timer_state = 'RUNNING'
    source_revision = $sourceRevision
    started_at_utc = [DateTime]::UtcNow.ToString('o')
    clock_source = 'Stopwatch.GetTimestamp'
    start_tick = [Diagnostics.Stopwatch]::GetTimestamp()
    frequency = [Diagnostics.Stopwatch]::Frequency
}
$receipt | ConvertTo-Json | Set-Content -LiteralPath $receiptPath -Encoding UTF8
make product
```

Keep `make product` in the foreground. Continue the browser flow while that
window runs. Do not start it as a background process.

## Complete the first task

Follow the [Getting Started first-use path](../guides/getting-started.md#4-first-use-path)
and the [Task Workflow Guide](../guides/task-workflow.md).

1. Register `dev@example.com` in the fresh local database.
2. Create a team and project, then select them for task routing.
3. Create a task-capable Agent that uses a Container CLI.
4. Select **Project files** and set the Agent's place for new tasks.
5. Create and assign one small task with clear acceptance criteria.
6. Wait for the real sidecar execution to finish.
7. Open the exact task and read its Result, Activity, and artifact.
8. Review the output against the task criteria and record a human decision.

Use the real Rust API, PostgreSQL, and sidecar path. Do not use a mock Agent,
fake result, synthetic task state, or manual database update.

Record the exact task ID and run ID. Record the source revision from the timer
receipt. Record the produced revision when the task creates one. Otherwise,
record `not_created` for the result revision. Record the persisted result or
artifact reference without copying its contents into the receipt.

Do not treat a terminal state, an automated checklist, or GitHub checks as human
judgement. A human must read the actual result or artifact and accept or reject
it against the task criteria.

### Maintenance task path

An ordinary task uses the task Result and Activity. It does not use the
maintenance report or verdict path.

If you choose a maintenance task, prepare valid GitHub App and repository access
before timing. After execution, refresh the observed GitHub state. Record a
human verdict only when that observation matches the exact produced revision.
Do not inject or simulate GitHub state.

The [Maintenance Delivery Guide](../guides/maintenance-delivery.md#first-operator-journey)
describes the report and verdict flow. A reported command does not run through
that form. GitHub checks do not replace the human verdict.

## Stop the timer

Stop the timer after a human completes review of the real result, whether the
human accepts or rejects it. Stop after a final failed run when no real result
exists. Keep the timer running if a retry can finish within 900 seconds.

Use a second terminal or PowerShell window if `make product` still occupies the first one.

For Linux or macOS, run this command in terminal two with the same receipt path:

```bash
receipt="$HOME/.cache/first-task-validation/run-01.json"
python3 - "$receipt" <<'PY'
from datetime import datetime, timezone
import json
import sys
import time

with open(sys.argv[1], encoding="utf-8") as source:
    receipt = json.load(source)
if receipt.get("timer_state") != "RUNNING":
    raise SystemExit("Timer is not running or was already stopped.")
if receipt.get("clock_source") != "time.monotonic_ns":
    raise SystemExit("Timer clock does not match this stop command.")
finish = time.monotonic_ns()
if finish < receipt["start_monotonic_ns"]:
    raise SystemExit("Timer result is invalid.")
receipt["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
receipt["finish_monotonic_ns"] = finish
receipt["elapsed_seconds"] = round(
    (finish - receipt["start_monotonic_ns"]) / 1_000_000_000, 3
)
receipt["timer_state"] = "RECORDED"
with open(sys.argv[1], "w", encoding="utf-8") as output:
    json.dump(receipt, output, indent=2)
    output.write("\n")
print(receipt["elapsed_seconds"])
PY
```

For PowerShell, run this command in window two with the same receipt path:

```powershell
$receiptPath = Join-Path $env:LOCALAPPDATA 'FirstTaskValidation\run-01.json'
$receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
if ($receipt.timer_state -ne 'RUNNING') { throw 'Timer is not running or was already stopped.' }
if ($receipt.clock_source -ne 'Stopwatch.GetTimestamp') { throw 'Timer clock does not match this stop command.' }
$finish = [Diagnostics.Stopwatch]::GetTimestamp()
if ($finish -lt $receipt.start_tick) { throw 'Timer result is invalid.' }
$elapsed = ($finish - $receipt.start_tick) / [double]$receipt.frequency
$receipt | Add-Member -NotePropertyName finished_at_utc -NotePropertyValue ([DateTime]::UtcNow.ToString('o')) -Force
$receipt | Add-Member -NotePropertyName finish_tick -NotePropertyValue $finish -Force
$receipt | Add-Member -NotePropertyName elapsed_seconds -NotePropertyValue ([Math]::Round($elapsed, 3)) -Force
$receipt | Add-Member -NotePropertyName timer_state -NotePropertyValue 'RECORDED' -Force
$receipt | ConvertTo-Json | Set-Content -LiteralPath $receiptPath -Encoding UTF8
$receipt.elapsed_seconds
```

Add the task, run, result, failed attempt, retry, approval, and review fields to
the private receipt. Set `timing_status` after review. Keep
the receipt on the same host and boot session as the timer.

## Read the result

Set `timing_status` to `PASS` when a real result receives a complete human
review within 900 seconds. Record `accepted` or `rejected` in `quality_outcome`.
A rejected result can pass the timing target. It does not count as an accepted
outcome. Include every retry and approval wait in `elapsed_seconds`.

Set `timing_status` to `FAIL` when no real result or complete review exists, or
when review finishes after 900 seconds. Keep all attempts in the receipt. Never
use a fake result or review.

Set `timing_status` to `NOT_MEASURED` when a required prerequisite is absent
before timing or the timer evidence is invalid. Keep any attempted run in the
receipt. Do not count `NOT_MEASURED` as a pass.

Include these fields in the receipt:

```json
{
  "timer_state": "RECORDED",
  "timing_status": "PASS | FAIL | NOT_MEASURED",
  "quality_outcome": "accepted | rejected | not_reviewed",
  "preparation_started_at_utc": "<UTC timestamp>",
  "preparation_finished_at_utc": "<UTC timestamp>",
  "started_at_utc": "<UTC timestamp>",
  "finished_at_utc": "<UTC timestamp>",
  "elapsed_seconds": 0,
  "source_revision": "<repository commit SHA>",
  "runtime_images": [
    {
      "role": "API",
      "image_id_or_digest": "<value or unknown>",
      "source_revision": "<SHA or unknown>"
    },
    {
      "role": "Container CLI",
      "image_id_or_digest": "<value or unknown>",
      "source_revision": "<SHA or unknown>"
    }
  ],
  "task_id": "<task ID>",
  "run_id": "<run ID>",
  "task_path": "ordinary | maintenance",
  "result_revision": "<commit SHA or not_created>",
  "artifact_or_result_ref": "<local reference>",
  "failed_attempts": 0,
  "retry_count": 0,
  "approval_wait_included": true,
  "human_review_completed_at_utc": "<UTC timestamp>",
  "human_decision": "accepted | rejected | not_reviewed"
}
```

Use UTC for timestamps. Use the monotonic counter for elapsed time. Do not
calculate elapsed time from wall-clock timestamps. Keep the same host, boot
session, clock source, and timer receipt from start through stop. The stop command
rejects a repeated stop and a negative elapsed value. Record image IDs or digests
and their source revisions. Use `unknown` if either value is unavailable.

## Startup troubleshooting

The current timed acceptance has no qualifying result. The startup preflight
found Docker default address-pool exhaustion. Do not report a timing pass.

For the self-host smoke script only, a maintainer can set
`BEGINNER_SMOKE_AGENT_SUBNET` and `BEGINNER_SMOKE_NETWORK_SUBNET` to two checked
CIDRs that do not overlap. Use this option only when Docker reports address-pool
exhaustion on a shared host. Do not remove another network or reuse a shared
network.

## Privacy and cleanup

Keep the receipt private on the test machine. Do not record credentials, tokens,
task titles, result text, artifact contents, or private host names.

After the timer stops, press `Ctrl+C` in the terminal that runs `make product`.
If its backend remains active, run `make product-down` from the repository root.
Do not add `-v` or remove shared database volumes.

Keep the receipt and task evidence until the review is complete. Remove only
temporary files and resources created for this isolated run. Do not remove
shared accounts, credentials, workspaces, or unrelated data.

## Interpretation boundary

One local automated operation can provide local evidence for the activation
path. It does not prove a new-user cohort, repeated use, production readiness,
or product adoption. This local result does not require a pilot threshold.
