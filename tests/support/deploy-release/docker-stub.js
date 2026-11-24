// A stand-in `docker` executable for tests (round 6, S1/S2). It records every argv plus the names
// of the PG* variables it received, and answers from a rule table: the first rule whose regex
// matches the space-joined argv wins (a rule with `once` applies only the first time); the default
// is exit 0 with no output. The effect layer passes only a small environment to docker, so the
// stub finds its log and rules next to itself.
import fs from 'node:fs'
import path from 'node:path'

const STUB = `#!/usr/bin/env node
const fs = require('fs')
const path = require('path')
const here = path.dirname(fs.realpathSync(process.argv[1]))
const argv = process.argv.slice(2)
const pg = Object.keys(process.env).filter((k) => k.startsWith('PG')).sort()
fs.appendFileSync(path.join(here, 'calls.log'), JSON.stringify({ argv, pg, version: process.env.VERSION ?? null, parallel: process.env.COMPOSE_PARALLEL_LIMIT ?? null }) + '\\n')
const rules = JSON.parse(fs.readFileSync(path.join(here, 'rules.json'), 'utf8'))
const usedFile = path.join(here, 'used.json')
const used = fs.existsSync(usedFile) ? JSON.parse(fs.readFileSync(usedFile, 'utf8')) : []
const line = argv.join(' ')
for (const [i, rule] of rules.entries()) {
  if (rule.once && used.includes(i)) continue
  if (new RegExp(rule.match).test(line)) {
    if (rule.once) fs.writeFileSync(usedFile, JSON.stringify([...used, i]))
    if (rule.out) process.stdout.write(rule.out)
    if (rule.err) process.stderr.write(rule.err)
    process.exit(rule.code ?? 0)
  }
}
process.exit(0)
`

export function dockerStub(dir) {
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'docker'), STUB, { mode: 0o755 })
  fs.writeFileSync(path.join(dir, 'rules.json'), '[]')
  const log = path.join(dir, 'calls.log')
  return {
    dir,
    rules: (rules) => fs.writeFileSync(path.join(dir, 'rules.json'), JSON.stringify(rules)),
    calls: () =>
      fs.existsSync(log)
        ? fs
            .readFileSync(log, 'utf8')
            .split('\n')
            .filter(Boolean)
            .map((l) => JSON.parse(l))
        : [],
    reset: () => fs.rmSync(log, { force: true }),
  }
}
