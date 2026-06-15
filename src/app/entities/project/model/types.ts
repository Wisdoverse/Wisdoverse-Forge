/**
 * The clone lifecycle state for a project's optional git repository. Mirrors the
 * backend `projects.clone_status` column (`CloneStatus` in `domain/project_clone.rs`).
 * `none` means the project has no repository to clone (no badge is rendered).
 */
export type CloneStatus = 'none' | 'queued' | 'cloning' | 'ready' | 'failed'

/**
 * The latest clone attempt projected for the UI. Matches the Rust `CloneSummary`
 * serializer (`#[serde(rename_all = "camelCase")]`) exactly — keep field names in
 * sync with `rust/crates/api/src/domain/project_clone.rs`. This carries NO secret:
 * `errorMessage` is already redacted server-side and is never a raw token.
 */
export interface CloneSummary {
  /** The attempt's lifecycle status (`queued`/`cloning`/`ready`/`failed`/`cancelled`). */
  status: string
  /** Coarse failure class on a `failed` attempt (`auth`/`not_found`/…); absent otherwise. */
  errorClass?: string | null
  /** REDACTED, safe-to-display failure reason on a `failed` attempt; absent otherwise. */
  errorMessage?: string | null
  /** The default branch git resolved on a successful clone; absent until ready. */
  resolvedBranch?: string | null
  /** The cloned HEAD commit SHA on a successful clone; absent until ready. */
  headSha?: string | null
  /** The 1-based attempt number this summary describes (the latest attempt). */
  attempt: number
  /** ISO-8601 timestamp of when this attempt row was last updated. */
  updatedAt: string
}

export interface NavProject {
  id: string
  teamId: string
  workspaceId?: string
  name: string
  slug: string
  color: string
  description: string
  canManage?: boolean
  canDelete?: boolean
  /**
   * Denormalized clone lifecycle marker mirrored from `projects.clone_status`.
   * `none` (or absent) means there is no git repository, so no badge renders.
   */
  cloneStatus?: CloneStatus
  /** The latest clone attempt's detail; absent when the project has no attempt. */
  clone?: CloneSummary
}

export interface CreateProjectInput {
  name: string
  slug?: string
  color?: string
  description?: string
  /**
   * Optional HTTPS git repository to clone into the new project's workspace dir.
   * The server rejects non-HTTPS URLs and URLs with embedded credentials.
   */
  repositoryUrl?: string
}

export interface UpdateProjectInput {
  name?: string
  slug?: string
  color?: string
  description?: string
}
