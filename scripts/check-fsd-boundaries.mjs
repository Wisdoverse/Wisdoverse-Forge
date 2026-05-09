#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'

const root = process.cwd()
const appRoot = path.join(root, 'src/app')
const extensions = ['.tsx', '.ts', '.jsx', '.js']
const layerRank = new Map([
  ['shared', 0],
  ['entities', 1],
  ['features', 2],
  ['widgets', 3],
  ['pages', 4],
  ['app', 5],
])

const appLayerDirs = new Set(['routes', 'layouts', 'providers', 'hooks', 'i18n', 'styles'])

const importPattern =
  /(?:import|export)\s+(?:type\s+)?(?:[^'"]*?\s+from\s*)?['"]([^'"]+)['"]|import\(\s*['"]([^'"]+)['"]\s*\)/g

function walk(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true })
  return entries.flatMap((entry) => {
    const fullPath = path.join(dir, entry.name)
    if (entry.isDirectory()) return walk(fullPath)
    if (!extensions.some((ext) => fullPath.endsWith(ext))) return []
    return [fullPath]
  })
}

function resolveFile(base) {
  if (fs.existsSync(base) && fs.statSync(base).isFile()) return base
  for (const ext of extensions) {
    const withExt = `${base}${ext}`
    if (fs.existsSync(withExt)) return withExt
  }
  for (const ext of extensions) {
    const indexFile = path.join(base, `index${ext}`)
    if (fs.existsSync(indexFile)) return indexFile
  }
  return null
}

function toPosix(filePath) {
  return path.relative(root, filePath).split(path.sep).join('/')
}

function classify(filePath) {
  const relative = path.relative(appRoot, filePath).split(path.sep)
  const first = relative[0]
  if (first === 'shared') return { layer: 'shared', slice: relative[1] ?? null }
  if (first === 'entities') return { layer: 'entities', slice: relative[1] ?? null }
  if (first === 'features') return { layer: 'features', slice: relative[1] ?? null }
  if (first === 'widgets') return { layer: 'widgets', slice: relative[1] ?? null }
  if (first === 'pages') return { layer: 'pages', slice: relative[1] ?? null }
  if (appLayerDirs.has(first) || relative.length === 1) return { layer: 'app', slice: first }
  return { layer: 'app', slice: first }
}

function resolveSpecifier(sourceFile, specifier) {
  if (specifier.startsWith('@shared/')) return { kind: 'external-shared' }
  if (specifier.startsWith('@/')) return { kind: 'forbidden-alias' }
  if (specifier.startsWith('@app/')) {
    const resolved = resolveFile(path.join(appRoot, specifier.slice('@app/'.length)))
    return resolved ? { kind: 'app-file', file: resolved } : { kind: 'unresolved-app-alias' }
  }
  if (specifier.startsWith('.')) {
    const resolved = resolveFile(path.resolve(path.dirname(sourceFile), specifier))
    if (!resolved) return { kind: 'unresolved-relative' }
    if (!resolved.startsWith(`${appRoot}${path.sep}`) && resolved !== appRoot) {
      return { kind: 'outside-app', file: resolved }
    }
    return { kind: 'app-file', file: resolved }
  }
  return { kind: 'package' }
}

function isAllowedLayerImport(source, target) {
  const sourceRank = layerRank.get(source.layer)
  const targetRank = layerRank.get(target.layer)
  if (sourceRank === undefined || targetRank === undefined) return false
  if (targetRank > sourceRank) return false

  if (source.layer === 'features' && target.layer === 'features') {
    return source.slice === target.slice
  }

  return true
}

function collectImports(source) {
  const content = fs.readFileSync(source, 'utf8')
  const imports = []
  for (const match of content.matchAll(importPattern)) {
    imports.push(match[1] ?? match[2])
  }
  return imports
}

const errors = []

for (const sourceFile of walk(appRoot)) {
  const sourceLayer = classify(sourceFile)
  for (const specifier of collectImports(sourceFile)) {
    const resolved = resolveSpecifier(sourceFile, specifier)

    if (resolved.kind === 'package' || resolved.kind === 'external-shared') continue

    if (resolved.kind === 'forbidden-alias') {
      errors.push(
        `${toPosix(sourceFile)} imports ${specifier}; use @app/* or @shared/* inside src/app`
      )
      continue
    }

    if (resolved.kind === 'outside-app') {
      errors.push(`${toPosix(sourceFile)} imports ${specifier}, which resolves outside src/app`)
      continue
    }

    if (resolved.kind === 'unresolved-app-alias' || resolved.kind === 'unresolved-relative') {
      errors.push(`${toPosix(sourceFile)} imports unresolved module ${specifier}`)
      continue
    }

    const targetLayer = classify(resolved.file)
    if (!isAllowedLayerImport(sourceLayer, targetLayer)) {
      errors.push(
        `${toPosix(sourceFile)} (${sourceLayer.layer}/${sourceLayer.slice ?? '-'}) imports ${specifier} (${targetLayer.layer}/${targetLayer.slice ?? '-'})`
      )
    }
  }
}

if (errors.length > 0) {
  console.error('FSD boundary check failed:')
  for (const error of errors) {
    console.error(`- ${error}`)
  }
  process.exit(1)
}

console.log('FSD boundary check passed.')
