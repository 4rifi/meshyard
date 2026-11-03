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
        A shared place starts here, on this device. Create a workspace and
        register your first agent.
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
          <label htmlFor="mode">Connection mode</label>
          <select
            id="mode"
            value="Strict"
            onChange={() => {
              /* only Strict is available */
            }}
            aria-describedby="mode-help"
          >
            <option value="Strict">Strict P2P</option>
            <option value="Auto" disabled>
              Auto — Requires a self-hosted relay (G-1)
            </option>
          </select>
          <p id="mode-help">
            Strict is saved with the workspace. Networking is not built yet.
          </p>
          <button type="submit">
            {busy ? 'Creating…' : 'Create workspace'}
          </button>
        </fieldset>
      </form>
    </section>
  );
}
