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

const AGENT_SHARED_STATUS_DEAD_END_PATTERNS = [
  /\bStatus not reported\b/i,
  /\bStatus needs review\b/i,
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
  /\boriginal task preview (?:is )?unavailable\b/i,
  /\bThis cannot be saved because the original task preview is unavailable\./i,
]

const REVIEW_HISTORY_DEAD_END_PATTERNS = [/\bNo saved item history yet\b/i]

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

const PROVIDER_ZERO_READY_DEAD_END_PATTERNS = [/\bNo AI services are ready to use yet\b/i]

const PROVIDER_TEST_FAILURE_FIRST_PATTERNS = [
  /\bconnection check needs attention\. Forge could not (?:check|connect to) this AI service/i,
  /\bconnection check needs attention\. This AI service is receiving too many checks/i,
  /\bconnection check needs attention\. Review the AI service settings/i,
  /\bconnection check needs attention\. The model or service address was not found/i,
]

const PROVIDER_SETTINGS_FAILURE_FIRST_PATTERNS = [
  /\bAI service could not be (?:saved|removed)\./i,
  /\bRefresh Settings to load AI service settings\. Forge is receiving too many AI service requests/i,
  /\bRefresh Settings to load AI service settings\. Try again\./i,
]

const PROVIDER_ADDRESS_JARGON_PATTERNS = [
  /\bglobal address,\s*paste this:/i,
  /\bLeave blank to use the China address\b/i,
  /\bplaceholder=.*https?:\/\//i,
  /\breturn\s+['"`]https?:\/\//i,
]

const ADMIN_USERS_EMPTY_DEAD_END_PATTERNS = [/\bNo one is listed yet\b/i]

const ADMIN_ORGS_EMPTY_DEAD_END_PATTERNS = [
  /\bNo team spaces are visible yet\b/i,
  /\baccess needs review\b/i,
]

const ADMIN_AGENT_ACTIVITY_DEAD_END_PATTERNS = [
  /\bNo activity yet\b/i,
  /\bActivity time needs review\b/i,
]

const ADMIN_AGENT_FIELD_DEAD_END_PATTERNS = [
  /\bStatus not reported\b/i,
  /\bOwner not reported yet\b/i,
  /\bProject not reported yet\b/i,
]

const ADMIN_AGENT_STATUS_FALLBACK_DEAD_END_PATTERNS = [
  /\bstatus\.trim\(\)\s*\?\s*['"`]Needs review['"`]/i,
]

const ADMIN_AGENT_EMPTY_DEAD_END_PATTERNS = [/\bNo agents to show\b/i]

const ADMIN_LOAD_ERROR_DEAD_END_PATTERNS = [
  /\bThe admin [^'"`]+ could not load\./i,
  /\bThe agent tool update status could not load\./i,
]

const RUNTIME_SHORT_LABEL_JARGON_PATTERNS = [
  /\bWork location not reported\b/i,
  /\bLocation missing\b/i,
  /\breturn\s+['"`]Not reported['"`]/,
  /\breturn\s+['"`]Needs review['"`]/,
]

const CLIPBOARD_JARGON_PATTERNS = [/\bCopy is unavailable here\b/i, /\bno clipboard access\b/i]

const BILLING_CHECKPOINT_DEAD_END_PATTERNS = [/\bNo invoices yet\b/i]

const BILLING_USAGE_DEAD_END_PATTERNS = [/\bNo usage reported yet\b/i, /\busage areas shown\b/i]

const BILLING_RECEIPT_LINK_DEAD_END_PATTERNS = [/\bNo link\b/i]

const ANALYTICS_CHART_DEAD_END_PATTERNS = [/\bNo activity data\b/i, /\bNo tool usage data\b/i]

const ANALYTICS_USEFUL_EMPTY_DEAD_END_PATTERNS = [/\bNo useful saved items yet\b/i]

const ANALYTICS_REVIEW_EMPTY_DEAD_END_PATTERNS = [
  /\bNothing to check right now\b/i,
  /\bNothing looks outdated\b/i,
]

const ANALYTICS_UPDATED_TIME_DEAD_END_PATTERNS = [/\btime not available\b/i]

const ANALYTICS_GUIDANCE_JARGON_PATTERNS = [
  /\bfailed tool steps\b/i,
  /\bfailures first\b/i,
  /\bended in error\b/i,
]

const ACTIVITY_FEED_EMPTY_DEAD_END_PATTERNS = [
  /\bNo work has reported progress yet\b/i,
  /\bNo updates need action right now\b/i,
  /\bNo task is blocked or failed in recent activity\b/i,
  /\bNo progress updates in this view\b/i,
  /\bNo completed updates in this view\b/i,
  /\bNo updates in this view\b/i,
  /\bNo recent activity matches this view\b/i,
  /\bNo progress updates yet\b/i,
]

const SAVED_ITEM_OPTIONAL_EMPTY_DEAD_END_PATTERNS = [/\bNo other saved items were found\b/i]

const SAVED_ITEM_SELECTION_EMPTY_DEAD_END_PATTERNS = [
  /\bNothing will be shared yet\b/i,
  /\bNothing is kept yet\b/i,
  /\bNo saved items are selected yet\b/i,
]

const INBOX_NEEDS_ACTION_EMPTY_DEAD_END_PATTERNS = [/\bNothing needs action right now\b/i]

const TASK_AGENT_ASSIGNMENT_DEAD_END_PATTERNS = [
  /\bNo agent assigned yet\b/i,
  /\bAgent not reported yet\b/i,
]

const TIMELINE_EMPTY_DEAD_END_PATTERNS = [
  /\bNo timeline events yet\b/i,
  /\bWaiting for run events\b/i,
]

const WORKSHOP_3D_EMPTY_DEAD_END_PATTERNS = [/\bNo agents on the visual map yet\b/i]

const AGENT_DETAIL_ACTIVITY_DEAD_END_PATTERNS = [/\bNo task activity has been loaded yet\b/i]

const AGENT_DETAIL_FILE_ACCESS_DEAD_END_PATTERNS = [/\bNo file access needed\b/i]

const AGENT_DETAIL_AVAILABILITY_DEAD_END_PATTERNS = [
  /\bUnavailable until restarted or reconnected\b/i,
]

const AGENT_DETAIL_START_FAILURE_FIRST_PATTERNS = [/\bStart did not finish\b/i]

const AGENT_API_LIFECYCLE_DEAD_END_PATTERNS = [
  /\bNo workspace to (?:restart|start|stop)\b/i,
  /没有可(?:重启|启动|停止)的工作区/,
]

const TITLE_STYLE_GUIDANCE_PATTERNS = [
  /\bOpen Tasks\b/,
  /\bDo This Next\b/,
  /\bReview Current Work\b/,
  /\bReview current work\b/,
  /\bAdd AI Service\b/,
  /\bAI Services\b/,
  /\bActive Work\b/,
  /\bSuccess Rate\b/,
  /\bSuccess rate\b/,
  /\bCreate a Task Queue First\b/,
  /\bMy AI Service\b/,
  /\bPick a Project to Start\b/,
  /\bSave AI Service\b/,
  /\bgroup:\s*['"`]AI Setup['"`]/,
  /\bgroup:\s*['"`]Product Info['"`]/,
  /\bgroup:\s*['"`]Work Setup['"`]/,
  /\blabel:\s*['"`]Agent Work Setup['"`]/,
  /\blabel:\s*['"`]Code Access['"`]/,
  /\blabel:\s*['"`]Outside Tool Access['"`]/,
  /\blabel:\s*['"`]SSH Code Access['"`]/,
  /\blabel:\s*['"`]Team Members['"`]/,
  /\blabel:\s*['"`]Work Capacity['"`]/,
  /\bOpen Team Members\b/,
]

const CLI_IMAGE_STATUS_DEAD_END_PATTERNS = [
  /\bCheck failed\b/i,
  /\bTool for new agents:/i,
  /\bLatest tool found:/i,
  /\bNo agent tools are configured for update checks\b/i,
  /\bNo result yet\b/i,
  /\bNot downloaded yet\b/i,
  /\bNot checked yet\b/i,
  /\bNot checked — updates off\b/i,
  /\bVersion not reported yet\b/i,
]

const CLI_IMAGE_ACTION_FAILURE_FIRST_PATTERNS = [
  /\bThe build could not be started\b/i,
  /\bThe restart could not be started\b/i,
]

const CLI_IMAGE_RESULT_JARGON_PATTERNS = [
  /\$\{result\.failed\}\s*failed\b/,
  /\bskipped \(busy\)/i,
  /\$\{prune\.errors\}\s*errors\b/,
  /\bcleanup hit\b[^.!?\n]*\berrors?\b/i,
]

const SYSTEM_HEALTH_STATUS_DEAD_END_PATTERNS = [
  /\bNot checked(?: yet)?\b/i,
  /\bNeeds attention\b/i,
  /\bSome areas need attention\b/i,
  /\bstill needs attention\b/i,
  /\bitems marked Needs attention\b/i,
  /\bwhile this page is visible\b/i,
  /\bHidden tabs pause checks\b/i,
  /\bChecking\.\.\./i,
]

const ACCESS_KEY_LAST_USED_DEAD_END_PATTERNS = [/\bNot used yet\b/i]

const DATE_FALLBACK_DEAD_END_PATTERNS = [
  /\b(?:Added|Created|Sign-in|Last used) date not reported\b/i,
  /\b(?:Added|Created|Sign-in|Last used) date needs review\b/i,
]

const ACCOUNT_PROFILE_DEAD_END_PATTERNS = [
  /\bUsername not reported yet\b/i,
  /\bEmail not reported yet\b/i,
]

const ACCOUNT_PROFILE_ROLE_JARGON_PATTERNS = [/>Role</]

const LOCALE_ACCESS_ROLE_JARGON_PATTERNS = [
  /\brole:\s*['"`]Role['"`]/,
  /\brole:\s*['"`]角色['"`]/,
  /更新你的角色/,
]

const START_GUIDE_RESET_JARGON_PATTERNS = [
  /\bStart guide\b/i,
  /\bReset Start guide\b/i,
  /\bReset it here\b/i,
]

const START_GUIDE_FAILURE_FIRST_PATTERNS = [
  /\bStart could not be hidden\. Check your connection, then try Skip again\./i,
  /\bThe setup checklist could not be shown\. Check your connection, then try again\./i,
]

const TASK_VIEW_LABEL_JARGON_PATTERNS = [/\bid:\s*['"`]3d['"`]\s*,\s*label:\s*['"`]3D['"`]/]

const TOP_BAR_CREATE_TASK_JARGON_PATTERNS = [/\+\s*Task\b/]

const COMMAND_PALETTE_CREATE_TASK_JARGON_PATTERNS = [
  /\blabel:\s*['"`]Create task['"`]/,
  /\bStart a new piece of work\./,
]

const PROJECT_MENU_CREATE_TASK_JARGON_PATTERNS = [
  /\bCreate task here\b/,
  /\bStart work in this project\b/,
]

const TASK_FORM_SUBMIT_LABEL_JARGON_PATTERNS = [
  /\bCreate Anyway\b/,
  /\bCreate Task\b/,
  /\bPreparing Project\.\.\./,
]

const TASK_FORM_NO_PROJECT_DEAD_END_PATTERNS = [
  /\bNo projects available\. Create a project in Settings before creating tasks\./i,
]

const TASK_FORM_NO_AGENT_DEAD_END_PATTERNS = [
  /\bNo agents are online\. You can create the task now; it will wait here until an agent comes online\./i,
  /\bNo agents are available right now\. Keep the default choice so the next available agent can pick it up\./i,
  /\bCreate the task now, or open agent setup to (?:start or )?connect an agent first\./i,
]

const QUICK_CREATE_DRAFT_TASK_JARGON_PATTERNS = [
  /\bAdd Draft Task\b/i,
  /\bdraft task\b/i,
  /\bcreating the draft task\b/i,
]

const AGENT_TASK_QUEUE_SUBMIT_LABEL_JARGON_PATTERNS = [/\bCreate Task Queue\b/]
const AGENT_TASK_QUEUE_EMPTY_DEAD_END_PATTERNS = [
  /\bNo task queues yet\b/i,
  /\bNo tasks are in this task queue yet\b/i,
]
const TASK_LIST_EMPTY_DEAD_END_PATTERNS = [/\bCreate one small task from the board first\b/i]
const AGENT_LIST_SUMMARY_DEAD_END_PATTERNS = [/\bNo agents\b/i]
const AGENT_TOOL_SUMMARY_DEAD_END_PATTERNS = [
  /\bNo tool summary yet\. Ask an owner what this tool lets the agent do before turning it on\./i,
]
const CREATE_AGENT_OPTIONAL_CONTEXT_DEAD_END_PATTERNS = [
  /\bNo primary project\b/i,
  /\bNo task queue\b/i,
  /\bNo task queue selected yet\b/i,
]

const SKILL_MAINTAINER_FALLBACK_DEAD_END_PATTERNS = [
  /\bMaintainer not listed yet\b/i,
  /暂未列出维护者/,
]

const SAVED_INSTRUCTION_SUMMARY_FALLBACK_PATTERNS = [
  /\bNo summary yet\. Open details before using this saved instruction\./i,
  /\bNo summary yet\. Review the instructions below before using this saved instruction\./i,
  /还没有简介。/,
]

const SAVED_INSTRUCTION_TOOL_TOOLTIP_PATTERNS = [
  /\bWork tool setup needs review\./i,
  /工作工具设置需要检查。/,
]

const SAVED_INSTRUCTIONS_LOAD_DEAD_END_PATTERNS = [
  /\bSaved instructions could not load\./i,
  /\bForge could not load Saved instructions right now\./i,
]

const SAVED_INSTRUCTION_CREATE_FAILURE_FIRST_PATTERNS = [
  /\bThe instruction could not be created\./i,
  /\bForge could not connect while creating this instruction\./i,
  /\bYou do not have permission to create workspace instructions\./i,
  /\bSaved instructions could not be opened from this page\./i,
  /\bAn instruction with this name or trigger may already exist\./i,
  /\bInstruction setup is busy\./i,
  /\bForge could not create the instruction right now\./i,
]

const SAVED_INSTRUCTION_TEMPLATE_JARGON_PATTERNS = [
  /\bCheck GitHub or GitLab once\b/i,
  /\bPR or CI summary\b/i,
  /\bClassify the result as ACTION, WAIT, or DONE\b/i,
  /\bFor ACTION\b/,
  /\bFor WAIT\b/,
  /\bFor DONE\b/,
  /\bfailed check or job details\b/i,
  /\bstop monitoring in chat\b/i,
  /\bsuggest a background monitor\b/i,
]

const RUNTIME_SIGN_IN_DEAD_END_PATTERNS = [
  /\bNo work tool sign-ins are connected yet\b/i,
  /\bNo work tool sign-in saved\b/i,
]

const RUNTIME_DEFAULT_LOCATION_DEAD_END_PATTERNS = [/\bNot set yet\b/i]

const RUNTIME_SETUP_STATUS_DEAD_END_PATTERNS = [
  /\bNo work tool setup status yet\b/i,
  /\bNo agent has been seen online yet\b/i,
  /\bno agents are online yet\b/i,
]

const SETTINGS_LOAD_ERROR_DEAD_END_PATTERNS = [
  /\bAI service settings could not be loaded\./i,
  /\bOutside tool access keys could not be loaded\./i,
  /\bRepository access could not be loaded\./i,
  /\bRepository SSH access could not be loaded\./i,
]

const CODE_ACCESS_FAILURE_FIRST_PATTERNS = [
  /\bCode access could not be (?:saved|removed)\./i,
  /\bRefresh Settings to load code access\. Forge is receiving too many code access requests/i,
  /\bRefresh Settings to load code access\. Try again\./i,
]

const SSH_CODE_ACCESS_FAILURE_FIRST_PATTERNS = [
  /\bSSH code access could not be (?:saved|removed)\./i,
  /\bRefresh Settings to load SSH code access\. Forge is receiving too many SSH code access requests/i,
  /\bRefresh Settings to load SSH code access\. Try again\./i,
]

const PLATFORM_KEY_FAILURE_FIRST_PATTERNS = [
  /\bOutside tool access key could not be (?:created|removed)\./i,
  /\bRefresh Settings to load outside tool access keys\. Forge is receiving too many outside tool access requests/i,
  /\bRefresh Settings to load outside tool access keys\. Try again\./i,
]

const ACCOUNT_SETTINGS_FAILURE_FIRST_PATTERNS = [
  /\breturn\s+['"`]Your sign-in expired\. Sign in again/i,
  /\breturn\s+['"`]You do not have permission to (?:change this password|rename this team space)\./i,
  /\breturn\s+['"`](?:Password|Team space) settings are not available\. Refresh Settings/i,
  /\breturn\s+['"`]Your account changed while this form was open\. Refresh/i,
  /\breturn\s+['"`]This team space changed while you were editing\. Refresh/i,
  /\breturn\s+['"`]The current password did not match this account\. Re-enter/i,
  /\breturn\s+['"`]That team space name is already in use\. Choose/i,
  /\breturn\s+['"`]Forge is receiving too many account settings requests right now\. Wait/i,
  /\breturn\s+['"`](?:Password could not be changed|Team space name could not be saved)\./i,
  /\breturn\s+['"`]Account settings could not/i,
]

const COMMON_ERROR_FAILURE_FIRST_PATTERNS = [
  /\bSomething went wrong\. Try again/i,
  /\bRequest timed out\. Please try again\b/i,
  /\{\{resource\}\} was not found\. Refresh the page/i,
  /\bForge could not finish this right now\. Wait a moment/i,
  /\bThe agent could not finish this step\. Try again/i,
  /\bThe file could not be handled\. Check the file/i,
  /\bThe upload did not finish\. Check the file/i,
  /\bUpload did not finish\. Check the file/i,
  /\bThe download did not start\. Refresh the page/i,
  /\bToo many requests\. Please wait/i,
  /\{\{resource\}\} quota is used up\. Ask an owner/i,
  /出现了问题。请重试/,
  /请求超时，请重试/,
  /未找到 \{\{resource\}\}。请刷新页面/,
  /Forge 暂时无法完成这个操作。请稍等片刻/,
  /Agent 没有完成这一步。请重试/,
  /文件无法处理。请检查文件/,
  /上传没有完成。请检查文件/,
  /下载没有开始。请刷新页面/,
  /请求过于频繁，请等待/,
  /\{\{resource\}\} 配额已用完。请让所有者/,
]

const LOCALE_VAGUE_ERROR_LABEL_PATTERNS = [
  /\berror:\s*['"`]Needs attention['"`]/i,
  /\berror:\s*['"`]需要处理['"`]/,
  /\berror:\s*['"`]有内容需要处理。请查看提示信息，然后重试。['"`]/,
]

const WORKSPACE_SETTINGS_FAILURE_FIRST_PATTERNS = [
  /\bThe (?:team|project) was not created\./i,
  /\bRefresh Settings to load workspace (?:teams|projects)\. Sign in again/i,
  /\bRefresh Settings to load workspace (?:teams|projects)\. Ask an owner or admin/i,
  /\bRefresh Settings to load workspace (?:teams|projects)\. Check your connection/i,
  /\bRefresh Settings to load workspace (?:teams|projects)\. Too many setup changes/i,
]

const LOAD_ERROR_TITLE_DEAD_END_PATTERNS = [
  /\bConversation history could not be loaded\./i,
  /\bAgent tools could not be loaded\./i,
  /\bThis agent's work list could not be loaded\./i,
  /\bAgent Work Setup could not be loaded\./i,
  /\bMembers could not load for this\b/i,
  /\bAgents could not be loaded\./i,
  /\bForge could not update Agents right now\./i,
  /\bWorkspace (?:team|project)s could not be loaded\./i,
  /\bAgent sizes could not be loaded\./i,
  /\b(?:Plan and payment|Usage|Invoices) could not be loaded\./i,
]

const AGENT_SETUP_FALLBACK_DEAD_END_PATTERNS = [
  /\bAI service needs review\b/i,
  /\bWork tool needs review\b/i,
  /\bWork tool not listed\b/i,
  /\bAgent location needs review\b/i,
  /\bAgent location not listed\b/i,
  /\bWork location needs review\b/i,
  /\bWork location not listed\b/i,
  /\ban AI service that needs review\b/i,
  /\ba work tool that needs review\b/i,
]

const LIVE_WORK_STATUS_DEAD_END_PATTERNS = [/\bStatus not reported\b/i]

const TASK_DETAIL_RUN_STATUS_DEAD_END_PATTERNS = [/\bStatus not reported\b/i]

const TASK_STATUS_FALLBACK_DEAD_END_PATTERNS = [
  /\bStatus not listed\b/i,
  /\bStatus needs review\b/i,
  /\bPriority not listed\b/i,
  /\bPriority needs review\b/i,
]

const TASK_COMPLETION_SUMMARY_DEAD_END_PATTERNS = [/\bNo completion summary was provided\b/i]

const TASK_OWNER_INPUT_JARGON_PATTERNS = [/\bneeds owner input\b/i]

const TASK_RECOVERY_STATUS_DEAD_END_PATTERNS = [
  /\bfailed:\s*['"`]Needs review['"`]/,
  /\bfailed:\s*['"`]Stopped with an error['"`]/,
  /\blabel:\s*['"`]Needs review['"`]/,
  /\breturn\s+['"`]Needs review['"`]/,
  /\bThese tasks stopped before finishing\b/i,
  /\bfix the error\b/i,
  /\breview the failure\b/i,
  /\bread the failure\b/i,
  /\bTriage failure\b/,
]

const TASK_DETAIL_EMPTY_DEAD_END_PATTERNS = [
  /\bNo description provided\./i,
  /\bNo result files were attached\./i,
  /\bNo current work\b/i,
  /\bNo current agent work\b/i,
]

const CONTEXT_FALLBACK_DEAD_END_PATTERNS = [
  /\bSuggested item needs review\b/i,
  /\bSaved item needs review\b/i,
  /\bSharing setting needs review\b/i,
  /\bSafety label needs review\b/i,
  /\bSome note limits need review\b/i,
  /\bTask type not listed\b/i,
  /\bTask type needs review\b/i,
  /\bThe full saved note could not load\. Choose Show full saved note again\b/i,
]

const CONTEXT_WORK_HISTORY_JARGON_PATTERNS = [
  /\brun details\b/i,
  /\bRun details\b/,
  /\bWork run\b/,
  /\bduring this run\b/i,
  /\bfrom the run\b/i,
  /\bagent run\b/i,
  /\bactive runs\b/i,
  /\bfuture runs\b/i,
  /\bfuture agent runs\b/i,
  /\bcurrent run\b/i,
  /\bnext run\b/i,
  /\bafter the run\b/i,
  /\brun finishes\b/i,
  /\bcompletes the run\b/i,
  /\btask runs\b/i,
  /\bagent runs\b/i,
  /\bnew runs\b/i,
]

const CHAT_MESSAGE_FALLBACK_DEAD_END_PATTERNS = [
  /\bMessage needs review\b/i,
  /\bMessage sender not reported\b/i,
]

const CHAT_OPERATOR_JARGON_PATTERNS = [
  /\bThe You filter only shows requests sent by an operator\./i,
]

const CHAT_FILTER_EMPTY_DEAD_END_PATTERNS = [
  /\bNothing is marked blocked, failed, waiting, or needing review in this view\./i,
  /\bNo work steps have been reported yet\b/i,
  /\bworkspace agent reports commands or tool runs\b/i,
  /\bassign a workspace task to create work steps\b/i,
]

const CHAT_TOOL_STEP_DEAD_END_PATTERNS = [
  /\bThis step needs review\b/i,
  /\bThis step has not reported a result yet\b/i,
  /\blabel:\s*['"`]Needs review['"`]/i,
]

const VAGUE_NEEDS_REVIEW_COPY_PATTERNS = [
  /\bsummary that needs review\b/i,
  /\bnothing needs review\b/i,
  /\bsetup needs review\b/i,
  /\bneeding review\b/i,
]

const VAGUE_NEEDS_ATTENTION_COPY_PATTERNS = [
  /\bSomething needs attention\. Review the message, then try again\./i,
  /\bneeds attention\b/i,
  /\bAnalytics needs attention\b/i,
  /\bConversation needs attention\b/i,
  /\bThis agent's work list needs attention\b/i,
  /\bAgent work setup needs attention\b/i,
  /\bAI service setup needs attention\b/i,
  /\bCheck needs attention\b/i,
  /\bThe recorded result needs attention\./i,
  /\bStopped because sign-in or service access needs attention\./i,
]

const TECHNICAL_PROBLEM_JARGON_PATTERNS = [
  /\bThis step reported a technical problem\b/i,
  /\bThis record reported a technical problem\b/i,
  /\btechnical problem\b/i,
]

const CHAT_OFFLINE_DEAD_END_PATTERNS = [
  /\bThis agent is offline\. Start it before sending a message\./i,
  /\bStart it before sending a message\b/i,
]

const CHAT_STREAM_FAILURE_FIRST_PATTERNS = [
  /\bThe agent could not finish this reply\. Resend the message\./i,
]

const GOVERNANCE_AUDIT_FALLBACK_DEAD_END_PATTERNS = [
  /\bChange not listed\b/i,
  /\bNot checked\b/i,
  /\bResource not listed\b/i,
  /\|\|\s*['"`]not listed['"`]/i,
]

const GOVERNANCE_AUDIT_ERROR_FAILURE_FIRST_PATTERNS = [
  /\bThe audit export did not finish\./i,
  /\bGovernance audit history could not load\./i,
  /\bGovernance audit is not available from this view\./i,
  /\bThe audit data changed while export was running\./i,
  /\bGovernance audit is handling too many requests right now\./i,
  /\bForge could not (?:load|export) governance audit history right now\./i,
]

const DUPLICATE_RECOVERY_COPY_PATTERNS = [
  /\bForge could not load the board right now\. Refresh the board, then try again\./i,
  /\bForge could not finish this board action right now\. Refresh the board, then try again\./i,
  /\bForge could not load task details right now\. Refresh the task, then try again\./i,
  /\bForge could not finish this task action right now\. Refresh the task, then try again\./i,
  /\bForge could not load saved items right now\. Refresh the list, then try again\./i,
  /\bForge could not save this review decision right now\. Refresh the list, then try again\./i,
  /\bNavigation could not [^.]+\. Forge could not connect while loading the sidebar\./i,
  /\bForge could not load workspace navigation right now\. Refresh the sidebar, then try again\./i,
]

const TASK_FORM_AGENT_STATUS_DEAD_END_PATTERNS = [/\bstatus not reported\b/i]

const TASK_FORM_QUEUE_LOAD_FAILURE_FIRST_PATTERNS = [
  /\bTask queues could not load for this project\./i,
]

const TASK_SUPPORT_REFERENCE_DEAD_END_PATTERNS = [
  /\bSupport reference not (?:reported|listed)\b/i,
  /\breturn\s+['"`]not listed['"`]/i,
]

const CONTEXT_CANDIDATE_PREVIEW_DEAD_END_PATTERNS = [
  /\bNo preview is available yet\b/i,
  /\binspect the full suggestion\b/i,
]

const TASK_AGENT_CAPABILITY_JARGON_PATTERNS = [
  /\bparticipant\.capabilities\.join\(/,
  /\bimplementation,\s*review\b/i,
]

const TASK_DETAIL_AGENT_SETUP_DEAD_END_PATTERNS = [
  /\bNo available agent can take this task right now\./i,
  /\bNo agent is available for this task\. Start an agent or wait for one to finish, then try again\./i,
]

const TASK_DETAIL_LOAD_FAILURE_FIRST_PATTERNS = [
  /\bAvailable agents could not load\./i,
  /\bSaved notes and run details could not load\./i,
  /\bAgent work history could not load\./i,
  /\bThe saved item review could not load\./i,
  /\bForge could not connect while (?:loading|updating) this task\./i,
]

const BOARD_LOAD_FAILURE_FIRST_PATTERNS = [
  /\bAgent status could not load\./i,
  /\bThe task board could not load\./i,
  /\bThe saved item preview could not load\./i,
  /\bThis board item was not found\./i,
  /\bForge could not connect while (?:loading|updating) the board\./i,
]

const BOARD_AGENT_SETUP_DEAD_END_PATTERNS = [
  /\bNo agent is available for saved item preview\. Start an agent or wait for one to finish, then try again\./i,
  /\bNo agent can take work right now\./i,
  /\bConnect or free up an agent before (?:it|them) can start\./i,
  /\bNo recent activity\b/i,
]

const BOARD_CLEAR_DEAD_END_PATTERNS = [/\bTask queue is clear\./i]

const AUTH_FAILURE_FIRST_PATTERNS = [
  /\bToo many sign-in attempts\. Wait a few minutes/i,
  /\bWe could not sign you in\b/i,
  /\bThis account is not allowed to sign in here\. Ask an owner or admin/i,
  /\bToo many account creation attempts\. Wait a few minutes/i,
  /\bAn account may already exist for this email\. Sign in instead/i,
  /\bWe could not create the account right now\. Check the fields/i,
  /\bSign-in was cancelled\. Choose a sign-in option/i,
  /\bThis sign-in link expired or could not be verified\. Start sign-in again/i,
  /\bThis sign-in option is not ready\. Ask an owner or admin/i,
  /\bSign-in could not finish\. Choose a sign-in option/i,
  /\bThis reset link may have expired\. Request a new reset email/i,
  /\bPassword could not be updated\. Check the password rules/i,
  /\bReset email could not be requested\. Check the email address/i,
  /\bVerification email could not be sent\. Check that this is the email/i,
]

const AUTH_MANAGER_DEAD_END_PATTERNS = [
  /\bLogin failed\b/,
  /\bRegistration failed\b/,
  /\bAuth code exchange failed\b/,
  /\bFailed to resend\b/,
  /\bFailed to send reset email\b/,
  /\bFailed to reset password\b/,
]

const NETWORK_FAILURE_FIRST_PATTERNS = [
  /\bSign-in could not finish\. Forge could not connect while signing you in\./i,
  /\bAccount could not be created\. Forge could not connect while creating it\./i,
  /\bVerification email could not be sent\. Forge could not connect while sending it\./i,
  /\bReset email could not be requested\. Forge could not connect while sending the reset email\./i,
  /\bPassword could not be updated\. Forge could not connect while saving your new password\./i,
  /\bForge could not connect(?: while signing in)?\. Check your connection, then try again\./i,
  /\bForge could not connect while changing this agent\. Check your connection, refresh this agent, then try again\./i,
  /\bForge could not connect while sending this message\. Check your connection, then resend it\./i,
  /\bForge could not connect while loading this conversation\. Check your connection, then try again\./i,
  /\bForge could not connect while clearing this chat\. Check your connection, then try again\./i,
  /\bFeedback could not be saved\. Forge could not connect while saving it\. Check your connection, then try again\./i,
  /\b(?:Team|Project) name could not be saved\. Forge could not connect while saving it\. Check your connection, then save again\./i,
  /\bThe (?:team|project) was not created\. Forge could not connect while creating this (?:team|project)\. Check your connection, then try again\./i,
  /\bSettings could not [^.]+\. Forge could not connect while (?:loading|updating) Settings\. Check your connection, then try again\./i,
  /\b(?:Refresh Settings to load AI service settings|AI service could not be (?:saved|removed))\. Forge could not connect while opening AI service settings\. Check your connection, then try again\./i,
  /\b(?:Refresh Settings to load outside tool access keys|Outside tool access key could not be (?:created|removed))\. Forge could not connect while opening outside tool access settings\. Check your connection, then try again\./i,
  /\b(?:Refresh Settings to load repository access|Repository access could not be (?:saved|removed))\. Forge could not connect while opening repository access\. Check your connection, then try again\./i,
  /\b(?:Refresh Settings to load repository SSH access|Repository SSH access could not be (?:saved|removed))\. Forge could not connect while opening repository SSH access\. Check your connection, then try again\./i,
  /\b(?:Password could not be changed|Team space name could not be saved)\. Forge could not connect while opening (?:password|team space) settings\. Check your connection, then try again\./i,
  /Forge 登录时暂时连不上。请检查网络后重试。/,
  /Forge 暂时连不上。请检查网络后重试。/,
]

const WORKSPACE_RESOURCE_FAILURE_FIRST_PATTERNS = [
  /\b(?:Team|Project) could not be (?:saved|deleted)\./i,
  /\bThis (?:team|project) could not be found\./i,
  /\bForge could not (?:save workspace settings|delete this (?:team|project)) right now\./i,
]

const PROJECT_CREATE_FAILURE_FIRST_PATTERNS = [
  /\bToo many project changes are happening right now\. Wait a minute, then create this project again\./i,
  /\bForge could not create the project right now\. Wait a few minutes, then try again\./i,
  /\bCould not create the project\. Check the project name and team, then try again\./i,
]

const CLONE_RETRY_FAILURE_FIRST_PATTERNS = [
  /\bYou do not have permission to copy code into this project\. Ask an owner or admin to let you try again\./i,
  /\bThis project could not be found\. Refresh Projects, then try copying code again from the current project row\./i,
  /\bForge is already copying code for this project\. Wait a moment, then check the status again\./i,
  /\bToo many code import retries are happening right now\. Wait a minute, then try again\./i,
  /\bForge could not copy code right now\. Wait a few minutes, then try again\./i,
  /\bCould not copy code into the project\. Check the code link and saved code access, then try again\./i,
]

const CLONE_FAILURE_RAW_MESSAGE_PATTERNS = [/\bclone\?\.errorMessage\b/, /\bclone\.errorMessage\b/]

const AGENT_CONFIG_DETAIL_DEAD_END_PATTERNS = [
  /\bAI model not reported\b/i,
  /\bModel not reported\b/i,
  /\bRefresh agent details\b/i,
  /\bWork tool not reported\b/i,
  /\bNo instructions\b/i,
]

const AGENT_AI_SERVICE_DEAD_END_PATTERNS = [/\bAI service not reported\b/i]

const AGENT_MODEL_DEAD_END_PATTERNS = [
  /\bAI model not reported\b/i,
  /\bModel not reported\b/i,
  /\bmodel:\s*[^,\n]*['"`]unknown['"`]/i,
]

const ACCESS_LEVEL_DEAD_END_PATTERNS = [
  /\bAccess level not reported\b/i,
  /\bAccess level needs review\b/i,
]

const AGENT_TEMPLATE_ROLE_JARGON_PATTERNS = [/\bStart with a role\b/i, /\bAgent role templates\b/i]

const PROJECT_SHARE_ROLE_JARGON_PATTERNS = [/\bInvite people and choose roles\b/i]

const VAGUE_ACCESS_RECOVERY_PATTERNS = [
  /\bAsk an owner or admin to update your access\b/i,
  /\bAsk an owner or admin to update what you can do\b/i,
  /\bAsk an owner or admin to update project access\b/i,
]

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

const THIS_COMPUTER_SETUP_JARGON_PATTERNS = [
  /\bsetup command\b/i,
  /\byour team asks you to run a command\b/i,
  /\bCopy this setup command\b/,
  /\bCopy setup command\b/,
  /\bsetup command appears here\b/i,
  /\bsetup command in the box\b/i,
  /\bcommand window\b/i,
  /\bRun setup command on this computer\b/,
  /\bshows a setup command\b/i,
  /\bone-line Windows setup command\b/i,
  /\bpaste this command\b/i,
  /\bCopy this command and run it\b/i,
  /\bwhere you run the setup command\b/i,
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
  /\/shared\/lib\/taskFailureCopy\.ts$/,
  /\/shared\/lib\/workspaceResourceErrorMessage\.ts$/,
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
  if (relFile === 'src/app/entities/agent/model/display-labels.ts') return true
  if (relFile === 'src/app/entities/agent/model/runtime-kind.ts') return true
  if (relFile === 'src/app/entities/agent/model/status-labels.ts') return true
  if (relFile === 'src/app/entities/task/model/taskLabels.ts') return true
  if (relFile === 'src/app/entities/user/model/roleLabels.ts') return true
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
    line.includes('startsWith(') ||
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

function hasThisComputerSetupJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/agents/AgentListView.tsx') &&
    !relFile.endsWith('src/app/features/agents/CreateAgentModal.tsx') &&
    !relFile.endsWith('src/app/features/agents/AgentControlPanel.tsx') &&
    !relFile.endsWith('src/app/widgets/agent-detail/AgentDetailView.tsx') &&
    !relFile.endsWith('src/app/entities/agent/model/agents.store.ts') &&
    !relFile.endsWith('src/app/shared/model/agents.store.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return THIS_COMPUTER_SETUP_JARGON_PATTERNS.some((pattern) => pattern.test(line))
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

function hasAgentSharedStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/entities/agent/model/status-labels.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_SHARED_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasReviewDecisionJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return REVIEW_DECISION_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasReviewHistoryDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/context/ApprovalQueueView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return REVIEW_HISTORY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasNoteSpaceJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return NOTE_SPACE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasWorkSetupLoadDeadEndCopy(lines, index, line) {
  if (isLikelyGuardOrParserLine(line)) return false
  if (
    index > 0 &&
    /\bcouldNotLoad\b/.test(lines[index - 1]) &&
    WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(line))
  ) {
    return false
  }
  if (
    !/\bcouldNotLoad\b/.test(line) &&
    !WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(line))
  ) {
    return false
  }
  const context = lines.slice(index, Math.min(lines.length, index + 3)).join(' ')
  if (!WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(context))) return false
  if (!WORK_SETUP_LOAD_RECOVERY_PATTERN.test(context)) return true
  return WORK_SETUP_LOAD_PATTERNS.some((pattern) => pattern.test(context))
}

function hasProviderCheckJargonCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_CHECK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProviderZeroReadyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/ProvidersSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_ZERO_READY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProviderTestFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/providerTestErrorMessage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_TEST_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProviderSettingsFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/providerSettingsErrorMessage.ts')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_SETTINGS_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProviderAddressJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/ProvidersSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PROVIDER_ADDRESS_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminUsersEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/UserManagement.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_USERS_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminOrgsEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/OrganizationsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_ORGS_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminAgentActivityDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/AgentsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_AGENT_ACTIVITY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminAgentFieldDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/AgentsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_AGENT_FIELD_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminAgentStatusFallbackDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/AgentsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_AGENT_STATUS_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminAgentEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/AgentsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ADMIN_AGENT_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAdminLoadErrorDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/adminErrorCopy.ts')) return false
  return ADMIN_LOAD_ERROR_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
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

function hasBillingUsageDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/billing/BillingPage.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BILLING_USAGE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBillingReceiptLinkDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/billing/InvoiceList.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BILLING_RECEIPT_LINK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAnalyticsChartDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/analytics/AnalyticsDashboard.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ANALYTICS_CHART_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAnalyticsUsefulEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/analytics/ContextUsageDashboard.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ANALYTICS_USEFUL_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAnalyticsReviewEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/analytics/ContextUsageDashboard.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ANALYTICS_REVIEW_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAnalyticsUpdatedTimeDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/analytics/ContextUsageDashboard.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ANALYTICS_UPDATED_TIME_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAnalyticsGuidanceJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/analytics/AnalyticsDashboard.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ANALYTICS_GUIDANCE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasActivityFeedEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/feed/ActivityFeed.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACTIVITY_FEED_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedItemOptionalEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/entities/context/ui/InjectionPreviewModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_ITEM_OPTIONAL_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedItemSelectionEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/entities/context/ui/InjectionPreviewModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_ITEM_SELECTION_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasInboxNeedsActionEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/inbox/InboxView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return INBOX_NEEDS_ACTION_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskAgentAssignmentDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx') &&
    !relFile.endsWith('src/app/features/list/ListView.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_AGENT_ASSIGNMENT_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskListEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/list/ListView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_LIST_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskFormAgentStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/TaskFormModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_FORM_AGENT_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskFormQueueLoadFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/TaskFormModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_FORM_QUEUE_LOAD_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskSupportReferenceDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/TaskDetailPanel.tsx') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_SUPPORT_REFERENCE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasContextCandidatePreviewDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/detail/ContextCandidatesList.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CONTEXT_CANDIDATE_PREVIEW_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskAgentCapabilityJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/TaskDetailPanel.tsx') &&
    !relFile.endsWith('src/app/features/board/AssignmentReadinessPanel.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_AGENT_CAPABILITY_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskDetailAgentSetupDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/TaskDetailPanel.tsx') &&
    !relFile.endsWith('src/app/features/detail/taskDetailErrorMessages.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_DETAIL_AGENT_SETUP_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskDetailLoadFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/detail/taskDetailErrorMessages.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_DETAIL_LOAD_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBoardLoadFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/boardErrorMessages.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BOARD_LOAD_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBoardAgentSetupDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/board/boardErrorMessages.ts') &&
    !relFile.endsWith('src/app/features/board/AssignmentReadinessPanel.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return BOARD_AGENT_SETUP_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasBoardClearDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/AssignmentReadinessPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return BOARD_CLEAR_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAuthFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/auth/AuthPage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AUTH_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAuthManagerDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/shared/auth/AuthManager.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AUTH_MANAGER_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasNetworkFailureFirstCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/auth/AuthPage.ts') &&
    !relFile.endsWith('src/app/shared/auth/AuthManager.ts') &&
    !relFile.endsWith('src/app/shared/api/legacy/AgentAPI.ts') &&
    !relFile.endsWith('src/app/features/agents/AgentControlPanel.tsx') &&
    !relFile.endsWith('src/app/features/chat/useChatStream.ts') &&
    !relFile.endsWith('src/app/shared/model/chat.errors.ts') &&
    !relFile.endsWith('src/app/entities/context/model/feedbackErrorMessage.ts') &&
    !relFile.endsWith('src/app/layouts/sidebar/ProjectTree.tsx') &&
    !relFile.endsWith('src/app/pages/settings/model/workspaceSettingsErrorMessage.ts') &&
    !relFile.endsWith('src/app/shared/model/settings.store.ts') &&
    !relFile.endsWith('src/app/features/settings/providerSettingsErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/platformKeyErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/gitCredentialsErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/sshKeysErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/accountErrorMessages.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return NETWORK_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasWorkspaceResourceFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/shared/lib/workspaceResourceErrorMessage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return WORKSPACE_RESOURCE_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProjectCreateFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/manage-project/ui/CreateProjectForm.tsx')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return PROJECT_CREATE_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCloneRetryFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/manage-project/ui/CloneStatusBadge.tsx')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return CLONE_RETRY_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCloneFailureRawMessageCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/manage-project/ui/CloneStatusBadge.tsx')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return CLONE_FAILURE_RAW_MESSAGE_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentConfigDetailDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentConfigTab.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_CONFIG_DETAIL_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentListSummaryDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentListView.tsx')) return false
  if (line.trim().startsWith('//') || line.trim().startsWith('*')) return false
  return AGENT_LIST_SUMMARY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentToolSummaryDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentPluginsTab.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_TOOL_SUMMARY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentAiServiceDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/agent/model/display-labels.ts') &&
    !relFile.endsWith('src/app/entities/agent/model/agents.store.ts') &&
    !relFile.endsWith('src/app/shared/model/agents.store.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_AI_SERVICE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentModelDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/agent/model/agents.store.ts') &&
    !relFile.endsWith('src/app/shared/model/agents.store.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_MODEL_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAccessLevelDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/entities/user/model/roleLabels.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACCESS_LEVEL_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentTemplateRoleJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/CreateAgentModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_TEMPLATE_ROLE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCreateAgentOptionalContextDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/CreateAgentModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CREATE_AGENT_OPTIONAL_CONTEXT_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProjectShareRoleJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/layouts/sidebar/ProjectTree.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PROJECT_SHARE_ROLE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasVagueAccessRecoveryCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return VAGUE_ACCESS_RECOVERY_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTimelineEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/views/TimelineView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TIMELINE_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasWorkshop3DEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/views/Workshop3DView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return WORKSHOP_3D_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentDetailActivityDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/agent-detail/AgentDetailView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_DETAIL_ACTIVITY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentDetailFileAccessDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/agent-detail/AgentDetailView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_DETAIL_FILE_ACCESS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentDetailAvailabilityDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/agent-detail/AgentDetailView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_DETAIL_AVAILABILITY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentDetailStartFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/widgets/agent-detail/AgentDetailView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_DETAIL_START_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentApiLifecycleDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_API_LIFECYCLE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTitleStyleGuidanceCopy(line) {
  if (isLikelyGuardOrParserLine(line)) return false
  return TITLE_STYLE_GUIDANCE_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCliImageStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/CliImagesPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CLI_IMAGE_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCliImageActionFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/CliImagesPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CLI_IMAGE_ACTION_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCliImageResultJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/CliImagesPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CLI_IMAGE_RESULT_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSystemHealthStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/admin/SystemHealth.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return SYSTEM_HEALTH_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAccessKeyLastUsedDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/KeysSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACCESS_KEY_LAST_USED_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasDateFallbackDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/settings/GitCredentialsSection.tsx') &&
    !relFile.endsWith('src/app/features/settings/SshKeysSection.tsx') &&
    !relFile.endsWith('src/app/features/settings/KeysSection.tsx') &&
    !relFile.endsWith('src/app/features/admin/UserManagement.tsx') &&
    !relFile.endsWith('src/app/features/admin/OrganizationsPanel.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return DATE_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAccountProfileDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/AccountSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACCOUNT_PROFILE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAccountProfileRoleJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/AccountSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACCOUNT_PROFILE_ROLE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasLocaleAccessRoleJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return LOCALE_ACCESS_ROLE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasStartGuideResetJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/settings/AccountSection.tsx') &&
    !relFile.endsWith('src/app/pages/settings/ui/SettingsLayout.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return START_GUIDE_RESET_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasStartGuideFailureFirstCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/settings/AccountSection.tsx') &&
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return START_GUIDE_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskViewLabelJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/layouts/TopBar.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_VIEW_LABEL_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTopBarCreateTaskJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/layouts/TopBar.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TOP_BAR_CREATE_TASK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCommandPaletteCreateTaskJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/cmdk/CommandPalette.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return COMMAND_PALETTE_CREATE_TASK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasProjectMenuCreateTaskJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/layouts/sidebar/ProjectTree.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PROJECT_MENU_CREATE_TASK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskFormSubmitLabelJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/TaskFormModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_FORM_SUBMIT_LABEL_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskFormNoProjectDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/TaskFormModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_FORM_NO_PROJECT_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskFormNoAgentDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/board/TaskFormModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_FORM_NO_AGENT_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasQuickCreateDraftTaskJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/board/QuickCreate.tsx') &&
    !relFile.endsWith('src/app/features/board/KanbanColumn.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return QUICK_CREATE_DRAFT_TASK_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentTaskQueueSubmitLabelJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentGroupsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_TASK_QUEUE_SUBMIT_LABEL_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentTaskQueueEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentGroupsPanel.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_TASK_QUEUE_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSkillMaintainerFallbackDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return SKILL_MAINTAINER_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedInstructionSummaryFallbackCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/skills/SkillCard.tsx') &&
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_INSTRUCTION_SUMMARY_FALLBACK_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedInstructionToolTooltipFallbackCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_INSTRUCTION_TOOL_TOOLTIP_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedInstructionsLoadDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/skills/SkillsView.tsx') &&
    !relFile.endsWith('src/app/shared/model/skills.store.ts')
  ) {
    return false
  }
  return SAVED_INSTRUCTIONS_LOAD_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedInstructionCreateFailureFirstCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/skills/model/createSkillErrorMessage.ts') &&
    !relFile.endsWith('src/app/shared/model/skills.store.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_INSTRUCTION_CREATE_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSavedInstructionTemplateJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/skills/CreateSkillModal.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return SAVED_INSTRUCTION_TEMPLATE_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRuntimeSignInDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/RuntimeSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return RUNTIME_SIGN_IN_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRuntimeDefaultLocationDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/RuntimeSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return RUNTIME_DEFAULT_LOCATION_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasRuntimeSetupStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/RuntimeSection.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return RUNTIME_SETUP_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSettingsLoadErrorDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/settings/gitCredentialsErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/platformKeyErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/providerSettingsErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/sshKeysErrorMessage.ts')
  ) {
    return false
  }
  return SETTINGS_LOAD_ERROR_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCodeAccessFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/gitCredentialsErrorMessage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CODE_ACCESS_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasSshCodeAccessFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/sshKeysErrorMessage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return SSH_CODE_ACCESS_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasPlatformKeyFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/platformKeyErrorMessage.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return PLATFORM_KEY_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAccountSettingsFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/settings/accountErrorMessages.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return ACCOUNT_SETTINGS_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasCommonErrorFailureFirstCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return COMMON_ERROR_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasLocaleVagueErrorLabelCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/i18n/locales/zh.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return LOCALE_VAGUE_ERROR_LABEL_PATTERNS.some((pattern) => pattern.test(line))
}

function hasWorkspaceSettingsFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/pages/settings/model/workspaceSettingsErrorMessage.ts')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return WORKSPACE_SETTINGS_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasLoadErrorTitleDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/shared/model/chat.errors.ts') &&
    !relFile.endsWith('src/app/shared/model/agents.store.ts') &&
    !relFile.endsWith('src/app/shared/model/billing.store.ts') &&
    !relFile.endsWith('src/app/entities/agent/model/agents.store.ts') &&
    !relFile.endsWith('src/app/features/agents/model/pluginErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/agents/model/taskErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/manage-members/model/resourceMemberErrorMessages.ts') &&
    !relFile.endsWith('src/app/features/settings/runtimeErrorMessages.ts') &&
    !relFile.endsWith('src/app/pages/settings/model/workspaceSettingsErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/ResourcesSection.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return LOAD_ERROR_TITLE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasAgentSetupFallbackDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/agent/model/display-labels.ts') &&
    !relFile.endsWith('src/app/entities/agent/model/agents.store.ts') &&
    !relFile.endsWith('src/app/entities/agent/model/runtime-kind.ts') &&
    !relFile.endsWith('src/app/entities/context/ui/InjectionPreviewModal.tsx') &&
    !relFile.endsWith('src/app/features/admin/AgentsPanel.tsx') &&
    !relFile.endsWith('src/app/features/agents/AgentConfigTab.tsx') &&
    !relFile.endsWith('src/app/features/analytics/ContextUsageDashboard.tsx') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx') &&
    !relFile.endsWith('src/app/features/settings/RuntimeSection.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return AGENT_SETUP_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasLiveWorkStatusDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/agents/AgentTerminalTab.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return LIVE_WORK_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskDetailRunStatusDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextTab.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_DETAIL_RUN_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskStatusFallbackDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/task/model/taskLabels.ts') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextTab.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_STATUS_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskCompletionSummaryDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/hooks/useWsDispatch.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_COMPLETION_SUMMARY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskOwnerInputJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/hooks/useWsDispatch.ts') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_OWNER_INPUT_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskRecoveryStatusDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/task/model/taskLabels.ts') &&
    !relFile.endsWith('src/app/features/agents/AgentTasksTab.tsx') &&
    !relFile.endsWith('src/app/features/agents/AgentGroupsPanel.tsx') &&
    !relFile.endsWith('src/app/features/board/TaskCard.tsx') &&
    !relFile.endsWith('src/app/features/board/KanbanColumn.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/DescriptionTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx') &&
    !relFile.endsWith('src/app/features/feed/FeedItem.tsx') &&
    !relFile.endsWith('src/app/features/inbox/InboxItem.tsx') &&
    !relFile.endsWith('src/app/features/list/ListView.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_RECOVERY_STATUS_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTaskDetailEmptyDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/DescriptionTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/HistoryTab.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TASK_DETAIL_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasContextFallbackDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/entities/context/ui/InjectionPreviewModal.tsx') &&
    !relFile.endsWith('src/app/features/analytics/ContextUsageDashboard.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextAppliedList.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextCandidatesList.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return CONTEXT_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasContextWorkHistoryJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/detail/ContextTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextEvidenceList.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextCandidatesList.tsx') &&
    !relFile.endsWith('src/app/features/detail/DescriptionTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/taskDetailErrorMessages.ts') &&
    !relFile.endsWith('src/app/features/chat/ToolCallDetail.tsx') &&
    !relFile.endsWith('src/app/features/board/KanbanColumn.tsx') &&
    !relFile.endsWith('src/app/entities/context/ui/FeedbackControls.tsx') &&
    !relFile.endsWith('src/app/features/inbox/InboxView.tsx') &&
    !relFile.endsWith('src/app/features/billing/UsageMeter.tsx') &&
    !relFile.endsWith('src/app/features/analytics/AnalyticsDashboard.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return CONTEXT_WORK_HISTORY_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatMessageFallbackDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/chat/ChatView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_MESSAGE_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatOperatorJargonCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/chat/ChatView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_OPERATOR_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatFilterEmptyDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/chat/ChatView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_FILTER_EMPTY_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatToolStepDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/chat/ToolCallDetail.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_TOOL_STEP_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasVagueNeedsReviewCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/hooks/useWsDispatch.ts') &&
    !relFile.endsWith('src/app/features/context/ApprovalQueueView.tsx') &&
    !relFile.endsWith('src/app/features/settings/AccountSection.tsx') &&
    !relFile.endsWith('src/app/features/chat/ChatView.tsx') &&
    !relFile.endsWith('src/app/entities/context/ui/FeedbackControls.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return VAGUE_NEEDS_REVIEW_COPY_PATTERNS.some((pattern) => pattern.test(line))
}

function hasVagueNeedsAttentionCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/analytics/AnalyticsDashboard.tsx') &&
    !relFile.endsWith('src/app/features/chat/ChatView.tsx') &&
    !relFile.endsWith('src/app/features/chat/ToolCallDetail.tsx') &&
    !relFile.endsWith('src/app/features/agents/AgentTasksTab.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextEvidenceList.tsx') &&
    !relFile.endsWith('src/app/features/settings/RuntimeSection.tsx') &&
    !relFile.endsWith('src/app/features/settings/providerTestErrorMessage.ts') &&
    !relFile.endsWith('src/app/features/settings/ProvidersSection.tsx') &&
    !relFile.endsWith('src/app/features/admin/CliImagesPanel.tsx') &&
    !relFile.endsWith('src/app/widgets/views/TimelineView.tsx') &&
    !relFile.endsWith('src/app/shared/i18n/locales/en.ts') &&
    !relFile.endsWith('src/app/shared/lib/taskFailureCopy.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return VAGUE_NEEDS_ATTENTION_COPY_PATTERNS.some((pattern) => pattern.test(line))
}

function hasTechnicalProblemJargonCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/chat/ToolCallDetail.tsx') &&
    !relFile.endsWith('src/app/features/detail/ContextEvidenceList.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return TECHNICAL_PROBLEM_JARGON_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatOfflineDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/chat/ChatView.tsx') &&
    !relFile.endsWith('src/app/features/chat/ChatComposer.tsx')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_OFFLINE_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasChatStreamFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/chat/useChatStream.ts')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return CHAT_STREAM_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasGovernanceAuditFallbackDeadEndCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/governance/AuditLogView.tsx')) return false
  if (isLikelyGuardOrParserLine(line)) return false
  return GOVERNANCE_AUDIT_FALLBACK_DEAD_END_PATTERNS.some((pattern) => pattern.test(line))
}

function hasGovernanceAuditErrorFailureFirstCopy(relFile, line) {
  if (!relFile.endsWith('src/app/features/governance/governanceAuditErrorMessages.ts')) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return GOVERNANCE_AUDIT_ERROR_FAILURE_FIRST_PATTERNS.some((pattern) => pattern.test(line))
}

function hasDuplicateRecoveryDeadEndCopy(relFile, line) {
  if (
    !relFile.endsWith('src/app/features/board/boardErrorMessages.ts') &&
    !relFile.endsWith('src/app/features/detail/taskDetailErrorMessages.ts') &&
    !relFile.endsWith('src/app/features/context/approvalQueueErrorMessages.ts') &&
    !relFile.endsWith('src/app/entities/navigation/model/navigation.store.ts')
  ) {
    return false
  }
  if (isLikelyGuardOrParserLine(line)) return false
  return DUPLICATE_RECOVERY_COPY_PATTERNS.some((pattern) => pattern.test(line))
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

    if (hasThisComputerSetupJargonCopy(relFile, line)) {
      findings.push({
        type: 'this-computer-setup-copy',
        location,
        message:
          'This-computer setup copy must use setup text/steps and plain Terminal or PowerShell guidance instead of command-window jargon.',
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

    if (hasAgentSharedStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-shared-status-copy',
        location,
        message: 'Shared agent status fallbacks must tell beginners to refresh or check status.',
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

    if (hasReviewHistoryDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'review-history-empty-copy',
        location,
        message:
          'Saved-item review history empty states must tell beginners to review the first suggestion.',
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

    if (hasProviderZeroReadyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'provider-zero-ready-copy',
        location,
        message:
          'AI service setup summaries must tell beginners to check, enable, or add a service.',
        sample: line.trim(),
      })
    }

    if (hasCodeAccessFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'code-access-error-copy',
        location,
        message: 'Code access errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasSshCodeAccessFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'ssh-code-access-error-copy',
        location,
        message: 'SSH code access errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasPlatformKeyFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'platform-key-error-copy',
        location,
        message:
          'Outside tool access key errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasAccountSettingsFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'account-settings-error-copy',
        location,
        message:
          'Account settings errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasCommonErrorFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'common-error-copy',
        location,
        message: 'Common error translations must start with the recovery action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasLocaleVagueErrorLabelCopy(relFile, line)) {
      findings.push({
        type: 'locale-vague-error-label-copy',
        location,
        message:
          'Localized error and status labels must tell beginners what to check, not say needs attention.',
        sample: line.trim(),
      })
    }

    if (hasWorkspaceSettingsFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'workspace-settings-error-copy',
        location,
        message:
          'Workspace settings errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasProviderTestFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'provider-test-error-copy',
        location,
        message:
          'AI service connection check errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasProviderSettingsFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'provider-settings-error-copy',
        location,
        message:
          'AI service settings errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasProviderAddressJargonCopy(relFile, line)) {
      findings.push({
        type: 'provider-address-copy',
        location,
        message:
          'AI service address copy must avoid raw endpoint URLs and start from the safe default.',
        sample: line.trim(),
      })
    }

    if (hasAdminUsersEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-users-empty-copy',
        location,
        message: 'User management empty states must tell beginners to invite people first.',
        sample: line.trim(),
      })
    }

    if (hasAdminOrgsEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-orgs-empty-copy',
        location,
        message:
          'Team space empty states must tell beginners to create or sync a team space first.',
        sample: line.trim(),
      })
    }

    if (hasAdminAgentActivityDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-agent-activity-copy',
        location,
        message: 'Admin agent activity copy must explain that activity appears after work starts.',
        sample: line.trim(),
      })
    }

    if (hasAdminAgentFieldDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-agent-field-copy',
        location,
        message:
          'Admin agent missing-field copy must tell beginners to refresh agents before deciding.',
        sample: line.trim(),
      })
    }

    if (hasAdminAgentStatusFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-agent-status-fallback-copy',
        location,
        message: 'Admin agent status fallback copy must tell beginners which status to check.',
        sample: line.trim(),
      })
    }

    if (hasAdminAgentEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-agent-empty-copy',
        location,
        message:
          'Admin agent empty states must tell beginners to create, connect, or clear filters.',
        sample: line.trim(),
      })
    }

    if (hasAdminLoadErrorDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'admin-load-error-copy',
        location,
        message: 'Admin load error titles must tell beginners what to refresh or check.',
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

    if (hasBillingUsageDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'billing-usage-copy',
        location,
        message: 'Billing usage copy must explain what creates the first usage report.',
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

    if (hasAnalyticsChartDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'analytics-chart-empty-copy',
        location,
        message: 'Analytics chart empty states must tell beginners what creates the first data.',
        sample: line.trim(),
      })
    }

    if (hasAnalyticsUsefulEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'analytics-useful-empty-copy',
        location,
        message: 'Saved item reuse empty states must tell beginners to mark useful items first.',
        sample: line.trim(),
      })
    }

    if (hasAnalyticsReviewEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'analytics-review-empty-copy',
        location,
        message:
          'Saved item review empty states must name what is clear instead of using vague nothing-copy.',
        sample: line.trim(),
      })
    }

    if (hasAnalyticsUpdatedTimeDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'analytics-updated-time-copy',
        location,
        message: 'Analytics updated-time fallback must tell beginners to refresh analytics.',
        sample: line.trim(),
      })
    }

    if (hasAnalyticsGuidanceJargonCopy(relFile, line)) {
      findings.push({
        type: 'analytics-guidance-copy',
        location,
        message: 'Analytics guidance must describe the next check without failed-tool jargon.',
        sample: line.trim(),
      })
    }

    if (hasActivityFeedEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'activity-feed-empty-copy',
        location,
        message:
          'Activity feed empty states must explain that progress updates have not appeared yet.',
        sample: line.trim(),
      })
    }

    if (hasSavedItemOptionalEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'saved-item-optional-empty-copy',
        location,
        message: 'Saved item preview empty states must explain how more saved items appear later.',
        sample: line.trim(),
      })
    }

    if (hasSavedItemSelectionEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'saved-item-selection-empty-copy',
        location,
        message:
          'Saved item selection empty states must name selected or pinned saved items instead of using vague nothing-copy.',
        sample: line.trim(),
      })
    }

    if (hasInboxNeedsActionEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'inbox-needs-action-empty-copy',
        location,
        message:
          'Inbox action-item empty states must say the user is caught up instead of using vague nothing-copy.',
        sample: line.trim(),
      })
    }

    if (hasTaskAgentAssignmentDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-agent-assignment-copy',
        location,
        message:
          'Task agent copy must tell beginners to choose an agent or refresh task data before deciding.',
        sample: line.trim(),
      })
    }

    if (hasTaskListEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-list-empty-copy',
        location,
        message: 'Task list empty states must point beginners to the board action.',
        sample: line.trim(),
      })
    }

    if (hasTaskFormAgentStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-form-agent-status-copy',
        location,
        message: 'Task creation agent status copy must tell beginners to refresh agent status.',
        sample: line.trim(),
      })
    }

    if (hasTaskFormQueueLoadFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'task-form-queue-load-copy',
        location,
        message: 'Task creation queue load errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasTaskSupportReferenceDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-support-reference-copy',
        location,
        message: 'Task support reference fallback must tell beginners to refresh task details.',
        sample: line.trim(),
      })
    }

    if (hasContextCandidatePreviewDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'context-candidate-preview-copy',
        location,
        message: 'Saved item preview fallbacks must tell beginners to read the suggestion.',
        sample: line.trim(),
      })
    }

    if (hasTaskAgentCapabilityJargonCopy(relFile, line)) {
      findings.push({
        type: 'task-agent-capability-copy',
        location,
        message: 'Task agent capability copy must use beginner-facing action wording.',
        sample: line.trim(),
      })
    }

    if (hasTaskDetailAgentSetupDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-detail-agent-setup-copy',
        location,
        message:
          'Task detail no-agent copy must tell beginners to open Agents, start or connect an agent, and refresh the task.',
        sample: line.trim(),
      })
    }

    if (hasTaskDetailLoadFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'task-detail-load-copy',
        location,
        message: 'Task detail load errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasBoardLoadFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'board-load-copy',
        location,
        message: 'Board load errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasBoardAgentSetupDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'board-agent-setup-copy',
        location,
        message:
          'Board no-agent copy must tell beginners to open Agents, start or connect an agent, and refresh the board.',
        sample: line.trim(),
      })
    }

    if (hasBoardClearDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'board-clear-copy',
        location,
        message:
          'Board clear-state copy must tell beginners to create a task when they have work to send.',
        sample: line.trim(),
      })
    }

    if (hasAuthFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'auth-error-copy',
        location,
        message: 'Authentication errors must start with the next recovery action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasAuthManagerDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'auth-manager-copy',
        location,
        message: 'AuthManager fallbacks must tell beginners what to do next.',
        sample: line.trim(),
      })
    }

    if (hasNetworkFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'network-copy',
        location,
        message: 'Network errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasWorkspaceResourceFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'workspace-resource-copy',
        location,
        message: 'Team and project setting errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasProjectCreateFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'project-create-error-copy',
        location,
        message: 'Project creation errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasCloneRetryFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'clone-retry-error-copy',
        location,
        message: 'Code import retry errors must start with the next action for beginners.',
        sample: line.trim(),
      })
    }

    if (hasCloneFailureRawMessageCopy(relFile, line)) {
      findings.push({
        type: 'clone-failure-message-copy',
        location,
        message: 'Code import failure details must use beginner-safe recovery copy.',
        sample: line.trim(),
      })
    }

    if (hasAgentConfigDetailDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-config-detail-copy',
        location,
        message: 'Agent configuration missing-detail copy must tell beginners what to refresh.',
        sample: line.trim(),
      })
    }

    if (hasAgentListSummaryDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-list-summary-copy',
        location,
        message: 'Agent list empty summaries must point beginners to creating the first agent.',
        sample: line.trim(),
      })
    }

    if (hasAgentToolSummaryDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-tool-summary-copy',
        location,
        message: 'Agent tool missing-summary copy must tell beginners to keep the team setting.',
        sample: line.trim(),
      })
    }

    if (hasAgentAiServiceDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-ai-service-copy',
        location,
        message: 'Agent AI service fallback copy must tell beginners to refresh service data.',
        sample: line.trim(),
      })
    }

    if (hasAgentModelDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-model-copy',
        location,
        message: 'Agent AI model fallback copy must tell beginners to refresh model data.',
        sample: line.trim(),
      })
    }

    if (hasAccessLevelDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'access-level-copy',
        location,
        message: 'Access level fallback copy must tell beginners to refresh role data.',
        sample: line.trim(),
      })
    }

    if (hasAgentTemplateRoleJargonCopy(relFile, line)) {
      findings.push({
        type: 'agent-template-role-copy',
        location,
        message: 'Agent creation templates must say starter template instead of role template.',
        sample: line.trim(),
      })
    }

    if (hasProjectShareRoleJargonCopy(relFile, line)) {
      findings.push({
        type: 'project-share-role-copy',
        location,
        message: 'Project sharing copy must say what people can do instead of choose roles.',
        sample: line.trim(),
      })
    }

    if (hasVagueAccessRecoveryCopy(line)) {
      findings.push({
        type: 'vague-access-recovery-copy',
        location,
        message: 'Permission recovery copy must name the specific access the user needs.',
        sample: line.trim(),
      })
    }

    if (hasTimelineEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'timeline-empty-copy',
        location,
        message:
          'Timeline empty states must use an action title that tells beginners how to begin.',
        sample: line.trim(),
      })
    }

    if (hasWorkshop3DEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'workshop-3d-empty-copy',
        location,
        message: 'Visual map empty states must tell beginners to open Agents first.',
        sample: line.trim(),
      })
    }

    if (hasAgentDetailActivityDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-detail-activity-copy',
        location,
        message: 'Agent detail activity copy must tell beginners to open Tasks first.',
        sample: line.trim(),
      })
    }

    if (hasAgentDetailFileAccessDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-detail-file-access-copy',
        location,
        message:
          'Chat-only agent file access copy must tell beginners to use another agent for file work.',
        sample: line.trim(),
      })
    }

    if (hasAgentDetailAvailabilityDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-detail-availability-copy',
        location,
        message:
          'Agent detail availability copy must tell beginners which page or control to use next.',
        sample: line.trim(),
      })
    }

    if (hasAgentDetailStartFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'agent-detail-start-failure-copy',
        location,
        message:
          'Agent detail start failure copy must start with the recovery action, not the failure result.',
        sample: line.trim(),
      })
    }

    if (hasAgentApiLifecycleDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-api-lifecycle-copy',
        location,
        message:
          'Chat-only agent lifecycle copy must tell beginners to send a message, wait, or close chat.',
        sample: line.trim(),
      })
    }

    if (hasTitleStyleGuidanceCopy(line)) {
      findings.push({
        type: 'title-style-guidance-copy',
        location,
        message:
          'Beginner guidance must use action-first sentences instead of title-style menu labels.',
        sample: line.trim(),
      })
    }

    if (hasCliImageStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'cli-image-status-copy',
        location,
        message: 'Agent tool update status copy must tell beginners to choose Check now.',
        sample: line.trim(),
      })
    }

    if (hasCliImageActionFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'cli-image-action-copy',
        location,
        message: 'Agent tool update action errors must start with the next step.',
        sample: line.trim(),
      })
    }

    if (hasCliImageResultJargonCopy(relFile, line)) {
      findings.push({
        type: 'cli-image-result-copy',
        location,
        message:
          'Agent tool update result copy must explain what to retry or check instead of failed/skipped/error jargon.',
        sample: line.trim(),
      })
    }

    if (hasSystemHealthStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'system-health-status-copy',
        location,
        message: 'App health status copy must tell beginners to choose Check now.',
        sample: line.trim(),
      })
    }

    if (hasAccessKeyLastUsedDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'access-key-last-used-copy',
        location,
        message: 'Outside tool access copy must explain that a trusted tool uses the key first.',
        sample: line.trim(),
      })
    }

    if (hasDateFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'date-fallback-copy',
        location,
        message: 'Date fallback copy must tell beginners which list to refresh or check.',
        sample: line.trim(),
      })
    }

    if (hasAccountProfileDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'account-profile-copy',
        location,
        message:
          'Account profile fallbacks must tell beginners to refresh and reload account data.',
        sample: line.trim(),
      })
    }

    if (hasAccountProfileRoleJargonCopy(relFile, line)) {
      findings.push({
        type: 'account-profile-role-copy',
        location,
        message: 'Account profile must say access level instead of role.',
        sample: line.trim(),
      })
    }

    if (hasLocaleAccessRoleJargonCopy(relFile, line)) {
      findings.push({
        type: 'locale-access-role-copy',
        location,
        message: 'Localized user access copy must say access level instead of role jargon.',
        sample: line.trim(),
      })
    }

    if (hasStartGuideResetJargonCopy(relFile, line)) {
      findings.push({
        type: 'start-guide-reset-copy',
        location,
        message:
          'Start guide restore copy must say it shows the setup checklist again, not that it resets the guide.',
        sample: line.trim(),
      })
    }

    if (hasStartGuideFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'start-guide-error-copy',
        location,
        message:
          'Start and setup checklist errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasTaskViewLabelJargonCopy(relFile, line)) {
      findings.push({
        type: 'task-view-label-copy',
        location,
        message: 'Task view labels must use beginner-facing names such as Map instead of bare 3D.',
        sample: line.trim(),
      })
    }

    if (hasTopBarCreateTaskJargonCopy(relFile, line)) {
      findings.push({
        type: 'top-bar-create-task-copy',
        location,
        message:
          'Top bar task creation must use a Plus icon with a clear New task label instead of a manual + Task label.',
        sample: line.trim(),
      })
    }

    if (hasCommandPaletteCreateTaskJargonCopy(relFile, line)) {
      findings.push({
        type: 'command-palette-create-task-copy',
        location,
        message:
          'Command palette task creation must use the same clear New task label and a concrete agent-task description.',
        sample: line.trim(),
      })
    }

    if (hasProjectMenuCreateTaskJargonCopy(relFile, line)) {
      findings.push({
        type: 'project-menu-create-task-copy',
        location,
        message:
          'Project menu task creation must say New task for this project and explain that it opens the task form with this project selected.',
        sample: line.trim(),
      })
    }

    if (hasTaskFormSubmitLabelJargonCopy(relFile, line)) {
      findings.push({
        type: 'task-form-submit-label-copy',
        location,
        message:
          'Task creation submit labels must use sentence case and keep the task action explicit for first-time users.',
        sample: line.trim(),
      })
    }

    if (hasTaskFormNoProjectDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-form-no-project-copy',
        location,
        message:
          'Task creation with no projects must explain why a project is needed and offer project settings.',
        sample: line.trim(),
      })
    }

    if (hasTaskFormNoAgentDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-form-no-agent-copy',
        location,
        message:
          'Task creation with no online agents must offer agent setup while still explaining that the task can wait.',
        sample: line.trim(),
      })
    }

    if (hasQuickCreateDraftTaskJargonCopy(relFile, line)) {
      findings.push({
        type: 'quick-create-draft-task-copy',
        location,
        message:
          'Quick task creation must say Add Task or Save Task and explain Not sent yet instead of draft-task jargon.',
        sample: line.trim(),
      })
    }

    if (hasAgentTaskQueueSubmitLabelJargonCopy(relFile, line)) {
      findings.push({
        type: 'agent-task-queue-submit-label-copy',
        location,
        message:
          'Task queue submit labels must use sentence case so first-time users see one consistent action style.',
        sample: line.trim(),
      })
    }

    if (hasAgentTaskQueueEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-task-queue-empty-copy',
        location,
        message:
          'Task queue empty states must start with the action to create the first queue or task.',
        sample: line.trim(),
      })
    }

    if (hasCreateAgentOptionalContextDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'create-agent-optional-context-copy',
        location,
        message:
          'Create Agent optional project and queue copy must say the user can choose later instead of showing no-selection labels.',
        sample: line.trim(),
      })
    }

    if (hasSkillMaintainerFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'skill-maintainer-fallback-copy',
        location,
        message:
          'Saved instruction maintainer fallback copy must tell beginners to refresh saved instructions.',
        sample: line.trim(),
      })
    }

    if (hasSavedInstructionSummaryFallbackCopy(relFile, line)) {
      findings.push({
        type: 'saved-instruction-summary-fallback-copy',
        location,
        message:
          'Saved instruction summary fallback copy must tell beginners to check details before using it.',
        sample: line.trim(),
      })
    }

    if (hasSavedInstructionToolTooltipFallbackCopy(relFile, line)) {
      findings.push({
        type: 'saved-instruction-tool-tooltip-copy',
        location,
        message:
          'Saved instruction work-tool tooltip copy must tell beginners where to check setup.',
        sample: line.trim(),
      })
    }

    if (hasSavedInstructionsLoadDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'saved-instructions-load-copy',
        location,
        message: 'Saved instructions load fallback copy must point beginners to the retry action.',
        sample: line.trim(),
      })
    }

    if (hasSavedInstructionCreateFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'saved-instruction-create-copy',
        location,
        message:
          'Saved instruction creation errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasSavedInstructionTemplateJargonCopy(relFile, line)) {
      findings.push({
        type: 'saved-instruction-template-copy',
        location,
        message:
          'Saved instruction templates must use plain result language instead of PR/CI status jargon.',
        sample: line.trim(),
      })
    }

    if (hasRuntimeSignInDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'runtime-sign-in-copy',
        location,
        message:
          'Work setup summaries must tell beginners to sign in before starting affected agents.',
        sample: line.trim(),
      })
    }

    if (hasRuntimeDefaultLocationDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'runtime-default-location-copy',
        location,
        message:
          'Default agent location copy must tell beginners to load setup before choosing a location.',
        sample: line.trim(),
      })
    }

    if (hasRuntimeSetupStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'runtime-setup-status-copy',
        location,
        message:
          'Runtime setup status copy must tell beginners what to start, wake, or check next.',
        sample: line.trim(),
      })
    }

    if (hasSettingsLoadErrorDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'settings-load-error-copy',
        location,
        message: 'Settings load error titles must tell beginners to refresh Settings.',
        sample: line.trim(),
      })
    }

    if (hasLoadErrorTitleDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'load-error-title-copy',
        location,
        message: 'Load error titles must tell beginners which view to retry or refresh.',
        sample: line.trim(),
      })
    }

    if (hasAgentSetupFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'agent-setup-fallback-copy',
        location,
        message:
          'Agent setup fallback copy must tell beginners to refresh or check the exact setup field.',
        sample: line.trim(),
      })
    }

    if (hasLiveWorkStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'live-work-status-copy',
        location,
        message: 'Live work status copy must tell beginners to refresh status before deciding.',
        sample: line.trim(),
      })
    }

    if (hasTaskDetailRunStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-detail-run-status-copy',
        location,
        message:
          'Task detail run status copy must tell beginners to refresh task status before deciding.',
        sample: line.trim(),
      })
    }

    if (hasTaskStatusFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-status-fallback-copy',
        location,
        message:
          'Task status and priority fallback copy must tell beginners to refresh or check the task field.',
        sample: line.trim(),
      })
    }

    if (hasTaskCompletionSummaryDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-completion-summary-copy',
        location,
        message: 'Completed-task notifications must tell beginners where to confirm the result.',
        sample: line.trim(),
      })
    }

    if (hasTaskOwnerInputJargonCopy(relFile, line)) {
      findings.push({
        type: 'task-owner-input-copy',
        location,
        message: 'Task owner guidance must ask for the user answer, not owner-input jargon.',
        sample: line.trim(),
      })
    }

    if (hasTaskRecoveryStatusDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-recovery-status-copy',
        location,
        message: 'Failed task status copy must tell beginners to review recovery.',
        sample: line.trim(),
      })
    }

    if (hasTaskDetailEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'task-detail-empty-copy',
        location,
        message: 'Task detail empty states must tell beginners where to check or what to do next.',
        sample: line.trim(),
      })
    }

    if (hasContextFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'context-fallback-copy',
        location,
        message:
          'Saved item, sharing, safety, and task-type fallbacks must tell beginners what to check or refresh.',
        sample: line.trim(),
      })
    }

    if (hasContextWorkHistoryJargonCopy(relFile, line)) {
      findings.push({
        type: 'context-work-history-copy',
        location,
        message:
          'Task and saved-item copy must use work or task wording instead of run-detail jargon.',
        sample: line.trim(),
      })
    }

    if (hasChatMessageFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'chat-message-fallback-copy',
        location,
        message:
          'Chat message sender fallbacks must tell beginners to refresh or check the sender.',
        sample: line.trim(),
      })
    }

    if (hasChatOperatorJargonCopy(relFile, line)) {
      findings.push({
        type: 'chat-operator-copy',
        location,
        message: 'Chat filters must explain You as the current user, not an operator.',
        sample: line.trim(),
      })
    }

    if (hasChatFilterEmptyDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'chat-filter-empty-copy',
        location,
        message:
          'Chat filter empty states must explain what users can check without reported-work jargon.',
        sample: line.trim(),
      })
    }

    if (hasChatToolStepDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'chat-tool-step-copy',
        location,
        message:
          'Chat tool step fallbacks must tell beginners to check the step before relying on it.',
        sample: line.trim(),
      })
    }

    if (hasVagueNeedsReviewCopy(relFile, line)) {
      findings.push({
        type: 'vague-needs-review-copy',
        location,
        message:
          'User-facing review copy must tell beginners what to check instead of saying needs review.',
        sample: line.trim(),
      })
    }

    if (hasVagueNeedsAttentionCopy(relFile, line)) {
      findings.push({
        type: 'vague-needs-attention-copy',
        location,
        message: 'User-facing attention copy must tell beginners what to do or what failed.',
        sample: line.trim(),
      })
    }

    if (hasTechnicalProblemJargonCopy(relFile, line)) {
      findings.push({
        type: 'technical-problem-copy',
        location,
        message:
          'Tool and saved-item problem copy must explain what to do without technical-problem jargon.',
        sample: line.trim(),
      })
    }

    if (hasChatOfflineDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'chat-offline-copy',
        location,
        message:
          'Chat offline copy must route beginners to the correct setup area instead of saying to start it.',
        sample: line.trim(),
      })
    }

    if (hasChatStreamFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'chat-stream-error-copy',
        location,
        message:
          'Chat stream errors must start with the resend action before explaining the failure.',
        sample: line.trim(),
      })
    }

    if (hasGovernanceAuditFallbackDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'governance-audit-fallback-copy',
        location,
        message: 'Governance audit fallbacks must tell beginners which audit field to check.',
        sample: line.trim(),
      })
    }

    if (hasGovernanceAuditErrorFailureFirstCopy(relFile, line)) {
      findings.push({
        type: 'governance-audit-error-copy',
        location,
        message:
          'Governance audit errors must start with the next action, not the failure summary.',
        sample: line.trim(),
      })
    }

    if (hasDuplicateRecoveryDeadEndCopy(relFile, line)) {
      findings.push({
        type: 'duplicate-recovery-copy',
        location,
        message: 'Recovery copy must avoid repeating the same refresh or retry step.',
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
