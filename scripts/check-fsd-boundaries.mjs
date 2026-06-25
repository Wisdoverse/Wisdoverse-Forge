#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const extensions = ['.tsx', '.ts', '.jsx', '.js']
// F074: `unknown` (an unrecognised src/app dir) is deliberately NOT in this rank
// map. The old default classified such dirs as `app` (highest), letting them
// import from any layer with no violation flagged. They are now handled
// explicitly in `isAllowedLayerImport` (import-only-shared, importable-by-none),
// which a single rank cannot express.
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
const staticPageEntrypointPattern =
  /(?:import\s+(?:type\s+)?([^'";]+?)\s+from|export\s+(?:type\s+)?([^'";]+?)\s+from)\s*['"]([^'"]+)['"]/g

function walk(dir) {
  if (!fs.existsSync(dir)) return []
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

function toPosix(root, filePath) {
  return path.relative(root, filePath).split(path.sep).join('/')
}

function classify(appRoot, filePath) {
  const relative = path.relative(appRoot, filePath).split(path.sep)
  const first = relative[0]
  if (first === 'shared') return { layer: 'shared', slice: relative[1] ?? null }
  if (first === 'entities') return { layer: 'entities', slice: relative[1] ?? null }
  if (first === 'features') return { layer: 'features', slice: relative[1] ?? null }
  if (first === 'widgets') return { layer: 'widgets', slice: relative[1] ?? null }
  if (first === 'pages') return { layer: 'pages', slice: relative[1] ?? null }
  if (appLayerDirs.has(first) || relative.length === 1) return { layer: 'app', slice: first }
  // F074: an unrecognised multi-segment dir (not an FSD layer, not a known app
  // dir, not a single top-level file) is `unknown` — ranked lowest, not `app`,
  // so it cannot silently import from higher layers.
  return { layer: 'unknown', slice: first }
}

function resolveSpecifier(appRoot, sourceFile, specifier) {
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
  // F074: an unrecognised src/app dir is `unknown`. It is not a valid module
  // location, so NOTHING may import from it (importing an unknown dir is itself a
  // violation), and it may itself depend ONLY on `shared`. This is asymmetric, so
  // it cannot be expressed by a single rank — a misplaced dir is surfaced rather
  // than silently bypassing the layer rules as the old `app`-rank default did.
  if (target.layer === 'unknown') return false
  if (source.layer === 'unknown') return target.layer === 'shared'

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

function shouldRouteUsePageEntrypoint(appRoot, sourceFile) {
  const routesRoot = path.join(appRoot, 'routes')
  const relative = path.relative(routesRoot, sourceFile).split(path.sep).join('/')
  if (relative.startsWith('..') || path.isAbsolute(relative)) return false
  return !['__root.tsx', '__root.ts', 'landing.ts', 'public-auth.ts'].includes(relative)
}

function isRouteEntrypointBypass(source, target) {
  if (source.layer !== 'app' || source.slice !== 'routes') return false
  return target.layer === 'features' || target.layer === 'widgets'
}

function collectNamedImports(importClause) {
  const namedMatch = importClause.match(/\{([^}]+)\}/)
  if (!namedMatch) {
    const defaultImport = importClause.split(',')[0]?.trim()
    return defaultImport ? [defaultImport] : []
  }

  return namedMatch[1]
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) =>
      part
        .replace(/^type\s+/, '')
        .split(/\s+as\s+/i)[0]
        ?.trim()
    )
    .filter(Boolean)
}

function collectRoutePageEntrypointErrors(root, appRoot, sourceFile, sourceLayer) {
  if (!shouldRouteUsePageEntrypoint(appRoot, sourceFile)) return []

  const errors = []
  const content = fs.readFileSync(sourceFile, 'utf8')
  for (const match of content.matchAll(staticPageEntrypointPattern)) {
    const importClause = match[1] ?? match[2]
    const specifier = match[3]
    const resolved = resolveSpecifier(appRoot, sourceFile, specifier)
    if (resolved.kind !== 'app-file') continue

    const targetLayer = classify(appRoot, resolved.file)
    if (
      sourceLayer.layer !== 'app' ||
      sourceLayer.slice !== 'routes' ||
      targetLayer.layer !== 'pages'
    ) {
      continue
    }

    for (const importedName of collectNamedImports(importClause)) {
      if (!importedName.endsWith('Page')) {
        errors.push(
          `${toPosix(root, sourceFile)} imports ${importedName} from ${specifier}; route files must import Page entrypoints from @app/pages/*`
        )
      }
    }
  }

  return errors
}

export function checkFsdBoundaries({ cwd = process.cwd() } = {}) {
  const root = cwd
  const appRoot = path.join(root, 'src/app')
  const errors = []

  for (const sourceFile of walk(appRoot)) {
    const sourceLayer = classify(appRoot, sourceFile)
    errors.push(...collectRoutePageEntrypointErrors(root, appRoot, sourceFile, sourceLayer))
    for (const specifier of collectImports(sourceFile)) {
      const resolved = resolveSpecifier(appRoot, sourceFile, specifier)

      if (resolved.kind === 'package' || resolved.kind === 'external-shared') continue

      if (resolved.kind === 'forbidden-alias') {
        errors.push(
          `${toPosix(root, sourceFile)} imports ${specifier}; use @app/* or @shared/* inside src/app`
        )
        continue
      }

      if (resolved.kind === 'outside-app') {
        errors.push(
          `${toPosix(root, sourceFile)} imports ${specifier}, which resolves outside src/app`
        )
        continue
      }

      if (resolved.kind === 'unresolved-app-alias' || resolved.kind === 'unresolved-relative') {
        errors.push(`${toPosix(root, sourceFile)} imports unresolved module ${specifier}`)
        continue
      }

      const targetLayer = classify(appRoot, resolved.file)
      if (
        shouldRouteUsePageEntrypoint(appRoot, sourceFile) &&
        isRouteEntrypointBypass(sourceLayer, targetLayer)
      ) {
        errors.push(
          `${toPosix(root, sourceFile)} imports ${specifier}; route files must render @app/pages/* entrypoints instead of feature/widget view modules`
        )
        continue
      }

      if (!isAllowedLayerImport(sourceLayer, targetLayer)) {
        errors.push(
          `${toPosix(root, sourceFile)} (${sourceLayer.layer}/${sourceLayer.slice ?? '-'}) imports ${specifier} (${targetLayer.layer}/${targetLayer.slice ?? '-'})`
        )
      }
    }
  }

  return { ok: errors.length === 0, errors }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = checkFsdBoundaries()
  if (result.errors.length === 0) {
    console.log('FSD boundary check passed.')
    process.exit(0)
  }

  console.error('FSD boundary check failed:')
  for (const error of result.errors) {
    console.error(`- ${error}`)
  }
  process.exit(1)
}
