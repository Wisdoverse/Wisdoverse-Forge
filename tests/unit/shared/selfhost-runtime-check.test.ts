import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const runtimeCheckScript = fs.readFileSync(
  path.join(projectRoot, 'scripts/check-selfhost-runtime.sh'),
  'utf8'
)
const beginnerAuditScript = fs.readFileSync(
  path.join(projectRoot, 'scripts/audit-beginner-selfhost.sh'),
  'utf8'
)

describe('self-host runtime check', () => {
  it('can verify the public default ingress ports explicitly', () => {
    expect(runtimeCheckScript).toContain('--public-ingress')
    expect(runtimeCheckScript).toContain('Public HTTP :80 redirects to HTTPS')
    expect(runtimeCheckScript).toContain('Public HTTPS :443 has trusted TLS')
    expect(runtimeCheckScript).toContain('http://${DOMAIN}/')
    expect(runtimeCheckScript).toContain('https://${DOMAIN}/')
  })

  it('can bypass CDN DNS and verify the origin VPS directly', () => {
    expect(runtimeCheckScript).toContain('--origin-ip')
    expect(runtimeCheckScript).toContain('BEGINNER_ORIGIN_IP')
    expect(runtimeCheckScript).toContain('curl_body()')
    expect(runtimeCheckScript).toContain('--resolve "${DOMAIN}:${port}:${ORIGIN_IP}"')
    expect(runtimeCheckScript).toContain('--resolve "${DOMAIN}:80:${ORIGIN_IP}"')
    expect(runtimeCheckScript).toContain('--resolve "${DOMAIN}:443:${ORIGIN_IP}"')
    expect(runtimeCheckScript).toContain('Frontend shell through origin ingress')
    expect(runtimeCheckScript).toContain('Rust API /api/health through origin ingress')
  })

  it('uses the strict public ingress check for beginner live audits', () => {
    expect(beginnerAuditScript).toContain('--public-ingress')
    expect(beginnerAuditScript).toContain('BEGINNER_ORIGIN_IP')
    expect(beginnerAuditScript).toContain('trusted :443 TLS')
    expect(beginnerAuditScript).toContain('live origin ingress has :80 redirect')
  })
})
