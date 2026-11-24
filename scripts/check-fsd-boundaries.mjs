#!/usr/bin/env node
// FSD-5: AST-grade Feature-Sliced Design boundary checker.
//
// Imports are read from each file's TypeScript source AST
// (`ts.createSourceFile`): static imports and re-exports (type-only ones
// included), `import x = require()`, dynamic `import()` calls, and
// `import('x').T` type references are all checked. Comments and strings never
// count as imports.
//
// Rule set (ALL rules are errors — the FSD-strict gate closed with FSD-2):
//     downward-layering    imports may only point down the layer order
//                          app → pages → widgets → features → entities → shared
//                          (feature ↛ feature cross-slice included)
//     unknown-dir          F074: unrecognized src/app paths are rejected as
//                          files and may be imported by nothing
//     public-api           an import crossing INTO an entities/features/
//                          widgets/pages slice must target the slice root
//                          barrel, not a deep file
//     cross-entity         an entity slice must not import another entity slice
//     same-layer-isolation widget ↛ widget, page ↛ page sibling-slice imports
//     shared-purity        domain stores must not live under
//                          shared/model/*.store.ts, and every file under
//                          shared/api or shared/lib must be listed in
//                          sharedFileInventory as generic or frozen-domain
//     own-barrel           a file inside a slice must not import that
//                          slice's own root barrel
//     computed-import      import() must take a string literal
//     import-glob          import.meta.glob / globEager are not allowed
//     (plus the existing alias/unresolved/route-page-entrypoint errors; an
//     alias import containing a '..' segment is an alias error)
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const extensions = ['.tsx', '.ts', '.jsx', '.js']
// F074: `unknown` (an unrecognised src/app dir) is deliberately NOT in this rank
// map. The old default classified such dirs as `app` (highest), letting them
// import from any layer with no violation flagged. They are now handled
// explicitly (invalid files, importable-by-none), which a single rank
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
const sliceOwningLayers = new Set(['entities', 'features', 'widgets', 'pages'])

// shared-purity: genuinely generic infra stores (theme/toast-style UI state)
// may stay under shared/model. FSD-2 relocated all 10 domain stores to their
// owning feature/entities slices, so the allowlist is empty. Add a file name
// here only when the store carries no domain state.
const genericSharedStores = new Set([])

// shared-purity inventory: every source file under shared/api and shared/lib
// (paths relative to src/app/shared, sub-directories included). Classify by
// what the code does, not by its name:
//   generic        names no product concept (agent, task, workspace,
//                  organization, billing, self-fix, onboarding, ...) and would
//                  work unchanged in another product. A new generic file is
//                  added here with a one-line reason.
//   frozen-domain  legacy domain code that predates this inventory. A mixed
//                  file is frozen-domain until it is split; when in doubt,
//                  frozen-domain. These entries may only be removed: move the
//                  file to its owning entities slice when it next changes.
// New domain code belongs in entities/<slice>.
export const sharedFileInventory = new Map([
  ['api/agent-api-types.ts', 'frozen-domain'], // agent types: SSH keys, git credentials, profiles
  ['api/auth.ts', 'frozen-domain'], // sign-in provider endpoint client
  ['api/authFetch.ts', 'generic'], // auth-aware fetch: token header, 401 refresh and retry
  ['api/client.ts', 'generic'], // JSON fetch wrapper over authFetch
  ['api/index.ts', 'generic'], // barrel for the JSON fetch wrapper
  ['api/legacy.ts', 'frozen-domain'], // agent, settings, billing and user API singletons
  ['api/legacy/AgentAPI.ts', 'frozen-domain'], // agent API client
  ['api/legacy/authFetch.ts', 'generic'], // fetch wrapper: 401 refresh, retry, offline queue
  ['api/legacy/billingApi.ts', 'frozen-domain'], // billing API client
  ['api/legacy/settingsApi.ts', 'frozen-domain'], // settings API client
  ['api/legacy/userApi.ts', 'frozen-domain'], // user API client
  ['api/orchestration.ts', 'frozen-domain'], // task orchestration API client and types
  ['api/promptStream.ts', 'frozen-domain'], // agent prompt stream and chat error copy
  ['api/selfFix.ts', 'frozen-domain'], // self-fix repository setup client
  ['lib/agentCapabilityCopy.ts', 'frozen-domain'], // agent capability copy
  ['lib/chunkError.ts', 'generic'], // reload once after a failed lazy chunk load
  ['lib/frontendReliability.ts', 'frozen-domain'], // organization-bound session telemetry
  ['lib/gettingStartedPreference.ts', 'frozen-domain'], // onboarding preference
  ['lib/gettingStartedProgress.ts', 'frozen-domain'], // onboarding progress from tasks
  ['lib/guidePreference.ts', 'generic'], // dismissed-guide check by key
  ['lib/passwordRules.ts', 'frozen-domain'], // product password policy and copy
  ['lib/publicAuth.ts', 'frozen-domain'], // password-reset and invite link parsing
  ['lib/slugify.ts', 'generic'], // string to URL slug
  ['lib/taskFailureCopy.ts', 'frozen-domain'], // task failure copy
  ['lib/time.ts', 'generic'], // relative time formatting
  ['lib/uiStyles.ts', 'generic'], // UI kit class names
  ['lib/utils.ts', 'generic'], // Tailwind class name merge
  ['lib/versionCheck.ts', 'generic'], // semantic version comparison
  ['lib/workspaceResourceErrorMessage.ts', 'frozen-domain'], // team and project error copy
])
const inventoriedSharedDirs = ['api', 'lib']

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
  if (sliceOwningLayers.has(first) && relative.length < 3) {
    return { layer: 'unknown', slice: first }
  }
  if (first === 'shared') return { layer: 'shared', slice: relative[1] ?? null }
  if (first === 'entities') return { layer: 'entities', slice: relative[1] ?? null }
  if (first === 'features') return { layer: 'features', slice: relative[1] ?? null }
  if (first === 'widgets') return { layer: 'widgets', slice: relative[1] ?? null }
  if (first === 'pages') return { layer: 'pages', slice: relative[1] ?? null }
  if (appLayerDirs.has(first) || relative.length === 1) return { layer: 'app', slice: first }
  // F074: an unrecognised multi-segment dir or malformed sliced-layer path is
  // `unknown`, so it cannot silently import from higher layers.
  return { layer: 'unknown', slice: first }
}

function resolveSpecifier(appRoot, sourceFile, specifier) {
  // `@shared/../src/app/...` would otherwise pass as an unchecked package path.
  if (/^@(app|shared)?\//.test(specifier) && specifier.split('/').includes('..')) {
    return { kind: 'alias-traversal' }
  }
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

// --- import extraction (TypeScript source AST) ---

const scriptKinds = {
  '.ts': ts.ScriptKind.TS,
  '.tsx': ts.ScriptKind.TSX,
  '.js': ts.ScriptKind.JS,
  '.jsx': ts.ScriptKind.JSX,
}

// Names an import/export clause pulls from its module (original names, type
// specifiers included); `*` stands for a namespace import or star re-export.
function clauseNames(node) {
  if (ts.isImportDeclaration(node)) {
    const clause = node.importClause
    if (!clause) return []
    const names = clause.name ? [clause.name.text] : []
    const bindings = clause.namedBindings
    if (bindings && ts.isNamespaceImport(bindings)) names.push('*')
    if (bindings && ts.isNamedImports(bindings)) {
      for (const element of bindings.elements) {
        names.push((element.propertyName ?? element.name).text)
      }
    }
    return names
  }
  const clause = node.exportClause
  if (!clause || ts.isNamespaceExport(clause)) return ['*']
  return clause.elements.map((element) => (element.propertyName ?? element.name).text)
}

function isStaticString(node) {
  return (
    node !== undefined && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node))
  )
}

// Every module reference in the file, read from the source AST so type-only
// forms survive and comments or strings never count: import/export
// declarations, `import x = require()`, `import()` calls, `import('x').T` type
// references, and `import.meta.glob` (which the caller rejects). `specifier`
// is null for an `import()` whose argument is not a static string.
function extractImports(sourceFile) {
  const raw = fs.readFileSync(sourceFile, 'utf8')
  const kind = scriptKinds[path.extname(sourceFile)] ?? ts.ScriptKind.TS
  const ast = ts.createSourceFile(sourceFile, raw, ts.ScriptTarget.Latest, false, kind)
  const entries = []
  const visit = (node) => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      entries.push({ specifier: node.moduleSpecifier.text, names: clauseNames(node) })
    } else if (
      ts.isExportDeclaration(node) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      entries.push({ specifier: node.moduleSpecifier.text, names: clauseNames(node) })
    } else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference) &&
      ts.isStringLiteral(node.moduleReference.expression)
    ) {
      entries.push({ specifier: node.moduleReference.expression.text, names: null })
    } else if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      const [argument] = node.arguments
      entries.push({ specifier: isStaticString(argument) ? argument.text : null, names: null })
    } else if (
      ts.isImportTypeNode(node) &&
      ts.isLiteralTypeNode(node.argument) &&
      ts.isStringLiteral(node.argument.literal)
    ) {
      entries.push({ specifier: node.argument.literal.text, names: null })
    } else if (
      ts.isPropertyAccessExpression(node) &&
      ts.isMetaProperty(node.expression) &&
      node.expression.keywordToken === ts.SyntaxKind.ImportKeyword &&
      (node.name.text === 'glob' || node.name.text === 'globEager')
    ) {
      entries.push({ specifier: null, names: null, glob: node.name.text })
    }
    ts.forEachChild(node, visit)
  }
  visit(ast)
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

function isSliceBarrel(appRoot, file) {
  const parts = path.relative(appRoot, file).split(path.sep)
  return parts.length === 3 && extensions.some((ext) => parts[2] === `index${ext}`)
}

export function checkFsdBoundaries({
  cwd = process.cwd(),
  // Injectable for tests; the repository always uses the inventory above.
  sharedInventory = sharedFileInventory,
} = {}) {
  const root = cwd
  const appRoot = path.join(root, 'src/app')
  const errors = []
  const fileLayers = new Map()
  const violatingFiles = new Set()

  const addError = (file, message) => {
    errors.push(message)
    violatingFiles.add(file)
  }
  const addRuleError = (file, rule, target, reason) => {
    addError(file, `[${rule}] ${file}${target ? ` -> ${target}` : ''} (${reason})`)
  }

  for (const sourceFile of walk(appRoot)) {
    const source = classify(appRoot, sourceFile)
    const relFile = toPosix(root, sourceFile)
    fileLayers.set(relFile, source.layer)
    if (source.layer === 'unknown') {
      addError(relFile, `[unknown-file] ${relFile} does not match a known src/app location`)
    }

    let imports
    try {
      imports = extractImports(sourceFile)
    } catch (parseError) {
      addError(relFile, `${relFile} could not be parsed for imports: ${parseError.message}`)
      continue
    }

    for (const { specifier, names, glob } of imports) {
      if (glob) {
        addRuleError(
          relFile,
          'import-glob',
          null,
          `import.meta.${glob} bypasses the boundary rules; import each module explicitly, through the slice barrel when it crosses into a slice`
        )
        continue
      }

      if (specifier === null) {
        addRuleError(
          relFile,
          'computed-import',
          null,
          'import() needs a string literal so the boundary rules can check it'
        )
        continue
      }

      const resolved = resolveSpecifier(appRoot, sourceFile, specifier)

      if (resolved.kind === 'package' || resolved.kind === 'external-shared') continue

      if (resolved.kind === 'alias-traversal') {
        addError(relFile, `${relFile} imports ${specifier}; an alias import must not contain '..'`)
        continue
      }

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
          names
        ) {
          const badNames = names.filter((name) => !name.endsWith('Page'))
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

      // unknown-dir (ERROR, F074): an unrecognised src/app path is not a valid
      // module location — nothing may import from it, and its own imports must
      // still follow the unknown layer's shared-only dependency rule.
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

      // same-layer-isolation (ERROR): widget ↛ widget, page ↛ page.
      if (
        (source.layer === 'widgets' || source.layer === 'pages') &&
        target.layer === source.layer &&
        source.slice !== target.slice
      ) {
        addRuleError(
          relFile,
          'same-layer-isolation',
          specifier,
          `${source.layer}/${source.slice} imports sibling slice ${target.layer}/${target.slice}; promote shared code to a lower layer`
        )
        continue
      }

      // cross-entity (ERROR): entity slices stay independent.
      if (
        source.layer === 'entities' &&
        target.layer === 'entities' &&
        source.slice !== target.slice
      ) {
        addRuleError(
          relFile,
          'cross-entity',
          specifier,
          `entities/${source.slice} must not import entities/${target.slice}; entity slices stay independent`
        )
        continue
      }

      // own-barrel (ERROR): a slice imports its own files directly; going
      // through its own barrel invites import cycles.
      if (
        sliceOwningLayers.has(source.layer) &&
        target.layer === source.layer &&
        target.slice === source.slice &&
        isSliceBarrel(appRoot, resolved.file) &&
        resolved.file !== sourceFile
      ) {
        addRuleError(
          relFile,
          'own-barrel',
          specifier,
          `${source.layer}/${source.slice} imports its own barrel; import the file directly`
        )
        continue
      }

      // public-api (ERROR): imports crossing into an entities/features/widgets/
      // pages slice must target the slice root barrel, not a deep file.
      if (
        sliceOwningLayers.has(target.layer) &&
        (source.layer !== target.layer || source.slice !== target.slice) &&
        !isSliceBarrel(appRoot, resolved.file)
      ) {
        addRuleError(
          relFile,
          'public-api',
          specifier,
          `deep import into ${target.layer}/${target.slice}; import the slice barrel @app/${target.layer}/${target.slice}`
        )
      }
    }
  }

  // shared-purity (ERROR): domain stores must not be parked under shared/model;
  // they live in their owning feature or entities slice (FSD-2).
  const sharedModelDir = path.join(appRoot, 'shared', 'model')
  if (fs.existsSync(sharedModelDir)) {
    for (const entry of fs.readdirSync(sharedModelDir).sort()) {
      if (!entry.endsWith('.store.ts') || genericSharedStores.has(entry)) continue
      const relFile = toPosix(root, path.join(sharedModelDir, entry))
      addRuleError(
        relFile,
        'shared-purity',
        null,
        'domain store in shared — move it to its owning feature or entities slice'
      )
    }
  }

  // shared-purity inventory (ERROR): shared/api and shared/lib accept only
  // listed files, and every listed file must exist.
  const sharedDir = path.join(appRoot, 'shared')
  for (const entry of [...sharedInventory.keys()].sort()) {
    if (!fs.existsSync(path.join(sharedDir, entry))) {
      addRuleError(
        `src/app/shared/${entry}`,
        'shared-purity',
        null,
        'stale inventory entry: the file does not exist; remove it from sharedFileInventory'
      )
    }
  }
  for (const dir of inventoriedSharedDirs) {
    for (const file of walk(path.join(sharedDir, dir)).sort()) {
      if (sharedInventory.has(toPosix(sharedDir, file))) continue
      addRuleError(
        toPosix(root, file),
        'shared-purity',
        null,
        `unlisted file in shared/${dir} — domain code belongs in entities/<slice>; if the file is generic infrastructure, add it to sharedFileInventory in scripts/check-fsd-boundaries.mjs as 'generic' with a one-line reason`
      )
    }
  }

  const layerStats = {}
  for (const [file, layer] of fileLayers) {
    layerStats[layer] ??= { files: 0, clean: 0 }
    layerStats[layer].files += 1
    if (!violatingFiles.has(file)) layerStats[layer].clean += 1
  }

  return { ok: errors.length === 0, errors, layerStats }
}

function printReport(result) {
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
  console.log('FSD boundary check passed.')
  process.exit(0)
}
