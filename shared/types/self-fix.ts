/** Read-only repository setup contract; mirrors Rust SelfFixRepositorySetup. */
export interface SelfFixRepositorySetup {
  repository: string
  defaultBranch: string
  baseSha: string
  contentsWrite: boolean
  pullRequestsWrite: boolean
  checksRead: boolean
  squashMergeAllowed: boolean
}
