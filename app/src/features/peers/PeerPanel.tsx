import { useEffect, useMemo, useState } from 'react';
import QRCode from 'qrcode';
import * as client from '../../ipc/client';
import type { NetworkView } from '../../ipc/generated/NetworkView';

function InviteQr({ link }: { link: string }) {
  const code = useMemo(
    () => QRCode.create(link, { errorCorrectionLevel: 'L' }),
    [link],
  );
  const size = code.modules.size;
  let path = '';
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      if (code.modules.get(y, x))
        path += `M${String(x + 4)} ${String(y + 4)}h1v1h-1z`;
    }
  return (
    <svg
      className="invite-qr"
      width={(size + 8) * 2}
      height={(size + 8) * 2}
      viewBox={`0 0 ${String(size + 8)} ${String(size + 8)}`}
      role="img"
      aria-label="Private invitation QR code"
      shapeRendering="crispEdges"
    >
      <rect width={size + 8} height={size + 8} fill="white" />
      <path d={path} fill="black" />
    </svg>
  );
}
export function PeerPanel({
  setup = false,
  owner = false,
  recovering = false,
}: {
  setup?: boolean;
  owner?: boolean;
  recovering?: boolean;
}) {
  const [view, setView] = useState<NetworkView | null>(null);
  const [invite, setInvite] = useState('');
  const [showInvite, setShowInvite] = useState(false);
  const [link, setLink] = useState('');
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        const next = await client.networkStatus();
        if (active) {
          setView(next);
          if (!next) setError('Connection status is unavailable.');
        }
      } catch {
        if (active) setError('Connection status is unavailable.');
      }
    };
    void refresh();
    const timer = setInterval(() => {
      void refresh();
    }, 500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
      setView(await client.networkStatus());
    } catch {
      setError(
        'This action could not complete. Check the private invitation, direct reachability and confirmation deadline. No relay fallback is used.',
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <section
      className={setup || recovering ? 'join-flow' : 'panel peer-panel'}
      aria-label={setup ? 'Join directly' : 'Share the yard'}
    >
      {!setup && !recovering && (
        <header className="peer-heading">
          <h2 id="pairing-heading">Share the yard</h2>
          <p className="eyebrow">Strict P2P · direct only</p>
        </header>
      )}
      {recovering ? (
        <p>
          Your confirmed join is saved. Waiting for the owner’s signed
          membership; Meshyard retries directly and keeps your device identity.
        </p>
      ) : setup ? (
        <p>
          Paste a private Strict P2P invitation. Compare all six words on both
          devices before sharing.
        </p>
      ) : (
        <details className="peer-privacy">
          <summary>Privacy and direct reachability</summary>
          <p>
            Paired devices share agent labels, activity and provenance. Prompts,
            files and commands stay private. Firewalls or NAT can prevent direct
            connections; no relay fallback is used.
          </p>
        </details>
      )}
      {setup ? (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void run(async () => {
              await client.networkJoin(link.trim(), name.trim());
              setLink('');
            });
          }}
        >
          <fieldset disabled={busy || view?.running}>
            <label htmlFor="join-name">Your display name</label>
            <input
              id="join-name"
              required
              maxLength={64}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
              }}
            />
            <label htmlFor="join-link">Private invitation link</label>
            <input
              id="join-link"
              type="password"
              required
              autoComplete="off"
              spellCheck={false}
              value={link}
              onChange={(e) => {
                setLink(e.target.value);
              }}
            />
            <button type="submit">Join directly</button>
          </fieldset>
        </form>
      ) : (
        <>
          {!view?.running && (
            <p>Sharing is off. Your local agents continue working.</p>
          )}
          <div className="peer-actions">
            {view?.running && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void run(async () => {
                    await client.networkStop();
                    setInvite('');
                  });
                }}
              >
                Stop sharing
              </button>
            )}
            {owner && !view?.pairings.some((p) => p.state === 'Reserved') && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void run(async () => {
                    setInvite(await client.networkInvite());
                    setShowInvite(false);
                  });
                }}
              >
                Create private invite
              </button>
            )}
            {!view?.running && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void run(client.networkStart);
                }}
              >
                {recovering ? 'Retry direct sync' : 'Connect paired devices'}
              </button>
            )}
          </div>
          {!owner && !recovering && (
            <p>
              Only the workspace owner can invite another device. Each member
              sends only its own signed activity; an offline member’s log is
              unavailable until that member connects.
            </p>
          )}
        </>
      )}
      {invite && (
        <div className="invite-secret">
          <p>
            Single use · 15 minutes · includes this device’s IP addresses. Share
            privately.
          </p>
          <button
            type="button"
            onClick={() => {
              setShowInvite(!showInvite);
            }}
          >
            {showInvite ? 'Hide invitation' : 'Show private link and QR'}
          </button>
          {showInvite && (
            <div className="invite-layout">
              <div>
                <label htmlFor="private-invite">Copy this private link</label>
                <textarea
                  id="private-invite"
                  readOnly
                  value={invite}
                  onFocus={(e) => {
                    e.currentTarget.select();
                  }}
                />
              </div>
              <InviteQr link={invite} />
            </div>
          )}
        </div>
      )}
      {view?.pairings.map((pair) => (
        <article key={pair.id} className="pairing-card">
          <h3>Compare with device {pair.peer_short}</h3>
          <p className="sas-words" aria-label="Six comparison words">
            {pair.words.join(' · ')}
          </p>
          <p>
            Compare all six words by trusted call or in person. Confirm on both
            devices; reject any mismatch.
          </p>
          <p role="status">
            {pair.state === 'Reserved'
              ? pair.local_confirmed
                ? 'You confirmed. Waiting for the other device.'
                : 'Waiting for your comparison.'
              : pair.state === 'Completed'
                ? 'Paired. Signed activity is shared.'
                : pair.state === 'Aborted'
                  ? 'Pairing rejected. Request a new private invitation.'
                  : 'Connection lost or timed out. Retry within five minutes or request a new invitation.'}
          </p>
          {pair.state === 'Reserved' && !pair.local_confirmed && (
            <div className="peer-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void run(() => client.networkConfirm(pair.id, true));
                }}
              >
                Words match
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void run(() => client.networkConfirm(pair.id, false));
                }}
              >
                Reject mismatch
              </button>
            </div>
          )}
        </article>
      ))}
      {!!view?.peers.length && (
        <ul aria-label="Peer paths">
          {view.peers.map((peer) => (
            <li key={peer.device_short}>
              {peer.device_short} · {peer.path}
            </li>
          ))}
        </ul>
      )}
      {(error || view?.error) && (
        <p className="error-notice" role="alert">
          {error || view?.error}
        </p>
      )}
      {view?.running && setup && (
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void run(async () => {
              await client.networkStop();
              setInvite('');
            });
          }}
        >
          Stop sharing
        </button>
      )}
    </section>
  );
}
