import { useState } from 'react';
import type { AppError } from '../../ipc/generated/AppError';

const advice: Record<AppError, string> = {
  IdentityLocked: 'Unlock the device identity store, then restart Meshyard.',
  IdentityUnavailable:
    'Make the selected identity store available, then restart Meshyard.',
  IdentityInvalid:
    'Restore the original valid device identity before restarting.',
  IdentityMismatch: 'Reopen this profile with its original member identity.',
  DataDirectoryBusy:
    'Data directory is already in use. Close the other instance before restarting.',
  UnsafePermissions:
    'Use a private data directory owned by your OS user, then restart.',
  Storage:
    'Check that local storage is available; reload the saved workspace before retrying a write.',
  Migration:
    'Keep your data intact and ask for help with the database migration.',
  ProjectionLimit:
    'This workspace exceeds the 100,000-event read limit; an incremental projection is needed.',
  MultipleWorkspaces:
    'This profile contains multiple workspaces; use a profile with exactly one workspace.',
  WorkspaceExists: 'A workspace already exists; reload it to continue.',
  NoWorkspace: 'Reload and create a workspace first.',
  InvalidInput:
    'Enter a non-empty name or label and choose an available mode or agent kind.',
  RandomUnavailable: 'Restore the OS secure random source before retrying.',
  PartialCreate:
    'The workspace was saved, but its display name was not; save a display name below to finish setup.',
  Configuration:
    'Check the data directory and explicitly select file storage only for a development profile.',
  IpcUnavailable:
    'The desktop connection is unavailable; open Meshyard in its native window and retry.',
};
export function ErrorScreen({
  error,
  busy,
  onReload,
  onRepair,
}: {
  error: AppError;
  busy: boolean;
  onReload: () => void;
  onRepair: (name: string) => void;
}) {
  const [name, setName] = useState('');
  return (
    <section className="panel error">
      <p className="eyebrow">Action needed</p>
      <h1 tabIndex={-1}>Unable to continue</h1>
      <p role="alert">
        <strong>{error}</strong>: {advice[error]}
      </p>
      {error === 'PartialCreate' && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            onRepair(name);
          }}
        >
          <label htmlFor="repair-name">Your display name</label>
          <input
            id="repair-name"
            required
            maxLength={64}
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            disabled={busy}
          />
          <button disabled={busy} type="submit">
            Save display name
          </button>
        </form>
      )}
      <button type="button" disabled={busy} onClick={onReload}>
        Reload saved state
      </button>
    </section>
  );
}
