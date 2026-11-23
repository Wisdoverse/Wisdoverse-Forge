// Runs plans against an executor. The executor is the only thing that touches the world, so
// the same engine runs with the real Docker/psql effects and with the fake in simulate.js.
import { TERMINAL, announce, complete, emptyState, markRecoveryRequired } from './journal.js'
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

async function runSteps(ctx, steps) {
  for (const step of steps) {
    await save(ctx, announce(ctx.state, step, now(ctx)))
    let result = {}
    try {
      result = (await ctx.exec.run?.(step, ctx.state)) ?? {}
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
  ctx.state = (await ctx.exec.readState()) ?? emptyState()
  const unfinished = ctx.state.attempt && !TERMINAL.has(ctx.state.attempt.phase)
  if (unfinished) {
    ctx.log(
      `refused: attempt ${ctx.state.attempt.attemptId} is unfinished (${ctx.state.attempt.phase}); run recover`
    )
    return EXIT.PREFLIGHT_REFUSED
  }
  if (ctx.state.current === ctx.config.target) {
    ctx.log(`${ctx.config.target} is already the current release; nothing to do`)
    return EXIT.OK
  }
  const observed = await ctx.exec.preflight(ctx.state)
  const plan = planDeploy(ctx.config, ctx.state, observed)
  if (plan.status !== 'plan') {
    plan.reasons.forEach((r) => ctx.log(`${plan.status === 'noop' ? '' : 'refused: '}${r}`))
    return plan.status === 'noop' ? EXIT.OK : EXIT.PREFLIGHT_REFUSED
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
    const code = await rollBack(ctx)
    return code === EXIT.OK ? EXIT.ROLLED_BACK : code
  }
}

async function rollBack(ctx) {
  const attempt = ctx.state.attempt
  try {
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
  ctx.state = (await ctx.exec.readState()) ?? emptyState()
  const attempt = ctx.state.attempt
  if (!attempt || TERMINAL.has(attempt.phase)) {
    ctx.log('no unfinished attempt; nothing to recover')
    return EXIT.OK
  }
  ctx.log(`recovering attempt ${attempt.attemptId} from phase ${attempt.phase}`)
  return rollBack(ctx)
}
