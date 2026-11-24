// Runs plans against an executor. The executor is the only thing that touches the world, so
// the same engine runs with the real Docker/psql effects and with the fake used by self-test.
import {
  COMMITTED,
  StateUnreadable,
  TERMINAL,
  announce,
  complete,
  emptyState,
  markRecoveryRequired,
  releaseId,
  validateState,
  withPointers,
} from './journal.js'
import { RecoveryError, planDeploy, planRecovery } from './plan.js'

export const EXIT = {
  OK: 0,
  FAILED: 1,
  ROLLED_BACK: 10,
  RECOVERY_REQUIRED: 11,
  PREFLIGHT_REFUSED: 12,
  LOCK_BUSY: 13,
  // The new release is committed and was left running; only finishing work failed.
  FINALIZE_FAILED: 14,
  // The commit write was attempted and its durability is unknown; nothing was stopped.
  COMMIT_UNKNOWN: 15,
}

// Thrown only by the fake executor to model SIGKILL / host loss: nothing may catch it.
export class Crash extends Error {}

const now = (ctx) => (ctx.now ? ctx.now() : new Date().toISOString())

async function save(ctx, state) {
  const next = withPointers(state)
  await ctx.exec.writeState(next)
  ctx.state = next
  return next
}

const validation = (ctx) => ({
  ledgerKeys: ctx.config.databases.filter((d) => d.migrations).map((d) => d.key),
})

// Returns false (after explaining) when the state file exists but cannot be read.
export async function loadState(ctx) {
  try {
    ctx.state = validateState((await ctx.exec.readState()) ?? emptyState(), validation(ctx))
    return true
  } catch (err) {
    if (!(err instanceof StateUnreadable)) {
      throw err
    }
    ctx.log(`RECOVERY REQUIRED: the deploy state file cannot be read (${err.message}).`)
    ctx.log('Do not deploy and do not start services by hand. The journal is the only record of')
    ctx.log(
      'which databases are snapshots. Inspect state.json in the private root (and state.json.tmp'
    )
    ctx.log('if present); repair or restore it, then run `status` and `recover`.')
    return false
  }
}

// The SQL client helper lives for the locked work only and is removed on every path; a helper
// that cannot be removed is reported as a failure. A crash (fake SIGKILL) skips this on purpose:
// the next recover removes the leftover.
async function withSqlHelper(ctx, refusal, body) {
  try {
    await ctx.exec.removeHelpers()
    await ctx.exec.startSql()
  } catch (err) {
    ctx.log(`refused: helper containers could not be replaced: ${err.message}`)
    ctx.log(
      'Remove them by hand (`docker ps -a --filter label=forge.deploy.project=<project>`), then retry.'
    )
    return refusal
  }
  let code
  try {
    code = await body()
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    await stopSqlHelper(ctx, EXIT.FAILED)
    throw err
  }
  return stopSqlHelper(ctx, code)
}

async function stopSqlHelper(ctx, code) {
  try {
    await ctx.exec.stopSql()
    return code
  } catch (err) {
    ctx.log(`ERROR: the SQL helper container could not be removed: ${err.message}`)
    return code === EXIT.OK ? EXIT.FINALIZE_FAILED : code
  }
}

async function runSteps(ctx, steps) {
  for (const step of steps) {
    if (step.phase === 'verified' && ctx.state.attempt.phase !== 'verified') {
      ctx.commitAttempted = true
    }
    await save(ctx, announce(ctx.state, step, now(ctx)))
    let result = {}
    try {
      result = (await ctx.exec.run(step, ctx.state)) ?? {}
    } catch (err) {
      if (!step.bestEffort || err instanceof Crash) {
        throw err
      }
      ctx.log(`warning: ${step.op} did not complete: ${err.message}`)
    }
    await save(ctx, complete(ctx.state, step, result, now(ctx)))
  }
}

export async function deploy(ctx) {
  if (!(await loadState(ctx))) {
    return EXIT.RECOVERY_REQUIRED
  }
  const { attempt, current } = ctx.state
  if (attempt && !TERMINAL.has(attempt.phase)) {
    ctx.log(`refused: attempt ${attempt.attemptId} is unfinished (${attempt.phase}); run recover`)
    return EXIT.PREFLIGHT_REFUSED
  }
  if (!current || !ctx.state.records[current]) {
    ctx.log('refused: no current release is recorded: run `baseline` once before the first deploy')
    return EXIT.PREFLIGHT_REFUSED
  }
  // Round 5, F3: the staged inputs are read and checked before anything is persisted.
  let staged
  try {
    staged = await ctx.exec.readStaged()
  } catch (err) {
    ctx.log(`refused: the staged inputs cannot be used (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  // Round 5, F4a: a release is (revision, env content). Same both: nothing to do. Same revision
  // with another env (for example a rotated credential) is a normal deploy of a new release.
  const live = ctx.state.records[current]
  if (live.revision === ctx.config.target && live.inputs.envSha256 === staged.envSha256) {
    ctx.log(`${ctx.config.target} with this env file is already the current release; nothing to do`)
    return EXIT.OK
  }
  const targetId = releaseId(ctx.config.target, staged.envSha256)
  return withSqlHelper(ctx, EXIT.PREFLIGHT_REFUSED, () => deployLocked(ctx, targetId, staged))
}

async function deployLocked(ctx, targetId, staged) {
  let observed
  try {
    observed = await ctx.exec.preflight(ctx.state, targetId)
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    ctx.log(`refused: preflight could not run (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  const planConfig = { ...ctx.config, target: targetId, revision: ctx.config.target }
  const plan = planDeploy(planConfig, ctx.state, observed, { at: now(ctx) })
  if (plan.status !== 'plan') {
    plan.reasons.forEach((r) => ctx.log(`refused: ${r}`))
    return EXIT.PREFLIGHT_REFUSED
  }
  plan.notes.forEach((n) => ctx.log(n))
  const inputs = { generation: plan.attempt.attemptId, ...staged }
  const record = { ...plan.record, rev: targetId, revision: ctx.config.target, inputs }
  const intent = {
    ...ctx.state,
    records: { ...ctx.state.records, [targetId]: record },
    attempt: { ...plan.attempt, inputs },
  }
  // Persisted state that went bad is a recovery matter (11); a bad new intent changed nothing (12).
  try {
    validateState(ctx.state, validation(ctx))
  } catch (err) {
    ctx.log(
      `RECOVERY REQUIRED: the persisted deploy state is invalid (${err.message}); nothing was changed.`
    )
    return EXIT.RECOVERY_REQUIRED
  }
  try {
    validateState(intent, validation(ctx))
  } catch (err) {
    ctx.log(`refused: the new attempt would be invalid (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  // One rename makes the staged inputs the attempt's generation; the intent then points to it.
  // A crash in between leaves an orphan generation that nothing uses.
  const activated = await ctx.exec.activateStaged(plan.attempt.attemptId)
  if (['envSha256', 'identity', 'runner'].some((k) => activated[k] !== staged[k])) {
    ctx.log(
      'refused: the staged inputs changed while this deploy prepared (nothing was deployed); stage again'
    )
    return EXIT.PREFLIGHT_REFUSED
  }
  await save(ctx, intent)
  try {
    await runSteps(ctx, plan.steps)
    ctx.log(`deployed ${targetId}`)
    return EXIT.OK
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    const outcome = await commitOutcome(ctx)
    if (outcome === 'committed') {
      return finalizeFailed(ctx, err)
    }
    if (outcome === 'unknown') {
      return commitUnknown(ctx, err)
    }
    ctx.log(`deploy failed at ${ctx.state.attempt.step?.op ?? 'unknown step'}: ${err.message}`)
    const code = await rollBack(ctx, { restartHelpers: true })
    return code === EXIT.OK ? EXIT.ROLLED_BACK : code
  }
}

// Round 4, C1: once the commit write was attempted, the outcome is committed, not committed or
// unknown. Only "the disk still shows the pre-commit journal" proves not committed; a rename
// that happened but could not be made durable, or a journal that cannot be re-read, is unknown.
async function commitOutcome(ctx) {
  const isCommitted = (phase) => COMMITTED.has(phase) || phase === 'done'
  if (!ctx.commitAttempted) {
    return 'none'
  }
  if (isCommitted(ctx.state.attempt?.phase)) {
    return 'committed'
  }
  // Round 5, F1: "not committed" only from a successfully read, VALID journal of the SAME attempt
  // that still shows a pre-commit phase. Absent, unreadable, invalid or foreign: unknown.
  let disk
  try {
    disk = validateState(await ctx.exec.readState(), validation(ctx))
  } catch {
    return 'unknown'
  }
  if (disk.attempt?.attemptId !== ctx.state.attempt.attemptId || isCommitted(disk.attempt.phase)) {
    return 'unknown'
  }
  return 'none'
}

function commitUnknown(ctx, err) {
  ctx.log(`COMMIT OUTCOME UNKNOWN: ${err.message}`)
  ctx.log(
    'The commit record may or may not be durable. Nothing was stopped and nothing was rolled back.'
  )
  ctx.log(
    'Next: repair the cause, then run `recover`; it decides from the journal on disk and the running stack.'
  )
  return EXIT.COMMIT_UNKNOWN
}

function finalizeFailed(ctx, err) {
  ctx.log(`FINALIZE FAILED: ${err.message}`)
  ctx.log(
    `The new release ${ctx.state.attempt.target} is committed and was left running; nothing was stopped.`
  )
  ctx.log(
    'Next: repair the cause (for example disk space), then run `recover` to confirm and close.'
  )
  return EXIT.FINALIZE_FAILED
}

async function rollBack(ctx, { restartHelpers }) {
  const attempt = ctx.state.attempt
  if (COMMITTED.has(attempt.phase)) {
    try {
      await runSteps(ctx, planRecovery(attempt, null))
      ctx.log(`attempt ${attempt.attemptId}: committed release confirmed running; closed`)
      return EXIT.OK
    } catch (err) {
      if (err instanceof Crash) {
        throw err
      }
      return finalizeFailed(ctx, err)
    }
  }
  try {
    // A helper that outlived a failed step could still be writing; it must be gone first.
    if (restartHelpers) {
      await ctx.exec.removeHelpers()
      await ctx.exec.startSql()
    }
    const observed = await ctx.exec.observe(attempt)
    await runSteps(ctx, planRecovery(attempt, observed))
    ctx.log(`attempt ${attempt.attemptId} finished in phase ${ctx.state.attempt.phase}`)
    return EXIT.OK
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    const where = ctx.state.attempt.step?.op ?? 'planning'
    const message = `${err instanceof RecoveryError ? '' : `${where}: `}${err.message}`
    try {
      await ctx.exec.run({ op: 'stop_project', phase: 'rolling_back' }, ctx.state)
    } catch (stopErr) {
      ctx.log(`warning: could not stop project services: ${stopErr.message}`)
    }
    try {
      await save(ctx, markRecoveryRequired(ctx.state, message, now(ctx)))
    } catch (writeErr) {
      ctx.log(`the journal could not record recovery_required either: ${writeErr.message}`)
    }
    ctx.log(`RECOVERY REQUIRED: ${message}`)
    ctx.log(
      'All project services were asked to stop. Every later deploy refuses until recover succeeds.'
    )
    ctx.log(
      'Next: run `status`, repair the cause above, then run `recover` (Jenkins ACTION=recover).'
    )
    return EXIT.RECOVERY_REQUIRED
  }
}

export async function recover(ctx) {
  if (!(await loadState(ctx))) {
    return EXIT.RECOVERY_REQUIRED
  }
  const attempt = ctx.state.attempt
  if (!attempt || TERMINAL.has(attempt.phase)) {
    // A runner killed after its last journal write can still leave helpers behind.
    await ctx.exec.removeHelpers()
    ctx.log('no unfinished attempt; nothing to recover')
    return EXIT.OK
  }
  // An unfinished attempt is recovered with the inputs its journal points to. Round 5, F4b: a
  // changed secret is allowed (credential rotation); a changed deployment identity is not.
  const facts = await ctx.exec.inputsFacts(attempt.inputs.generation)
  if (!facts || facts.identity !== attempt.inputs.identity) {
    ctx.log(
      `RECOVERY REQUIRED: input generation ${attempt.inputs.generation} is missing or now names another deployment.`
    )
    ctx.log(
      'Restore the identity fields of inputs/<generation>/deploy.env (runbook: Rotating a credential).'
    )
    return EXIT.RECOVERY_REQUIRED
  }
  if (facts.envSha256 !== attempt.inputs.envSha256) {
    ctx.log(
      'note: the env file differs from the recorded one; the deployment identity is unchanged'
    )
  }
  ctx.log(`recovering attempt ${attempt.attemptId} from phase ${attempt.phase}`)
  // withSqlHelper removes every leftover helper of this project (and confirms) before planning.
  return withSqlHelper(ctx, EXIT.RECOVERY_REQUIRED, () => rollBack(ctx, { restartHelpers: false }))
}

// D5: record the hand-deployed release once. Its inputs become the first generation with one
// rename; a crash before the state write leaves an orphan, and re-staging lets it run again.
export async function baseline(ctx) {
  if (!(await loadState(ctx))) {
    return EXIT.RECOVERY_REQUIRED
  }
  if (ctx.state.current) {
    ctx.log(
      `refused: a current release (${ctx.state.current}) is already recorded; baseline runs once`
    )
    return EXIT.PREFLIGHT_REFUSED
  }
  // Round 6, S3: staged inputs first, before anything renders or tags.
  let staged
  try {
    staged = await ctx.exec.readStaged()
  } catch (err) {
    ctx.log(`refused: the staged inputs cannot be used (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  let findings
  let record
  try {
    ;({ findings, record } = await ctx.exec.baseline())
  } catch (err) {
    ctx.log(`refused: ${err.message} (no baseline was recorded)`)
    return EXIT.PREFLIGHT_REFUSED
  }
  if (findings.length > 0) {
    findings.forEach((f) => ctx.log(`refused: ${f}`))
    return EXIT.PREFLIGHT_REFUSED
  }
  const rec = {
    ...record,
    rev: 'baseline',
    revision: 'baseline',
    inputs: { generation: ctx.config.attemptId, ...staged },
  }
  const next = {
    ...ctx.state,
    current: 'baseline',
    records: { ...ctx.state.records, baseline: rec },
  }
  try {
    validateState(next, validation(ctx))
  } catch (err) {
    ctx.log(`refused: the baseline record would be invalid (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  const activated = await ctx.exec.activateStaged(ctx.config.attemptId)
  if (['envSha256', 'identity', 'runner'].some((k) => activated[k] !== staged[k])) {
    ctx.log('refused: the staged inputs changed while baseline ran; stage again')
    return EXIT.PREFLIGHT_REFUSED
  }
  await save(ctx, next)
  ctx.log('baseline recorded: the next deploy can roll back to the running release')
  return EXIT.OK
}
