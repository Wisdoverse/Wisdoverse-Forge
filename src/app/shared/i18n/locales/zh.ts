/**
 * Chinese (Simplified) Translations
 *
 * 简体中文翻译
 */

import type { TranslationKeys } from './en'

export const zh: TranslationKeys = {
  // =========================================================================
  // 通用
  // =========================================================================
  common: {
    save: '保存',
    cancel: '取消',
    delete: '删除',
    confirm: '确认',
    close: '关闭',
    edit: '编辑',
    create: '创建',
    update: '更新',
    submit: '提交',
    reset: '重置',
    clear: '清除',
    search: '搜索',
    filter: '筛选',
    sort: '排序',
    refresh: '刷新',
    loading: '加载中...',
    saving: '保存中...',
    deleting: '删除中...',
    processing: '处理中...',
    error: '请查看提示信息，然后重试。',
    success: '成功',
    warning: '警告',
    info: '信息',
    yes: '是',
    no: '否',
    ok: '确定',
    back: '返回',
    next: '下一步',
    previous: '上一步',
    done: '完成',
    retry: '重试',
    copy: '复制',
    copied: '已复制！',
    download: '下载',
    upload: '上传',
    more: '更多',
    less: '收起',
    all: '全部',
    none: '无',
    select: '选择',
    selected: '已选择 {{count}} 项',
    noResults: '没有匹配结果。可以放宽搜索条件，或清除筛选后再试。',
    noData: '这里暂时没有内容。可以先创建第一项，或在设置完成后刷新。',
    optional: '可选',
    required: '必填',
  },

  // =========================================================================
  // 导航
  // =========================================================================
  nav: {
    home: '首页',
    start: '设置清单',
    dashboard: '仪表板',
    tasks: '任务',
    inbox: '收件箱',
    context: '保存内容',
    agents: 'Agent',
    skills: '保存的指令',
    analytics: '分析',
    billing: '账单',
    settings: '设置',
    help: '帮助',
    about: '关于',
    logout: '退出登录',
    profile: '个人资料',
    admin: '管理',
  },

  // =========================================================================
  // 新手开始
  // =========================================================================
  gettingStarted: {
    eyebrow: '首次使用',
    title: '按清单安全设置第一个 Agent',
    description: '一次只做一步。按这份设置清单创建 Agent、发送任务、验收结果。',
    skip: '跳过并打开任务',
    skipSaving: '正在跳过...',
    skipHint:
      '这只会隐藏左侧菜单里的 Start。项目、Agent 和任务都不会变化，也可以在设置里重新显示它。',
    skipError: '请检查网络，然后再点一次跳过。暂时无法隐藏设置清单。',
    progressCount: '{{complete}} / {{total}}',
    nextTitle: '下一步先做这个',
    readyTitle: '已经可以开始工作',
    readyDetail: '从任务页写一条小任务；想让 Agent 复用经验时，再查看保存好的指令。',
    readyCta: '写一条小任务',
    successLabel: '成功的样子：',
    currentProject: '当前项目',
    noProject: '打开项目设置，创建或选择一个项目。',
    projects: '项目',
    workLocations: {
      managed: '项目文件选项',
      local: '这台电脑',
      textOnly: '只处理文字的模式',
      ready: '一个工作位置',
    },
    stepStatus: {
      done: '已完成',
      next: '下一步',
      later: '稍后',
    },
    steps: {
      workspace: {
        title: '团队和项目',
        empty: '先创建一个团队和项目，让任务有明确归属。',
        why: '项目能让任务有明确归属，避免不知道任务该给谁处理。',
        success: '已经有团队和项目，并且当前项目已被选中。',
        create: '创建团队和项目',
        review: '查看团队和项目',
      },
      runtime: {
        title: '工作位置',
        empty: '先选择 Agent 在哪里工作：项目文件，或这台电脑。',
        ready: '{{location}} 已经可以接收 Agent 工作。',
        why: 'Agent 需要一个安全的工作位置，才能接收任务。',
        success: '项目文件或这台电脑已经可以接收 Agent 工作。',
        open: '选择工作位置',
        review: '查看工作位置',
      },
      provider: {
        title: '给 Agent 一个工作方式',
        empty: '先选一种方式：添加模型服务，或把这台电脑接入为 Agent。',
        needsTest: '先检查模型服务，再给 Agent 分配工作。',
        cliReady: '{{name}} 已经可以从{{location}}执行工作。',
        why: 'Agent 需要一种可用方式：已检查的模型服务用于文字回答，或从这台电脑接入的 Agent 用于实际操作。',
        success: '已经有一种可用方式：模型服务已检查通过，或这台电脑已接入为 Agent。',
        create: '添加模型服务',
        connectCli: '接入这台电脑',
        test: '检查模型服务',
        reviewProviders: '查看模型服务',
        reviewAgents: '查看 Agent',
      },
      agent: {
        title: 'Agent',
        empty: '先创建一个简单 Agent：文字 Agent、项目文件 Agent，或这台电脑上的 Agent。',
        why: 'Agent 会接收任务并返回结果。先创建一个简单 Agent 即可。',
        success: 'Agent 页面里至少能看到一个 Agent。',
        create: '创建 Agent',
        review: '打开 Agent',
      },
      routing: {
        title: '任务队列',
        emptyWithProject: '为当前项目创建任务队列。',
        emptyWithoutProject: '先选择项目，再创建任务队列。',
        why: '任务队列是新任务等待的地方，Agent 准备好后会从这里领取任务。',
        success: '当前项目下已经有一个任务队列。',
        create: '创建任务队列',
        review: '查看任务队列',
      },
      task: {
        title: '第一个任务',
        emptyWithRouting: '写一个小任务。Forge 会把它放进队列，等可用的 Agent 领取。',
        emptyWithoutRouting: '先创建任务队列，再创建第一个任务。',
        ready: '看板上已有 {{count}} 个任务。',
        why: '先用一个小任务验证流程，避免一开始就把真实工作卡住。',
        success: '看板上能看到任务，状态是等待领取或已分配给 Agent。',
        create: '写第一个任务',
        open: '打开看板',
      },
      review: {
        title: '验收输出',
        empty: '已分配任务的输出会出现在详情面板。',
        inFlight: '已有任务被分配，可从看板查看进度。',
        ready: '{{count}} 个已完成任务等待验收。',
        why: '验收结果能确认 Agent 返回了可采用的输出。',
        success: '任务已经完成，并且能看到输出或结果文件。',
        open: '查看工作',
      },
      reuse: {
        title: '复用有效做法',
        empty: '任务完成后，查看哪些有用指令可以保存到下次使用。',
        ready: '已有保存好的指令，可用于后续任务。',
        why: '保存有效指令后，Agent 处理相似任务时不用你重新说明。',
        success: '已经保存有用指令，或有任务用过这些指令。',
        review: '查看要保存的内容',
        open: '查看保存的指令',
      },
    },
  },

  // =========================================================================
  // 认证
  // =========================================================================
  auth: {
    login: '登录',
    logout: '退出登录',
    register: '注册',
    forgotPassword: '忘记密码？',
    resetPassword: '重置密码',
    changePassword: '修改密码',
    email: '邮箱',
    password: '密码',
    confirmPassword: '确认密码',
    username: '用户名',
    rememberMe: '记住我',
    loginSuccess: '你已登录。',
    logoutSuccess: '你已退出登录。',
    registerSuccess: '账户已准备好，现在可以登录。',
    invalidCredentials: '请检查邮箱和密码，然后重试。',
    accountLocked: '这个账户暂时被锁定。请等几分钟后重试，或让所有者/管理员帮忙。',
    agentExpired: '登录已过期。请重新登录后继续。',
    passwordResetSent: '请查看邮箱里的密码重置链接。',
    passwordChanged: '密码已更新。下次登录时请使用新密码。',
    passwordTooShort: '密码请至少输入 {{min}} 个字符。',
    passwordMismatch: '请在两个密码框中输入相同的密码。',
    emailInvalid: '请输入有效的邮箱地址。',
    emailInUse: '这个邮箱已被使用。请直接登录，或重置密码。',
    usernameInUse: '这个用户名已被使用。请换一个用户名。',
    emailDomainRestricted: '请使用已批准的工作邮箱，或让所有者邀请你加入。',
    passwordRequirements: '12+ 字符、大小写字母、数字、特殊字符',
    passwordWeak: '弱',
    passwordFair: '一般',
    passwordGood: '良好',
    passwordStrong: '强',
    createAccount: '创建账户',
    fillAllFields: '请填完所有字段，然后重试。',
    fillRequiredFields: '请填完必填字段，然后重试。',
    networkError: '请检查网络，然后重新登录。Forge 暂时无法连接登录。',
  },

  // =========================================================================
  // Agent
  // =========================================================================
  agents: {
    title: 'Agent',
    newAgent: '创建 Agent',
    createAgent: '创建 Agent',
    editAgent: '编辑 Agent',
    deleteAgent: '删除 Agent',
    noAgents: '还没有 Agent。先创建一个 Agent，再开始分配任务。',
    agentName: 'Agent 名称',
    projectPath: '项目文件夹位置',
    workingDirectory: '工作目录',
    startAgent: '启动 Agent',
    stopAgent: '停止 Agent',
    restartAgent: '重启 Agent',
    duplicateAgent: '复制 Agent',
    exportAgent: '导出 Agent',
    importAgent: '导入 Agent',
    agentDetails: 'Agent 详情',
    agentSettings: 'Agent 设置',
    agentHistory: 'Agent 历史',
    activeAgent: '活跃 Agent',
    lastActive: '{{time}}活跃',
    createdAt: '创建于 {{time}}',
    status: {
      idle: '可接收任务',
      working: '工作中',
      waiting: '需要输入',
      offline: '未连接',
      starting: '正在启动工作...',
      stopping: '正在停止工作...',
      error: '检查 Agent 状态',
      connecting: '连接中...',
    },
    confirmDelete: '要删除这个 Agent 吗？这会移除它的设置，并停止给它分配新任务。',
    confirmStop: '要停止这个 Agent 吗？当前工作会暂停，直到你重新启动它。',
    // 创建 Agent 弹窗
    startNewAgent: '开始新 Agent',
    pickProject: '选择一个项目开始',
    tellClaude: '告诉 Claude 你要做什么',
    searchProjects: '搜索项目或输入文件夹位置...',
    enterFolderPath: '输入项目文件夹位置...',
    moreOptions: '更多选项',
    behavior: '行为设置',
    autoApprove: '自动批准操作',
    resumeLast: '继续上次 Agent',
    enableBrowser: '启用浏览器',
    start: '开始',
    nAgents: '{{count}} 个 Agent',
    agentStarted: 'Agent 已启动',
    agentStopped: 'Agent 已停止',
    agentDeleted: 'Agent 已删除',
    agentCreated: 'Agent 已创建',
    maxAgentsReached: 'Agent 数量已达上限。请先停止或删除不用的 Agent，然后重试。',
    invalidProjectPath: '请输入项目文件夹位置，然后重试。',
  },

  // =========================================================================
  // 任务队列
  // =========================================================================
  groups: {
    title: '任务队列',
    newGroup: '新建任务队列',
    createGroup: '创建任务队列',
    editGroup: '编辑任务队列',
    deleteGroup: '删除任务队列',
    noGroups: '暂无任务队列。先创建一个，让新任务有地方等待 Agent 接手。',
    groupName: '任务队列名称',
    groupColor: '任务队列颜色',
    addToGroup: '添加到任务队列',
    removeFromGroup: '从任务队列移除',
    moveToGroup: '移动到任务队列',
    ungrouped: '暂无任务队列',
    confirmDelete: '要删除这个任务队列吗？Agent 仍会保留，但任务需要选择其他任务队列后才能发送。',
    groupCreated: '任务队列已创建',
    groupDeleted: '任务队列已删除',
    groupUpdated: '任务队列已更新',
  },

  // =========================================================================
  // 活动流
  // =========================================================================
  feed: {
    title: '活动',
    noActivity: '暂无活动。先启动一个任务，后续更新会显示在这里。',
    clearActivity: '清除活动',
    filterByType: '按类型筛选',
    filterByAgent: '按 Agent 筛选',
    showAll: '显示全部',
    eventTypes: {
      tool_use: 'Agent 使用了工具',
      tool_result: '工具已完成',
      text: 'Agent 消息',
      error: '检查这条更新',
      thinking: '正在规划下一步',
      system: '系统更新',
    },
    tools: {
      Read: '打开文件',
      Write: '创建文件',
      Edit: '修改文件',
      Bash: '运行命令',
      Glob: '查找文件',
      Grep: '搜索文件内容',
      WebFetch: '打开网页',
      WebSearch: '搜索网页',
      Task: '请另一个 Agent 协助',
    },
    expandAll: '全部展开',
    collapseAll: '全部折叠',
    copyContent: '复制内容',
    viewDetails: '查看详情',
    timestamp: '{{time}}',
  },

  // =========================================================================
  // 提示输入
  // =========================================================================
  prompt: {
    placeholder: '输入一条给 Agent 的指令...',
    placeholderShort: '输入一条指令...',
    send: '发送',
    sending: '发送中...',
    cancel: '取消',
    clear: '清除',
    history: '历史记录',
    suggestions: '建议',
    attachFile: '附加文件',
    voiceInput: '语音输入',
    recording: '录音中...',
    processing: '处理中...',
    characterCount: '{{count}} / {{max}} 字符',
    characterLimitWarning: '接近字符限制',
    emptyPrompt: '请先输入一条指令。',
    selectAgent: '请先选择一个 Agent',
    noAgentSelected: '请先选择一个 Agent，再发送任务。',
    multipleAgentsSelected: '已选择 {{count}} 个 Agent',
    shortcuts: {
      send: '按 Enter 发送',
      newLine: '按 Shift+Enter 换行',
      history: '按 ↑ 调出历史记录',
    },
  },

  // =========================================================================
  // 视觉地图
  // =========================================================================
  workshop: {
    title: '视觉地图',
    loading: '加载视觉地图...',
    loadError: '视觉地图无法加载。等 Agent 可用后刷新，再试一次。',
    controls: {
      zoom: '滚动缩放',
      pan: '中键平移',
      rotate: '右键旋转',
      select: '点击选择',
    },
    shortcuts: {
      numbers: '按 1-9 选择 Agent',
      escape: '按 Esc 取消选择',
      help: '按 ? 获取帮助',
      fullscreen: '按 F 全屏',
      drawMode: '按 D 添加绘图备注',
    },
    performance: {
      fps: '{{value}} FPS',
      memory: '{{value}} MB',
      renderTime: '{{value}} 毫秒',
    },
  },

  // =========================================================================
  // 设置
  // =========================================================================
  settings: {
    title: '设置',
    general: '通用',
    appearance: '外观',
    notifications: '通知',
    keyboard: '键盘快捷键',
    advanced: '高级',
    account: '账户',
    security: '安全',
    integrations: '集成',
    language: '语言',
    theme: '主题',
    themes: {
      light: '浅色',
      dark: '深色',
      system: '跟随系统',
    },
    fontSize: '字体大小',
    autoSave: '自动保存',
    autoSaveInterval: '自动保存间隔',
    soundEffects: '音效',
    enableNotifications: '启用通知',
    desktopNotifications: '桌面通知',
    emailNotifications: '邮件通知',
    saved: '设置已保存',
    reset: '恢复默认',
    resetConfirm: '要恢复所有设置吗？这会使用默认值替换当前选择。',
    runtime: {
      title: 'Agent 工作设置',
      description: '选择实际操作型 Agent 在哪里工作，并在分配任务前检查工具和登录状态。',
      saving: '保存中...',
      loading: '加载工作设置...',
      couldNotLoad:
        '请刷新这个设置页来加载 Agent 工作设置。如果仍然无法加载，请找 owner 或 admin 检查 Agent 工作设置。',
      defaultRuntimeLabel: '默认 Agent 运行位置',
      defaultRuntimeDescription:
        '处理共享项目文件时，选择“项目文件”最简单。只有要把这台电脑接入为 Agent 时，才选择这台电脑。',
      defaultContainerCliLabel: '项目工作默认工具',
      defaultContainerCliDescription:
        'Agent 编辑文件或运行命令时使用的 Claude Code、Codex、Gemini 或 OpenCode',
      availableRuntimesLabel: '可用的 Agent 运行位置',
      availableRuntimesDescription: '当前安装可以在哪里运行实际操作型 Agent',
      availableContainerClisLabel: 'Agent 可使用的工作工具',
      availableContainerClisDescription: '用于编辑文件、运行命令和实时工作的已安装工具',
      runtimeLabels: {
        cli: '这台电脑',
        api: '只处理文字的模型服务',
        container: '项目文件',
      },
      cliToolLabels: {
        claude: 'Claude Code',
        opencode: 'OpenCode',
        codex: 'Codex',
        gemini: 'Gemini',
      },
    },
  },

  // =========================================================================
  // 技能
  // =========================================================================
  skills: {
    detail: {
      closeAria: '关闭',
      close: '完成',
      subtitle: 'agent 在处理任务时可以复用的说明。',
      statusReady: '可以使用',
      statusNeedsInstall: '需要先安装，agent 才能使用',
      cliFit: '最适合 {{tool}}',
      unknownToolFit: '使用前先检查这个工作工具',
      allAgentsFit: '适用于任意 agent',
      allAgentsTooltip: '不需要指定工作工具。',
      containerCliTooltip: '工作工具：{{tool}}',
      unknownToolTooltip: '打开设置检查工作工具，再使用这条保存的说明。',
      nextStepHeading: '下一步做什么',
      nextStepReady: '创建任务时可以使用这条保存的说明，也可以让匹配词在类似任务中提示它。',
      nextStepNeedsInstall: '先请所有者或管理员安装它，再期望 Agent 在任务中使用它。',
      sourceLabel: '来源',
      authorLabel: '维护者',
      availabilityLabel: '可用范围',
      descriptionHeading: '它能帮什么',
      noDescription: '使用这条保存的说明前，请先查看下面的可复用说明。',
      triggerHeading: '什么时候有帮助',
      triggerHelper: '当任务里出现类似这些词时，Agent 就知道这条保存的说明可能有帮助。',
      detailsHeading: '可复用说明',
      detailsHelper: '查看这段文字，了解这条保存的说明会给 Agent 工作补充什么。',
      noContent: '还没有保存可复用说明。请先补充说明，再让 Agent 使用这条保存的说明。',
      unknownAuthor: '刷新保存的说明以加载维护者',
      unknownSource: '保存的说明库',
      availabilityWorkspace: '当前团队空间',
      availabilityGlobal: '保存的说明库',
      availabilityProject: '当前项目',
      availabilityLatest: '最新保存版本',
      availabilityNeedsReview: '检查保存说明的可用范围',
    },
  },

  // =========================================================================
  // 错误
  // =========================================================================
  errors: {
    generic: '请重试；如果反复发生，请让管理员检查系统。',
    network: '请检查网络，然后重试。Forge 暂时无法连接。',
    timeout: '请稍等片刻后重试。请求时间太长。',
    notFound: '请刷新页面后重试。未找到 {{resource}}。',
    unauthorized: '请重新登录，然后再试一次。',
    forbidden: '你当前无法执行这个操作。请让所有者或管理员检查你的团队空间访问权限。',
    validation: '请检查高亮字段，然后重试。',
    serverError: '请稍等片刻后重试。Forge 暂时无法完成这个操作。',
    connectionLost: '连接断开，正在重连...',
    reconnecting: '重新连接中...',
    reconnected: '连接已恢复',
    agentError: '请重试这一步；如果反复出现，请检查 Agent 状态。Agent 没有完成这一步。',
    fileError: '请检查文件后重试。Forge 暂时无法处理这个文件。',
    uploadError: '请检查文件和网络后重新上传。上传没有完成。',
    downloadError: '请刷新页面后重新下载。下载没有开始。',
    rateLimited: '请等待 {{seconds}} 秒后重试。请求过于频繁。',
    quotaExceeded: '请让所有者提高额度，或释放一些容量。{{resource}} 配额已用完。',
    agent: {
      lifecycle: {
        restart_host_cli: {
          title: '请在这台电脑上重启连接助手',
          detail: 'Forge 不能替你重启它。请在那台电脑上重新运行设置命令。',
        },
        restart_api: {
          title: '请重新发送消息，而不是重启工作区',
          detail: '该 Agent 通过 AI 服务回复消息。请重新发送消息来重新尝试。',
        },
        start_host_cli: {
          title: '请在这台电脑上启动连接助手',
          detail: '请在那台电脑上重新运行设置命令，让 Agent 上线。',
        },
        start_api: {
          title: '请发送消息来启动这个聊天 Agent',
          detail: '聊天 Agent 会在你发送消息时开始工作，没有需要启动的命令窗口。',
        },
        stop_host_cli: {
          title: '请在这台电脑上停止连接助手',
          detail: 'Forge 不能替你停止它。请关闭那台电脑上的 Terminal 或 PowerShell 窗口。',
        },
        stop_api: {
          title: '请关闭聊天或等待回复结束',
          detail: '聊天 Agent 没有需要停止的命令窗口。需要继续时再发送新消息。',
        },
        not_permitted: {
          title: '你不能管理这个 Agent',
          detail: '你只能管理你拥有的 Agent。如需访问请联系 Agent 所有者。',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: '请选择一个工作工具',
          detail:
            '会编辑项目文件的 Agent 需要一个工作工具：Claude Code、Codex、Gemini 或 OpenCode。',
        },
        api_cannot_have_cli_tool: {
          title: '只处理文字的模型 Agent 不能有工作工具',
          detail: '请移除工作工具，或将工作类型改为“项目文件”。',
        },
        missing_cli_tool_for_host_cli: {
          title: '请选择一个工作工具',
          detail:
            '从这台电脑加入的 Agent 需要一个工作工具：Claude Code、Codex、Gemini 或 OpenCode。',
        },
      },
      enroll: {
        missing_idempotency_key: {
          title: '需要重新运行设置命令',
          detail:
            '请在这台电脑上重新运行设置命令。如果反复出现，请让管理员检查这台电脑的 Agent 工作设置。',
        },
        plaintext_nats_blocked: {
          title: '这台电脑的连接需要安全通道',
          detail:
            '请使用“从这台电脑接入 Agent”的安全连接地址。如果不确定该填什么，请让管理员检查这台电脑的 Agent 连接设置。',
        },
      },
    },
  },

  // =========================================================================
  // 确认
  // =========================================================================
  confirm: {
    delete: '要删除这一项吗？它会从当前团队空间移除。',
    unsavedChanges: '不保存就离开吗？未保存的更改会丢失。',
    logout: '现在退出登录吗？打开表单里的未保存内容可能会丢失。',
    reset: '要重置吗？当前更改会被默认值替换。',
    stop: '要停止此操作吗？当前进度可能会暂停，需要重新开始。',
    discard: '要放弃更改吗？你的编辑会丢失。',
  },

  // =========================================================================
  // 时间
  // =========================================================================
  time: {
    now: '刚刚',
    seconds: '{{count}} 秒前',
    minutes: '{{count}} 分钟前',
    hours: '{{count}} 小时前',
    days: '{{count}} 天前',
    weeks: '{{count}} 周前',
    months: '{{count}} 个月前',
    years: '{{count}} 年前',
  },

  // =========================================================================
  // 文件操作
  // =========================================================================
  files: {
    upload: '上传文件',
    download: '下载文件',
    delete: '删除文件',
    rename: '重命名文件',
    move: '移动文件',
    copy: '复制文件',
    size: '大小：{{size}}',
    type: '类型：{{type}}',
    modified: '修改时间：{{date}}',
    created: '创建时间：{{date}}',
    dropzone: '拖放文件到此处或点击上传',
    maxSize: '最大文件大小：{{size}}',
    allowedTypes: '允许的类型：{{types}}',
    uploading: '上传中...',
    uploaded: '文件上传成功',
    uploadFailed: '请检查文件后重新上传。上传没有完成。',
    tooLarge: '请选择小于 {{size}} 的文件，然后重新上传。',
    invalidType: '请选择这些类型之一的文件：{{types}}。',
  },

  // =========================================================================
  // 键盘快捷键
  // =========================================================================
  shortcuts: {
    title: '键盘快捷键',
    general: '通用',
    navigation: '导航',
    editing: '编辑',
    agents: 'Agent',
    keys: {
      enter: 'Enter',
      escape: 'Esc',
      tab: 'Tab',
      shift: 'Shift',
      ctrl: 'Ctrl',
      alt: 'Alt',
      cmd: 'Cmd',
      space: '空格',
      up: '↑',
      down: '↓',
      left: '←',
      right: '→',
    },
  },

  // =========================================================================
  // 无障碍
  // =========================================================================
  a11y: {
    skipToContent: '跳转到内容',
    openMenu: '打开菜单',
    closeMenu: '关闭菜单',
    expandSection: '展开部分',
    collapseSection: '折叠部分',
    loading: '加载中，请稍候',
    error: '请查看提示信息，然后重试。',
    success: '操作成功',
    required: '此字段为必填',
    invalid: '请检查此字段，然后重试',
  },

  // =========================================================================
  // 工具提示
  // =========================================================================
  tooltips: {
    copy: '复制到剪贴板',
    edit: '编辑',
    delete: '删除',
    expand: '展开',
    collapse: '折叠',
    refresh: '刷新',
    settings: '打开设置',
    help: '获取帮助',
    close: '关闭',
    maximize: '最大化',
    minimize: '最小化',
  },

  // =========================================================================
  // 管理面板
  // =========================================================================
  admin: {
    title: '管理面板',
    tabs: {
      agents: 'Agent',
      metrics: '指标',
      users: '用户',
      health: '健康状态',
    },
    agents: {
      title: 'Agent 管理',
      search: '搜索 Agent...',
      status: '状态',
      actions: '操作',
      noAgents: '当前视图没有匹配的 Agent。可以清除搜索，或先创建 Agent。',
      pause: '暂停',
      resume: '恢复',
      stop: '停止',
      delete: '删除',
    },
    metrics: {
      title: '系统指标',
      activeAgents: '活跃 Agent',
      totalEvents: '总事件数',
      eventsPerMinute: '事件/分钟',
      memoryUsage: '内存使用',
      cpuUsage: 'CPU 使用率',
      uptime: '运行时间',
      wsConnections: '实时浏览器连接',
      requestsPerMinute: '请求/分钟',
    },
    users: {
      title: '用户管理',
      search: '搜索用户...',
      addUser: '添加用户',
      editUser: '编辑用户',
      deleteUser: '删除用户',
      noUsers: '当前视图没有匹配的用户。可以清除搜索，或先邀请用户。',
      role: '访问级别',
      roles: {
        admin: '管理员',
        operator: '成员',
        viewer: '查看者',
      },
      status: {
        active: '活跃',
        inactive: '未激活',
        suspended: '已暂停',
      },
      confirmDelete: '要删除这个用户吗？该用户将失去当前团队空间访问权限。',
    },
    health: {
      title: '系统健康',
      overall: '整体状态',
      services: '服务',
      alerts: '警报',
      status: {
        healthy: '健康',
        degraded: '降级',
        down: '离线',
      },
      noAlerts: '无活跃警报',
      acknowledge: '确认',
      latency: '延迟',
      lastCheck: '上次检查',
    },
  },
} as const
