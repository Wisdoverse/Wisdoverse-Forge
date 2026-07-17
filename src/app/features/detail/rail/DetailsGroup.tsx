import type { TaskSummary } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'
import { RailRow, RailSection } from './RailSection'

export function DetailsGroup({ task }: { task: TaskSummary }) {
  return (
    <RailSection title="Details">
      {task.createdBy && <RailRow label="Created by">{task.createdBy}</RailRow>}
      <RailRow label="Created">{formatRelativeTime(task.createdAt)}</RailRow>
      <RailRow label="Updated">{formatRelativeTime(task.updatedAt)}</RailRow>
      {task.attempt > 1 && <RailRow label="Attempt">{task.attempt}</RailRow>}
    </RailSection>
  )
}
