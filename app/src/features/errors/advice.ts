import type { AppError } from '../../ipc/generated/AppError';

export const advice: Record<AppError, string> = {
  IdentityMissing:
    'This workspace’s device identity is missing; restore it or use the matching key store.',
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
  IngestUnavailable:
    'Local hook intake is unavailable. Check the private profile and restart Meshyard.',
  HookInstallLocation:
    'Move Meshyard from the disk image or temporary location to /Applications, then reopen it and review hooks again.',
  HookBinaryMissing:
    'The bundled hook executable is missing or not executable. Reinstall Meshyard in /Applications and reopen it.',
  HookPlanStale:
    'Settings changed since review. Review a fresh plan before applying.',
  HookConfigInvalid:
    'Agent hook settings contain invalid or unsupported JSON. They were left untouched.',
  IpcUnavailable:
    'The desktop connection is unavailable; open Meshyard in its native window and retry.',
};
