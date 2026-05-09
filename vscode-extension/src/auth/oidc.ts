import * as vscode from 'vscode';
import { getConfig } from '../config';

const AUTH_TYPE = 'orchestrator-oidc';
const AUTH_NAME = 'Orchestrator OIDC';

export class OrchestratorAuthProvider implements vscode.AuthenticationProvider, vscode.Disposable {
  private readonly _onDidChangeSessions = new vscode.EventEmitter<vscode.AuthenticationProviderAuthenticationSessionsChangeEvent>();
  readonly onDidChangeSessions = this._onDidChangeSessions.event;

  private sessions: vscode.AuthenticationSession[] = [];
  private readonly secretKey = 'orchestrator.auth.sessions';

  constructor(private readonly context: vscode.ExtensionContext) {}

  static register(context: vscode.ExtensionContext): OrchestratorAuthProvider {
    const provider = new OrchestratorAuthProvider(context);
    context.subscriptions.push(
      vscode.authentication.registerAuthenticationProvider(AUTH_TYPE, AUTH_NAME, provider, {
        supportsMultipleAccounts: false,
      }),
    );
    return provider;
  }

  async getSessions(scopes?: readonly string[]): Promise<vscode.AuthenticationSession[]> {
    await this.restoreSessions();

    if (!scopes || scopes.length === 0) {
      return this.sessions;
    }

    return this.sessions.filter((s) =>
      scopes.every((scope) => s.scopes.includes(scope)),
    );
  }

  async createSession(scopes: readonly string[]): Promise<vscode.AuthenticationSession> {
    const config = getConfig();

    if (!config.oidcAuthority || !config.oidcClientId) {
      throw new Error(
        'OIDC not configured. Set orchestrator.oidc.authority and orchestrator.oidc.clientId in settings.',
      );
    }

    const callbackUri = await vscode.env.asExternalUri(
      vscode.Uri.parse(`${vscode.env.uriScheme}://wisdoverse.orchestrator/callback`),
    );

    const discoveryUrl = `${config.oidcAuthority}/.well-known/openid-configuration`;
    const discoveryRes = await fetch(discoveryUrl);
    if (!discoveryRes.ok) {
      throw new Error(`OIDC discovery failed: ${discoveryRes.status}`);
    }
    const discovery = (await discoveryRes.json()) as {
      authorization_endpoint: string;
      token_endpoint: string;
    };

    const state = generateRandomString(32);
    const codeVerifier = generateRandomString(64);
    const codeChallenge = await computeCodeChallenge(codeVerifier);

    const authUrl = new URL(discovery.authorization_endpoint);
    authUrl.searchParams.set('client_id', config.oidcClientId);
    authUrl.searchParams.set('redirect_uri', callbackUri.toString());
    authUrl.searchParams.set('response_type', 'code');
    authUrl.searchParams.set('scope', scopes.join(' ') || 'openid profile email');
    authUrl.searchParams.set('state', state);
    authUrl.searchParams.set('code_challenge', codeChallenge);
    authUrl.searchParams.set('code_challenge_method', 'S256');

    const opened = await vscode.env.openExternal(vscode.Uri.parse(authUrl.toString()));
    if (!opened) {
      throw new Error('Failed to open browser for authentication');
    }

    const callbackResult = await waitForCallback(state);

    const tokenRes = await fetch(discovery.token_endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        client_id: config.oidcClientId,
        code: callbackResult.code,
        redirect_uri: callbackUri.toString(),
        code_verifier: codeVerifier,
      }),
    });

    if (!tokenRes.ok) {
      throw new Error(`Token exchange failed: ${tokenRes.status}`);
    }

    const tokens = (await tokenRes.json()) as {
      access_token: string;
      refresh_token?: string;
      expires_in?: number;
      id_token?: string;
    };

    const claims = parseJwtClaims(tokens.access_token);
    const account: vscode.AuthenticationSessionAccountInformation = {
      id: claims.sub || 'unknown',
      label: claims.email || claims.name || claims.sub || 'Orchestrator User',
    };

    const session: vscode.AuthenticationSession = {
      id: generateRandomString(16),
      accessToken: tokens.access_token,
      account,
      scopes: scopes as string[],
    };

    this.sessions = [session];
    await this.persistSessions();

    this._onDidChangeSessions.fire({ added: [session], removed: [], changed: [] });
    return session;
  }

  async removeSession(sessionId: string): Promise<void> {
    const removed = this.sessions.filter((s) => s.id === sessionId);
    this.sessions = this.sessions.filter((s) => s.id !== sessionId);
    await this.persistSessions();

    if (removed.length > 0) {
      this._onDidChangeSessions.fire({ added: [], removed, changed: [] });
    }
  }

  async getToken(): Promise<string | null> {
    const sessions = await this.getSessions();
    return sessions[0]?.accessToken ?? null;
  }

  private async persistSessions(): Promise<void> {
    const data = this.sessions.map((s) => ({
      id: s.id,
      accessToken: s.accessToken,
      accountId: s.account.id,
      accountLabel: s.account.label,
      scopes: s.scopes,
    }));
    await this.context.secrets.store(this.secretKey, JSON.stringify(data));
  }

  private async restoreSessions(): Promise<void> {
    const raw = await this.context.secrets.get(this.secretKey);
    if (!raw) {
      return;
    }
    try {
      const data = JSON.parse(raw) as {
        id: string;
        accessToken: string;
        accountId: string;
        accountLabel: string;
        scopes: string[];
      }[];
      this.sessions = data.map((d) => ({
        id: d.id,
        accessToken: d.accessToken,
        account: { id: d.accountId, label: d.accountLabel },
        scopes: d.scopes,
      }));
    } catch (err) {
      console.warn(`[Orchestrator] Failed to restore saved sessions, user will need to re-authenticate: ${err}`);
      this.sessions = [];
    }
  }

  dispose(): void {
    this._onDidChangeSessions.dispose();
  }
}

function generateRandomString(length: number): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';
  const values = new Uint8Array(length);
  crypto.getRandomValues(values);
  return Array.from(values, (v) => chars[v % chars.length]).join('');
}

async function computeCodeChallenge(verifier: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(verifier);
  const digest = await crypto.subtle.digest('SHA-256', data);
  return base64UrlEncode(new Uint8Array(digest));
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function parseJwtClaims(token: string): Record<string, string> {
  try {
    const payload = token.split('.')[1];
    if (!payload) {
      console.warn('[Orchestrator] JWT has no payload segment');
      return {};
    }
    const padded = payload + '='.repeat((4 - (payload.length % 4)) % 4);
    const decoded = atob(padded.replace(/-/g, '+').replace(/_/g, '/'));
    return JSON.parse(decoded);
  } catch (err) {
    console.warn(`[Orchestrator] Failed to parse JWT claims: ${err}`);
    return {};
  }
}

function waitForCallback(expectedState: string): Promise<{ code: string }> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      disposable.dispose();
      reject(new Error('Authentication timed out after 120 seconds'));
    }, 120_000);

    const disposable = vscode.window.registerUriHandler({
      handleUri(uri: vscode.Uri) {
        clearTimeout(timeout);
        disposable.dispose();

        const params = new URLSearchParams(uri.query);
        const state = params.get('state');
        const code = params.get('code');
        const error = params.get('error');

        if (error) {
          reject(new Error(`OIDC error: ${error}`));
          return;
        }

        if (state !== expectedState) {
          reject(new Error('OIDC state mismatch'));
          return;
        }

        if (!code) {
          reject(new Error('No authorization code received'));
          return;
        }

        resolve({ code });
      },
    });
  });
}
