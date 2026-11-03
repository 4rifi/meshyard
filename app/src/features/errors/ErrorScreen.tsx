import { useState } from 'react';
import type { AppError } from '../../ipc/generated/AppError';

import { advice } from './advice';
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
