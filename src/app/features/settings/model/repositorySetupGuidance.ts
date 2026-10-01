import type { RepositorySetupFailure } from '@app/shared/api/selfFix'

interface Guidance {
  title: string
  detail: string
}

export const repositorySetupGuidance: Record<RepositorySetupFailure, Guidance> = {
  forbidden: {
    title: 'Administrator access is required',
    detail:
      'Ask a Forge administrator to check the connection. Your access may have changed; the previous connection details have been cleared.',
  },
  unauthenticated: {
    title: 'Sign in again to check the connection',
    detail:
      'Your sign-in could not be renewed. Sign in again, then return to Maintenance repository.',
  },
  'not-configured': {
    title: 'Connect a maintenance repository',
    detail:
      'Ask the person who runs Forge to configure its GitHub App for one approved repository using the connection guide, then choose Check connection.',
  },
  permissions: {
    title: 'The connection needs more access',
    detail:
      'Ask the repository owner to grant the GitHub App read and write access to Contents and Pull requests. Restart the Forge API or wait for its access token to expire, then choose Check connection.',
  },
  unavailable: {
    title: 'The repository is unavailable',
    detail:
      'Ask the repository owner to restore access to the archived or disabled repository, then choose Check connection.',
  },
  access: {
    title: 'Forge cannot read the approved repository',
    detail:
      'Ask the person who runs Forge to confirm the selected repository and the GitHub App installation access, then choose Check connection.',
  },
  'empty-repository': {
    title: 'The repository needs a starting version',
    detail:
      'Ask the repository owner to confirm that the default branch contains a commit, then choose Check connection.',
  },
  unsupported: {
    title: 'Update Forge to check this connection',
    detail:
      'Ask the person who runs Forge to update the app and API together, then choose Check connection.',
  },
  'invalid-response': {
    title: 'The connection result could not be verified',
    detail:
      'Choose Check connection again. If this continues, ask the person who runs Forge to check that the app and API versions match.',
  },
  timeout: {
    title: 'The connection check took too long',
    detail:
      'Choose Check connection again. If this continues, ask the person who runs Forge to check its connection to GitHub.',
  },
  unreachable: {
    title: 'Forge could not check the connection',
    detail:
      'Check your connection, then choose Check connection again. If this continues, ask the person who runs Forge to check the API.',
  },
  cancelled: {
    title: 'The connection check was interrupted',
    detail: 'Choose Check connection to get a new result.',
  },
  failed: {
    title: 'The connection check could not finish',
    detail:
      'Choose Check connection again. If this continues, ask the person who runs Forge to check the GitHub connection using the connection guide.',
  },
}
