import { useEffect, useRef, useState } from 'react';
import { PeerPanel } from '../peers/PeerPanel';
import type { CreateWorkspace as CreateInput } from '../../ipc/generated/CreateWorkspace';

export function CreateWorkspace({
  busy,
  onCreate,
}: {
  busy: boolean;
  onCreate: (input: CreateInput) => void;
}) {
  const [joining, setJoining] = useState(false);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus();
  }, [joining]);
  const [name, setName] = useState('');
  const [displayName, setDisplayName] = useState('');
  if (joining)
    return (
      <section className="setup panel">
        <h1 ref={heading} tabIndex={-1}>
          Join existing workspace
        </h1>
        <button
          type="button"
          onClick={() => {
            setJoining(false);
          }}
        >
          Back to create workspace
        </button>
        <PeerPanel setup />
      </section>
    );
  return (
    <>
      <section className="setup panel">
        <p className="eyebrow">Your local workspace</p>
        <h1 ref={heading} tabIndex={-1}>
          Create workspace
        </h1>
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
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setJoining(true);
          }}
        >
          Join existing workspace
        </button>
      </section>
    </>
  );
}
