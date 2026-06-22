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
    noResults: 'Try a broader search or clear the filters.',
    noData: 'Create the first item, or open this page again after setup finishes.',
    optional: 'Optional',
    required: 'Fill this in',
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
    eyebrow: 'Setup checklist',
    title: 'Set up your first agent safely',
    description:
      'Follow one step at a time. Finish this checklist to create an agent, send work, and check the result.',
    skip: 'Skip and open Tasks',
    skipSaving: 'Skipping...',
    skipHint:
      'This only hides the setup checklist from the left menu. Your projects, agents, and tasks stay the same, and you can reset it from Settings.',
    skipError:
      'Check your connection, then choose Skip again. The setup checklist could not be hidden.',
    progressCount: '{{complete}} of {{total}}',
    nextTitle: 'Do this next',
    readyTitle: 'Ready to run work',
    readyDetail:
      'Write one small task from Tasks, or save useful steps when you want agents to repeat what worked.',
    readyCta: 'Write one small task',
    successLabel: 'Success looks like:',
    currentProject: 'Current project',
    noProject: 'Open project settings to create or choose a project.',
    projects: 'Projects',
    workLocations: {
      managed: 'the Project files option',
      local: 'this computer',
      textOnly: 'chat-only work',
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
        review: 'Check team and project',
      },
      runtime: {
        title: 'Work location',
        empty: 'Choose where agents should work: Project files or this computer.',
        ready: '{{location}} is ready for agent work.',
        why: 'Agents need a safe place to run before they can receive tasks.',
        success: 'At least one work location is ready for agent work.',
        open: 'Choose work location',
        review: 'Check work location',
      },
      provider: {
        title: 'Give agents a way to work',
        empty:
          'Choose one way to let agents work: add an AI service for chat answers, or open work tool sign-in for Codex before file work.',
        needsTest: 'Check the AI service before giving agents work.',
        cliReady: '{{name}} is ready to run work from {{location}}.',
        why: 'Agents need one ready option: a checked AI service for chat answers, or a signed-in work tool plus an agent for hands-on file work.',
        success:
          'One ready option exists: a checked AI service or an agent that can open file work.',
        create: 'Add AI service',
        signInTool: 'Open work tool sign-in',
        test: 'Check AI service',
        reviewProviders: 'Check AI services',
        reviewAgents: 'Open agents',
      },
      agent: {
        title: 'Agent',
        empty: 'Create one simple agent: chat-only, Project files, or this computer.',
        why: 'Agents receive tasks and return results. Start with one simple agent.',
        success: 'At least one agent appears in the Agents page.',
        create: 'Add agent',
        review: 'Open agents',
      },
      routing: {
        title: 'Where tasks wait',
        emptyWithProject: 'Set up where tasks wait for this project.',
        emptyWithoutProject: 'Select a project, then set up where tasks wait.',
        why: 'This gives new work a place to wait until the next available agent starts it.',
        success: 'A waiting place exists for the selected project.',
        create: 'Set up waiting place',
        review: 'Check waiting places',
      },
      task: {
        title: 'First task',
        emptyWithRouting:
          'Write one small task. Forge puts it where tasks wait until the next available agent starts it.',
        emptyWithoutRouting: 'Set up where tasks wait before the first task.',
        emptyWithoutProject:
          'Create or choose a project, then set up where tasks wait before the first task.',
        ready: '{{count}} task on the board.',
        why: 'A small first task proves the setup works before you depend on it for real work.',
        success: 'The task appears on the board, either waiting for an agent or already has one.',
        create: 'Write first task',
        open: 'Open board',
      },
      review: {
        title: 'Check the result',
        empty: 'After an agent starts a task, open it to see progress and results.',
        inFlight: 'A task has an agent. Check progress from the board.',
        ready: '{{count}} completed task ready to check.',
        why: 'Checking the result helps you decide whether the agent returned useful output you can trust.',
        success: 'A task has completed output or result files you can open.',
        open: 'Check work',
      },
      reuse: {
        title: 'Save useful steps',
        empty: 'After a completed task, choose helpful steps you can save for next time.',
        ready: 'Saved steps are available for future tasks.',
        why: 'Saved steps help agents repeat the parts that worked without you rewriting them.',
        success: 'Useful steps are saved or were used on a task.',
        review: 'Choose steps to save',
        open: 'Show saved steps',
      },
    },
  },

  // =========================================================================
  // Command Palette
  // =========================================================================
  commandPalette: {
    title: 'Find what you need',
    inputLabel: 'Search pages and things to do',
    placeholder: 'Search what you want to do, e.g. send work, add agent, sign in',
    discovery: {
      tasks: 'Write one small task when you want work done.',
      inbox: 'Check updates that need a person before you keep working.',
      settings: 'Fix setup blockers for agents, sign-ins, projects, and access.',
    },
    groups: {
      navigation: 'Go to a page',
      actions: 'Create or change something',
      views: 'Change task view',
    },
    empty: {
      title: 'No page or option matches that search',
      listSeparator: ', ',
      tryShorter: 'Try a shorter search, or open Settings to browse setup.',
      tryOne: 'Try {{label}} to open a page people use often.',
      tryMany: 'Try {{prefix}}, or {{last}} to open a page people use often.',
      commonPages: 'Common pages',
      openPage: 'Open {{label}}',
      showAll: 'Show all pages and actions',
    },
    commands: {
      nav: {
        start: {
          label: 'Setup checklist',
          description: 'Open setup steps again when you want a guided checklist.',
        },
        tasks: {
          label: 'Tasks',
          description: 'See work that is planned, active, or done.',
        },
        inbox: {
          label: 'Inbox',
          description: 'Check alerts that may need a person.',
        },
        context: {
          label: 'Saved items',
          description: 'Check saved notes and instructions before agents reuse them.',
        },
        agents: {
          label: 'Agents',
          description: 'Create or check agents that handle work.',
        },
        skills: {
          label: 'Saved instructions',
          description: 'Reuse instructions for repeated work.',
        },
        settings: {
          label: 'Settings',
          description: 'Connect tools, account access, teams, and projects.',
        },
      },
      actions: {
        createTask: {
          label: 'New task',
          description: 'Tell an agent the result you want and how to check it.',
        },
        workToolSignIns: {
          label: 'Codex sign-in',
          description: 'Sign in before agents edit files with Codex or another work tool.',
        },
        keys: {
          label: 'Outside tool access',
          description: 'Let trusted outside tools connect to Forge without a person signing in.',
        },
        gitCredentials: {
          label: 'HTTPS code access',
          description: 'Use this when a private code link starts with https://.',
        },
        sshKeys: {
          label: 'SSH code access',
          description: 'Use this when a private code link starts with git@.',
        },
        resources: {
          label: 'Agent size limits',
          description: 'Choose small, standard, or large limits before agents start file work.',
        },
        projects: {
          label: 'Projects',
          description: 'Create or choose where tasks, agents, and files belong.',
        },
        teams: {
          label: 'Teams',
          description: 'Create teams and manage who can change work.',
        },
        providers: {
          label: 'AI services',
          description: 'Connect the AI account agents use to answer.',
        },
        runtime: {
          label: 'Where agents work',
          description:
            'Choose Project files for the usual setup, or This computer for local-only work.',
        },
        account: {
          label: 'Account',
          description: 'Update profile, password, and reset the setup checklist.',
        },
        theme: {
          label: 'Change theme',
          description: 'Switch the app appearance.',
        },
        setupChecklistRecovery: {
          label: 'Reset setup checklist',
          description:
            'Show the setup checklist in the left menu again. Projects, agents, and tasks stay unchanged.',
        },
      },
      views: {
        board: {
          label: 'Board view',
          description: 'Move tasks through simple columns.',
        },
        list: {
          label: 'List view',
          description: 'Scan tasks in one sortable table.',
        },
        timeline: {
          label: 'Timeline view',
          description: 'See when work happened.',
        },
        visualMap: {
          label: 'Visual map',
          description: 'See agents and tasks on a visual map.',
        },
      },
    },
    taskSetup: {
      noProjectOptions: {
        label: 'Set up project before task',
        buttonLabel: 'Set up project',
        description: 'Open project settings so tasks have a place to belong.',
      },
      chooseProject: {
        label: 'Choose project for new task',
        buttonLabel: 'New task',
        description: 'Pick a project first, then write the task for an agent.',
      },
      noWaitingPlace: {
        label: 'Set up where tasks wait',
        buttonLabel: 'Set up waiting place',
        description: 'Open Agents to add a waiting place before creating a task.',
      },
      ready: {
        label: 'New task',
        buttonLabel: 'New task',
        description: 'Tell an agent the result you want and how to check it.',
      },
    },
  },

  // =========================================================================
  // App Layout
  // =========================================================================
  appLayout: {
    pages: {
      start: {
        title: 'Setup checklist',
        subtitle: 'Set up Forge and send your first task',
      },
      tasks: {
        title: 'Tasks',
        subtitle: 'Create tasks and follow agent progress',
      },
      inbox: {
        title: 'Inbox',
        subtitle: 'Check updates that need a next step',
      },
      savedItemHistory: {
        title: 'Saved item history',
        subtitle: 'See what was checked or reused',
      },
      savedItems: {
        title: 'Saved notes and instructions',
        subtitle: 'Check what agents may reuse later',
      },
      agents: {
        title: 'Agents',
        subtitle: 'Create and manage agents that handle tasks',
      },
      skills: {
        title: 'Saved instructions',
        subtitle: 'Instructions agents can follow again',
      },
      analytics: {
        title: 'Analytics',
        subtitle: 'See agent activity and results',
      },
      billing: {
        title: 'Billing',
        subtitle: 'Plan, payments, and invoices',
      },
      settings: {
        title: 'Settings',
        subtitle: 'Set up your account, AI services, and team',
      },
      admin: {
        title: 'Admin',
        subtitle: 'Check app health and manage people',
      },
      fallback: {
        title: 'Wisdoverse Forge',
      },
    },
    topBar: {
      openNavigation: 'Open navigation',
      search: 'Search',
      searchLabel: 'Search pages and things to do',
      switchToLight: 'Switch to light mode',
      switchToDark: 'Switch to dark mode',
      views: {
        board: 'Board',
        list: 'List',
        timeline: 'Timeline',
        map: 'Map',
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
    newAgent: 'New agent',
    createAgent: 'Add agent',
    editAgent: 'Edit Agent',
    deleteAgent: 'Delete Agent',
    noAgents: 'Create one agent before sending work.',
    agentName: 'Agent Name',
    projectPath: 'Project folder location',
    workingDirectory: 'Work folder',
    startAgent: 'Start Agent',
    stopAgent: 'Stop Agent',
    restartAgent: 'Restart Agent',
    duplicateAgent: 'Duplicate Agent',
    exportAgent: 'Export Agent',
    importAgent: 'Import Agent',
    agentDetails: 'Agent overview',
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
    confirmDelete: 'Delete this agent? This removes its setup and stops sending new work to it.',
    confirmStop: 'Stop this agent? Current work pauses until you start it again.',
    // New agent modal
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
    maxAgentsReached:
      'Stop or delete an unused agent, then try again. You already have the allowed number of agents.',
    invalidProjectPath: 'Enter the project folder location, then try again.',
  },

  // =========================================================================
  // Where tasks wait
  // =========================================================================
  groups: {
    title: 'Where tasks wait',
    newGroup: 'New waiting place',
    createGroup: 'Create waiting place',
    editGroup: 'Edit waiting place',
    deleteGroup: 'Delete waiting place',
    noGroups: 'Create a waiting place so new tasks have a place to wait for agents.',
    groupName: 'Waiting place name',
    groupColor: 'Waiting place color',
    addToGroup: 'Add to waiting place',
    removeFromGroup: 'Remove from waiting place',
    moveToGroup: 'Move to waiting place',
    ungrouped: 'Set a waiting place before sending',
    confirmDelete:
      'Delete this waiting place? Agents stay available, but tasks need another waiting place before they can be sent.',
    groupCreated: 'Waiting place created',
    groupDeleted: 'Waiting place deleted',
    groupUpdated: 'Waiting place updated',
  },

  // =========================================================================
  // Activity Feed
  // =========================================================================
  feed: {
    title: 'Activity',
    noActivity: 'Start a task, then updates will appear here.',
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
    viewDetails: 'View update',
    timestamp: '{{time}}',
  },

  // =========================================================================
  // Prompt Input
  // =========================================================================
  prompt: {
    placeholder: 'Type one message for the agent...',
    placeholderShort: 'Type a message...',
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
    emptyPrompt: 'Type a message before sending.',
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
    loading: 'Checking visual map...',
    loadError: 'Open Agents, wait until one shows Ready, then open Visual map again.',
    controls: {
      zoom: 'Use the agent list to find an agent',
      pan: 'Select a robot in the map',
      rotate: 'The map updates on its own',
      select: 'Choose an agent from the list or map',
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
      title: 'Where agents work',
      description:
        'Choose where agents can open files, then check tools and sign-ins before sending tasks.',
      saving: 'Saving...',
      loading: 'Checking where agents can work...',
      couldNotLoad:
        'Open Settings, then open Where agents work. If it still does not load, ask an owner or admin to check Where agents work in Settings.',
      defaultRuntimeLabel: 'Where project files open',
      defaultRuntimeDescription:
        'Choose Project files for the simplest shared file work. Choose This computer only when this machine should join as an agent that Forge can manage here.',
      defaultContainerCliLabel: 'Default tool for project work',
      defaultContainerCliDescription:
        'Claude Code, Codex, Gemini, or OpenCode when an agent edits files or runs commands',
      availableRuntimesLabel: 'Places that can open project files',
      availableRuntimesDescription: 'Places where agents can open project files',
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
      statusNeedsInstall: 'Needs setup before use',
      cliFit: 'Best with {{tool}}',
      unknownToolFit: 'Check work tool in Settings',
      allAgentsFit: 'Works with any agent',
      allAgentsTooltip: 'No specific work tool is required.',
      containerCliTooltip: 'Work tool: {{tool}}',
      unknownToolTooltip: 'Open Settings, check the work tool, then use this saved instruction.',
      nextStepHeading: 'What to do next',
      nextStepReady:
        'Use this saved instruction when creating a task, or rely on its matching words to suggest it for similar work.',
      nextStepNeedsInstall:
        'Ask an owner or admin to finish setup, then use this saved instruction in a task.',
      sourceLabel: 'Where it came from',
      authorLabel: 'Updated by',
      availabilityLabel: 'Available to',
      descriptionHeading: 'What this helps with',
      noDescription: 'Check the reusable instructions below before using this saved instruction.',
      triggerHeading: 'When this helps',
      triggerHelper: 'Use this saved instruction for tasks that include words like these.',
      detailsHeading: 'Reusable instructions',
      detailsHelper: 'Read these reusable steps before using this saved instruction.',
      noContent:
        'No reusable steps are saved yet. Add the steps agents should follow before using this saved instruction.',
      unknownAuthor: 'Open Saved instructions again to show who keeps this updated',
      unknownSource: 'Saved instructions',
      availabilityWorkspace: 'This team space',
      availabilityGlobal: 'Saved instructions',
      availabilityProject: 'This project',
      availabilityLatest: 'Latest saved copy',
      availabilityNeedsReview: 'Check saved instruction access',
    },
  },

  // =========================================================================
  // Errors
  // =========================================================================
  errors: {
    generic: 'Try again after a moment. If it repeats, ask an owner to check app health.',
    network: 'Check your connection, then try again. Forge could not connect.',
    timeout: 'Wait a moment, then try again. The request took too long.',
    notFound: 'Open this page again, then try again. {{resource}} was not found.',
    unauthorized: 'Sign in again, then retry this action.',
    forbidden:
      'You do not have access for this action. Ask an owner or admin to check your team space access.',
    validation: 'Check the highlighted fields, then try again.',
    serverError: 'Wait a moment, then try again. Forge could not finish this right now.',
    connectionLost:
      'Forge is trying to reconnect. Keep this page open; refresh only if updates do not return.',
    reconnecting: 'Still reconnecting. Keep this page open.',
    reconnected: 'Live updates are back. New progress will appear here again.',
    agentError:
      'Try this step again, then check the agent status if it repeats. The agent could not finish this step.',
    fileError: 'Check the file, then try again. Forge could not handle it.',
    uploadError: 'Check the file and connection, then upload again. The upload did not finish.',
    downloadError: 'Open this page again, then download again. The download did not start.',
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
          detail: 'Forge cannot stop it for you. Close Terminal or PowerShell on that computer.',
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
            'Paste the setup text on this computer again. If this repeats, ask an owner to check Where agents work for this computer.',
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
    required: 'Fill in this field, then try again',
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
  // Admin
  // =========================================================================
  admin: {
    title: 'Admin',
    tabs: {
      agents: 'Agents',
      metrics: 'Activity',
      users: 'People',
      health: 'App health',
    },
    agents: {
      title: 'Agents',
      search: 'Search agents',
      status: 'Can take work',
      actions: 'What you can do',
      noAgents:
        'No agents match this view. Clear search or check whether agents have been created.',
      pause: 'Pause',
      resume: 'Resume',
      stop: 'Stop',
      delete: 'Delete',
    },
    metrics: {
      title: 'Activity and capacity',
      activeAgents: 'Agents working now',
      totalEvents: 'Work updates',
      eventsPerMinute: 'Updates each minute',
      memoryUsage: 'Memory in use',
      cpuUsage: 'Processor in use',
      uptime: 'Time running',
      wsConnections: 'Open browser views',
      requestsPerMinute: 'Requests each minute',
    },
    users: {
      title: 'People with access',
      search: 'Search people',
      addUser: 'Invite person',
      editUser: 'Change access',
      deleteUser: 'Remove access',
      noUsers: 'No people match this view. Clear search or invite someone first.',
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
      confirmDelete: 'Remove this person? They will lose access to this team space.',
    },
    health: {
      title: 'App health',
      overall: 'Overall app status',
      services: 'App areas',
      alerts: 'Items to check',
      status: {
        healthy: 'Working normally',
        degraded: 'Check this area',
        down: 'Not working',
      },
      noAlerts: 'All app areas are working.',
      acknowledge: 'Mark checked',
      latency: 'Response time',
      lastCheck: 'Last checked',
    },
  },
} as const

// Convert all string literal types to string for translation flexibility
type DeepStringify<T> = {
  [K in keyof T]: T[K] extends string ? string : DeepStringify<T[K]>
}

export type TranslationKeys = DeepStringify<typeof en>
