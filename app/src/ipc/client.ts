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
  'HookPlanStale',
  'HookConfigInvalid',
  'IngestUnavailable',
  'UnsafePermissions',
  'Storage',
  'NoWorkspace',
  'Configuration',
  'RandomUnavailable',
];
async function plan(command: 'hook_plan' | 'hook_remove'): Promise<PlanResult> {
  try {
    return { plan: await invoke<HookPlan>(command) };
  } catch (error) {
    return {
      error: hookErrors.find((kind) => kind === error) ?? 'IpcUnavailable',
    };
  }
}
export const hookPlan = (): Promise<PlanResult> => plan('hook_plan');
export const hookRemove = (): Promise<PlanResult> => plan('hook_remove');
export const listenWorkspace = (refresh: () => void): Promise<() => void> =>
  listen('workspace-changed', refresh);
