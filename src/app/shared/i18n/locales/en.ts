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
        success: 'A managed workspace or local computer is available for agent work.',
        open: 'Choose work location',
        review: 'Review work location',
      },
      provider: {
        title: 'Model or local access',
        empty: 'Add a model service or connect a local agent.',
        needsTest: 'Test the model service before assigning work.',
        cliReady: '{{name}} can run work from {{location}}.',
        why: 'Agents need either a tested model service or a connected local agent before they can do work.',
        success: 'A model service is tested, or a local agent is connected.',
        create: 'Add model service',
        connectCli: 'Connect local agent',
        test: 'Test model service',
        reviewProviders: 'Review model services',
        reviewAgents: 'Review agents',
      },
      agent: {
        title: 'Agent',
        empty: 'Create a text-only, managed-workspace, or local agent.',
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
    networkError: 'The browser could not reach the server. Check your connection, then try again.',
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
    loadError: 'Workshop could not load. Refresh after agents are available, then try again.',
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
      title: 'Agent Work Setup',
      description: 'Choose where agents work and check what still needs setup.',
      saving: 'Saving...',
      loading: 'Loading work setup...',
      couldNotLoad: 'Could not load work setup',
      defaultRuntimeLabel: 'Default work location',
      defaultRuntimeDescription: 'Where new agents should do work by default',
      defaultContainerCliLabel: 'Default local work tool',
      defaultContainerCliDescription: 'The tool used when an agent needs a local work tool',
      availableRuntimesLabel: 'Available work locations',
      availableRuntimesDescription: 'Places this installation can use for agent work',
      availableContainerClisLabel: 'Available local tools',
      availableContainerClisDescription: 'Tools this installation can use for agent work',
      runtimeLabels: {
        cli: 'This computer',
        api: 'Text-only model service',
        container: 'Managed workspace',
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
      noDescription: 'No summary yet. Review the instructions below before using this skill.',
      triggerHeading: 'When agents should consider it',
      triggerHelper: 'Agents can use this skill when the task matches this phrase.',
      detailsHeading: 'Instructions the agent will read',
      detailsHelper: 'Review this text if you need to check exactly what will be reused.',
      noContent:
        'No reusable instructions have been saved yet. Add instructions before asking agents to use this skill.',
      unknownAuthor: 'Unknown',
      unknownSource: 'Skills library',
      versionLatest: 'latest',
    },
  },

  // =========================================================================
  // Errors
  // =========================================================================
  errors: {
    generic:
      'Something went wrong. Try again, then ask an owner to check the system if it repeats.',
    network: 'The browser could not reach the server. Check your connection, then try again.',
    timeout: 'Request timed out. Please try again.',
    notFound: '{{resource}} was not found. Refresh the page, then try again.',
    unauthorized: 'Sign in again, then retry this action.',
    forbidden: 'You do not have access for this action. Ask an owner or admin to update your role.',
    validation: 'Check the highlighted fields, then try again.',
    serverError: 'The server had a problem. Wait a moment, then try again.',
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
          title: 'Restart the sidecar from your machine',
          detail:
            'The platform does not manage the local sidecar. Re-run the enrollment shell script on the operator machine.',
        },
        restart_api: {
          title: 'No container to restart',
          detail:
            'This agent calls the LLM provider directly. Send a new prompt to invoke the model again.',
        },
        start_host_cli: {
          title: 'Start the sidecar from your machine',
          detail:
            'Re-run the enrollment shell script on the operator machine to launch the sidecar.',
        },
        start_api: {
          title: 'No container to start',
          detail: 'Provider agents have no shell to start.',
        },
        stop_host_cli: {
          title: 'Stop the sidecar from your machine',
          detail:
            'The platform cannot stop a remote sidecar. Stop the process on the operator machine.',
        },
        stop_api: {
          title: 'No container to stop',
          detail: 'Provider agents have no shell to stop.',
        },
        not_permitted: {
          title: 'Operation not permitted on this agent',
          detail: 'You can manage only agents you own. Contact the agent owner if you need access.',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: 'Choose a CLI tool',
          detail:
            'Container-backed agents need a Container CLI: claude, codex, gemini, or opencode.',
        },
        api_cannot_have_cli_tool: {
          title: 'Provider agent cannot have a CLI tool',
          detail: 'Remove the CLI tool, or change the runtime to "Container (Docker)".',
        },
        missing_cli_tool_for_host_cli: {
          title: 'Choose a CLI tool',
          detail: 'Host CLI enrollment needs a Container CLI: claude, codex, gemini, or opencode.',
        },
      },
      enroll: {
        missing_idempotency_key: {
          title: 'Idempotency-Key header required',
          detail: 'Resend with a fresh UUID in the `Idempotency-Key` header.',
        },
        plaintext_nats_blocked: {
          title: 'Plaintext NATS not allowed for Host CLI',
          detail:
            'Configure `NATS_AGENT_URL` to use `tls://`, or set the org policy `allow_plaintext_host_nats=true` to permit it.',
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
      wsConnections: 'WS Connections',
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
