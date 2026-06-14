#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const ROOTS = ['src/app']
const EXTENSIONS = new Set(['.ts', '.tsx'])

const EMPTY_STATE_PATTERNS = [
  /\bNo [A-Za-z][^.!?\n]{0,80} (?:yet|found|available|to show)\b/,
  /\bNo (?:active|recent) [A-Za-z][^.!?\n]{0,80}\b/,
  /\bNothing [^.!?\n]{0,80}\b/,
]

const NEXT_ACTION_PATTERN =
  /\b(Add|Ask|Check|Choose|Clear|Close|Connect|Create|Enter|Fix|Invite|Keep|Open|Reconnect|Refresh|Review|Retry|Run|Save|Select|Send|Sign in|Start|Try|Use|Wait)\b/i

const RAW_USER_VISIBLE_PATTERNS = [
  /\bAn error occurred\b/,
  /\bError occurred\b/,
  /\bConnection failed\b/,
  /\bFailed to fetch\b/,
  /\bInternal Server Error\b/,
  /\bNetwork error\b/,
  /\bOperation not permitted\b/i,
  /\bServer error\s*\(\d{3}\)/,
  /\bStack trace\b/i,
  /\bUnhandled exception\b/i,
  /\bSQL error\b/i,
  /\bUnknown error\b/,
  /\bdatabase unavailable\b/i,
]

const RECOVERABLE_ERROR_PATTERNS = [
  /\b(?:could not|did not|was not|were not)\b/i,
  /\bfailed to\b/i,
  /\b(?:are|is|was|were) not (?:created|deleted|loaded|saved|started|updated)\b/i,
]

const DEAD_END_VALIDATION_PATTERNS = [
  /\bInvalid project path\b/,
  /\bInvalid file type\b/,
  /\bThis field is invalid\b/,
  /无效的项目路径/,
  /无效的文件类型/,
  /此字段无效/,
]

const DEAD_END_CONFIRMATION_PATTERNS = [
  /\bAre you sure you want to delete this(?: agent| group| user)?\??/i,
  /\bAre you sure you want to reset all settings\??/i,
  /\bYou have unsaved changes\. Are you sure you want to leave\??/i,
  /\bAre you sure you want to logout\??/i,
  /\bAre you sure you want to reset\??/i,
  /\bAre you sure you want to stop this operation\??/i,
  /\bAre you sure you want to discard your changes\??/i,
  /确定要删除(?:此|这个)?.*吗？/,
  /确定要恢复所有设置吗？/,
  /您有未保存的更改，确定要离开吗？/,
  /确定要退出登录吗？/,
  /确定要重置吗？/,
  /确定要停止此操作吗？/,
  /确定要放弃更改吗？/,
]

const DEAD_END_LIMIT_CONFLICT_PATTERNS = [
  /\bPassword must be at least \{\{min\}\} characters\b/,
  /\bPasswords do not match\b/,
  /\bThis email is already in use\b/,
  /\bThis username is already taken\b/,
  /\bRegistration restricted to authorized email domains\b/,
  /\bMaximum number of agents reached\b/,
  /\bFile upload failed\b/,
  /\bFile is too large\. Maximum size is \{\{size\}\}\.?/,
  /密码至少需要 \{\{min\}\} 个字符/,
  /两次输入的密码不一致/,
  /该邮箱已被使用/,
  /该用户名已被使用/,
  /仅允许使用授权邮箱域名注册/,
  /已达到最大 Agent 数量/,
  /文件上传失败/,
  /文件过大，最大允许 \{\{size\}\}/,
]

const ACTIVITY_JARGON_PATTERNS = [
  /\btool_use:\s*['"`]Tool Use['"`]/,
  /\btool_result:\s*['"`]Tool Result['"`]/,
  /\bTask:\s*['"`]Subagent Task['"`]/,
  /\btool_use:\s*['"`]工具调用['"`]/,
  /\btool_result:\s*['"`]工具结果['"`]/,
  /\bTask:\s*['"`]子任务['"`]/,
]

const AGENT_STATUS_JARGON_PATTERNS = [
  /\bidle:\s*['"`]Idle['"`]/,
  /\boffline:\s*['"`]Offline['"`]/,
  /\berror:\s*['"`]Error['"`]/,
  /\blabel:\s*['"`]Offline['"`]/,
  /\breturn\s+['"`]Offline['"`]/,
  /\btitle=(?:['"`]Offline['"`]|\{\s*['"`]Offline['"`]\s*\})/,
  /\bvalue:\s*['"`]idle['"`]\s*,\s*label:\s*['"`]Idle['"`]/,
  /\bvalue:\s*['"`]offline['"`]\s*,\s*label:\s*['"`]Offline['"`]/,
  /\bidle:\s*['"`]空闲['"`]/,
  /\boffline:\s*['"`]离线['"`]/,
  /\berror:\s*['"`]错误['"`]/,
]

const REVIEW_DECISION_JARGON_PATTERNS = [
  /\bvalue:\s*['"`]pending['"`]\s*,\s*label:\s*['"`]Pending['"`]/,
  /\btitleCase\(state\)/,
  /\bApprove only when\b/,
  /\bReject when\b/,
  /\bApprove and save this item\b/,
  /\baria-label=\{approving \? `Approve /,
  /<span>Approve<\/span>/,
  /<span>Reject<\/span>/,
  /\bField label=["'`]Reject reason["'`]/,
  /\bswitch back to Pending\b/,
]

const NOTE_SPACE_JARGON_PATTERNS = [
  /\bunits of note space\b/i,
  /\bunits available\b/i,
  /\bcontext units\b/i,
]

const WORK_SETUP_LOAD_PATTERNS = [/\bAgent Work Setup could not load\b/i, /无法加载工作设置/]

const WORK_SETUP_LOAD_RECOVERY_PATTERN =
  /\bRefresh\b|ask an owner|owner or admin|刷新|找\s*owner|找\s*admin|管理员|检查/i

const PROVIDER_CHECK_JARGON_PATTERNS = [
  /\bnone need Check\b/,
  /\bstill needs Check\b/,
  /\bstill need Check\b/,
]

const RUNTIME_SHORT_LABEL_JARGON_PATTERNS = [
  /\breturn\s+['"`]Not reported['"`]/,
  /\breturn\s+['"`]Needs review['"`]/,
]

const CLIPBOARD_JARGON_PATTERNS = [/\bCopy is unavailable here\b/i, /\bno clipboard access\b/i]

const BILLING_CHECKPOINT_DEAD_END_PATTERNS = [/\bNo invoices yet\b/i]

const BILLING_RECEIPT_LINK_DEAD_END_PATTERNS = [/\bNo link\b/i]

const BEGINNER_JARGON_PATTERNS = [
  /\blocal agents?\b/i,
  /\bmanaged local agent\b/i,
  /\bmanaged workspace agents?\b/i,
  /\bclaude,\s*codex,\s*gemini,?\s*or\s*opencode\b/,
  /claude、codex、gemini\s*或\s*opencode/,
  /\bHost CLI\b/i,
  /\bPlatform CLI\b/i,
  /\bForge CLI\b/i,
]

const PLACEHOLDER_COPY_PATTERNS = [/\bUnknown\b/, /\bunknown\b/, /\bN\/A\b/, /\bTBD\b/]

const PLACEHOLDER_STRING_LITERAL_PATTERN = /(['"`])[^'"`]*(?:Unknown|unknown|N\/A|TBD)[^'"`]*\1/

const NON_UI_PATH_PARTS = [
  '/api/',
  '/lib/',
  '/model/',
  '/models/',
  '/store/',
  '/stores/',
  '/types/',
]

const NON_UI_FILE_PATTERNS = [
  /\.test\.[jt]sx?$/,
  /\.spec\.[jt]sx?$/,
  /ErrorMessage\.ts$/,
  /ErrorMessages\.ts$/,
  /errors\.ts$/,
  /\.store\.ts$/,
]

const USER_VISIBLE_ERROR_FILE_PATTERNS = [
  /ErrorMessages?\.ts$/,
  /errors\.ts$/,
  /\/model\/agents\.store\.ts$/,
  /\/model\/navigation\.store\.ts$/,
  /\/model\/settings\.store\.ts$/,
  /\/model\/billing\.store\.ts$/,
  /\/model\/admin\.store\.ts$/,
  /\/model\/skills\.store\.ts$/,
  /\/model\/analytics\.store\.ts$/,
]

const USER_VISIBLE_ERROR_FRAGMENT_FILE_PATTERNS = [
  /ErrorCopy\.ts$/,
  /ErrorMessages?\.ts$/,
  /errors\.ts$/,
]

function toPosix(value) {
  return value.split(path.sep).join('/')
}

function walk(dir, files) {
  if (!fs.existsSync(dir)) return
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'coverage') {
        continue
      }
      walk(full, files)
      continue
    }

    if (!EXTENSIONS.has(path.extname(entry.name))) continue
    files.push(full)
  }
}

function isUiCopyFile(relFile) {
  if (relFile === 'src/app/shared/api/legacy/AgentAPI.ts') return true
  if (relFile === 'src/app/entities/agent/model/runtime-kind.ts') return true
  if (USER_VISIBLE_ERROR_FILE_PATTERNS.some((pattern) => pattern.test(relFile))) return true
  if (NON_UI_FILE_PATTERNS.some((pattern) => pattern.test(relFile))) return false
  if (NON_UI_PATH_PARTS.some((part) => relFile.includes(part))) return false
  return true
}

function isLikelyEmptyStateContext(lines, index, line) {
  if (/\bempty\s*[:=]/i.test(line)) return true
  if (/^\s*no[A-Z][A-Za-z0-9_]*\s*:/.test(line)) return true

  const start = Math.max(0, index - 20)
  const end = Math.min(lines.length, index + 4)
  const context = lines.slice(start, end).join('\n')
  return (
    /EmptyState\b/.test(context) ||
    /\bempty[-_\s]?state\b/i.test(context) ||
    /ProfileSummaryRow\b/.test(context)
  )
}

function hasEmptyStateCopy(lines, index) {
  const line = lines[index] ?? ''
  return (
    isLikelyEmptyStateContext(lines, index, line) &&
    EMPTY_STATE_PATTERNS.some((pattern) => pattern.test(line))
  )
}

function hasNextAction(lines, index) {
  const start = Math.max(0, index - 2)
  const end = Math.min(lines.length, index + 9)
  return lines.slice(start, end).some((line) => NEXT_ACTION_PATTERN.test(line))
}

function isLikelyGuardOrParserLine(line) {
  return (
    line.includes('includes(') ||
    line.includes('match(') ||
    line.includes('.test(') ||
    line.includes('.replace(/') ||
    line.includes('= /') ||
    line.includes('new Error(') ||
    line.includes('new TypeError(') ||
    line.includes('RAW_') ||
    line.includes('console.') ||
    line.includes('===') ||
    line.includes('!==') ||
    line.includes('throw ') ||
    line.trim().startsWith('/') ||
    line.trim().startsWith('//') ||
    line.trim().startsWith('*')
  )
}

function hasRawUserVisibleCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return RAW_USER_VISIBLE_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBeginnerJargon(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return BEGINNER_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function looksLikeUserVisibleCopyLine(line) {
  if (/<[^>]*>[^<]*(?:Unknown|unknown|N\/A|TBD)[^<]*<\/[^>]+>/.test(line)) return true
  if (/\b(?:aria-label|title|placeholder)\s*=/.test(line)) return true
  if (
    /\b[A-Za-z][A-Za-z0-9_]*(?:Label|Title|Description|Message|Detail|Tooltip|Placeholder|Help|Hint|Text|Copy)?\s*:\s*['"`]/.test(
      line
    )
  ) {
    return true
  }
  if (
    /\b(?:label|title|description|message|detail|tooltip|placeholder|help|hint|text|copy)\s*=\s*['"`]/i.test(
      line
    )
  ) {
    return true
  }
  return false
}

function hasPlaceholderCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  if (
    !/<[^>]*>[^<]*(?:Unknown|unknown|N\/A|TBD)[^<]*<\/[^>]+>/.test(line) &&
    !PLACEHOLDER_STRING_LITERAL_PATTERN.test(line)
  ) {
    return false
  }
  if (!looksLikeUserVisibleCopyLine(line)) return false
  return PLACEHOLDER_COPY_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRecoverableErrorCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return RECOVERABLE_ERROR_PATTERNS.some((pattern) => pattern.test(line))
}

function hasDeadEndValidationCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return DEAD_END_VALIDATION_PATTERNS.some((pattern) => pattern.test(line))
}

function hasDeadEndConfirmationCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return DEAD_END_CONFIRMATION_PATTERNS.some((pattern) => pattern.test(line))
}

function hasDeadEndLimitConflictCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return DEAD_END_LIMIT_CONFLICT_PATTERNS.some((pattern) => pattern.test(line))
}

function hasActivityJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return ACTIVITY_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentStatusJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_STATUS_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasReviewDecisionJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return REVIEW_DECISION_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasNoteSpaceJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return NOTE_SPACE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasWorkSetupLoadDeadEndCopy(lines, index, line) {
  if (isLikelyGuardOrParserLine(line)) return false
  if (
    !/\bcouldNotLoad\b/.test(line) &&
    !WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(line))
  ) {
    return false
  }
  const context = lines.slice(index, Math.min(lines.length, index + 3)).join(' ')
  if (!WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(context))) return false
  return !WORK_SETUP_LOAD_RECOVERY_PATTERN.test(context)
}

function hasProviderCheckJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_CHECK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRuntimeShortLabelJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/entities/agent/model/runtime-kind.ts')) return false
  return RUNTIME_SHORT_LABEL_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasClipboardJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return CLIPBOARD_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBillingCheckpointDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/billing/BillingPage.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BILLING_CHECKPOINT_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBillingReceiptLinkDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/billing/InvoiceList.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BILLING_RECEIPT_LINK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function scanFile(file, relFile) {
  const lines = fs.readFileSync(file, 'utf8').split('\n')
  const findings = []

  lines.forEach((line, index) => {
    const location = `${relFile}:${index + 1}`
    if (hasEmptyStateCopy(lines, index) && !hasNextAction(lines, index)) {
      findings.push({
        type: 'empty-state-next-action',
        location,
        message: 'Empty state copy must include a nearby next action for first-time operators.',
        sample: line.trim(),
      })
    }

    const rawUserVisibleCopy = hasRawUserVisibleCopy(line)
    if (rawUserVisibleCopy) {
      findings.push({
        type: 'raw-error-copy',
        location,
        message: 'User-visible copy must not expose raw transport or backend failure wording.',
        sample: line.trim(),
      })
    }

    if (hasBeginnerJargon(line)) {
      findings.push({
        type: 'beginner-jargon-copy',
        location,
        message: 'User-visible copy must use beginner-facing agent location wording.',
        sample: line.trim(),
      })
    }

    if (hasPlaceholderCopy(line)) {
      findings.push({
        type: 'placeholder-copy',
        location,
        message:
          'User-visible copy must explain missing information instead of showing placeholder labels.',
        sample: line.trim(),
      })
    }

    if (hasDeadEndValidationCopy(line)) {
      findings.push({
        type: 'validation-next-action',
        location,
        message: 'User-visible validation copy must explain what to change next.',
        sample: line.trim(),
      })
    }

    if (hasDeadEndConfirmationCopy(line)) {
      findings.push({
        type: 'confirmation-impact',
        location,
        message: 'User-visible confirmation copy must explain the impact before users confirm.',
        sample: line.trim(),
      })
    }

    if (hasDeadEndLimitConflictCopy(line)) {
      findings.push({
        type: 'limit-conflict-next-action',
        location,
        message: 'User-visible limit or conflict copy must explain what to change next.',
        sample: line.trim(),
      })
    }

    if (hasActivityJargonCopy(line)) {
      findings.push({
        type: 'activity-jargon-copy',
        location,
        message: 'Activity feed labels must describe what the agent did in beginner language.',
        sample: line.trim(),
      })
    }

    if (hasAgentStatusJargonCopy(line)) {
      findings.push({
        type: 'agent-status-copy',
        location,
        message: 'Agent status labels must explain whether work can be assigned.',
        sample: line.trim(),
      })
    }

    if (hasReviewDecisionJargonCopy(line)) {
      findings.push({
        type: 'review-decision-copy',
        location,
        message: 'Saved-item review copy must say what will be saved instead of approval jargon.',
        sample: line.trim(),
      })
    }

    if (hasNoteSpaceJargonCopy(line)) {
      findings.push({
        type: 'note-space-copy',
        location,
        message: 'Saved-note capacity copy must use plain size language instead of unit counts.',
        sample: line.trim(),
      })
    }

    if (hasWorkSetupLoadDeadEndCopy(lines, index, line)) {
      findings.push({
        type: 'work-setup-load-next-action',
        location,
        message: 'Work setup load failure copy must tell first-time operators how to recover.',
        sample: line.trim(),
      })
    }

    if (hasProviderCheckJargonCopy(line)) {
      findings.push({
        type: 'provider-check-copy',
        location,
        message:
          'AI service setup copy must describe the connection check instead of using button-label grammar.',
        sample: line.trim(),
      })
    }

    if (hasRuntimeShortLabelJargonCopy(relFile, line)) {
      findings.push({
        type: 'runtime-short-label-copy',
        location,
        message:
          'Compact work-location labels must name the missing location instead of using generic review placeholders.',
        sample: line.trim(),
      })
    }

    if (hasClipboardJargonCopy(line)) {
      findings.push({
        type: 'clipboard-copy',
        location,
        message:
          'Copy failure guidance must tell beginners how to copy manually instead of naming clipboard access.',
        sample: line.trim(),
      })
    }

    if (hasBillingCheckpointDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'billing-checkpoint-copy',
        location,
        message:
          'Billing checkpoint copy must explain when invoices appear instead of only saying none exist.',
        sample: line.trim(),
      })
    }

    if (hasBillingReceiptLinkDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'billing-receipt-link-copy',
        location,
        message:
          'Invoice receipt copy must explain when a link will appear instead of only saying no link.',
        sample: line.trim(),
      })
    }

    if (
      !rawUserVisibleCopy &&
      !USER_VISIBLE_ERROR_FRAGMENT_FILE_PATTERNS.some((pattern) => pattern.test(relFile)) &&
      hasRecoverableErrorCopy(line) &&
      !hasNextAction(lines, index)
    ) {
      findings.push({
        type: 'error-next-action',
        location,
        message: 'User-visible failure copy must include a nearby next action for beginners.',
        sample: line.trim(),
      })
    }
  })

  return findings
}

export function checkBeginnerUxCopy(options = {}) {
  const cwd = options.cwd || process.cwd()
  const files = []
  for (const root of ROOTS) {
    walk(path.join(cwd, root), files)
  }

  const findings = []
  for (const file of files) {
    const relFile = toPosix(path.relative(cwd, file))
    if (!isUiCopyFile(relFile)) continue
    findings.push(...scanFile(file, relFile))
  }

  return {
    ok: findings.length === 0,
    findings,
  }
}

export function runBeginnerUxCopyCheck(options = {}) {
  const stdout = options.stdout || process.stdout
  const stderr = options.stderr || process.stderr
  const result = checkBeginnerUxCopy({ cwd: options.cwd || process.cwd() })

  if (result.ok) {
    stdout.write('[beginner-ux-copy] UI copy guard passed.\n')
    return 0
  }

  stderr.write('[beginner-ux-copy] Beginner UX copy guard failed.\n')
  for (const finding of result.findings) {
    stderr.write(`[${finding.type}] ${finding.location}: ${finding.message}\n`)
    stderr.write(`  -> ${finding.sample}\n`)
  }
  return 1
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(runBeginnerUxCopyCheck())
}
