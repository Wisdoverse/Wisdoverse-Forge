#!/usr/bin/env node
// FSD-5: AST-grade Feature-Sliced Design boundary checker.
//
// Import extraction runs es-module-lexer over `ts.transpileModule` output:
// es-module-lexer lexes JS modules only (JSX/TS syntax is a parse error by
// design), so each file is first normalized with TypeScript using
// `verbatimModuleSyntax`, which keeps every import/re-export statement —
// including type-only ones after the `import type` → `import` prefix rewrite —
// visible to the lexer. Static imports, re-exports, and dynamic `import()`
// calls with static string literals are all checked.
//
// Rule set and modes:
//   ERROR (enforced — pre-FSD-5 behavior, unchanged):
//     downward-layering    imports may only point down the layer order
//                          app → pages → widgets → features → entities → shared
//                          (feature ↛ feature cross-slice was already enforced
//                          here pre-FSD-5 and stays an error)
//     unknown-dir          F074: an unrecognized src/app dir may import only
//                          shared and may be imported by nothing
//     (plus the existing alias/unresolved/route-page-entrypoint errors)
//   WARN (new in FSD-5 — printed, never affect the exit code; they flip to
//   error after the FSD-1/2/4 migrations land):
//     public-api           an import crossing INTO a features/widgets/pages
//                          slice must target the slice root barrel, not a
//                          deep file
//     cross-entity         an entity slice must not import another entity slice
//     same-layer-isolation widget ↛ widget, page ↛ page sibling-slice imports
//     shared-purity        domain stores under shared/model/*.store.ts are
//                          flagged for relocation in FSD-2
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { init, parse } from 'es-module-lexer'
import ts from 'typescript'

await init

const extensions = ['.tsx', '.ts', '.jsx', '.js']
// F074: `unknown` (an unrecognised src/app dir) is deliberately NOT in this rank
// map. The old default classified such dirs as `app` (highest), letting them
// import from any layer with no violation flagged. They are now handled
// explicitly (import-only-shared, importable-by-none), which a single rank
// cannot express.
const layerRank = new Map([
  ['shared', 0],
  ['entities', 1],
  ['features', 2],
  ['widgets', 3],
  ['pages', 4],
  ['app', 5],
])

const appLayerDirs = new Set(['routes', 'layouts', 'providers', 'hooks', 'i18n', 'styles'])

// Layers whose slices expose a public API through their root barrel.
const slicedLayers = new Set(['features', 'widgets', 'pages'])

// shared-purity: genuinely generic infra stores (theme/toast-style UI state)
// may stay under shared/model. Reviewed 2026-07: every current
// shared/model/*.store.ts is domain-specific (admin, analytics, billing,
// board, chat, context, context-features, feed, settings, skills), so the
// allowlist is empty. Add a file name here only when the store carries no
// domain state.
const genericSharedStores = new Set([])

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

// --- import extraction (es-module-lexer over TypeScript-normalized source) ---

const typeImportPrefix = /^(\s*)import\s+type(?=[\s{*])/gm
const typeReExportPrefix = /^(\s*)export\s+type(?=\s*[{*])/gm
// Named-specifier brace group of an import/export statement. The `[^'"()=]`
// head guard keeps this off object/function bodies (`export const x = {`,
// `export function f() {`), which always contain `=` or `(` first.
const importExportClauseBraces = /(^|\n)(\s*(?:import|export)\s[^'"()=]*?\{)([^}]*)(\})/g
// String-literal import() call/type reference, matched against the RAW source.
const dynamicImportLiteral = /\bimport\s*\(\s*(['"])([^'"]+)\1\s*\)/g

function extractImports(sourceFile) {
  const raw = fs.readFileSync(sourceFile, 'utf8')
  // Rewrite `import type` / `export type ... from` statements AND inline
  // `type X` specifiers to their value form so `verbatimModuleSyntax` keeps
  // them: boundary rules (including the route page-entrypoint name gate)
  // apply to type-only imports too, and transpilation would otherwise drop
  // inline type specifiers before the statement is sliced (codex P2).
  const normalized = raw
    .replace(typeImportPrefix, '$1import')
    .replace(typeReExportPrefix, '$1export')
    .replace(
      importExportClauseBraces,
      (_match, lead, head, names, close) =>
        `${lead}${head}${names.replace(/\btype\s+/g, '')}${close}`
    )
  const { outputText } = ts.transpileModule(normalized, {
    // transpileModule refuses to emit declaration files ("Debug Failure").
    // The content of a .d.ts is valid in a plain .ts module (ambient
    // `declare` blocks erase cleanly), so emit it under a .ts name instead of
    // hard-erroring the file (codex round-2 P2).
    fileName: sourceFile.replace(/\.d\.ts$/, '.ts'),
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ESNext,
      jsx: ts.JsxEmit.React,
      verbatimModuleSyntax: true,
    },
  })
  const [imports] = parse(outputText, sourceFile)
  const entries = imports
    .filter((entry) => entry.n !== undefined && entry.d !== -2) // -2 = import.meta
    .map((entry) => ({
      specifier: entry.n,
      // Full statement text for static imports/re-exports; dynamic import()
      // records carry no clause.
      statement: entry.d === -1 ? outputText.slice(entry.ss, entry.se) : null,
    }))
  // Type-position import() references (`type T = import('@app/x').Y`) are
  // erased by the transpile before the lexer sees them; recover any
  // string-literal import() specifier from the raw source that the lexer
  // did not already report (codex round-2 P2).
  const seen = new Set(entries.map((entry) => entry.specifier))
  for (const match of raw.matchAll(dynamicImportLiteral)) {
    if (!seen.has(match[2])) {
      seen.add(match[2])
      entries.push({ specifier: match[2], statement: null })
    }
  }
  return entries
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

function nonPageEntrypointNames(statement) {
  const clauseMatch = statement.match(/^(?:import|export)\s+([\s\S]*?)\s+from\s+['"]/)
  if (!clauseMatch) return []
  return collectNamedImports(clauseMatch[1]).filter((name) => !name.endsWith('Page'))
}

function isSliceBarrel(appRoot, file) {
  const parts = path.relative(appRoot, file).split(path.sep)
  return parts.length === 3 && extensions.some((ext) => parts[2] === `index${ext}`)
}

export function checkFsdBoundaries({ cwd = process.cwd() } = {}) {
  const root = cwd
  const appRoot = path.join(root, 'src/app')
  const errors = []
  const warnings = []
  const fileLayers = new Map()
  const violatingFiles = new Set()

  const addError = (file, message) => {
    errors.push(message)
    violatingFiles.add(file)
  }
  const addWarning = (file, rule, target, reason) => {
    warnings.push({ rule, file, target, reason })
    violatingFiles.add(file)
  }

  for (const sourceFile of walk(appRoot)) {
    const source = classify(appRoot, sourceFile)
    const relFile = toPosix(root, sourceFile)
    fileLayers.set(relFile, source.layer)

    let imports
    try {
      imports = extractImports(sourceFile)
    } catch (parseError) {
      addError(relFile, `${relFile} could not be parsed for imports: ${parseError.message}`)
      continue
    }

    for (const { specifier, statement } of imports) {
      const resolved = resolveSpecifier(appRoot, sourceFile, specifier)

      if (resolved.kind === 'package' || resolved.kind === 'external-shared') continue

      if (resolved.kind === 'forbidden-alias') {
        addError(relFile, `${relFile} imports ${specifier}; use @app/* or @shared/* inside src/app`)
        continue
      }

      if (resolved.kind === 'outside-app') {
        addError(relFile, `${relFile} imports ${specifier}, which resolves outside src/app`)
        continue
      }

      if (resolved.kind === 'unresolved-app-alias' || resolved.kind === 'unresolved-relative') {
        addError(relFile, `${relFile} imports unresolved module ${specifier}`)
        continue
      }

      const target = classify(appRoot, resolved.file)

      // Existing route → pages entrypoint contract (ERROR).
      if (shouldRouteUsePageEntrypoint(appRoot, sourceFile)) {
        if (isRouteEntrypointBypass(source, target)) {
          addError(
            relFile,
            `${relFile} imports ${specifier}; route files must render @app/pages/* entrypoints instead of feature/widget view modules`
          )
          continue
        }
        if (
          source.layer === 'app' &&
          source.slice === 'routes' &&
          target.layer === 'pages' &&
          statement
        ) {
          const badNames = nonPageEntrypointNames(statement)
          if (badNames.length > 0) {
            for (const name of badNames) {
              addError(
                relFile,
                `${relFile} imports ${name} from ${specifier}; route files must import Page entrypoints from @app/pages/*`
              )
            }
            continue
          }
        }
      }

      const layerViolation = `${relFile} (${source.layer}/${source.slice ?? '-'}) imports ${specifier} (${target.layer}/${target.slice ?? '-'})`

      // unknown-dir (ERROR, F074): an unrecognised src/app dir is not a valid
      // module location — nothing may import from it, and it may itself depend
      // only on shared.
      if (target.layer === 'unknown' || (source.layer === 'unknown' && target.layer !== 'shared')) {
        addError(relFile, layerViolation)
        continue
      }

      if (source.layer !== 'unknown') {
        // downward-layering (ERROR): imports may only point down the layer order.
        if (layerRank.get(target.layer) > layerRank.get(source.layer)) {
          addError(relFile, layerViolation)
          continue
        }
        // feature ↛ feature cross-slice was already an ERROR pre-FSD-5; keep it.
        if (
          source.layer === 'features' &&
          target.layer === 'features' &&
          source.slice !== target.slice
        ) {
          addError(relFile, layerViolation)
          continue
        }
      }

      // same-layer-isolation (WARN — new): widget ↛ widget, page ↛ page.
      if (
        (source.layer === 'widgets' || source.layer === 'pages') &&
        target.layer === source.layer &&
        source.slice !== target.slice
      ) {
        addWarning(
          relFile,
          'same-layer-isolation',
          specifier,
          `${source.layer}/${source.slice} imports sibling slice ${target.layer}/${target.slice}; promote shared code to a lower layer`
        )
        continue
      }

      // cross-entity (WARN — new): entity slices stay independent.
      if (
        source.layer === 'entities' &&
        target.layer === 'entities' &&
        source.slice !== target.slice
      ) {
        addWarning(
          relFile,
          'cross-entity',
          specifier,
          `entities/${source.slice} must not import entities/${target.slice}; entity slices stay independent`
        )
        continue
      }

      // public-api (WARN — new): imports crossing into a features/widgets/pages
      // slice must target the slice root barrel, not a deep file.
      if (
        slicedLayers.has(target.layer) &&
        (source.layer !== target.layer || source.slice !== target.slice) &&
        !isSliceBarrel(appRoot, resolved.file)
      ) {
        addWarning(
          relFile,
          'public-api',
          specifier,
          `deep import into ${target.layer}/${target.slice}; import the slice barrel @app/${target.layer}/${target.slice}`
        )
      }
    }
  }

  // shared-purity (WARN — new): domain stores parked under shared/model are
  // FSD debt; they relocate to their owning slice in FSD-2.
  const sharedModelDir = path.join(appRoot, 'shared', 'model')
  if (fs.existsSync(sharedModelDir)) {
    for (const entry of fs.readdirSync(sharedModelDir).sort()) {
      if (!entry.endsWith('.store.ts') || genericSharedStores.has(entry)) continue
      const relFile = toPosix(root, path.join(sharedModelDir, entry))
      addWarning(relFile, 'shared-purity', null, 'domain store in shared — relocate in FSD-2')
    }
  }

  const layerStats = {}
  for (const [file, layer] of fileLayers) {
    layerStats[layer] ??= { files: 0, clean: 0 }
    layerStats[layer].files += 1
    if (!violatingFiles.has(file)) layerStats[layer].clean += 1
  }

  return { ok: errors.length === 0, errors, warnings, layerStats }
}

function printReport(result) {
  for (const warning of result.warnings) {
    const target = warning.target ? ` -> ${warning.target}` : ''
    console.warn(`FSD WARN [${warning.rule}] ${warning.file}${target} (${warning.reason})`)
  }

  const counts = new Map()
  for (const warning of result.warnings) {
    counts.set(warning.rule, (counts.get(warning.rule) ?? 0) + 1)
  }
  const summary = [...counts.entries()].map(([rule, count]) => `${rule}=${count}`).join(' ')
  console.warn(`FSD warn summary: ${summary || 'none'} (total ${result.warnings.length})`)

  const layerOrder = [...layerRank.keys()].reverse().concat('unknown')
  const conformance = layerOrder
    .filter((layer) => result.layerStats[layer])
    .map((layer) => {
      const { files, clean } = result.layerStats[layer]
      const percent = files === 0 ? 100 : Math.round((clean / files) * 100)
      return `${layer} ${percent}% (${clean}/${files})`
    })
    .join(' | ')
  console.log(`FSD layer conformance (clean files/total): ${conformance}`)
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = checkFsdBoundaries()

  if (result.errors.length > 0) {
    console.error('FSD boundary check failed:')
    for (const error of result.errors) {
      console.error(`- ${error}`)
    }
    printReport(result)
    process.exit(1)
  }

  printReport(result)
  console.log(`FSD boundary check passed (${result.warnings.length} warnings).`)
  process.exit(0)
}
