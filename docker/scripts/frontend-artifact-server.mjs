#!/usr/bin/env node
import { createServer } from 'node:http'
import { createReadStream, existsSync, statSync } from 'node:fs'
import { extname, join, normalize } from 'node:path'

const port = Number.parseInt(process.env.AGENTFORGE_PORT ?? process.env.PORT ?? '4003', 10)
const root = '/app/dist'
const indexPath = join(root, 'index.html')

const mimeTypes = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.ico': 'image/x-icon',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.txt': 'text/plain; charset=utf-8',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
}

function sendJson(res, status, body) {
  const payload = JSON.stringify(body)
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
  })
  res.end(payload)
}

function sendFile(res, filePath) {
  const type = mimeTypes[extname(filePath)] ?? 'application/octet-stream'
  const stat = statSync(filePath)
  res.writeHead(200, {
    'content-type': type,
    'content-length': stat.size,
    'cache-control': filePath === indexPath ? 'no-cache' : 'public, max-age=31536000, immutable',
  })
  createReadStream(filePath).pipe(res)
}

function resolvePath(urlPath) {
  const cleaned = normalize(urlPath.replace(/^\/+/, ''))
  const candidate = join(root, cleaned)
  if (!candidate.startsWith(root)) {
    return indexPath
  }
  if (existsSync(candidate) && statSync(candidate).isFile()) {
    return candidate
  }
  return indexPath
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? 'localhost'}`)
  if (url.pathname === '/health' || url.pathname === '/health/live') {
    sendJson(res, 200, { ok: true })
    return
  }

  if (req.method !== 'GET' && req.method !== 'HEAD') {
    sendJson(res, 405, { error: 'method_not_allowed' })
    return
  }

  const filePath = resolvePath(url.pathname === '/' ? '/index.html' : url.pathname)
  if (req.method === 'HEAD') {
    const type = mimeTypes[extname(filePath)] ?? 'application/octet-stream'
    const stat = statSync(filePath)
    res.writeHead(200, { 'content-type': type, 'content-length': stat.size })
    res.end()
    return
  }

  sendFile(res, filePath)
})

server.listen(port, '0.0.0.0', () => {
  console.log(`frontend artifact server listening on ${port}`)
})
