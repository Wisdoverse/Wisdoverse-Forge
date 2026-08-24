import { useEffect, useMemo, useState } from 'react'
import { useAgentsStore } from '@app/entities/agent'
import { useSkillsStore } from '@app/entities/skill'
import { SkillAgentPickerView } from '@app/shared/ui/SkillAgentPickerView'
import { useTranslation } from 'react-i18next'

/**
 * Attach management for a skill on the skills page: shows the agents already
 * following it and lets a member attach or detach matching agents.
 */
export function SkillAgentPicker({ skillId }: { skillId: string }) {
  const { t } = useTranslation()
  const agents = useAgentsStore((state) => state.agents)
  const loadAgents = useAgentsStore((state) => state.loadAgents)
  const skillAgents = useSkillsStore((state) => state.skillAgents)
  const loadSkillAgents = useSkillsStore((state) => state.loadSkillAgents)
  const attachSkillToAgent = useSkillsStore((state) => state.attachSkillToAgent)
  const detachSkillFromAgent = useSkillsStore((state) => state.detachSkillFromAgent)
  const [selectedAgentId, setSelectedAgentId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [detachingId, setDetachingId] = useState<string | null>(null)

  const linked = useMemo(() => skillAgents[skillId] ?? [], [skillAgents, skillId])
  const linkedIds = useMemo(() => new Set(linked.map((agent) => agent.agentId)), [linked])
  const available = useMemo(
    () => agents.filter((agent) => !linkedIds.has(agent.id)),
    [agents, linkedIds]
  )

  useEffect(() => {
    void loadAgents()
    void loadSkillAgents(skillId)
  }, [loadAgents, loadSkillAgents, skillId])

  async function handleAttach(event: React.FormEvent) {
    event.preventDefault()
    if (!selectedAgentId || busy) return
    setBusy(true)
    setError(null)
    try {
      await attachSkillToAgent(skillId, selectedAgentId)
      setSelectedAgentId('')
    } catch {
      setError(t('skillAgents.attachError'))
    } finally {
      setBusy(false)
    }
  }

  async function handleDetach(agentId: string) {
    if (detachingId) return
    setDetachingId(agentId)
    setError(null)
    try {
      await detachSkillFromAgent(skillId, agentId)
    } catch {
      setError(t('skillAgents.detachError'))
    } finally {
      setDetachingId(null)
    }
  }

  return (
    <SkillAgentPickerView
      available={available.map((agent) => ({ id: agent.id, name: agent.name }))}
      linked={linked}
      selectedAgentId={selectedAgentId}
      busy={busy}
      error={error}
      detachingId={detachingId}
      onSelect={setSelectedAgentId}
      onAttach={(event) => void handleAttach(event)}
      onDetach={(agentId) => void handleDetach(agentId)}
    />
  )
}
