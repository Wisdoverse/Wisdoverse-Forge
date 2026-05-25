/**
 * English Translations
 *
 * Primary language for Wisdoverse Forge.
 * This file serves as the source of truth for translation keys.
 */

export const en = {
  // =========================================================================
  // Common
  // =========================================================================
  common: {
    save: 'Save',
    cancel: 'Cancel',
    delete: 'Delete',
    confirm: 'Confirm',
    close: 'Close',
    edit: 'Edit',
    create: 'Create',
    update: 'Update',
    submit: 'Submit',
    reset: 'Reset',
    clear: 'Clear',
    search: 'Search',
    filter: 'Filter',
    sort: 'Sort',
    refresh: 'Refresh',
    loading: 'Loading...',
    saving: 'Saving...',
    deleting: 'Deleting...',
    processing: 'Processing...',
    error: 'An error occurred',
    success: 'Success',
    warning: 'Warning',
    info: 'Information',
    yes: 'Yes',
    no: 'No',
    ok: 'OK',
    back: 'Back',
    next: 'Next',
    previous: 'Previous',
    done: 'Done',
    retry: 'Retry',
    copy: 'Copy',
    copied: 'Copied!',
    download: 'Download',
    upload: 'Upload',
    more: 'More',
    less: 'Less',
    all: 'All',
    none: 'None',
    select: 'Select',
    selected: '{{count}} selected',
    noResults: 'No results found',
    noData: 'No data available',
    optional: 'Optional',
    required: 'Required',
  },

  // =========================================================================
  // Navigation
  // =========================================================================
  nav: {
    home: 'Home',
    start: 'Start',
    dashboard: 'Dashboard',
    tasks: 'Tasks',
    inbox: 'Inbox',
    context: 'Context',
    agents: 'Agents',
    skills: 'Skills',
    analytics: 'Analytics',
    billing: 'Billing',
    settings: 'Settings',
    help: 'Help',
    about: 'About',
    logout: 'Logout',
    profile: 'Profile',
    admin: 'Admin',
  },

  // =========================================================================
  // Getting Started
  // =========================================================================
  gettingStarted: {
    eyebrow: 'First run',
    title: 'Start with one safe path',
    description:
      'Follow one step at a time. Finish this path to create an agent, send work, and review the result.',
    progressCount: '{{complete}} of {{total}}',
    nextTitle: 'Do this next',
    readyTitle: 'Ready to run work',
    readyDetail: 'The basic path is complete. You can create more tasks or review reusable skills.',
    successLabel: 'Success looks like:',
    currentProject: 'Current project',
    noProject: 'No project selected',
    projects: 'Projects',
    stepStatus: {
      done: 'Done',
      next: 'Next',
      later: 'Later',
    },
    steps: {
      workspace: {
        title: 'Workspace',
        empty: 'Create a team and project only when routing needs them.',
        why: 'A project gives tasks a clear home so agents know what work belongs together.',
        success: 'A team and project exist, and the project is selected.',
        create: 'Create workspace',
        review: 'Review workspace',
      },
      runtime: {
        title: 'Runtime',
        empty: 'Confirm the execution runtime and Container CLI options.',
        ready: '{{runtime}} runtime with {{cli}} as default CLI.',
        why: 'The platform needs a runtime before it can start or assign agent work.',
        success: 'Runtime settings show at least one runtime and one Container CLI option.',
        open: 'Check runtime',
        review: 'Review runtime',
      },
      provider: {
        title: 'Execution credential',
        empty: 'Add a model provider or connect a CLI agent.',
        needsTest: 'Run Test on a provider before creating an agent.',
        cliReady: '{{name}} is connected through {{runtime}}.',
        why: 'An agent needs either a tested provider key or a connected CLI before it can do work.',
        success: 'A provider test passes, or a Host CLI or Container CLI agent is connected.',
        create: 'Add provider',
        connectCli: 'Connect CLI agent',
        test: 'Test provider',
        reviewProviders: 'Review providers',
        reviewAgents: 'Review agents',
      },
      agent: {
        title: 'Agent',
        empty: 'Create a provider-backed or container agent.',
        why: 'Agents are the workers that receive tasks. Start with one simple agent.',
        success: 'At least one agent appears in the Agents page.',
        create: 'Create agent',
        review: 'Open agents',
      },
      routing: {
        title: 'Task routing',
        emptyWithProject: 'Create a task group for this project.',
        emptyWithoutProject: 'Select a project, then create a task group.',
        why: 'A task group tells the platform where new work should wait and which agents can pick it up.',
        success: 'A task group exists for the selected project.',
        create: 'Create task group',
        review: 'Review routing',
      },
      task: {
        title: 'First task',
        emptyWithRouting: 'Create a task, assign it, and watch the run start.',
        emptyWithoutRouting: 'Finish routing before creating the first task.',
        ready: '{{count}} task on the board.',
        why: 'A small first task proves the setup works before you depend on it for real work.',
        success: 'A task appears on the board and is assigned or queued.',
        create: 'Create task',
        open: 'Open board',
      },
      review: {
        title: 'Review output',
        empty: 'Assigned task output will appear in the detail panel.',
        inFlight: 'A task is assigned. Review progress from the board.',
        ready: '{{count}} completed task ready for acceptance.',
        why: 'Reviewing the result confirms the agent returned useful work and evidence.',
        success: 'A task has completed output or attached evidence.',
        open: 'Review work',
      },
      reuse: {
        title: 'Reusable learning',
        empty: 'Approve context or skill candidates after completed work.',
        ready: 'Reusable skills or applied skill context exist.',
        why: 'Approved skills and context make the next similar task easier and more consistent.',
        success: 'A reusable skill or applied skill context exists.',
        review: 'Review candidates',
        open: 'Open skills',
      },
    },
  },

  // =========================================================================
  // Authentication
  // =========================================================================
  auth: {
    login: 'Login',
    logout: 'Logout',
    register: 'Register',
    forgotPassword: 'Forgot Password?',
    resetPassword: 'Reset Password',
    changePassword: 'Change Password',
    email: 'Email',
    password: 'Password',
    confirmPassword: 'Confirm Password',
    username: 'Username',
    rememberMe: 'Remember me',
    loginSuccess: 'Successfully logged in',
    logoutSuccess: 'Successfully logged out',
    registerSuccess: 'Account created successfully',
    invalidCredentials: 'Invalid email or password',
    accountLocked: 'Account is locked. Please try again later.',
    agentExpired: 'Your agent has expired. Please login again.',
    passwordResetSent: 'Password reset instructions have been sent to your email',
    passwordChanged: 'Password changed successfully',
    passwordTooShort: 'Password must be at least {{min}} characters',
    passwordMismatch: 'Passwords do not match',
    emailInvalid: 'Please enter a valid email address',
    emailInUse: 'This email is already in use',
    usernameInUse: 'This username is already taken',
    emailDomainRestricted: 'Registration restricted to authorized email domains',
    passwordRequirements: '12+ characters, uppercase, lowercase, number, special character',
    passwordWeak: 'Weak',
    passwordFair: 'Fair',
    passwordGood: 'Good',
    passwordStrong: 'Strong',
    createAccount: 'Create Account',
    fillAllFields: 'Please fill in all fields',
    fillRequiredFields: 'Please fill in all required fields',
    networkError: 'Network error',
  },

  // =========================================================================
  // Agents
  // =========================================================================
  agents: {
    title: 'Agents',
    newAgent: 'New Agent',
    createAgent: 'Create Agent',
    editAgent: 'Edit Agent',
    deleteAgent: 'Delete Agent',
    noAgents: 'No agents yet',
    agentName: 'Agent Name',
    projectPath: 'Project Path',
    workingDirectory: 'Working Directory',
    startAgent: 'Start Agent',
    stopAgent: 'Stop Agent',
    restartAgent: 'Restart Agent',
    duplicateAgent: 'Duplicate Agent',
    exportAgent: 'Export Agent',
    importAgent: 'Import Agent',
    agentDetails: 'Agent Details',
    agentSettings: 'Agent Settings',
    agentHistory: 'Agent History',
    activeAgent: 'Active Agent',
    lastActive: 'Last active {{time}}',
    createdAt: 'Created {{time}}',
    status: {
      idle: 'Idle',
      working: 'Working',
      waiting: 'Waiting for input',
      offline: 'Offline',
      starting: 'Starting...',
      stopping: 'Stopping...',
      error: 'Error',
      connecting: 'Connecting...',
    },
    confirmDelete: 'Are you sure you want to delete this agent?',
    confirmStop: 'Are you sure you want to stop this agent?',
    // New agent modal
    startNewAgent: 'Start a new agent',
    pickProject: 'Pick a project to begin',
    tellClaude: 'Tell Claude what to work on',
    searchProjects: 'Search projects or enter a folder path...',
    enterFolderPath: 'Enter a project folder path...',
    moreOptions: 'More options',
    behavior: 'Behavior',
    autoApprove: 'Auto-approve actions',
    resumeLast: 'Resume last agent',
    enableBrowser: 'Enable browser',
    start: 'Start',
    nAgents: '{{count}} agents',
    agentStarted: 'Agent started',
    agentStopped: 'Agent stopped',
    agentDeleted: 'Agent deleted',
    agentCreated: 'Agent created',
    maxAgentsReached: 'Maximum number of agents reached',
    invalidProjectPath: 'Invalid project path',
  },

  // =========================================================================
  // Groups
  // =========================================================================
  groups: {
    title: 'Groups',
    newGroup: 'New Group',
    createGroup: 'Create Group',
    editGroup: 'Edit Group',
    deleteGroup: 'Delete Group',
    noGroups: 'No groups yet',
    groupName: 'Group Name',
    groupColor: 'Group Color',
    addToGroup: 'Add to Group',
    removeFromGroup: 'Remove from Group',
    moveToGroup: 'Move to Group',
    ungrouped: 'Ungrouped',
    confirmDelete: 'Are you sure you want to delete this group?',
    groupCreated: 'Group created',
    groupDeleted: 'Group deleted',
    groupUpdated: 'Group updated',
  },

  // =========================================================================
  // Activity Feed
  // =========================================================================
  feed: {
    title: 'Activity',
    noActivity: 'No activity yet',
    clearActivity: 'Clear Activity',
    filterByType: 'Filter by type',
    filterByAgent: 'Filter by agent',
    showAll: 'Show all',
    eventTypes: {
      tool_use: 'Tool Use',
      tool_result: 'Tool Result',
      text: 'Text',
      error: 'Error',
      thinking: 'Thinking',
      system: 'System',
    },
    tools: {
      Read: 'Read File',
      Write: 'Write File',
      Edit: 'Edit File',
      Bash: 'Run Command',
      Glob: 'Find Files',
      Grep: 'Search Content',
      WebFetch: 'Fetch URL',
      WebSearch: 'Web Search',
      Task: 'Subagent Task',
    },
    expandAll: 'Expand all',
    collapseAll: 'Collapse all',
    copyContent: 'Copy content',
    viewDetails: 'View details',
    timestamp: '{{time}}',
  },

  // =========================================================================
  // Prompt Input
  // =========================================================================
  prompt: {
    placeholder: 'Type your prompt here...',
    placeholderShort: 'Type a prompt...',
    send: 'Send',
    sending: 'Sending...',
    cancel: 'Cancel',
    clear: 'Clear',
    history: 'History',
    suggestions: 'Suggestions',
    attachFile: 'Attach file',
    voiceInput: 'Voice input',
    recording: 'Recording...',
    processing: 'Processing...',
    characterCount: '{{count}} / {{max}} characters',
    characterLimitWarning: 'Approaching character limit',
    emptyPrompt: 'Please enter a prompt',
    selectAgent: 'Please select a agent first',
    noAgentSelected: 'No agent selected',
    multipleAgentsSelected: '{{count}} agents selected',
    shortcuts: {
      send: 'Press Enter to send',
      newLine: 'Press Shift+Enter for new line',
      history: 'Press ↑ to recall history',
    },
  },

  // =========================================================================
  // Workshop (3D Scene)
  // =========================================================================
  workshop: {
    title: 'Workshop',
    loading: 'Loading workshop...',
    loadError: 'Failed to load workshop',
    controls: {
      zoom: 'Scroll to zoom',
      pan: 'Middle-click to pan',
      rotate: 'Right-click to rotate',
      select: 'Click to select',
    },
    shortcuts: {
      numbers: 'Press 1-9 to select agents',
      escape: 'Press Esc to deselect',
      help: 'Press ? for help',
      fullscreen: 'Press F for fullscreen',
      drawMode: 'Press D for draw mode',
    },
    performance: {
      fps: '{{value}} FPS',
      memory: '{{value}} MB',
      renderTime: '{{value}} ms',
    },
  },

  // =========================================================================
  // Settings
  // =========================================================================
  settings: {
    title: 'Settings',
    general: 'General',
    appearance: 'Appearance',
    notifications: 'Notifications',
    keyboard: 'Keyboard Shortcuts',
    advanced: 'Advanced',
    account: 'Account',
    security: 'Security',
    integrations: 'Integrations',
    language: 'Language',
    theme: 'Theme',
    themes: {
      light: 'Light',
      dark: 'Dark',
      system: 'System',
    },
    fontSize: 'Font Size',
    autoSave: 'Auto Save',
    autoSaveInterval: 'Auto Save Interval',
    soundEffects: 'Sound Effects',
    enableNotifications: 'Enable Notifications',
    desktopNotifications: 'Desktop Notifications',
    emailNotifications: 'Email Notifications',
    saved: 'Settings saved',
    reset: 'Reset to defaults',
    resetConfirm: 'Are you sure you want to reset all settings?',
    runtime: {
      title: 'Runtime',
      description: 'Default agent runtime and Container CLI configuration',
      saving: 'Saving...',
      loading: 'Loading runtime settings...',
      couldNotLoad: 'Could not load runtime settings',
      defaultRuntimeLabel: 'Default Runtime',
      defaultRuntimeDescription: 'How agents are executed by default',
      defaultContainerCliLabel: 'Default Container CLI',
      defaultContainerCliDescription: 'Container CLI used when runtime is Host CLI or Container',
      availableRuntimesLabel: 'Available Runtimes',
      availableRuntimesDescription: 'Runtimes enabled on this instance',
      availableContainerClisLabel: 'Available Container CLIs',
      availableContainerClisDescription: 'Container CLI agents enabled on this instance',
      runtimeLabels: {
        cli: 'Host CLI (local process)',
        api: 'API (direct LLM calls)',
        container: 'Container (Docker)',
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
  // Skills
  // =========================================================================
  skills: {
    detail: {
      closeAria: 'Close',
      close: 'Done',
      subtitle: 'Reusable instructions agents can apply during task work.',
      statusReady: 'Ready to use',
      statusNeedsInstall: 'Needs install before agents can use it',
      cliFit: 'Best with {{tool}}',
      allAgentsFit: 'Works with any agent',
      allAgentsTooltip: 'No specific Container CLI is required.',
      containerCliTooltip: 'Container CLI: {{tool}}',
      sourceLabel: 'Where it came from',
      authorLabel: 'Maintainer',
      versionLabel: 'Version',
      descriptionHeading: 'What this helps with',
      noDescription: 'No summary is available yet.',
      triggerHeading: 'When agents should consider it',
      triggerHelper: 'Agents can use this skill when the task matches this phrase.',
      detailsHeading: 'Instructions the agent will read',
      detailsHelper: 'Review this text if you need to check exactly what will be reused.',
      noContent: 'No reusable instructions have been saved yet.',
      unknownAuthor: 'Unknown',
      unknownSource: 'Skills library',
      versionLatest: 'latest',
    },
  },

  // =========================================================================
  // Errors
  // =========================================================================
  errors: {
    generic: 'An unexpected error occurred',
    network: 'Network error. Please check your connection.',
    timeout: 'Request timed out. Please try again.',
    notFound: '{{resource}} not found',
    unauthorized: 'You are not authorized to perform this action',
    forbidden: 'Access denied',
    validation: 'Please check your input',
    serverError: 'Server error. Please try again later.',
    connectionLost: 'Connection lost. Reconnecting...',
    reconnecting: 'Reconnecting...',
    reconnected: 'Connection restored',
    agentError: 'Agent error: {{message}}',
    fileError: 'File error: {{message}}',
    uploadError: 'Upload failed: {{message}}',
    downloadError: 'Download failed: {{message}}',
    rateLimited: 'Too many requests. Please wait {{seconds}} seconds.',
    quotaExceeded: 'Quota exceeded for {{resource}}',
  },

  // =========================================================================
  // Confirmations
  // =========================================================================
  confirm: {
    delete: 'Are you sure you want to delete this?',
    unsavedChanges: 'You have unsaved changes. Are you sure you want to leave?',
    logout: 'Are you sure you want to logout?',
    reset: 'Are you sure you want to reset? This cannot be undone.',
    stop: 'Are you sure you want to stop this operation?',
    discard: 'Are you sure you want to discard your changes?',
  },

  // =========================================================================
  // Time
  // =========================================================================
  time: {
    now: 'Just now',
    seconds: '{{count}} second ago|{{count}} seconds ago',
    minutes: '{{count}} minute ago|{{count}} minutes ago',
    hours: '{{count}} hour ago|{{count}} hours ago',
    days: '{{count}} day ago|{{count}} days ago',
    weeks: '{{count}} week ago|{{count}} weeks ago',
    months: '{{count}} month ago|{{count}} months ago',
    years: '{{count}} year ago|{{count}} years ago',
  },

  // =========================================================================
  // File Operations
  // =========================================================================
  files: {
    upload: 'Upload File',
    download: 'Download File',
    delete: 'Delete File',
    rename: 'Rename File',
    move: 'Move File',
    copy: 'Copy File',
    size: 'Size: {{size}}',
    type: 'Type: {{type}}',
    modified: 'Modified: {{date}}',
    created: 'Created: {{date}}',
    dropzone: 'Drop files here or click to upload',
    maxSize: 'Maximum file size: {{size}}',
    allowedTypes: 'Allowed types: {{types}}',
    uploading: 'Uploading...',
    uploaded: 'File uploaded successfully',
    uploadFailed: 'File upload failed',
    tooLarge: 'File is too large. Maximum size is {{size}}.',
    invalidType: 'Invalid file type. Allowed types are: {{types}}',
  },

  // =========================================================================
  // Keyboard Shortcuts
  // =========================================================================
  shortcuts: {
    title: 'Keyboard Shortcuts',
    general: 'General',
    navigation: 'Navigation',
    editing: 'Editing',
    agents: 'Agents',
    keys: {
      enter: 'Enter',
      escape: 'Esc',
      tab: 'Tab',
      shift: 'Shift',
      ctrl: 'Ctrl',
      alt: 'Alt',
      cmd: 'Cmd',
      space: 'Space',
      up: '↑',
      down: '↓',
      left: '←',
      right: '→',
    },
  },

  // =========================================================================
  // Accessibility
  // =========================================================================
  a11y: {
    skipToContent: 'Skip to content',
    openMenu: 'Open menu',
    closeMenu: 'Close menu',
    expandSection: 'Expand section',
    collapseSection: 'Collapse section',
    loading: 'Loading, please wait',
    error: 'Error occurred',
    success: 'Operation successful',
    required: 'This field is required',
    invalid: 'This field is invalid',
  },

  // =========================================================================
  // Tooltips
  // =========================================================================
  tooltips: {
    copy: 'Copy to clipboard',
    edit: 'Edit',
    delete: 'Delete',
    expand: 'Expand',
    collapse: 'Collapse',
    refresh: 'Refresh',
    settings: 'Open settings',
    help: 'Get help',
    close: 'Close',
    maximize: 'Maximize',
    minimize: 'Minimize',
  },

  // =========================================================================
  // Admin Dashboard
  // =========================================================================
  admin: {
    title: 'Admin Dashboard',
    tabs: {
      agents: 'Agents',
      metrics: 'Metrics',
      users: 'Users',
      health: 'Health',
    },
    agents: {
      title: 'Agent Management',
      search: 'Search agents...',
      status: 'Status',
      actions: 'Actions',
      noAgents: 'No agents found',
      pause: 'Pause',
      resume: 'Resume',
      stop: 'Stop',
      delete: 'Delete',
    },
    metrics: {
      title: 'System Metrics',
      activeAgents: 'Active Agents',
      totalEvents: 'Total Events',
      eventsPerMinute: 'Events/min',
      memoryUsage: 'Memory Usage',
      cpuUsage: 'CPU Usage',
      uptime: 'Uptime',
      wsConnections: 'WS Connections',
      requestsPerMinute: 'Requests/min',
    },
    users: {
      title: 'User Management',
      search: 'Search users...',
      addUser: 'Add User',
      editUser: 'Edit User',
      deleteUser: 'Delete User',
      noUsers: 'No users found',
      role: 'Role',
      roles: {
        admin: 'Admin',
        operator: 'Operator',
        viewer: 'Viewer',
      },
      status: {
        active: 'Active',
        inactive: 'Inactive',
        suspended: 'Suspended',
      },
      confirmDelete: 'Are you sure you want to delete this user?',
    },
    health: {
      title: 'System Health',
      overall: 'Overall Status',
      services: 'Services',
      alerts: 'Alerts',
      status: {
        healthy: 'Healthy',
        degraded: 'Degraded',
        down: 'Down',
      },
      noAlerts: 'No active alerts',
      acknowledge: 'Acknowledge',
      latency: 'Latency',
      lastCheck: 'Last check',
    },
  },
} as const

// Convert all string literal types to string for translation flexibility
type DeepStringify<T> = {
  [K in keyof T]: T[K] extends string ? string : DeepStringify<T[K]>
}

export type TranslationKeys = DeepStringify<typeof en>
