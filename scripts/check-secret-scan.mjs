import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const args = new Set(process.argv.slice(2))
const scanRoots = ['.github', 'rust', 'shared', 'src', 'scripts', 'docker', 'index.html']
const allowedExtensions = new Set([
  '.html',
  '.ts',
  '.js',
  '.mjs',
  '.cjs',
  '.rs',
  '.toml',
  '.json',
  '.yml',
  '.yaml',
])
if (args.has('--include-k8s')) {
  scanRoots.push('k8s')
}

const useGitFileList = canUseGit()
const trackedFiles = (useGitFileList ? gitLsFiles(scanRoots) : walkFiles(scanRoots)).filter(
  (file) => allowedExtensions.has(path.posix.extname(file))
)
const allTrackedFiles = useGitFileList ? gitLsFiles() : fallbackEnvFiles()
const findings = []

const secretPatterns = [
  { kind: 'literal', regex: /sk-[A-Za-z0-9]{20,}/ },
  { kind: 'literal', regex: /AKIA[0-9A-Z]{16}/ },
  { kind: 'literal', regex: /m(?:inioadmin)/ },
  { kind: 'literal', regex: /cdn\.amplitude\.com\/script\/[A-Za-z0-9]{16,}\.js/ },
  { kind: 'literal', regex: /amplitude\.init\(['"][A-Za-z0-9]{16,}['"]/ },
  { kind: 'assignment', regex: /password\s*[:=]\s*['"][^'"]{8,}['"]/ },
  { kind: 'assignment', regex: /secret\s*[:=]\s*['"][^'"]{8,}['"]/ },
]

for (const file of trackedFiles) {
  if (shouldSkipFile(file)) {
    continue
  }

  const source = fs.readFileSync(file, 'utf8')
  const lines = source.split(/\r?\n/)

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index]
    for (const pattern of secretPatterns) {
      if (!pattern.regex.test(line)) {
        continue
      }
      if (shouldIgnoreMatch(file, line, pattern.kind)) {
        continue
      }
      findings.push(`${file}:${index + 1}:${line}`)
      break
    }
  }
}

const trackedEnvFiles = allTrackedFiles.filter((file) => {
  const basename = path.posix.basename(file)
  return basename === '.env' || basename === '.env.production' || basename === '.env.local'
})

if (findings.length > 0) {
  console.error('ERROR: Potential secrets detected in source:')
  for (const finding of findings) {
    console.error(finding)
  }
}

if (trackedEnvFiles.length > 0) {
  console.error('WARNING: .env files found in repository:')
  for (const file of trackedEnvFiles) {
    console.error(file)
  }
}

if (findings.length > 0 || trackedEnvFiles.length > 0) {
  const categories = Number(findings.length > 0) + Number(trackedEnvFiles.length > 0)
  console.error(`Found ${categories} secret leak categories — review and remediate`)
  process.exit(1)
}

console.log('No secret leaks detected')

function gitLsFiles(paths = []) {
  const command = ['ls-files', '-z']
  if (paths.length > 0) {
    command.push('--', ...paths)
  }
  return execFileSync('git', command, { encoding: 'utf8' }).split('\0').filter(Boolean)
}

function canUseGit() {
  try {
    execFileSync('git', ['--version'], { stdio: 'ignore' })
    return true
  } catch {
    return false
  }
}

function walkFiles(roots) {
  const found = new Set()
  for (const root of roots) {
    if (!fs.existsSync(root)) {
      continue
    }
    walkInto(root, found)
  }
  return [...found].sort()
}

function walkInto(currentPath, found) {
  const stat = fs.statSync(currentPath)
  if (stat.isFile()) {
    found.add(normalizePath(currentPath))
    return
  }

  const basename = path.basename(currentPath)
  if (basename === '.git' || basename === 'node_modules') {
    return
  }

  for (const entry of fs.readdirSync(currentPath, { withFileTypes: true })) {
    walkInto(path.join(currentPath, entry.name), found)
  }
}

function normalizePath(filePath) {
  return filePath.split(path.sep).join(path.posix.sep).replace(/^\.\//, '')
}

function fallbackEnvFiles() {
  return ['.env', '.env.production', '.env.local'].filter((file) => fs.existsSync(file))
}

function shouldSkipFile(file) {
  return (
    file.includes('node_modules/') ||
    file.includes('/__tests__/') ||
    file.includes('.test.') ||
    file.endsWith('secret.yaml') ||
    file.endsWith('package-lock.json') ||
    file.includes('.example')
  )
}

function shouldIgnoreMatch(file, line, kind) {
  if (line.includes('REPLACE_VIA_SECRET_MANAGER')) {
    return true
  }

  if (kind === 'assignment' && file.startsWith('src/app/shared/i18n/locales/')) {
    // Locale credential labels are UI copy, not secrets.
    return true
  }

  return false
}
