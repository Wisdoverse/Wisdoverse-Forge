import type { TaskSummary } from '@app/shared/api/orchestration'

interface DescriptionTabProps {
  task: TaskSummary
}

export function DescriptionTab({ task }: DescriptionTabProps) {
  return (
    <div className="py-3">
      {task.params.message ? (
        <p className="text-xs text-foreground-light dark:text-foreground-dark leading-relaxed whitespace-pre-wrap">
          {task.params.message}
        </p>
      ) : (
        <p className="text-xs text-secondary-light dark:text-secondary-dark italic">
          No description provided.
        </p>
      )}
    </div>
  )
}
