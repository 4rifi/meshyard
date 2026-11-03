import { invoke } from '@tauri-apps/api/core';
import type { AppStatus } from './generated/AppStatus';
import type { CreateWorkspace } from './generated/CreateWorkspace';
import type { RegisterAgent } from './generated/RegisterAgent';

type CommandArgs = {
  app_status: undefined;
  create_workspace: { input: CreateWorkspace };
  update_profile: { name: string };
  register_agent: { input: RegisterAgent };
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
