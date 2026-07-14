// scripts/ui-qa-screens.mjs
// UI QA: screenshot app routes in both themes against a running preview.
// Prereqs: API healthy on :4003, `npm run build` done, AF_QA_PASSWORD set.
// Usage: node scripts/ui-qa-screens.mjs [--routes /a,/b] [--base http://localhost:4502] [--out .ui-qa]
/* eslint-disable import/order, no-undef */
import { chromium } from '@playwright/test'
import { mkdirSync } from 'node:fs'

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`)
  return i > -1 ? process.argv[i + 1] : dflt
}
const BASE = arg('base', 'http://localhost:4502')
const OUT = arg('out', '.ui-qa')
const ROUTES = arg(
  'routes',
  '/,/tasks,/agents,/inbox,/context,/context/audit,/skills,/analytics,/billing,/settings,/admin,/start,/login'
).split(',')

const email = 'dev@example.com'
const password = process.env.AF_QA_PASSWORD
if (!password) throw new Error('AF_QA_PASSWORD not set')

const res = await fetch('http://localhost:4003/api/v1/auth/login', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ email, password }),
})
if (!res.ok) throw new Error(`login failed: ${res.status}`)
const body = await res.json()
const token = body.access_token ?? body.data?.access_token
if (!token) throw new Error('no access_token in login response')

mkdirSync(OUT, { recursive: true })
const browser = await chromium.launch()
for (const theme of ['light', 'dark']) {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } })
  await ctx.addInitScript(
    ([t, tk]) => {
      localStorage.setItem('agentforge-theme', t)
      localStorage.setItem('af:auth:access', tk)
    },
    [theme, token]
  )
  const page = await ctx.newPage()
  for (const route of ROUTES) {
    await page.goto(BASE + route, { waitUntil: 'networkidle' })
    const slug = route === '/' ? 'root' : route.replaceAll('/', '_').replace(/^_/, '')
    await page.screenshot({ path: `${OUT}/${slug}-${theme}.png`, fullPage: false })
    console.log(`${OUT}/${slug}-${theme}.png`)
  }
  await ctx.close()
}
await browser.close()
