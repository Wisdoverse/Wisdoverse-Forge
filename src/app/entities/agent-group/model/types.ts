export interface NavAgentGroup {
  id: string
  name: string
  projectId: string
}

export interface CreateAgentGroupInput {
  projectId: string
  name: string
  description?: string
}
