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
    error: 'Check the message, then try again.',
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
    start: 'Setup checklist',
    dashboard: 'Dashboard',
    tasks: 'Tasks',
    inbox: 'Inbox',
    context: 'Saved items',
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
    title: 'Set up your first agent safely',
    description:
      'Follow one step at a time. Finish this checklist to create an agent, send work, and review the result.',
    skip: 'Skip and open Tasks',
    skipSaving: 'Skipping...',
    skipHint:
      'This only hides Start from the left menu. Your projects, agents, and tasks stay the same, and you can show it again from Settings.',
    skipError:
      'Check your connection, then choose Skip again. The setup checklist could not be hidden.',
    progressCount: '{{complete}} of {{total}}',
    nextTitle: 'Do this next',
    readyTitle: 'Ready to run work',
    readyDetail:
      'Write one small task from Tasks, or review saved instructions when you want agents to repeat what worked.',
    readyCta: 'Write one small task',
    successLabel: 'Success looks like:',
    currentProject: 'Current project',
    noProject: 'Open project settings to create or choose a project.',
    projects: 'Projects',
    workLocations: {
      managed: 'the Project files option',
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
        title: 'Team and project',
        empty: 'Create one team and project so tasks have a clear home.',
        why: 'A project gives tasks a clear home so agents know what work belongs together.',
        success: 'A team and project exist, and the project is selected.',
        create: 'Create team and project',
        review: 'Review team and project',
      },
      runtime: {
        title: 'Work location',
        empty: 'Choose where agents should work: Project files or this computer.',
        ready: '{{location}} is ready for agent work.',
        why: 'Agents need a safe place to run before they can receive tasks.',
        success: 'At least one work location is ready for agent work.',
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
        empty: 'Create one simple agent: chat-only, Project files, or this computer.',
        why: 'Agents receive tasks and return results. Start with one simple agent.',
        success: 'At least one agent appears in the Agents page.',
        create: 'Create agent',
        review: 'Open agents',
      },
      routing: {
        title: 'Task queue',
        emptyWithProject: 'Create a task queue for this project.',
        emptyWithoutProject: 'Select a project, then create a task queue.',
        why: 'A task queue gives new work a place to wait for the next available agent.',
        success: 'A task queue exists for the selected project.',
        create: 'Create task queue',
        review: 'Review task queues',
      },
      task: {
        title: 'First task',
        emptyWithRouting:
          'Write one small task. Forge adds it to the queue so the next available agent can pick it up.',
        emptyWithoutRouting: 'Create a task queue before the first task.',
        ready: '{{count}} task on the board.',
        why: 'A small first task proves the setup works before you depend on it for real work.',
        success:
          'The task appears on the board, either waiting in the queue or assigned to an agent.',
        create: 'Write first task',
        open: 'Open board',
      },
      review: {
        title: 'Review output',
        empty: 'Assigned task output will appear in the detail panel.',
        inFlight: 'A task is assigned. Review progress from the board.',
        ready: '{{count}} completed task ready for acceptance.',
        why: 'Reviewing the result confirms the agent returned useful output you can trust.',
        success: 'A task has completed output or result files you can open.',
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
    passwordTooShort: 'Use at least {{min}} characters for the password.',
    passwordMismatch: 'Enter the same password in both fields.',
    emailInvalid: 'Enter a valid email address.',
    emailInUse: 'Use a different email, or sign in and reset the password if this is yours.',
    usernameInUse: 'Choose a different username; this one is already taken.',
    emailDomainRestricted: 'Use an approved work email, or ask an owner for an invite.',
    passwordRequirements: '12+ characters, uppercase, lowercase, number, special character',
    passwordWeak: 'Weak',
    passwordFair: 'Fair',
    passwordGood: 'Good',
    passwordStrong: 'Strong',
    createAccount: 'Create Account',
    fillAllFields: 'Fill in every field, then try again.',
    fillRequiredFields: 'Fill in the required fields, then try again.',
    networkError:
      'Check your connection, then try signing in again. Forge could not reach sign-in.',
  },

  // =========================================================================
  // Agents
  // =========================================================================
  agents: {
    title: 'Agents',
    newAgent: 'Create Agent',
    createAgent: 'Create Agent',
    editAgent: 'Edit Agent',
    deleteAgent: 'Delete Agent',
    noAgents: 'No agents yet. Create one agent to start assigning work.',
    agentName: 'Agent Name',
    projectPath: 'Project folder location',
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
      idle: 'Ready',
      working: 'Working now',
      waiting: 'Needs input',
      offline: 'Not connected',
      starting: 'Starting work...',
      stopping: 'Stopping work...',
      error: 'Check agent status',
      connecting: 'Connecting...',
    },
    confirmDelete: 'Delete this agent? This removes its setup and stops assigning new work to it.',
    confirmStop: 'Stop this agent? Current work pauses until you start it again.',
    // Create Agent modal
    startNewAgent: 'Start a new agent',
    pickProject: 'Pick a project to begin',
    tellClaude: 'Tell Claude what to work on',
    searchProjects: 'Search projects or enter a folder location...',
    enterFolderPath: 'Enter the project folder location...',
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
    maxAgentsReached: 'Agent limit reached. Stop or delete an unused agent, then try again.',
    invalidProjectPath: 'Enter the project folder location, then try again.',
  },

  // =========================================================================
  // Task queues
  // =========================================================================
  groups: {
    title: 'Task queues',
    newGroup: 'New task queue',
    createGroup: 'Create task queue',
    editGroup: 'Edit task queue',
    deleteGroup: 'Delete task queue',
    noGroups: 'No task queues yet. Create one so new tasks have a place to wait for agents.',
    groupName: 'Task queue name',
    groupColor: 'Task queue color',
    addToGroup: 'Add to task queue',
    removeFromGroup: 'Remove from task queue',
    moveToGroup: 'Move to task queue',
    ungrouped: 'No task queue yet',
    confirmDelete:
      'Delete this task queue? Agents stay available, but tasks will need another task queue before they can be sent.',
    groupCreated: 'Task queue created',
    groupDeleted: 'Task queue deleted',
    groupUpdated: 'Task queue updated',
  },

  // =========================================================================
  // Activity Feed
  // =========================================================================
  feed: {
    title: 'Activity',
    noActivity: 'No activity yet. Start a task, then updates will appear here.',
    clearActivity: 'Clear Activity',
    filterByType: 'Filter by type',
    filterByAgent: 'Filter by agent',
    showAll: 'Show all',
    eventTypes: {
      tool_use: 'Agent used a tool',
      tool_result: 'Tool finished',
      text: 'Agent message',
      error: 'Check update',
      thinking: 'Planning next step',
      system: 'System update',
    },
    tools: {
      Read: 'Opened a file',
      Write: 'Created a file',
      Edit: 'Changed a file',
      Bash: 'Ran a command',
      Glob: 'Found files',
      Grep: 'Searched file text',
      WebFetch: 'Opened a web page',
      WebSearch: 'Searched the web',
      Task: 'Asked another agent',
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
    noAgentSelected: 'Choose an agent before sending work.',
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
    resetConfirm: 'Reset all settings? This restores defaults and replaces your current choices.',
    runtime: {
      title: 'Agent work setup',
      description:
        'Choose where hands-on agents work, then check tools and sign-ins before assigning tasks.',
      saving: 'Saving...',
      loading: 'Loading work setup...',
      couldNotLoad:
        'Refresh this settings page to load Agent work setup. If it still does not load, ask an owner or admin to check Agent work setup in Settings.',
      defaultRuntimeLabel: 'Default file work place',
      defaultRuntimeDescription:
        'Choose Project files for the simplest shared file work. Choose This computer only when this machine should join as an agent.',
      defaultContainerCliLabel: 'Default tool for project work',
      defaultContainerCliDescription:
        'Claude Code, Codex, Gemini, or OpenCode when an agent edits files or runs commands',
      availableRuntimesLabel: 'Places agents can edit files',
      availableRuntimesDescription: 'Where this setup can open files for hands-on agents',
      availableContainerClisLabel: 'Work tools agents can use',
      availableContainerClisDescription: 'Installed tools for file edits, commands, and live work',
      runtimeLabels: {
        cli: 'This computer',
        api: 'Chat-only AI service',
        container: 'Project files',
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
      unknownToolFit: 'Check this work tool before using',
      allAgentsFit: 'Works with any agent',
      allAgentsTooltip: 'No specific work tool is required.',
      containerCliTooltip: 'Work tool: {{tool}}',
      unknownToolTooltip:
        'Open Settings and check the work tool before using this saved instruction.',
      nextStepHeading: 'What to do next',
      nextStepReady:
        'Use this saved instruction when creating a task, or rely on its matching words to suggest it for similar work.',
      nextStepNeedsInstall:
        'Ask an owner or admin to install it before expecting agents to use it in tasks.',
      sourceLabel: 'Where it came from',
      authorLabel: 'Maintainer',
      availabilityLabel: 'Available to',
      descriptionHeading: 'What this helps with',
      noDescription: 'Check the reusable instructions below before using this saved instruction.',
      triggerHeading: 'When this helps',
      triggerHelper:
        'When a task uses words like these, agents know this saved instruction may help.',
      detailsHeading: 'Reusable instructions',
      detailsHelper:
        'Review this text to understand what the saved instruction adds to agent work.',
      noContent:
        'No reusable instructions have been saved yet. Add instructions before asking agents to use this saved instruction.',
      unknownAuthor: 'Refresh saved instructions to load maintainer',
      unknownSource: 'Saved instructions library',
      availabilityWorkspace: 'This team space',
      availabilityGlobal: 'Saved instructions library',
      availabilityProject: 'This project',
      availabilityLatest: 'Latest saved copy',
      availabilityNeedsReview: 'Check saved instruction access',
    },
  },

  // =========================================================================
  // Errors
  // =========================================================================
  errors: {
    generic: 'Try again. If it repeats, ask an owner to check the system.',
    network: 'Check your connection, then try again. Forge could not connect.',
    timeout: 'Wait a moment, then try again. The request took too long.',
    notFound: 'Refresh the page, then try again. {{resource}} was not found.',
    unauthorized: 'Sign in again, then retry this action.',
    forbidden:
      'You do not have access for this action. Ask an owner or admin to check your team space access.',
    validation: 'Check the highlighted fields, then try again.',
    serverError: 'Wait a moment, then try again. Forge could not finish this right now.',
    connectionLost: 'Connection lost. Reconnecting...',
    reconnecting: 'Reconnecting...',
    reconnected: 'Connection restored',
    agentError:
      'Try this step again, then check the agent status if it repeats. The agent could not finish this step.',
    fileError: 'Check the file, then try again. Forge could not handle it.',
    uploadError: 'Check the file and connection, then upload again. The upload did not finish.',
    downloadError: 'Refresh the page, then download again. The download did not start.',
    rateLimited:
      'Wait {{seconds}} seconds, then try again. Too many requests are happening right now.',
    quotaExceeded:
      'Ask an owner to raise the limit or free capacity. {{resource}} quota is used up.',
    agent: {
      lifecycle: {
        restart_host_cli: {
          title: 'Restart the connection helper on your computer',
          detail: 'Forge cannot restart it for you. Paste the setup text on that computer again.',
        },
        restart_api: {
          title: 'Send the message again instead of restarting',
          detail:
            'This chat-only agent replies through an AI service. Send the message again to try a fresh reply.',
        },
        start_host_cli: {
          title: 'Start the connection helper on your computer',
          detail: 'Paste the setup text on that computer again to bring the agent online.',
        },
        start_api: {
          title: 'Send a message to start this chat-only agent',
          detail:
            'Chat-only agents start work when you send a message. There is no file work area to start.',
        },
        stop_host_cli: {
          title: 'Stop the connection helper on your computer',
          detail: "Forge cannot stop it for you. Close that computer's command app.",
        },
        stop_api: {
          title: 'Close the chat or wait for the reply to finish',
          detail:
            'Chat-only agents have no file work area to stop. Send a new message when you need more help.',
        },
        not_permitted: {
          title: 'You cannot manage this agent',
          detail: 'You can manage only agents you own. Contact the agent owner if you need access.',
        },
      },
      create: {
        missing_cli_tool_for_container: {
          title: 'Choose a work tool',
          detail:
            'Agents that edit project files need a work tool: Claude Code, Codex, Gemini, or OpenCode.',
        },
        api_cannot_have_cli_tool: {
          title: 'Chat-only agent cannot have a work tool',
          detail: 'Remove the work tool, or change the work location to "Project files".',
        },
        missing_cli_tool_for_host_cli: {
          title: 'Choose a work tool',
          detail:
            'Agents joined from this computer need a work tool: Claude Code, Codex, Gemini, or OpenCode.',
        },
      },
      enroll: {
        missing_idempotency_key: {
          title: 'Setup text needs to be pasted again',
          detail:
            'Paste the setup text on this computer again. If this repeats, ask an owner to check Agent work setup for this computer.',
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
    delete: 'Delete this item? It will be removed from this team space.',
    unsavedChanges: 'Leave without saving? Unsaved changes will be lost.',
    logout: 'Sign out now? Unsaved work in open forms may be lost.',
    reset: 'Reset this? Current changes will be replaced by defaults.',
    stop: 'Stop this operation? Current progress may pause and need to be started again.',
    discard: 'Discard changes? Your edits will be lost.',
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
    uploadFailed: 'Check the file, then upload again. The upload did not finish.',
    tooLarge: 'Choose a file under {{size}}, then upload it again.',
    invalidType: 'Choose a file with one of these types: {{types}}.',
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
    error: 'Check the message, then try again.',
    success: 'Operation successful',
    required: 'This field is required',
    invalid: 'Check this field, then try again',
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
      role: 'Access level',
      roles: {
        admin: 'Admin',
        operator: 'Member',
        viewer: 'Viewer',
      },
      status: {
        active: 'Active',
        inactive: 'Inactive',
        suspended: 'Suspended',
      },
      confirmDelete: 'Delete this user? They will lose access to this team space.',
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
