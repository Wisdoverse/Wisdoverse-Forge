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
    noResults: 'No matching results. Try a broader search or clear the filters.',
    noData: 'Nothing to show yet. Create the first item or refresh after setup finishes.',
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
    skills: 'Saved instructions',
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
    skip: 'Skip the guide',
    progressCount: '{{complete}} of {{total}}',
    nextTitle: 'Do this next',
    readyTitle: 'Ready to run work',
    readyDetail:
      'The basic path is complete. You can create more tasks or review saved instructions.',
    successLabel: 'Success looks like:',
    currentProject: 'Current project',
    noProject: 'No project selected',
    projects: 'Projects',
    workLocations: {
      managed: 'a managed workspace',
      local: 'this computer',
      textOnly: 'text-only work',
      ready: 'a work location',
    },
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
        title: 'Work location',
        empty: 'Choose where agents should do work: in a managed workspace or on this computer.',
        ready: '{{location}} is ready for agent work.',
        why: 'Agents need a safe place to run before they can receive tasks.',
        success: 'A managed workspace or this computer is available for agent work.',
        open: 'Choose work location',
        review: 'Review work location',
      },
      provider: {
        title: 'Give agents a way to work',
        empty:
          'Choose one way to let agents work: add an AI service, or join this computer as an agent.',
        needsTest: 'Check the AI service before giving agents work.',
        cliReady: '{{name}} is ready to run work from {{location}}.',
        why: 'Agents need one ready option: a checked AI service for chat answers, or an agent joined from this computer for hands-on work.',
        success:
          'One ready option exists: a checked AI service or an agent joined from this computer.',
        create: 'Add AI service',
        connectCli: 'Join this computer',
        test: 'Check AI service',
        reviewProviders: 'Review AI services',
        reviewAgents: 'Review agents',
      },
      agent: {
        title: 'Agent',
        empty: 'Create one simple agent: text-only, managed workspace, or this computer.',
        why: 'Agents receive tasks and return results. Start with one simple agent.',
        success: 'At least one agent appears in the Agents page.',
        create: 'Create agent',
        review: 'Open agents',
      },
      routing: {
        title: 'Task queue',
        emptyWithProject: 'Create a task queue for this project.',
        emptyWithoutProject: 'Select a project, then create a task queue.',
        why: 'A task queue is the place new work waits until an agent is ready to pick it up.',
        success: 'A task queue exists for the selected project.',
        create: 'Create task queue',
        review: 'Review task queues',
      },
      task: {
        title: 'First task',
        emptyWithRouting: 'Create a task, assign it, and watch the run start.',
        emptyWithoutRouting: 'Create a task queue before the first task.',
        ready: '{{count}} task on the board.',
        why: 'A small first task proves the setup works before you depend on it for real work.',
        success: 'A task appears on the board and is assigned or waiting to start.',
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
        title: 'Reuse what worked',
        empty: 'After a completed task, review useful instructions you can save for next time.',
        ready: 'Saved instructions are available for future tasks.',
        why: 'Saved instructions help agents repeat the parts that worked without you rewriting them.',
        success: 'Useful instructions are saved or were used on a task.',
        review: 'Review what to save',
        open: 'Show saved instructions',
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
    loginSuccess: 'You are signed in.',
    logoutSuccess: 'You are signed out.',
    registerSuccess: 'Your account is ready. You can sign in now.',
    invalidCredentials: 'Check your email and password, then try again.',
    accountLocked:
      'This account is temporarily locked. Wait a few minutes, then try again or ask an owner or admin for help.',
    agentExpired: 'Your sign-in expired. Sign in again to continue.',
    passwordResetSent: 'Check your email for the password reset link.',
    passwordChanged: 'Your password was updated. Use it the next time you sign in.',
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
    fillAllFields: 'Fill in every field, then try again.',
    fillRequiredFields: 'Fill in the required fields, then try again.',
    networkError:
      'Forge could not connect while signing in. Check your connection, then try again.',
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
    noAgents: 'No agents yet. Create one agent to start assigning work.',
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
    placeholder: 'Type one instruction for the agent...',
    placeholderShort: 'Type an instruction...',
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
    emptyPrompt: 'Type an instruction before sending.',
    selectAgent: 'Choose an agent first.',
    noAgentSelected: 'No agent selected',
    multipleAgentsSelected: '{{count}} agents selected',
    shortcuts: {
      send: 'Press Enter to send',
      newLine: 'Press Shift+Enter for new line',
      history: 'Press ↑ to recall history',
    },
  },

  // =========================================================================
  // Visual map
  // =========================================================================
  workshop: {
    title: 'Visual map',
    loading: 'Loading visual map...',
    loadError: 'Visual map could not load. Refresh after agents are available, then try again.',
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
      drawMode: 'Press D to add drawing notes',
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
      title: 'Agent Work Setup',
      description:
        'Choose where hands-on agents work, then check tools and sign-ins before assigning tasks.',
      saving: 'Saving...',
      loading: 'Loading work setup...',
      couldNotLoad: 'Could not load work setup',
      defaultRuntimeLabel: 'Default agent location',
      defaultRuntimeDescription:
        'Choose Managed workspace unless an owner tells you agents should run on this computer',
      defaultContainerCliLabel: 'Default tool for project work',
      defaultContainerCliDescription:
        'Claude Code, Codex, Gemini, or OpenCode when an agent edits files or runs commands',
      availableRuntimesLabel: 'Agent locations available',
      availableRuntimesDescription: 'Where this installation can run hands-on agents',
      availableContainerClisLabel: 'Work tools agents can use',
      availableContainerClisDescription: 'Installed tools for file edits, commands, and live work',
      runtimeLabels: {
        cli: 'This computer',
        api: 'Chat-only AI service',
        container: 'Managed workspace',
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
      allAgentsTooltip: 'No specific work tool is required.',
      containerCliTooltip: 'Work tool: {{tool}}',
      sourceLabel: 'Where it came from',
      authorLabel: 'Maintainer',
      availabilityLabel: 'Available to',
      descriptionHeading: 'What this helps with',
      noDescription:
        'No summary yet. Review the instructions below before using this saved instruction.',
      triggerHeading: 'When this helps',
      triggerHelper:
        'When a task uses words like these, agents know this saved instruction may help.',
      detailsHeading: 'Reusable instructions',
      detailsHelper:
        'Review this text to understand what the saved instruction adds to agent work.',
      noContent:
        'No reusable instructions have been saved yet. Add instructions before asking agents to use this saved instruction.',
      unknownAuthor: 'Maintainer not listed yet',
      unknownSource: 'Saved instructions library',
      availabilityWorkspace: 'This workspace',
      availabilityGlobal: 'Saved instructions library',
      availabilityProject: 'This project',
      availabilityLatest: 'Latest saved copy',
    },
  },

  // =========================================================================
  // Errors
  // =========================================================================
  errors: {
    generic:
      'Something went wrong. Try again, then ask an owner to check the system if it repeats.',
    network: 'Forge could not connect. Check your connection, then try again.',
    timeout: 'Request timed out. Please try again.',
    notFound: '{{resource}} was not found. Refresh the page, then try again.',
    unauthorized: 'Sign in again, then retry this action.',
    forbidden: 'You do not have access for this action. Ask an owner or admin to update your role.',
    validation: 'Check the highlighted fields, then try again.',
    serverError: 'Forge could not finish this right now. Wait a moment, then try again.',
    connectionLost: 'Connection lost. Reconnecting...',
    reconnecting: 'Reconnecting...',
    reconnected: 'Connection restored',
    agentError:
      'The agent could not finish this step. Try again, then check the agent status if it repeats.',
    fileError: 'The file could not be handled. Check the file, then try again.',
    uploadError: 'The upload did not finish. Check the file and connection, then try again.',
    downloadError: 'The download did not start. Refresh the page, then try again.',
    rateLimited: 'Too many requests. Please wait {{seconds}} seconds.',
    quotaExceeded:
      '{{resource}} quota is used up. Ask an owner to raise the limit or free capacity.',
    agent: {
      lifecycle: {
        restart_host_cli: {
          title: 'Restart the connection helper on your computer',
          detail: 'Forge cannot restart it for you. Run the setup command on that computer again.',
        },
        restart_api: {
          title: 'No workspace to restart',
          detail:
            'This chat-only agent replies through an AI service. Send a new message to try again.',
        },
        start_host_cli: {
          title: 'Start the connection helper on your computer',
          detail: 'Run the setup command on that computer again to bring the agent online.',
        },
        start_api: {
          title: 'No workspace to start',
          detail: 'Chat-only agents do not have live work to start.',
        },
        stop_host_cli: {
          title: 'Stop the connection helper on your computer',
          detail:
            'Forge cannot stop it for you. Close the Terminal or PowerShell window on that computer.',
        },
        stop_api: {
          title: 'No workspace to stop',
          detail: 'Chat-only agents do not have live work to stop.',
        },
        not_permitted: {
          title: 'Operation not permitted on this agent',
          detail: 'You can manage only agents you own. Contact the agent owner if you need access.',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: 'Choose a work tool',
          detail: 'Managed workspace agents need a work tool: claude, codex, gemini, or opencode.',
        },
        api_cannot_have_cli_tool: {
          title: 'Chat-only agent cannot have a work tool',
          detail: 'Remove the work tool, or change the work location to "Managed workspace".',
        },
        missing_cli_tool_for_host_cli: {
          title: 'Choose a work tool',
          detail:
            'Agents joined from this computer need a work tool: claude, codex, gemini, or opencode.',
        },
      },
      enroll: {
        missing_idempotency_key: {
          title: 'Setup command needs to be run again',
          detail:
            'Run the setup command on this computer again. If this repeats, ask an owner to check Agent Work Setup for this computer.',
        },
        plaintext_nats_blocked: {
          title: 'Secure connection required for this computer',
          detail:
            'Use the secure connection address for agents joined from this computer. If you are not sure what to enter, ask an owner to check this computer agent connection settings.',
        },
      },
    },
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
      noAgents:
        'No agents match this view. Clear search or check whether agents have been created.',
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
      wsConnections: 'Live browser connections',
      requestsPerMinute: 'Requests/min',
    },
    users: {
      title: 'User Management',
      search: 'Search users...',
      addUser: 'Add User',
      editUser: 'Edit User',
      deleteUser: 'Delete User',
      noUsers: 'No users match this view. Clear search or invite a user first.',
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
