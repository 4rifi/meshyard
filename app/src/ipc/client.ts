import type { AgentKind } from './generated/AgentKind';
import { listen } from '@tauri-apps/api/event';
import type { HookPlan } from './generated/HookPlan';
import type { AppError } from './generated/AppError';
import { invoke } from '@tauri-apps/api/core';
import type { AppStatus } from './generated/AppStatus';
import type { CreateWorkspace } from './generated/CreateWorkspace';
import type { RegisterAgent } from './generated/RegisterAgent';

type CommandArgs = {
  app_status: undefined;
  create_workspace: { input: CreateWorkspace };
  update_profile: { name: string };
  register_agent: { input: RegisterAgent };
  bind_agent: { agentId: string };
  hook_apply: { planId: string };
};
// Transport errors are deliberately reduced to a fixed kind, never rendered verbatim.
async function call<K extends keyof CommandArgs>(
  command: K,
  args: CommandArgs[K],
): Promise<AppStatus> {
  try {
    return await invoke<AppStatus>(command, args);
  } catch {
    return { Error: 'IpcUnavailable' };
  }
}
export const appStatus = (): Promise<AppStatus> =>
  call('app_status', undefined);
export const createWorkspace = (input: CreateWorkspace): Promise<AppStatus> =>
  call('create_workspace', { input });
export const updateProfile = (name: string): Promise<AppStatus> =>
  call('update_profile', { name });
export const registerAgent = (input: RegisterAgent): Promise<AppStatus> =>
  call('register_agent', { input });

export const bindAgent = (agentId: string): Promise<AppStatus> =>
  call('bind_agent', { agentId });
export const hookApply = (planId: string): Promise<AppStatus> =>
  call('hook_apply', { planId });
export type PlanResult = { plan: HookPlan } | { error: AppError };
const hookErrors: readonly AppError[] = [
  'HookBinaryMissing',
  'HookInstallLocation',
  'HookPlanStale',
  'HookConfigInvalid',
  'IngestUnavailable',
  'UnsafePermissions',
  'Storage',
  'NoWorkspace',
  'Configuration',
  'RandomUnavailable',
];
async function plan(
  command: 'hook_plan' | 'hook_remove',
  kind: AgentKind,
): Promise<PlanResult> {
  try {
    return { plan: await invoke<HookPlan>(command, { kind }) };
  } catch (error) {
    return {
      error: hookErrors.find((kind) => kind === error) ?? 'IpcUnavailable',
    };
  }
}
export const hookPlan = (kind: AgentKind = 'ClaudeCode'): Promise<PlanResult> =>
  plan('hook_plan', kind);
export const hookRemove = (
  kind: AgentKind = 'ClaudeCode',
): Promise<PlanResult> => plan('hook_remove', kind);
export const listenWorkspace = (refresh: () => void): Promise<() => void> =>
  listen('workspace-changed', refresh);

import type { NetworkView } from './generated/NetworkView';
export async function networkStatus(): Promise<NetworkView | null> {
  try {
    return await invoke<NetworkView>('network_status');
  } catch {
    return null;
  }
}
export async function networkStart(): Promise<void> {
  await invoke('network_start');
}
export async function networkStop(): Promise<void> {
  await invoke('network_stop');
}
export async function networkInvite(): Promise<string> {
  return invoke<string>('network_invite');
}
export async function networkJoin(
  link: string,
  displayName: string,
): Promise<void> {
  await invoke('network_join', { link, displayName });
}
export async function networkConfirm(
  id: string,
  matches: boolean,
): Promise<void> {
  await invoke('network_confirm', { id, matches });
}
