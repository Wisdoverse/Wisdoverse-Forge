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
    error: '发生错误',
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
    start: '开始',
    dashboard: '仪表板',
    tasks: '任务',
    inbox: '收件箱',
    context: '上下文',
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
    title: '先按一条安全路径跑通',
    description: '一次只做一步。先完成创建 Agent、发送任务、验收结果这条最小路径。',
    skip: '跳过引导',
    progressCount: '{{complete}} / {{total}}',
    nextTitle: '下一步先做这个',
    readyTitle: '已经可以开始工作',
    readyDetail: '基础路径已经跑通。现在可以继续创建任务，或整理保存好的指令。',
    successLabel: '成功的样子：',
    currentProject: '当前项目',
    noProject: '请先在侧边栏选择一个项目。',
    projects: '项目',
    workLocations: {
      managed: '托管工作区',
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
        title: '工作区',
        empty: '需要路由时再创建团队和项目。',
        why: '项目能让任务有明确归属，避免不知道任务该给谁处理。',
        success: '已经有团队和项目，并且当前项目已被选中。',
        create: '创建工作区',
        review: '查看工作区',
      },
      runtime: {
        title: '工作位置',
        empty: '先选择 Agent 在哪里工作：托管工作区，或这台电脑。',
        ready: '{{location}} 已经可以接收 Agent 工作。',
        why: 'Agent 需要一个安全的工作位置，才能接收任务。',
        success: '已经有可用的托管工作区，或这台电脑已经可用。',
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
        empty: '先创建一个简单 Agent：文字 Agent、托管工作区 Agent，或这台电脑上的 Agent。',
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
        emptyWithRouting: '创建任务、分配 Agent 并观察运行启动。',
        emptyWithoutRouting: '先创建任务队列，再创建第一个任务。',
        ready: '看板上已有 {{count}} 个任务。',
        why: '先用一个小任务验证流程，避免一开始就把真实工作卡住。',
        success: '看板上出现任务，并且任务已分配或正在等待开始。',
        create: '创建任务',
        open: '打开看板',
      },
      review: {
        title: '验收输出',
        empty: '已分配任务的输出会出现在详情面板。',
        inFlight: '已有任务被分配，可从看板查看进度。',
        ready: '{{count}} 个已完成任务等待验收。',
        why: '验收结果能确认 Agent 确实返回了有用输出和证据。',
        success: '任务已经完成，并且能看到输出或证据。',
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
    passwordTooShort: '密码至少需要 {{min}} 个字符',
    passwordMismatch: '两次输入的密码不一致',
    emailInvalid: '请输入有效的邮箱地址',
    emailInUse: '该邮箱已被使用',
    usernameInUse: '该用户名已被使用',
    emailDomainRestricted: '仅允许使用授权邮箱域名注册',
    passwordRequirements: '12+ 字符、大小写字母、数字、特殊字符',
    passwordWeak: '弱',
    passwordFair: '一般',
    passwordGood: '良好',
    passwordStrong: '强',
    createAccount: '创建账户',
    fillAllFields: '请填完所有字段，然后重试。',
    fillRequiredFields: '请填完必填字段，然后重试。',
    networkError: 'Forge 登录时暂时连不上。请检查网络后重试。',
  },

  // =========================================================================
  // Agent
  // =========================================================================
  agents: {
    title: 'Agent',
    newAgent: '新建 Agent',
    createAgent: '创建 Agent',
    editAgent: '编辑 Agent',
    deleteAgent: '删除 Agent',
    noAgents: '还没有 Agent。先创建一个 Agent，再开始分配任务。',
    agentName: 'Agent 名称',
    projectPath: '项目路径',
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
      idle: '空闲',
      working: '工作中',
      waiting: '等待输入',
      offline: '离线',
      starting: '启动中...',
      stopping: '停止中...',
      error: '错误',
      connecting: '连接中...',
    },
    confirmDelete: '确定要删除此 Agent 吗？',
    confirmStop: '确定要停止此 Agent 吗？',
    // 新建 Agent 弹窗
    startNewAgent: '开始新 Agent',
    pickProject: '选择一个项目开始',
    tellClaude: '告诉 Claude 你要做什么',
    searchProjects: '搜索项目或输入文件夹路径...',
    enterFolderPath: '输入项目文件夹路径...',
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
    maxAgentsReached: '已达到最大 Agent 数量',
    invalidProjectPath: '无效的项目路径',
  },

  // =========================================================================
  // 分组
  // =========================================================================
  groups: {
    title: '分组',
    newGroup: '新建分组',
    createGroup: '创建分组',
    editGroup: '编辑分组',
    deleteGroup: '删除分组',
    noGroups: '暂无分组',
    groupName: '分组名称',
    groupColor: '分组颜色',
    addToGroup: '添加到分组',
    removeFromGroup: '从分组中移除',
    moveToGroup: '移动到分组',
    ungrouped: '未分组',
    confirmDelete: '确定要删除此分组吗？',
    groupCreated: '分组已创建',
    groupDeleted: '分组已删除',
    groupUpdated: '分组已更新',
  },

  // =========================================================================
  // 活动流
  // =========================================================================
  feed: {
    title: '活动',
    noActivity: '暂无活动',
    clearActivity: '清除活动',
    filterByType: '按类型筛选',
    filterByAgent: '按 Agent 筛选',
    showAll: '显示全部',
    eventTypes: {
      tool_use: '工具调用',
      tool_result: '工具结果',
      text: '文本',
      error: '错误',
      thinking: '思考中',
      system: '系统',
    },
    tools: {
      Read: '读取文件',
      Write: '写入文件',
      Edit: '编辑文件',
      Bash: '运行命令',
      Glob: '查找文件',
      Grep: '搜索内容',
      WebFetch: '获取网页',
      WebSearch: '网络搜索',
      Task: '子任务',
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
    resetConfirm: '确定要恢复所有设置吗？',
    runtime: {
      title: 'Agent 工作设置',
      description: '选择实际操作型 Agent 在哪里工作，并在分配任务前检查工具和登录状态。',
      saving: '保存中...',
      loading: '加载工作设置...',
      couldNotLoad: '无法加载工作设置',
      defaultRuntimeLabel: '默认 Agent 运行位置',
      defaultRuntimeDescription:
        '托管工作区最简单。只有要把这台电脑接入为可管理的本地 Agent 时，才选择这台电脑。',
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
        container: '托管工作区',
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
      allAgentsFit: '适用于任意 agent',
      allAgentsTooltip: '不需要指定工作工具。',
      containerCliTooltip: '工作工具：{{tool}}',
      sourceLabel: '来源',
      authorLabel: '维护者',
      availabilityLabel: '可用范围',
      descriptionHeading: '它能帮什么',
      noDescription: '还没有简介。使用这条保存的说明前，请先查看下面的说明。',
      triggerHeading: '什么时候有帮助',
      triggerHelper: '当任务里出现类似这些词时，Agent 就知道这条保存的说明可能有帮助。',
      detailsHeading: '可复用说明',
      detailsHelper: '查看这段文字，了解这条保存的说明会给 Agent 工作补充什么。',
      noContent: '还没有保存可复用说明。请先补充说明，再让 Agent 使用这条保存的说明。',
      unknownAuthor: '暂未列出维护者',
      unknownSource: '保存的说明库',
      availabilityWorkspace: '当前工作区',
      availabilityGlobal: '保存的说明库',
      availabilityProject: '当前项目',
      availabilityLatest: '最新保存版本',
    },
  },

  // =========================================================================
  // 错误
  // =========================================================================
  errors: {
    generic: '出现了问题。请重试；如果反复发生，请让管理员检查系统。',
    network: 'Forge 暂时连不上。请检查网络后重试。',
    timeout: '请求超时，请重试',
    notFound: '未找到 {{resource}}。请刷新页面后重试。',
    unauthorized: '请重新登录，然后再试一次。',
    forbidden: '你当前没有权限执行这个操作。请让所有者或管理员更新你的角色。',
    validation: '请检查高亮字段，然后重试。',
    serverError: 'Forge 暂时无法完成这个操作。请稍等片刻后重试。',
    connectionLost: '连接断开，正在重连...',
    reconnecting: '重新连接中...',
    reconnected: '连接已恢复',
    agentError: 'Agent 没有完成这一步。请重试；如果反复出现，请检查 Agent 状态。',
    fileError: '文件无法处理。请检查文件后重试。',
    uploadError: '上传没有完成。请检查文件和网络后重试。',
    downloadError: '下载没有开始。请刷新页面后重试。',
    rateLimited: '请求过于频繁，请等待 {{seconds}} 秒',
    quotaExceeded: '{{resource}} 配额已用完。请让所有者提高额度，或释放一些容量。',
    agent: {
      lifecycle: {
        restart_host_cli: {
          title: '请在这台电脑上重启连接助手',
          detail: 'Forge 不能替你重启它。请在那台电脑上重新运行设置命令。',
        },
        restart_api: {
          title: '没有可重启的工作区',
          detail: '该 Agent 通过 AI 服务回复消息。请重新发送一条消息再试一次。',
        },
        start_host_cli: {
          title: '请在这台电脑上启动连接助手',
          detail: '请在那台电脑上重新运行设置命令，让 Agent 上线。',
        },
        start_api: {
          title: '没有可启动的工作区',
          detail: '只处理文字的模型 Agent 没有可启动的命令窗口。',
        },
        stop_host_cli: {
          title: '请在这台电脑上停止连接助手',
          detail: 'Forge 不能替你停止它。请关闭那台电脑上的 Terminal 或 PowerShell 窗口。',
        },
        stop_api: {
          title: '没有可停止的工作区',
          detail: '只处理文字的模型 Agent 没有可停止的命令窗口。',
        },
        not_permitted: {
          title: '无权操作该 Agent',
          detail: '你只能管理你拥有的 Agent。如需访问请联系 Agent 所有者。',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: '请选择一个工作工具',
          detail: '托管工作区 Agent 需要一个工作工具：claude、codex、gemini 或 opencode。',
        },
        api_cannot_have_cli_tool: {
          title: '只处理文字的模型 Agent 不能有工作工具',
          detail: '请移除工作工具，或将工作类型改为“托管工作区”。',
        },
        missing_cli_tool_for_host_cli: {
          title: '请选择一个工作工具',
          detail: '从这台电脑加入的 Agent 需要一个工作工具：claude、codex、gemini 或 opencode。',
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
    delete: '确定要删除吗？',
    unsavedChanges: '您有未保存的更改，确定要离开吗？',
    logout: '确定要退出登录吗？',
    reset: '确定要重置吗？此操作无法撤销。',
    stop: '确定要停止此操作吗？',
    discard: '确定要放弃更改吗？',
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
    uploadFailed: '文件上传失败',
    tooLarge: '文件过大，最大允许 {{size}}',
    invalidType: '无效的文件类型，允许的类型：{{types}}',
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
    error: '发生错误',
    success: '操作成功',
    required: '此字段为必填',
    invalid: '此字段无效',
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
      role: '角色',
      roles: {
        admin: '管理员',
        operator: '操作员',
        viewer: '查看者',
      },
      status: {
        active: '活跃',
        inactive: '未激活',
        suspended: '已暂停',
      },
      confirmDelete: '确定要删除此用户吗？',
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
