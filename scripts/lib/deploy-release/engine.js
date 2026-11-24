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
  validateState,
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
}

// Thrown only by the fake executor to model SIGKILL / host loss: nothing may catch it.
export class Crash extends Error {}

const now = (ctx) => (ctx.now ? ctx.now() : new Date().toISOString())

async function save(ctx, state) {
  await ctx.exec.writeState(state)
  ctx.state = state
  return state
}

// Returns false (after explaining) when the state file exists but cannot be read.
export async function loadState(ctx) {
  try {
    ctx.state = validateState((await ctx.exec.readState()) ?? emptyState())
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
  if (current === ctx.config.target) {
    ctx.log(`${ctx.config.target} is already the current release; nothing to do`)
    return EXIT.OK
  }
  if (!current || !ctx.state.records[current]) {
    ctx.log('refused: no current release is recorded: run `baseline` once before the first deploy')
    return EXIT.PREFLIGHT_REFUSED
  }
  return withSqlHelper(ctx, EXIT.PREFLIGHT_REFUSED, () => deployLocked(ctx))
}

async function deployLocked(ctx) {
  let observed
  try {
    observed = await ctx.exec.preflight(ctx.state)
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    ctx.log(`refused: preflight could not run (nothing was changed): ${err.message}`)
    return EXIT.PREFLIGHT_REFUSED
  }
  const plan = planDeploy(ctx.config, ctx.state, observed, { at: now(ctx) })
  if (plan.status !== 'plan') {
    plan.reasons.forEach((r) => ctx.log(`refused: ${r}`))
    return EXIT.PREFLIGHT_REFUSED
  }
  plan.notes.forEach((n) => ctx.log(n))
  // Staged inputs become active together with the intent; an unfinished attempt never gets here.
  const attempt = { ...plan.attempt, inputs: await ctx.exec.promoteInputs() }
  await save(ctx, {
    ...ctx.state,
    records: { ...ctx.state.records, [ctx.config.target]: plan.record },
    attempt,
  })
  try {
    await runSteps(ctx, plan.steps)
    ctx.log(`deployed ${ctx.config.target}`)
    return EXIT.OK
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    if (await committed(ctx)) {
      return finalizeFailed(ctx, err)
    }
    ctx.log(`deploy failed at ${ctx.state.attempt.step?.op ?? 'unknown step'}: ${err.message}`)
    const code = await rollBack(ctx, { restartHelpers: true })
    return code === EXIT.OK ? EXIT.ROLLED_BACK : code
  }
}

// Round-3 finding 2: once the commit record may be on disk, no path stops services or rolls back.
async function committed(ctx) {
  const isCommitted = (phase) => COMMITTED.has(phase) || phase === 'done'
  if (isCommitted(ctx.state.attempt?.phase)) {
    return true
  }
  try {
    return isCommitted((await ctx.exec.readState())?.attempt?.phase)
  } catch {
    return false
  }
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
  // Round-3 finding 5: an unfinished attempt is recovered with the inputs it started with.
  if ((await ctx.exec.activeInputs()).envSha256 !== attempt.inputs.envSha256) {
    ctx.log('RECOVERY REQUIRED: the active env file is not the one this attempt started with.')
    ctx.log('Restore that env file (deploy.env in the private root) and run recover again.')
    return EXIT.RECOVERY_REQUIRED
  }
  ctx.log(`recovering attempt ${attempt.attemptId} from phase ${attempt.phase}`)
  // withSqlHelper removes every leftover helper of this project (and confirms) before planning.
  return withSqlHelper(ctx, EXIT.RECOVERY_REQUIRED, () => rollBack(ctx, { restartHelpers: false }))
}
