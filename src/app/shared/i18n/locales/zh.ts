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
    noResults: '可以放宽搜索条件，或清除筛选后再试。',
    noData: '可以先创建第一项；设置完成后，请重新打开当前页面。',
    optional: '可选',
    required: '请填写',
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
    agents: '智能体',
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
    eyebrow: '设置清单',
    title: '按清单安全设置第一个智能体',
    description: '一次只做一步。按这份设置清单创建智能体、发送任务、检查结果。',
    skip: '跳过并打开任务',
    skipSaving: '正在跳过...',
    skipHint:
      '这只会隐藏左侧菜单里的设置清单。项目、智能体和任务都不会变化，也可以在设置里重置它。',
    skipError: '请检查网络，然后再点一次跳过。暂时无法隐藏设置清单。',
    progressCount: '{{complete}} / {{total}}',
    nextTitle: '下一步先做这个',
    readyTitle: '已经可以开始工作',
    readyDetail: '从任务页写一条小任务；想让智能体下次重复有效做法时，再保存有用步骤。',
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
        empty: '先选择智能体在哪里工作：项目文件，或这台电脑。',
        ready: '{{location}} 已经可以接收智能体工作。',
        why: '智能体需要一个安全的工作位置，才能接收任务。',
        success: '项目文件或这台电脑已经可以接收智能体工作。',
        open: '选择工作位置',
        review: '查看工作位置',
      },
      provider: {
        title: '给智能体一个工作方式',
        empty:
          '先选一种方式：添加模型服务用于文字回答，或打开工作工具登录，让 Codex 能处理文件工作。',
        needsTest: '先检查模型服务，再给智能体分配工作。',
        cliReady: '{{name}} 已经可以从{{location}}执行工作。',
        why: '智能体需要一种可用方式：已检查的模型服务用于文字回答，或已登录的工作工具加上智能体来处理文件工作。',
        success: '已经有一种可用方式：模型服务已检查通过，或有智能体可以打开文件工作。',
        create: '添加模型服务',
        signInTool: '打开工作工具登录',
        test: '检查模型服务',
        reviewProviders: '查看模型服务',
        reviewAgents: '查看智能体',
      },
      agent: {
        title: '智能体',
        empty: '先创建一个简单智能体：文字智能体、项目文件智能体，或这台电脑上的智能体。',
        why: '智能体会接收任务并返回结果。先创建一个简单智能体即可。',
        success: '智能体页面里至少能看到一个智能体。',
        create: '创建智能体',
        review: '打开智能体',
      },
      routing: {
        title: '任务等待位置',
        emptyWithProject: '为当前项目设置任务等待位置。',
        emptyWithoutProject: '先选择项目，再设置任务等待位置。',
        why: '这里是新任务等待的地方，可用的智能体会从这里开始处理任务。',
        success: '当前项目下已经有一个任务等待位置。',
        create: '设置任务等待位置',
        review: '查看任务等待位置',
      },
      task: {
        title: '第一个任务',
        emptyWithRouting: '写一个小任务。Forge 会把它放到任务等待位置，等可用的智能体开始处理。',
        emptyWithoutRouting: '先设置任务等待位置，再创建第一个任务。',
        emptyWithoutProject: '先创建或选择项目，再设置任务等待位置，然后创建第一个任务。',
        ready: '看板上已有 {{count}} 个任务。',
        why: '先用一个小任务验证流程，避免一开始就把真实工作卡住。',
        success: '看板上能看到任务，状态是等待开始或已分配给智能体。',
        create: '写第一个任务',
        open: '打开看板',
      },
      review: {
        title: '检查结果',
        empty: '智能体开始处理任务后，打开任务就能查看进度和结果。',
        inFlight: '已有任务被分配，可从看板查看进度。',
        ready: '{{count}} 个已完成任务可以检查。',
        why: '检查结果能帮你判断智能体是否返回了可以使用的输出。',
        success: '任务已经完成，并且能看到输出或结果文件。',
        open: '检查工作',
      },
      reuse: {
        title: '保存有用步骤',
        empty: '任务完成后，选择可以保存到下次使用的有用步骤。',
        ready: '已有保存好的步骤，可用于后续任务。',
        why: '保存有效步骤后，智能体处理相似任务时不用你重新说明。',
        success: '已经保存有用步骤，或有任务用过这些步骤。',
        review: '选择要保存的步骤',
        open: '查看保存的步骤',
      },
    },
  },

  // =========================================================================
  // 命令面板
  // =========================================================================
  commandPalette: {
    title: '找到你要做的事',
    inputLabel: '搜索页面和可做的事',
    placeholder: '搜索你想做什么，例如：发送任务、添加智能体、登录工具',
    discovery: {
      tasks: '想让智能体做事时，先写一条小任务。',
      inbox: '继续前先查看需要人工处理的更新。',
      settings: '处理智能体、登录、项目和访问权限里的设置卡点。',
    },
    groups: {
      navigation: '打开页面',
      actions: '创建或修改',
      views: '切换任务视图',
    },
    empty: {
      title: '没有匹配的页面或选项',
      listSeparator: '、',
      tryShorter: '可以换个更短的词，或打开设置浏览需要配置的内容。',
      tryOne: '可以试试{{label}}，打开常用页面。',
      tryMany: '可以试试{{prefix}}或{{last}}，打开常用页面。',
      commonPages: '常用页面',
      openPage: '打开{{label}}',
      showAll: '显示全部页面和操作',
    },
    commands: {
      nav: {
        start: {
          label: '设置清单',
          description: '需要引导步骤时，重新打开设置清单。',
        },
        tasks: {
          label: '任务',
          description: '查看计划中、进行中或已完成的工作。',
        },
        inbox: {
          label: '收件箱',
          description: '查看可能需要人工处理的提醒。',
        },
        context: {
          label: '保存内容',
          description: '查看智能体之后可能复用的笔记和指令。',
        },
        agents: {
          label: '智能体',
          description: '创建或查看负责处理任务的智能体。',
        },
        skills: {
          label: '保存的指令',
          description: '复用适合重复工作的指令。',
        },
        settings: {
          label: '设置',
          description: '连接工具、账号权限、团队和项目。',
        },
      },
      actions: {
        createTask: {
          label: '新任务',
          description: '告诉智能体你想要的结果，以及如何检查是否完成。',
        },
        workToolSignIns: {
          label: 'Codex 登录',
          description: '智能体用 Codex 或其他工作工具改文件前，先在这里登录。',
        },
        keys: {
          label: '外部工具访问',
          description: '让可信的外部工具连接 Forge，无需人工登录。',
        },
        gitCredentials: {
          label: 'HTTPS 代码访问',
          description: '私有代码链接以 https:// 开头时使用这里。',
        },
        sshKeys: {
          label: 'SSH 代码访问',
          description: '私有代码链接以 git@ 开头时使用这里。',
        },
        resources: {
          label: '智能体大小限制',
          description: '智能体开始文件工作前，选择小、标准或大的资源限制。',
        },
        projects: {
          label: '项目',
          description: '创建或选择任务、智能体和文件所属的位置。',
        },
        teams: {
          label: '团队',
          description: '创建团队，并管理谁可以修改工作。',
        },
        providers: {
          label: '模型服务',
          description: '连接智能体回答问题时使用的模型账号。',
        },
        runtime: {
          label: '智能体在哪里工作',
          description: '常规设置选项目文件；文件必须留在本机时选这台电脑。',
        },
        account: {
          label: '账号',
          description: '更新个人资料、密码，也可以重置设置清单。',
        },
        theme: {
          label: '切换主题',
          description: '切换应用外观。',
        },
        setupChecklistRecovery: {
          label: '重置设置清单',
          description: '把设置清单重新显示在左侧菜单并打开。项目、智能体和任务都不会变化。',
        },
      },
      views: {
        board: {
          label: '看板视图',
          description: '用简单列移动任务。',
        },
        list: {
          label: '列表视图',
          description: '在一个可排序表格里查看任务。',
        },
        timeline: {
          label: '时间线视图',
          description: '查看工作是什么时候发生的。',
        },
        visualMap: {
          label: '可视化地图',
          description: '在图上查看智能体和任务。',
        },
      },
    },
    taskSetup: {
      noProjectOptions: {
        label: '创建任务前先设置项目',
        buttonLabel: '设置项目',
        description: '打开项目设置，让任务有归属位置。',
      },
      chooseProject: {
        label: '为新任务选择项目',
        buttonLabel: '新任务',
        description: '先选择项目，再写给智能体的任务。',
      },
      noWaitingPlace: {
        label: '先设置任务等待位置',
        buttonLabel: '设置等待位置',
        description: '创建任务前，先打开智能体页面添加等待位置。',
      },
      ready: {
        label: '新任务',
        buttonLabel: '新任务',
        description: '告诉智能体你想要的结果，以及如何检查是否完成。',
      },
    },
  },

  // =========================================================================
  // 应用布局
  // =========================================================================
  appLayout: {
    pages: {
      start: {
        title: '设置清单',
        subtitle: '设置 Forge，并发送第一个任务',
      },
      tasks: {
        title: '任务',
        subtitle: '创建任务，并跟进智能体进度',
      },
      inbox: {
        title: '收件箱',
        subtitle: '查看需要下一步处理的更新',
      },
      savedItemHistory: {
        title: '保存内容历史',
        subtitle: '查看哪些内容被检查或复用过',
      },
      savedItems: {
        title: '保存的笔记和指令',
        subtitle: '查看智能体之后可能复用的内容',
      },
      agents: {
        title: '智能体',
        subtitle: '创建并管理处理任务的智能体',
      },
      skills: {
        title: '保存的指令',
        subtitle: '智能体可以再次使用的指令',
      },
      analytics: {
        title: '分析',
        subtitle: '查看智能体活动和结果',
      },
      billing: {
        title: '账单',
        subtitle: '查看套餐、付款和发票',
      },
      settings: {
        title: '设置',
        subtitle: '设置账号、模型服务和团队',
      },
      admin: {
        title: '管理',
        subtitle: '检查应用健康并管理人员',
      },
      fallback: {
        title: 'Wisdoverse Forge',
      },
    },
    topBar: {
      openNavigation: '打开导航',
      search: '搜索',
      searchLabel: '搜索页面和可做的事',
      switchToLight: '切换到浅色模式',
      switchToDark: '切换到深色模式',
      views: {
        board: '看板',
        list: '列表',
        timeline: '时间线',
        map: '地图',
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
    title: '智能体',
    newAgent: '新建智能体',
    createAgent: '添加智能体',
    editAgent: '编辑智能体',
    deleteAgent: '删除智能体',
    noAgents: '先创建一个智能体，再发送任务。',
    agentName: '智能体名称',
    projectPath: '项目文件夹位置',
    workingDirectory: '工作文件夹',
    startAgent: '启动智能体',
    stopAgent: '停止智能体',
    restartAgent: '重启智能体',
    duplicateAgent: '复制智能体',
    exportAgent: '导出智能体',
    importAgent: '导入智能体',
    agentDetails: '智能体概览',
    agentSettings: '智能体设置',
    agentHistory: '智能体历史',
    activeAgent: '活跃智能体',
    lastActive: '{{time}}活跃',
    createdAt: '创建于 {{time}}',
    status: {
      idle: '可接收任务',
      working: '工作中',
      waiting: '需要输入',
      offline: '未连接',
      starting: '正在启动工作...',
      stopping: '正在停止工作...',
      error: '检查智能体状态',
      connecting: '连接中...',
    },
    confirmDelete: '要删除这个智能体吗？这会移除它的设置，并停止给它分配新任务。',
    confirmStop: '要停止这个智能体吗？当前工作会暂停，直到你重新启动它。',
    // 创建智能体弹窗
    startNewAgent: '开始新智能体',
    pickProject: '选择一个项目开始',
    tellClaude: '告诉 Claude 你要做什么',
    searchProjects: '搜索项目或输入文件夹位置...',
    enterFolderPath: '输入项目文件夹位置...',
    moreOptions: '更多选项',
    behavior: '行为设置',
    autoApprove: '自动批准操作',
    resumeLast: '继续上次智能体',
    enableBrowser: '启用浏览器',
    start: '开始',
    nAgents: '{{count}} 个智能体',
    agentStarted: '智能体已启动',
    agentStopped: '智能体已停止',
    agentDeleted: '智能体已删除',
    agentCreated: '智能体已创建',
    maxAgentsReached: '请先停止或删除不用的智能体，然后重试。当前智能体数量已经到上限。',
    invalidProjectPath: '请输入项目文件夹位置，然后重试。',
  },

  // =========================================================================
  // 任务等待位置
  // =========================================================================
  groups: {
    title: '任务等待位置',
    newGroup: '新建任务等待位置',
    createGroup: '创建任务等待位置',
    editGroup: '编辑任务等待位置',
    deleteGroup: '删除任务等待位置',
    noGroups: '先创建一个任务等待位置，让新任务有地方等待智能体接手。',
    groupName: '任务等待位置名称',
    groupColor: '任务等待位置颜色',
    addToGroup: '添加到任务等待位置',
    removeFromGroup: '从任务等待位置移除',
    moveToGroup: '移动到任务等待位置',
    ungrouped: '请先设置任务等待位置',
    confirmDelete:
      '要删除这个任务等待位置吗？智能体仍会保留，但任务需要选择其他等待位置后才能发送。',
    groupCreated: '任务等待位置已创建',
    groupDeleted: '任务等待位置已删除',
    groupUpdated: '任务等待位置已更新',
  },

  // =========================================================================
  // 活动流
  // =========================================================================
  feed: {
    title: '活动',
    noActivity: '先启动一个任务，后续更新会显示在这里。',
    clearActivity: '清除活动',
    filterByType: '按类型筛选',
    filterByAgent: '按智能体筛选',
    showAll: '显示全部',
    eventTypes: {
      tool_use: '智能体使用了工具',
      tool_result: '工具已完成',
      text: '智能体消息',
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
      Task: '请另一个智能体协助',
    },
    expandAll: '全部展开',
    collapseAll: '全部折叠',
    copyContent: '复制内容',
    viewDetails: '查看这条更新',
    timestamp: '{{time}}',
  },

  // =========================================================================
  // 提示输入
  // =========================================================================
  prompt: {
    placeholder: '输入一条给智能体的消息...',
    placeholderShort: '输入一条消息...',
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
    emptyPrompt: '请先输入一条消息。',
    selectAgent: '请先选择一个智能体',
    noAgentSelected: '请先选择一个智能体，再发送任务。',
    multipleAgentsSelected: '已选择 {{count}} 个智能体',
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
    loading: '正在检查视觉地图...',
    loadError: '请打开智能体，等其中一个显示可接收任务后，再重新打开视觉地图。',
    controls: {
      zoom: '使用智能体列表查找智能体',
      pan: '在地图中选择机器人',
      rotate: '地图会自动更新',
      select: '从列表或地图中选择智能体',
    },
    shortcuts: {
      numbers: '按 1-9 选择智能体',
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
      title: '智能体在哪里工作',
      description: '选择智能体可以在哪里打开文件，并在分配任务前检查工具和登录状态。',
      saving: '保存中...',
      loading: '正在加载智能体在哪里工作...',
      couldNotLoad:
        '请打开设置，然后打开“智能体在哪里工作”。如果仍然无法加载，请找负责人或管理员检查设置里的“智能体在哪里工作”。',
      defaultRuntimeLabel: '项目文件打开位置',
      defaultRuntimeDescription:
        '处理共享项目文件时，选择“项目文件”最简单。只有要把这台电脑接入为可在 Forge 里管理的智能体时，才选择这台电脑。',
      defaultContainerCliLabel: '项目工作默认工具',
      defaultContainerCliDescription:
        '智能体编辑文件或运行命令时使用的 Claude Code、Codex、Gemini 或 OpenCode',
      availableRuntimesLabel: '可打开项目文件的位置',
      availableRuntimesDescription: '智能体可以打开项目文件的位置',
      availableContainerClisLabel: '智能体可使用的工作工具',
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
      subtitle: '智能体在处理任务时可以复用的说明。',
      statusReady: '可以使用',
      statusNeedsInstall: '需要先完成设置',
      cliFit: '最适合 {{tool}}',
      unknownToolFit: '在设置里检查工作工具',
      allAgentsFit: '适用于任意智能体',
      allAgentsTooltip: '不需要指定工作工具。',
      containerCliTooltip: '工作工具：{{tool}}',
      unknownToolTooltip: '打开设置，检查工作工具，然后再使用这条保存的说明。',
      nextStepHeading: '下一步做什么',
      nextStepReady: '创建任务时可以使用这条保存的说明，也可以让匹配词在类似任务中提示它。',
      nextStepNeedsInstall: '请让所有者或管理员先完成设置，然后再在任务中使用这条保存的说明。',
      sourceLabel: '来源',
      authorLabel: '更新人',
      availabilityLabel: '可用范围',
      descriptionHeading: '它能帮什么',
      noDescription: '使用这条保存的说明前，请先查看下面的可复用说明。',
      triggerHeading: '什么时候有帮助',
      triggerHelper: '任务里出现类似这些词时，可以推荐这条保存的说明。',
      detailsHeading: '可复用说明',
      detailsHelper: '使用这条保存的说明前，请先查看这些可复用步骤。',
      noContent: '还没有保存可复用步骤。请先补充智能体要遵循的步骤，再使用这条保存的说明。',
      unknownAuthor: '请重新打开保存的说明，查看谁负责更新它',
      unknownSource: '保存的说明',
      availabilityWorkspace: '当前团队空间',
      availabilityGlobal: '保存的说明',
      availabilityProject: '当前项目',
      availabilityLatest: '最新保存版本',
      availabilityNeedsReview: '检查保存说明的可用范围',
    },
  },

  // =========================================================================
  // 错误
  // =========================================================================
  errors: {
    generic: '请稍等一下再重试；如果反复发生，请让管理员检查应用健康状态。',
    network: '请检查网络，然后重试。Forge 暂时无法连接。',
    timeout: '请稍等片刻后重试。请求时间太长。',
    notFound: '请重新打开当前页面后重试。未找到 {{resource}}。',
    unauthorized: '请重新登录，然后再试一次。',
    forbidden: '你当前无法执行这个操作。请让所有者或管理员检查你的团队空间访问权限。',
    validation: '请检查高亮字段，然后重试。',
    serverError: '请稍等片刻后重试。Forge 暂时无法完成这个操作。',
    connectionLost: 'Forge 正在尝试重新连接。请先保持本页打开；如果更新仍未恢复，再刷新页面。',
    reconnecting: '仍在重新连接。请保持本页打开。',
    reconnected: '实时更新已恢复。新的进度会继续出现在这里。',
    agentError: '请重试这一步；如果反复出现，请检查智能体状态。智能体没有完成这一步。',
    fileError: '请检查文件后重试。Forge 暂时无法处理这个文件。',
    uploadError: '请检查文件和网络后重新上传。上传没有完成。',
    downloadError: '请重新打开当前页面后再次下载。下载没有开始。',
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
          detail: '该智能体通过 AI 服务回复消息。请重新发送消息来重新尝试。',
        },
        start_host_cli: {
          title: '请在这台电脑上启动连接助手',
          detail: '请在那台电脑上重新运行设置命令，让智能体上线。',
        },
        start_api: {
          title: '请发送消息来启动这个聊天智能体',
          detail: '聊天智能体会在你发送消息时开始工作，没有需要打开的本地窗口。',
        },
        stop_host_cli: {
          title: '请在这台电脑上停止连接助手',
          detail: 'Forge 不能替你停止它。请关闭那台电脑上的 Terminal 或 PowerShell 窗口。',
        },
        stop_api: {
          title: '请关闭聊天或等待回复结束',
          detail: '聊天智能体没有需要停止的本地窗口。需要继续时再发送新消息。',
        },
        not_permitted: {
          title: '你不能管理这个智能体',
          detail: '你只能管理你拥有的智能体。如需访问请联系智能体所有者。',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: '请选择一个工作工具',
          detail:
            '会编辑项目文件的智能体需要一个工作工具：Claude Code、Codex、Gemini 或 OpenCode。',
        },
        api_cannot_have_cli_tool: {
          title: '只处理文字的模型智能体不能有工作工具',
          detail: '请移除工作工具，或将工作类型改为“项目文件”。',
        },
        missing_cli_tool_for_host_cli: {
          title: '请选择一个工作工具',
          detail:
            '从这台电脑加入的智能体需要一个工作工具：Claude Code、Codex、Gemini 或 OpenCode。',
        },
      },
      enroll: {
        missing_idempotency_key: {
          title: '需要重新运行设置命令',
          detail:
            '请在这台电脑上重新运行设置命令。如果反复出现，请让管理员检查这台电脑的“智能体在哪里工作”。',
        },
        plaintext_nats_blocked: {
          title: '这台电脑的连接需要安全通道',
          detail:
            '请使用“从这台电脑接入智能体”的安全连接地址。如果不确定该填什么，请让管理员检查这台电脑的智能体连接设置。',
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
    agents: '智能体',
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
    required: '请填写这个字段，然后重试',
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
  // 管理
  // =========================================================================
  admin: {
    title: '管理',
    tabs: {
      agents: '智能体',
      metrics: '活动',
      users: '人员',
      health: '应用健康',
    },
    agents: {
      title: '智能体',
      search: '搜索智能体',
      status: '能否接任务',
      actions: '可执行操作',
      noAgents: '当前视图没有匹配的智能体。可以清除搜索，或先创建智能体。',
      pause: '暂停',
      resume: '恢复',
      stop: '停止',
      delete: '删除',
    },
    metrics: {
      title: '活动和容量',
      activeAgents: '正在工作的智能体',
      totalEvents: '工作更新',
      eventsPerMinute: '每分钟更新数',
      memoryUsage: '已用内存',
      cpuUsage: '处理器使用',
      uptime: '运行时长',
      wsConnections: '打开的浏览器页面',
      requestsPerMinute: '每分钟请求数',
    },
    users: {
      title: '有访问权限的人员',
      search: '搜索人员',
      addUser: '邀请成员',
      editUser: '调整访问权限',
      deleteUser: '移除访问权限',
      noUsers: '当前视图没有匹配的人员。可以清除搜索，或先邀请成员。',
      role: '访问级别',
      roles: {
        admin: '管理员',
        operator: '成员',
        viewer: '查看者',
      },
      status: {
        active: '可使用',
        inactive: '未开始使用',
        suspended: '已暂停',
      },
      confirmDelete: '要移除这个人的访问权限吗？该成员将失去当前团队空间访问权限。',
    },
    health: {
      title: '应用健康',
      overall: '整体应用状态',
      services: '应用区域',
      alerts: '待检查项目',
      status: {
        healthy: '正常工作',
        degraded: '需要检查',
        down: '无法使用',
      },
      noAlerts: '所有应用区域都正常工作。',
      acknowledge: '标记已查看',
      latency: '响应时间',
      lastCheck: '上次检查',
    },
  },
} as const
