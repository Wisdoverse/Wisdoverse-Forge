import type { TaskSummary } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'
import { RailRow, RailSection } from './RailSection'

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// A bare machine id tells a beginner nothing; omit the row until the API
// carries a display name for the creator.
function isMachineId(value: string): boolean {
  return UUID_PATTERN.test(value.trim())
}

export function DetailsGroup({ task }: { task: TaskSummary }) {
  return (
    <RailSection title="Details">
      {task.createdBy && !isMachineId(task.createdBy) && (
        <RailRow label="Created by">{task.createdBy}</RailRow>
      )}
      <RailRow label="Created">{formatRelativeTime(task.createdAt)}</RailRow>
      <RailRow label="Updated">{formatRelativeTime(task.updatedAt)}</RailRow>
      {task.attempt > 1 && <RailRow label="Attempt">{task.attempt}</RailRow>}
    </RailSection>
  )
}
