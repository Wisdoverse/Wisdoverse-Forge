# Frontend Complexity Audit - 2026-06-23

## Verdict

Yes. The frontend is still too complex for first-time and non-technical users.
The issue is not that the product has too many capabilities; it is that several
first screens expose setup education, operational state, filters, and advanced
configuration at the same visual weight.

The strongest screens are the ones with one obvious next action. The overloaded
screens mix "what is this?", "what should I do now?", and "advanced operators can
tune this" in the same viewport.

## Evidence Limits

- Captured on desktop at 1440 x 960 with a local Playwright fallback.
- Frontend ran from local Vite. API responses were mocked.
- The Rust backend, WebSocket path, live auth, and real agent runtime were not
  validated in this audit.
- Browser screenshots are visual evidence only; they do not prove keyboard,
  screen-reader, or full responsive accessibility behavior.

## Screenshot Set

| Screen                 | Screenshot                                                             | Assessment                                                                  |
| ---------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Tasks board            | [01-tasks-board.png](screenshots/01-tasks-board.png)                   | Usable, but dense before the user reaches tasks.                            |
| Agents list            | [02-agents-list.png](screenshots/02-agents-list.png)                   | Overloaded; mixes agent choice, filters, task queues, and local enrollment. |
| Create agent           | [03-create-agent-modal.png](screenshots/03-create-agent-modal.png)     | Correct but too tall and repetitive for a first agent.                      |
| Settings / AI services | [04-settings-ai-services.png](screenshots/04-settings-ai-services.png) | Main panel is understandable; settings navigation is too broad.             |
| Settings / Projects    | [05-settings-projects.png](screenshots/05-settings-projects.png)       | Simple content feels heavier because the settings sidebar dominates.        |
| Getting started        | [06-getting-started.png](screenshots/06-getting-started.png)           | Good intent, but it behaves like a dashboard instead of a guided wizard.    |

## P1 Findings

### 1. Settings exposes advanced setup too early

The settings sidebar shows eleven destinations across four groups, including
advanced concepts such as HTTPS code access, SSH code access, tool access keys,
and agent size limits. For a beginner, this makes even simple setup feel like an
admin console.

Source evidence:

- `src/app/pages/settings/ui/SettingsLayout.tsx:44` defines all settings
  sections in one visible navigation model.
- `src/app/pages/settings/ui/SettingsLayout.tsx:124` exposes all four groups.
- `src/app/pages/settings/ui/SettingsLayout.tsx:235` renders the full desktop
  sidebar.

Recommendation:

- Keep `Start here` visible by default.
- Collapse `Access and limits` and `Product info` behind an `Advanced setup`
  disclosure.
- On mobile and desktop, make the first settings screen a three-step path:
  `AI service`, `Where agents work`, `Work tool sign-in`.
- Move HTTPS, SSH, access keys, and size limits into advanced setup unless the
  user arrives from an error state that needs one of them.

### 2. Agents page combines too many jobs

The Agents page currently asks users to understand agent location, browse and
filter agents, manage task queues, and enroll this computer from one screen. The
right rail is useful, but it competes with the primary job: choose or create an
agent.

Source evidence:

- `src/app/features/agents/AgentListView.tsx:128` uses a two-column layout with
  a 320px right rail.
- `src/app/features/agents/AgentListView.tsx:164` always shows the agent choice
  guide when the page is usable.
- `src/app/features/agents/AgentListView.tsx:279` adds task queues and local
  enrollment beside the agent list.
- `src/app/features/agents/AgentListView.tsx:751` defines a dense fleet control
  panel with search, status filters, work-location filters, and sort.

Recommendation:

- Default Agents to one primary flow: `Create or choose an agent`.
- Move task queues behind a `Task queues` tab, drawer, or setup card that only
  expands when the user needs routing.
- Move local enrollment into the create-agent flow or a secondary `Use this
computer` CTA.
- Hide fleet filters until there are enough agents to justify filtering.

### 3. Create Agent over-explains the runtime choice

The modal is doing useful teaching, but the same decision is explained in
multiple places: three cards, a conditional paragraph, a "Not sure?" paragraph,
a runtime-fit summary, readiness copy, and service/tool fields. This is correct
content with too much first-view weight.

Source evidence:

- `src/app/features/agents/CreateAgentModal.tsx:990` renders three large runtime
  choice cards.
- `src/app/features/agents/CreateAgentModal.tsx:1024` adds conditional runtime
  explanation.
- `src/app/features/agents/CreateAgentModal.tsx:1031` adds a second "Not sure?"
  explanation.
- `src/app/features/agents/CreateAgentModal.tsx:1038` adds the runtime fit
  summary.
- `src/app/features/agents/CreateAgentModal.tsx:1079` adds readiness copy.
- `src/app/features/agents/CreateAgentModal.tsx:1135` adds provider-specific
  fields for chat-only agents.

Recommendation:

- First view should show the three runtime choices plus one plain recommendation.
- After a choice, show only the fields needed for that choice.
- Put detailed "best for", file access, task support, and command support
  behind a `Why this option?` disclosure.
- Make `Simple chat agent` visibly non-task-capable before submit: "Chat only.
  Cannot take Tasks, edit files, or run commands."

## P2 Findings

### 4. Tasks board has too many controls before task cards

The board is operationally useful, but the top stack contains readiness, search,
priority filters, assignee filters, count state, display mode, and then the
kanban columns. A beginner who just wants to send work sees several controls
before the work itself.

Source evidence:

- `src/app/features/board/BoardView.tsx:363` renders the board with readiness
  first.
- `src/app/features/board/BoardView.tsx:373` renders the toolbar before columns.
- `src/app/features/board/BoardView.tsx:420` renders the columns only after the
  readiness and toolbar stack.
- `src/app/features/board/BoardToolbar.tsx:64` renders the toolbar as a full
  bordered section.
- `src/app/features/board/BoardToolbar.tsx:97` and
  `src/app/features/board/BoardToolbar.tsx:108` expose two filter groups.

Recommendation:

- Keep `New task` and board cards dominant.
- Collapse readiness to a compact banner when healthy.
- Hide priority and assignee filters behind one `Filters` button by default.
- Show guided explanations only when a task has no agent or when setup is
  incomplete.

### 5. Getting Started shows too many cards at once

The page has the right intent: it gives a next step and success criteria. The
problem is that it also renders all setup steps as full cards. Completed and
future steps compete with the active step, and long labels can truncate in the
card grid.

Source evidence:

- `src/app/pages/getting-started/ui/GettingStartedView.tsx:140` builds an
  eight-step setup sequence.
- `src/app/pages/getting-started/ui/GettingStartedView.tsx:371` renders the
  progress and next-step region.
- `src/app/pages/getting-started/ui/GettingStartedView.tsx:466` renders every
  step as a visible card.
- `src/app/pages/getting-started/ui/GettingStartedView.tsx:544` renders each
  setup step as a full card with status, detail, success text, and CTA.

Recommendation:

- Treat this page as a wizard, not a dashboard.
- Expand only the current step.
- Collapse completed steps into one compact checklist.
- Group optional or advanced steps below the main setup path.
- Avoid truncated titles by using one column for step cards or shorter labels.

## P3 Findings

### 6. The vocabulary is clearer, but still has too many near-synonyms

The product is trying to teach distinctions that matter, but users see many
terms close together: `Agent`, `Simple chat agent`, `Chat-only AI service`,
`AI service`, `answer setting`, `Where agents work`, `Work tool sign-in`,
`Task queue`, and `Project files`.

Recommendation:

- Pick one beginner-facing taxonomy:
  - `Agent type`
  - `AI account`
  - `Work location`
  - `Task queue`
- Keep advanced protocol or implementation terms out of first-run flows.
- Use the same labels across Agents, Settings, Tasks, and Getting Started.

### 7. Cards and panels are overused

The visual system is consistent, but page-level sections, sidebars, toolbars,
explainers, and repeated items often all use card styling. That makes the UI
feel heavier than the actual task.

Recommendation:

- Use cards for repeated entities and modals.
- Use lighter full-width bands or inline rows for page-level guidance.
- Avoid putting an explanation card, a filter card, and entity cards in the same
  first viewport unless the explanation is dismissible.

## Suggested Simplification Order

1. Settings IA: collapse advanced setup and make `Start here` a three-step path.
2. Create Agent: reduce the first modal view and move runtime details behind
   disclosure.
3. Agents page: move task queues and local enrollment out of the default first
   view.
4. Tasks board: compact readiness and filters so work cards appear sooner.
5. Getting Started: turn the page into a current-step wizard with collapsed
   completed steps.

## Product Direction

The current UI is not broken. It is an expert-friendly governed workbench that
has beginner copy added on top. To make it truly beginner-first, the next design
pass should remove simultaneous choices from first-run screens instead of adding
more explanatory text.
