import { useState } from 'react';
import type { CreateWorkspace as CreateInput } from '../../ipc/generated/CreateWorkspace';

export function CreateWorkspace({
  busy,
  onCreate,
}: {
  busy: boolean;
  onCreate: (input: CreateInput) => void;
}) {
  const [name, setName] = useState('');
  const [displayName, setDisplayName] = useState('');
  return (
    <section className="setup panel">
      <p className="eyebrow">Your local workspace</p>
      <h1 tabIndex={-1}>Create workspace</h1>
      <p>
        A local place for your agents. Create a workspace and register your
        first agent.
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onCreate({ name, display_name: displayName, mode: 'Strict' });
        }}
      >
        <fieldset disabled={busy}>
          <label htmlFor="workspace-name">Workspace name</label>
          <input
            id="workspace-name"
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            required
            maxLength={64}
            autoComplete="organization"
          />
          <label htmlFor="display-name">Your display name</label>
          <input
            id="display-name"
            value={displayName}
            onChange={(e) => {
              setDisplayName(e.target.value);
            }}
            required
            maxLength={64}
            autoComplete="nickname"
          />
          <p>
            Your yard stays on this Mac. Connect Claude Code or Codex after
            setup.
          </p>
          <button type="submit">
            {busy ? 'Creating…' : 'Create workspace'}
          </button>
        </fieldset>
      </form>
    </section>
  );
}
