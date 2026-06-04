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
    agents: '会话',
    skills: '技能',
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
    description: '一次只做一步。先完成创建会话、发送任务、验收结果这条最小路径。',
    progressCount: '{{complete}} / {{total}}',
    nextTitle: '下一步先做这个',
    readyTitle: '已经可以开始工作',
    readyDetail: '基础路径已经跑通。现在可以继续创建任务，或整理可复用技能。',
    successLabel: '成功的样子：',
    currentProject: '当前项目',
    noProject: '未选择项目',
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
        empty: '先选择会话在哪里工作：托管工作区，或这台电脑。',
        ready: '{{location}} 已经可以接收会话工作。',
        why: '会话需要一个安全的工作位置，才能接收任务。',
        success: '已经有可用的托管工作区，或已接入本机电脑。',
        open: '选择工作位置',
        review: '查看工作位置',
      },
      provider: {
        title: '模型或本机接入',
        empty: '添加模型服务，或接入一个本机会话。',
        needsTest: '先测试模型服务，再分配工作。',
        cliReady: '{{name}} 可以从{{location}}执行工作。',
        why: '会话需要一个测试通过的模型服务，或一个已接入的本机会话，才能真正工作。',
        success: '模型服务测试通过，或本机会话已经接入。',
        create: '添加模型服务',
        connectCli: '接入本机会话',
        test: '测试模型服务',
        reviewProviders: '查看模型服务',
        reviewAgents: '查看会话',
      },
      agent: {
        title: '会话',
        empty: '创建一个文字会话、托管工作区会话，或本机会话。',
        why: '会话就是接收任务的工作者。先创建一个简单会话即可。',
        success: '会话页面里至少能看到一个会话。',
        create: '创建会话',
        review: '打开会话',
      },
      routing: {
        title: '任务路由',
        emptyWithProject: '为当前项目创建任务组。',
        emptyWithoutProject: '先选择项目，再创建任务组。',
        why: '任务组告诉平台新任务先放在哪里，以及哪些会话可以领取。',
        success: '当前项目下已经有一个任务组。',
        create: '创建任务组',
        review: '查看路由',
      },
      task: {
        title: '第一个任务',
        emptyWithRouting: '创建任务、分配会话并观察运行启动。',
        emptyWithoutRouting: '先完成路由，再创建第一个任务。',
        ready: '看板上已有 {{count}} 个任务。',
        why: '先用一个小任务验证流程，避免一开始就把真实工作卡住。',
        success: '看板上出现任务，并且任务已分配或进入队列。',
        create: '创建任务',
        open: '打开看板',
      },
      review: {
        title: '验收输出',
        empty: '已分配任务的输出会出现在详情面板。',
        inFlight: '已有任务被分配，可从看板查看进度。',
        ready: '{{count}} 个已完成任务等待验收。',
        why: '验收结果能确认会话确实返回了有用输出和证据。',
        success: '任务已经完成，并且能看到输出或证据。',
        open: '查看工作',
      },
      reuse: {
        title: '复用学习',
        empty: '完成任务后审批上下文或技能候选。',
        ready: '已有可复用技能或已应用的技能上下文。',
        why: '把经验沉淀成技能或上下文，下次相似任务会更稳定。',
        success: '已经有可复用技能，或任务应用过技能上下文。',
        review: '审批候选',
        open: '打开技能',
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
    loginSuccess: '登录成功',
    logoutSuccess: '已退出登录',
    registerSuccess: '账户创建成功',
    invalidCredentials: '邮箱或密码错误',
    accountLocked: '账户已锁定，请稍后重试',
    agentExpired: '会话已过期，请重新登录',
    passwordResetSent: '密码重置说明已发送到您的邮箱',
    passwordChanged: '密码修改成功',
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
    fillAllFields: '请填写所有字段',
    fillRequiredFields: '请填写所有必填字段',
    networkError: '浏览器暂时连不上服务器。请检查网络后重试。',
  },

  // =========================================================================
  // 会话
  // =========================================================================
  agents: {
    title: '会话',
    newAgent: '新建会话',
    createAgent: '创建会话',
    editAgent: '编辑会话',
    deleteAgent: '删除会话',
    noAgents: '还没有会话。先创建一个会话，再开始分配任务。',
    agentName: '会话名称',
    projectPath: '项目路径',
    workingDirectory: '工作目录',
    startAgent: '启动会话',
    stopAgent: '停止会话',
    restartAgent: '重启会话',
    duplicateAgent: '复制会话',
    exportAgent: '导出会话',
    importAgent: '导入会话',
    agentDetails: '会话详情',
    agentSettings: '会话设置',
    agentHistory: '会话历史',
    activeAgent: '活跃会话',
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
    confirmDelete: '确定要删除此会话吗？',
    confirmStop: '确定要停止此会话吗？',
    // 新建会话弹窗
    startNewAgent: '开始新会话',
    pickProject: '选择一个项目开始',
    tellClaude: '告诉 Claude 你要做什么',
    searchProjects: '搜索项目或输入文件夹路径...',
    enterFolderPath: '输入项目文件夹路径...',
    moreOptions: '更多选项',
    behavior: '行为设置',
    autoApprove: '自动批准操作',
    resumeLast: '继续上次会话',
    enableBrowser: '启用浏览器',
    start: '开始',
    nAgents: '{{count}} 个会话',
    agentStarted: '会话已启动',
    agentStopped: '会话已停止',
    agentDeleted: '会话已删除',
    agentCreated: '会话已创建',
    maxAgentsReached: '已达到最大会话数量',
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
    filterByAgent: '按会话筛选',
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
    placeholder: '在此输入提示...',
    placeholderShort: '输入提示...',
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
    emptyPrompt: '请输入提示',
    selectAgent: '请先选择一个会话',
    noAgentSelected: '未选择会话',
    multipleAgentsSelected: '已选择 {{count}} 个会话',
    shortcuts: {
      send: '按 Enter 发送',
      newLine: '按 Shift+Enter 换行',
      history: '按 ↑ 调出历史记录',
    },
  },

  // =========================================================================
  // 工作坊（3D 场景）
  // =========================================================================
  workshop: {
    title: '工作坊',
    loading: '加载工作坊...',
    loadError: '工作坊无法加载。等会话可用后刷新，再试一次。',
    controls: {
      zoom: '滚动缩放',
      pan: '中键平移',
      rotate: '右键旋转',
      select: '点击选择',
    },
    shortcuts: {
      numbers: '按 1-9 选择会话',
      escape: '按 Esc 取消选择',
      help: '按 ? 获取帮助',
      fullscreen: '按 F 全屏',
      drawMode: '按 D 绘图模式',
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
      title: '会话工作设置',
      description: '选择会话在哪里工作，并检查还需要补哪些设置。',
      saving: '保存中...',
      loading: '加载工作设置...',
      couldNotLoad: '无法加载工作设置',
      defaultRuntimeLabel: '默认工作位置',
      defaultRuntimeDescription: '新会话默认在哪里工作',
      defaultContainerCliLabel: '默认本地工作工具',
      defaultContainerCliDescription: '会话需要本地工作工具时使用的工具',
      availableRuntimesLabel: '可用工作位置',
      availableRuntimesDescription: '当前安装可用于会话工作的地方',
      availableContainerClisLabel: '可用本地工具',
      availableContainerClisDescription: '当前安装可用于会话工作的工具',
      runtimeLabels: {
        cli: '这台电脑',
        api: '只处理文字的模型服务',
        container: '托管工作区',
      },
      cliToolLabels: {
        claude: 'Claude Code',
        opencode: 'OpenCode',
        codex: 'Codex',
        gemini: 'Gemini CLI',
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
      versionLabel: '版本',
      descriptionHeading: '它能帮什么',
      noDescription: '还没有简介。使用这个技能前，请先查看下面的说明。',
      triggerHeading: '什么时候建议 agent 使用',
      triggerHelper: '当任务和这个短语匹配时，agent 可以考虑使用这个技能。',
      detailsHeading: 'agent 会读取的说明',
      detailsHelper: '如果要确认复用的具体内容，请查看这段文字。',
      noContent: '还没有保存可复用说明。请先补充说明，再让会话使用这个技能。',
      unknownAuthor: '未知',
      unknownSource: '技能库',
      versionLatest: '最新',
    },
  },

  // =========================================================================
  // 错误
  // =========================================================================
  errors: {
    generic: '出现了问题。请重试；如果反复发生，请让管理员检查系统。',
    network: '浏览器暂时连不上服务器。请检查网络后重试。',
    timeout: '请求超时，请重试',
    notFound: '未找到 {{resource}}。请刷新页面后重试。',
    unauthorized: '请重新登录，然后再试一次。',
    forbidden: '你当前没有权限执行这个操作。请让所有者或管理员更新你的角色。',
    validation: '请检查高亮字段，然后重试。',
    serverError: '服务器处理时出现问题。请稍等片刻后重试。',
    connectionLost: '连接断开，正在重连...',
    reconnecting: '重新连接中...',
    reconnected: '连接已恢复',
    agentError: '会话没有完成这一步。请重试；如果反复出现，请检查会话状态。',
    fileError: '文件无法处理。请检查文件后重试。',
    uploadError: '上传没有完成。请检查文件和网络后重试。',
    downloadError: '下载没有开始。请刷新页面后重试。',
    rateLimited: '请求过于频繁，请等待 {{seconds}} 秒',
    quotaExceeded: '{{resource}} 配额已用完。请让所有者提高额度，或释放一些容量。',
    agent: {
      lifecycle: {
        restart_host_cli: {
          title: '请在这台电脑上重启连接工具',
          detail: '平台不能替你重启这个本地进程。请在那台电脑上重新运行加入命令。',
        },
        restart_api: {
          title: '没有可重启的工作区',
          detail: '该 Agent 使用只处理文字的模型服务。再发送一次 prompt 即可使用模型。',
        },
        start_host_cli: {
          title: '请在这台电脑上启动连接工具',
          detail: '请在那台电脑上重新运行加入命令，让 Agent 上线。',
        },
        start_api: {
          title: '没有可启动的工作区',
          detail: '只处理文字的模型 Agent 没有可启动的终端。',
        },
        stop_host_cli: {
          title: '请在这台电脑上停止连接工具',
          detail: '平台不能替你停止这个本地进程。请在那台电脑上停止它。',
        },
        stop_api: {
          title: '没有可停止的工作区',
          detail: '只处理文字的模型 Agent 没有可停止的终端。',
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
          title: '缺少 Idempotency-Key 请求头',
          detail: '请在 `Idempotency-Key` 请求头中带上一个新的 UUID 后重试。',
        },
        plaintext_nats_blocked: {
          title: '这台电脑的连接需要安全通道',
          detail:
            '请将 `NATS_AGENT_URL` 设为 `tls://`，或先设置组织策略 `allow_plaintext_host_nats=true` 后再试。',
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
    agents: '会话',
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
      agents: '会话',
      metrics: '指标',
      users: '用户',
      health: '健康状态',
    },
    agents: {
      title: '会话管理',
      search: '搜索会话...',
      status: '状态',
      actions: '操作',
      noAgents: '当前视图没有匹配的会话。可以清除搜索，或先创建会话。',
      pause: '暂停',
      resume: '恢复',
      stop: '停止',
      delete: '删除',
    },
    metrics: {
      title: '系统指标',
      activeAgents: '活跃会话',
      totalEvents: '总事件数',
      eventsPerMinute: '事件/分钟',
      memoryUsage: '内存使用',
      cpuUsage: 'CPU 使用率',
      uptime: '运行时间',
      wsConnections: 'WebSocket 连接',
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
