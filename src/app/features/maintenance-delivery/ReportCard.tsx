import type {
  GithubVerification,
  ReviewDecision,
  VerificationReport,
} from '@shared/types/maintenance-delivery'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { Timestamp } from './fields'

export function CheckObservation({ checks, label }: { checks: GithubVerification; label: string }) {
  return (
    <div className="space-y-2">
      <h3 className="font-medium">{label}</h3>
      <p>
        {checks.status.replace('_', ' ')}
        {checks.checkedAt && (
          <>
            {' '}
            · <Timestamp value={checks.checkedAt} />
          </>
        )}
      </p>
      {checks.revision && (
        <p className="break-all font-mono text-ui-caption">Observed revision: {checks.revision}</p>
      )}
      {!checks.complete && checks.status === 'observed' && (
        <p className={uiStyles.note}>
          Only part of the check history was available. Review the repository checks before using
          this change.
        </p>
      )}
      {checks.status === 'observed' && checks.checks.length === 0 && (
        <p>No GitHub checks were recorded for this revision at the stated time.</p>
      )}
      {checks.status === 'head_changed' && (
        <p className={uiStyles.note}>
          The current PR head differs from this task’s recorded revision. Review the new version
          before acceptance.
        </p>
      )}
      {checks.status === 'unavailable' && (
        <p className={uiStyles.note}>
          GitHub could not be refreshed. Stored observations remain historical; refresh before
          recording acceptance.
        </p>
      )}
      {checks.checks.length > 0 && (
        <ul className="space-y-1">
          {checks.checks.map((c, i) => (
            <li key={i}>
              {c.name}: {c.conclusion ?? c.state}{' '}
              <span className="text-secondary-light dark:text-secondary-dark">
                ({c.kind === 'check_run' ? 'check run' : 'commit status'})
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
function download(report: VerificationReport) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
  )
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `verification-${report.id}.json`
  anchor.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export function ReportCard({
  report,
  decision,
  current,
}: {
  report: VerificationReport
  decision?: ReviewDecision
  current: boolean
}) {
  const s = report.snapshot
  return (
    <article className={`${uiStyles.cardPadded} space-y-3`} data-testid="verification-report">
      <h3 className="font-medium">
        Verification report ·{' '}
        {current ? 'current recorded revision and run' : 'historical revision or run'}
      </h3>
      <p className="text-ui-caption">
        Recorded <Timestamp value={report.createdAt} /> · report{' '}
        <span className="break-all font-mono">{report.id}</span>
      </p>
      <p className="break-all font-mono">{report.revision}</p>
      <a
        className="text-apple-blue underline"
        href={`https://github.com/${s.repository}/${s.revisionKind === 'produced' ? `compare/${report.startingRevision}...${report.revision}` : `commit/${report.revision}`}`}
        target="_blank"
        rel="noopener noreferrer"
      >
        {s.revisionKind === 'produced' ? 'Review the exact change' : 'Review the starting revision'}
      </a>
      <dl className="space-y-2">
        <div>
          <dt className="font-medium">Allowed scope</dt>
          <dd className="whitespace-pre-wrap">{s.scope}</dd>
        </div>
        <div>
          <dt className="font-medium">Acceptance criteria</dt>
          <dd className="whitespace-pre-wrap">{report.criteria}</dd>
        </div>
        <div>
          <dt className="font-medium">Changes and known failures</dt>
          <dd className="whitespace-pre-wrap">{s.changeSummary}</dd>
        </div>
        <div>
          <dt className="font-medium">Environment and constraints</dt>
          <dd className="whitespace-pre-wrap">{s.environmentNotes}</dd>
        </div>
      </dl>
      {s.noArtifactReason && (
        <p className={uiStyles.note}>Missing change artifact: {s.noArtifactReason}</p>
      )}
      <div>
        <h4 className="font-medium">Reported checks</h4>
        {s.reportedChecks.length ? (
          <ul className="space-y-2">
            {s.reportedChecks.map((c, i) => (
              <li key={i}>
                <p>
                  {c.name}: {c.status.replace('_', ' ')}
                </p>
                <code className="block whitespace-pre-wrap break-all">{c.command}</code>
                <p className="whitespace-pre-wrap">{c.evidence}</p>
              </li>
            ))}
          </ul>
        ) : (
          <p>No checks were reported.</p>
        )}
      </div>
      <div>
        <h4 className="font-medium">Unverified areas</h4>
        {s.unverified.length ? (
          <ul className="list-inside list-disc">
            {s.unverified.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ul>
        ) : (
          <p>No unverified areas were listed by the report author.</p>
        )}
      </div>
      <CheckObservation
        checks={s.github}
        label="GitHub checks observed when this report was saved"
      />
      {s.runtime ? (
        <div>
          <h4 className="font-medium">Recorded execution</h4>
          <p>
            {s.runtime.cliTool ?? s.runtime.providerName ?? 'Runtime unspecified'} ·{' '}
            {s.runtime.state} · <Timestamp value={s.runtime.startedAt} />
          </p>
          <p className="break-all">Run {s.runtime.runId}</p>
          {s.cliVersion && <p>Reported Container CLI version: {s.cliVersion}</p>}
          {s.runtime.image && (
            <p className="break-all">
              Recorded image: {s.runtime.image.imageId} ·{' '}
              {s.runtime.image.manifestDigest ?? 'digest unavailable'}
            </p>
          )}
        </div>
      ) : (
        <p>No execution was recorded for this report.</p>
      )}
      <details>
        <summary className="cursor-pointer">
          Evidence references ({s.evidence.length}
          {!s.evidenceComplete ? '+' : ''})
        </summary>
        {s.evidence.length ? (
          <ul className="mt-2 space-y-1">
            {s.evidence.map((e) => (
              <li key={`${e.sourceType}:${e.sourceId}`} className="break-all">
                {e.sourceType}: {e.sourceId} · <Timestamp value={e.createdAt} />
              </li>
            ))}
          </ul>
        ) : (
          <p>No run evidence references were available when saved.</p>
        )}
        <p className={uiStyles.sectionDescription}>
          Open the task execution history for the referenced evidence. This report retains
          references and observed metadata.
        </p>
      </details>
      <div>
        <h4 className="font-medium">Human verdict</h4>
        {decision ? (
          <>
            <p>
              {decision.verdict} · <Timestamp value={decision.createdAt} />
            </p>
            <p className="whitespace-pre-wrap">{decision.reason}</p>
            <p>
              Recorded cumulative human effort:{' '}
              {decision.totalMinutes === null ? 'incomplete' : `${decision.totalMinutes} minutes`}
            </p>
            <dl className="grid grid-cols-2 gap-2 sm:grid-cols-3">
              {Object.entries(decision.humanMinutes).map(([category, minutes]) => (
                <div key={category}>
                  <dt className="capitalize">{category}</dt>
                  <dd>{minutes === null ? 'Unknown' : `${minutes} minutes`}</dd>
                </div>
              ))}
            </dl>
            {decision.github?.checkedAt && (
              <p>
                Acceptance head observation: <Timestamp value={decision.github.checkedAt} />
              </p>
            )}
          </>
        ) : (
          <p>No human verdict is loaded for this report.</p>
        )}
      </div>
      <button type="button" className={uiStyles.secondaryButton} onClick={() => download(report)}>
        Download report JSON
      </button>
    </article>
  )
}
