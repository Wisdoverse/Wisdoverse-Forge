import { describe, it, expect } from 'vitest'
import { getStationForTool, TOOL_STATION_MAP } from '@shared/types/tools.js'

describe('getStationForTool', () => {
  it('returns correct station for known tools', () => {
    expect(getStationForTool('Read')).toBe('bookshelf')
    expect(getStationForTool('Write')).toBe('desk')
    expect(getStationForTool('Edit')).toBe('workbench')
    expect(getStationForTool('Bash')).toBe('terminal')
    expect(getStationForTool('Grep')).toBe('scanner')
    expect(getStationForTool('Glob')).toBe('scanner')
    expect(getStationForTool('WebFetch')).toBe('antenna')
    expect(getStationForTool('WebSearch')).toBe('antenna')
    expect(getStationForTool('Task')).toBe('portal')
    expect(getStationForTool('TodoWrite')).toBe('taskboard')
    expect(getStationForTool('AskUserQuestion')).toBe('center')
    expect(getStationForTool('NotebookEdit')).toBe('desk')
  })

  it('returns center for unknown/MCP tools', () => {
    expect(getStationForTool('mcp__some_tool')).toBe('center')
    expect(getStationForTool('UnknownTool')).toBe('center')
    expect(getStationForTool('')).toBe('center')
  })
})

describe('TOOL_STATION_MAP', () => {
  it('contains all expected tools', () => {
    expect(Object.keys(TOOL_STATION_MAP)).toHaveLength(12)
  })
})
