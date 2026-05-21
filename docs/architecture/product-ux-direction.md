# Product UX Direction

Wisdoverse Forge should keep its governed, auditable execution model, but the
primary user experience needs a shorter path from setup to useful agent work.
The product should feel less like a collection of infrastructure surfaces and
more like a team workspace where agents can be assigned work, report progress,
and leave reusable knowledge behind.

## Direction

- Make the first-run path explicit: connect a runtime, add a provider or
  credential, create an agent, create a task, assign it, watch progress, review
  output, and save reusable learning.
- Treat the task board as the primary work surface. Task details should combine
  description, assignment, comments or updates, execution log, result artifacts,
  evidence, context, and final review in one coherent flow.
- Make agents feel like managed teammates, not only runtime resources. Agent
  pages should foreground presence, current work, skills, recent updates,
  availability, and how the agent can be assigned.
- Promote runtimes to first-class setup and operations surfaces. Users should
  see runtime health, available Container CLIs, versions, last heartbeat,
  credential state, and direct remediation guidance.
- Turn context, evidence, governance, and skills into progressive capabilities
  inside the task flow. New users should not have to understand every governance
  primitive before assigning the first task.
- Close the skill reuse loop. Completed work should offer a clear path to
  extract a reusable skill, attach it to agents, and see when it is used later.
- Keep workspace/project/group concepts visible only when they help routing or
  permissions. The default workflow should hide unnecessary hierarchy until the
  team needs it.

## Priority UX Gaps

1. First-run onboarding is still concept-heavy.
2. Runtime readiness is not visible enough before agent creation.
3. Task assignment and execution progress need stronger status, comment, and
   blocker affordances.
4. Agent availability and capability matching are not obvious from the board.
5. Evidence, context, and skills are powerful but feel separate from the core
   task lifecycle.
6. The product needs clearer empty states and recovery actions when no runtime,
   no provider, no available agent, or no task group exists.

## Near-Term Product Work

- Add a guided setup checklist that links directly to the runtime, provider,
  agent, and first task actions.
- Add a runtime health page with CLI detection, credential readiness, and
  troubleshooting actions.
- Redesign task detail around work review: updates, execution log, artifacts,
  context/evidence, and final acceptance.
- Add board-level assignment controls that show available agents and why an
  agent can or cannot take a task.
- Add agent profile summaries for current task, recent activity, configured
  skills, runtime, and credential status.
- Add a post-completion "save as skill" path with review before publishing.
