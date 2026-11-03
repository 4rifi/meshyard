import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as client from './client';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
describe('typed IPC boundary', () => {
  it('sends the four allowlisted commands and typed argument envelopes', async () => {
    vi.mocked(invoke).mockResolvedValue('Setup');
    await client.appStatus();
    const create = {
      name: 'Demo',
      display_name: 'Dion',
      mode: 'Strict' as const,
    };
    await client.createWorkspace(create);
    await client.updateProfile('updated');
    const register = { kind: 'Codex' as const, label: 'worker' };
    await client.registerAgent(register);
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ['app_status', undefined],
      ['create_workspace', { input: create }],
      ['update_profile', { name: 'updated' }],
      ['register_agent', { input: register }],
    ]);
  });
  it('never leaks rejected native transport messages', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('/private/secret: os error'));
    expect(await client.appStatus()).toEqual({ Error: 'IpcUnavailable' });
  });
});
