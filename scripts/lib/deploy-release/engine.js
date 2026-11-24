// Runs plans against an executor. The executor is the only thing that touches the world, so
// the same engine runs with the real Docker/psql effects and with the fake used by self-test.
import {
  StateUnreadable,
  TERMINAL,
  announce,
  complete,
  emptyState,
  markRecoveryRequired,
} from './journal.js'
import { RecoveryError, planDeploy, planRecovery } from './plan.js'

export const EXIT = {
  OK: 0,
  FAILED: 1,
  ROLLED_BACK: 10,
  RECOVERY_REQUIRED: 11,
  PREFLIGHT_REFUSED: 12,
  LOCK_BUSY: 13,
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
    ctx.state = (await ctx.exec.readState()) ?? emptyState()
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
    return code === EXIT.OK ? EXIT.FAILED : code
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
  const plan = planDeploy(ctx.config, ctx.state, await ctx.exec.preflight(ctx.state))
  if (plan.status !== 'plan') {
    plan.reasons.forEach((r) => ctx.log(`refused: ${r}`))
    return EXIT.PREFLIGHT_REFUSED
  }
  plan.notes.forEach((n) => ctx.log(n))
  await save(ctx, {
    ...ctx.state,
    records: { ...ctx.state.records, [ctx.config.target]: plan.record },
    attempt: plan.attempt,
  })
  try {
    await runSteps(ctx, plan.steps)
    ctx.log(`deployed ${ctx.config.target}`)
    return EXIT.OK
  } catch (err) {
    if (err instanceof Crash) {
      throw err
    }
    ctx.log(`deploy failed at ${ctx.state.attempt.step?.op ?? 'unknown step'}: ${err.message}`)
    const code = await rollBack(ctx, { restartHelpers: true })
    return code === EXIT.OK ? EXIT.ROLLED_BACK : code
  }
}

async function rollBack(ctx, { restartHelpers }) {
  const attempt = ctx.state.attempt
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
    await save(ctx, markRecoveryRequired(ctx.state, message, now(ctx)))
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
  ctx.log(`recovering attempt ${attempt.attemptId} from phase ${attempt.phase}`)
  // withSqlHelper removes every leftover helper of this project (and confirms) before planning.
  return withSqlHelper(ctx, EXIT.RECOVERY_REQUIRED, () => rollBack(ctx, { restartHelpers: false }))
}
