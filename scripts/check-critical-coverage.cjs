#!/usr/bin/env node

const fs = require('node:fs')
const path = require('node:path')

const targets = [
  { name: 'turn-builder', prefix: 'shared/turn-builder.ts', minLines: 90, minBranches: 75 },
  { name: 'app-api', prefix: 'src/app/shared/api/legacy.ts', minLines: 90, minBranches: 75 },
]

function writeLine(stream, message) {
  stream.write(`${message}\n`)
}

function runCoverageGate(options = {}) {
  const stdout = options.stdout || process.stdout
  const stderr = options.stderr || process.stderr
  const coveragePath =
    options.coveragePath ||
    process.env.COVERAGE_FILE ||
    path.join('coverage', 'cobertura-coverage.xml')

  if (!fs.existsSync(coveragePath)) {
    writeLine(stderr, `Coverage file not found: ${coveragePath}`)
    return 2
  }

  let xml
  try {
    xml = fs.readFileSync(coveragePath, 'utf8')
  } catch (err) {
    writeLine(stderr, `Failed to read coverage file ${coveragePath}: ${err.message}`)
    return 2
  }

  const stats = Object.fromEntries(
    targets.map((target) => [
      target.prefix,
      {
        classes: 0,
        lines: 0,
        linesCovered: 0,
        branches: 0,
        branchesCovered: 0,
      },
    ])
  )

  const classRe = /<class\s+[^>]*filename="([^"]+)"[^>]*>([\s\S]*?)<\/class>/g
  let classMatch

  while ((classMatch = classRe.exec(xml)) !== null) {
    const filePath = classMatch[1]
    const body = classMatch[2]

    const target = targets.find((entry) => filePath.startsWith(entry.prefix))
    if (!target) continue

    const sectionMatches = [...body.matchAll(/<lines>([\s\S]*?)<\/lines>/g)]
    if (sectionMatches.length === 0) continue

    const classLinesBlock = sectionMatches[sectionMatches.length - 1][1]
    const lineRe = /<line\s+[^>]*number="\d+"[^>]*hits="(\d+)"([^>]*)\/>/g
    let lineMatch
    let linesInClass = 0

    while ((lineMatch = lineRe.exec(classLinesBlock)) !== null) {
      linesInClass += 1
      const lineHits = Number(lineMatch[1])

      stats[target.prefix].lines += 1
      if (lineHits > 0) {
        stats[target.prefix].linesCovered += 1
      }

      const attrs = lineMatch[2]
      const branchMatch = attrs.match(/condition-coverage="\d+% \((\d+)\/(\d+)\)"/)
      if (branchMatch) {
        stats[target.prefix].branchesCovered += Number(branchMatch[1])
        stats[target.prefix].branches += Number(branchMatch[2])
      }
    }

    if (linesInClass > 0) {
      stats[target.prefix].classes += 1
    }
  }

  let hasFailure = false

  writeLine(stdout, 'Critical-path coverage gate:')
  for (const target of targets) {
    const s = stats[target.prefix]
    if (s.classes === 0) {
      writeLine(stderr, `- ${target.name}: no matched files for prefix ${target.prefix}`)
      hasFailure = true
      continue
    }

    const lineRate = s.lines > 0 ? (s.linesCovered / s.lines) * 100 : 0
    const branchRate = s.branches > 0 ? (s.branchesCovered / s.branches) * 100 : 0

    const linePass = lineRate >= target.minLines
    const branchPass = branchRate >= target.minBranches

    writeLine(
      stdout,
      `- ${target.name}: lines ${lineRate.toFixed(1)}% (min ${target.minLines}%), ` +
        `branches ${branchRate.toFixed(1)}% (min ${target.minBranches}%)`
    )

    if (!linePass || !branchPass) {
      hasFailure = true
      writeLine(stderr, `  FAILED ${target.name} threshold`)
    }
  }

  if (hasFailure) {
    return 1
  }

  writeLine(stdout, 'Critical-path coverage gate passed.')
  return 0
}

if (require.main === module) {
  process.exit(runCoverageGate())
}

module.exports = { runCoverageGate }
