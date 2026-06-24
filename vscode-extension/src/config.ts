import * as vscode from 'vscode';

export interface OrchestratorConfig {
  serverUrl: string;
  wsUrl: string;
  autoRefreshInterval: number;
  oidcAuthority: string;
  oidcClientId: string;
  notificationsEnabled: boolean;
}

export function getConfig(): OrchestratorConfig {
  const cfg = vscode.workspace.getConfiguration('orchestrator');
  return {
    serverUrl: cfg.get<string>('serverUrl', 'http://localhost:4003').replace(/\/+$/, ''),
    wsUrl: cfg.get<string>('wsUrl', 'ws://localhost:4003').replace(/\/+$/, ''),
    autoRefreshInterval: Math.max(5, cfg.get<number>('autoRefreshInterval', 30)),
    oidcAuthority: cfg.get<string>('oidc.authority', ''),
    oidcClientId: cfg.get<string>('oidc.clientId', ''),
    notificationsEnabled: cfg.get<boolean>('notifications.enabled', true),
  };
}

export function onConfigChange(callback: () => void): vscode.Disposable {
  return vscode.workspace.onDidChangeConfiguration((e) => {
    if (e.affectsConfiguration('orchestrator')) {
      callback();
    }
  });
}
