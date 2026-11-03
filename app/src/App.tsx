import { useEffect, useRef, useState } from 'react';
import type { AppStatus } from './ipc/generated/AppStatus';
import type { CreateWorkspace as CreateInput } from './ipc/generated/CreateWorkspace';
import type { RegisterAgent as RegisterInput } from './ipc/generated/RegisterAgent';
import * as client from './ipc/client';
import { CreateWorkspace } from './features/setup/CreateWorkspace';
import { ErrorScreen } from './features/errors/ErrorScreen';
import { WorkspaceScreen } from './features/workspace/WorkspaceScreen';

type Actions = {
  create: (input: CreateInput) => void;
  register: (input: RegisterInput) => Promise<void>;
  profile: (name: string) => void;
  reload: () => void;
};
export function StatusScreen({
  status,
  busy,
  actions,
}: {
  status: AppStatus;
  busy: boolean;
  actions: Actions;
}) {
  if (status === 'Setup')
    return <CreateWorkspace busy={busy} onCreate={actions.create} />;
  if ('Error' in status)
    return (
      <ErrorScreen
        error={status.Error}
        busy={busy}
        onReload={actions.reload}
        onRepair={actions.profile}
      />
    );
  return (
    <WorkspaceScreen
      workspace={status.Ready}
      busy={busy}
      onRegister={actions.register}
      onProfile={actions.profile}
    />
  );
}
export function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const inFlight = useRef(false);
  const main = useRef<HTMLElement>(null);
  useEffect(() => {
    let active = true;
    void client.appStatus().then((value) => {
      if (active) setStatus(value);
    });
    return () => {
      active = false;
    };
  }, []);
  const screen =
    status === null
      ? 'Loading'
      : typeof status === 'string'
        ? status
        : 'Error' in status
          ? status.Error
          : `Ready:${status.Ready.id_short}`;
  useEffect(() => {
    main.current?.querySelector<HTMLElement>('h1')?.focus();
  }, [screen]);
  const run = async (action: () => Promise<AppStatus>) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    try {
      setStatus(await action());
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };
  return (
    <main ref={main} aria-busy={busy}>
      {status === null ? (
        <section className="loading-sign" role="status">
          <p className="eyebrow">Meshyard · This device</p>
          <h1 tabIndex={-1}>Opening local workspace…</h1>
          <p>Reading saved workspace and device identity.</p>
        </section>
      ) : (
        <StatusScreen
          status={status}
          busy={busy}
          actions={{
            create: (input) => {
              void run(() => client.createWorkspace(input));
            },
            register: (input) => run(() => client.registerAgent(input)),
            profile: (name) => {
              void run(() => client.updateProfile(name));
            },
            reload: () => {
              void run(client.appStatus);
            },
          }}
        />
      )}
      <p className="footnote" role="status">
        {busy
          ? 'Saving to this device…'
          : 'Local data · No networking or live hook in this slice'}
      </p>
    </main>
  );
}
