# Meshyard

See your local and paired teammates’ Claude Code and Codex agents in a pixel yard. Activity comes from
local hooks: editing moves an agent to Building, test commands to Testing, merge
commands to Merging, and a completed turn to Finished. Registration alone never
claims an agent is connected.

## Install and start

Use the Apple Silicon DMG and consult the accompanying German REPORT.md for the
exact package checks and limitations:

1. Quit any running Meshyard normally. Open the Meshyard DMG and drag Meshyard to `/Applications`.
2. Eject the DMG, then open Meshyard from Applications. Ad-hoc builds may require
   right-click → Open in macOS. See the [German tester guide](docs/TESTER-GUIDE.md).
3. Create a workspace with your name. Local data uses the application data directory;
   device identity uses macOS Keychain. Unlock/allow Keychain access if macOS asks.
   Errors are shown explicitly; Meshyard never silently switches key stores.
4. Register one agent of each kind you use. Open **Connect agents**, choose the kind
   and select your agent. Review the exact settings diff and press **Install**.
5. For Codex, open `/hooks` in Codex and review and trust the new Meshyard definitions.
   New or changed definitions do not run until trusted. Restart your CLI or send a
   message; the app changes from waiting to Connected only after an event arrives.

Claude Code installs to `~/.claude/settings.json`; Codex to `~/.codex/hooks.json`.
The installer keeps unrelated settings, creates a private backup before replacement,
and refuses changes if the reviewed settings have changed. Installation never
happens at startup. Hook commands use the executable inside the installed app.
Keep Meshyard running to receive events.

## Inspect activity

Click an agent name in the yard or select its name in the accessible agent list.
The inspector shows the last state and its provenance. **Verified** means a
hook-shaped event from the same OS user; **Inferred** means a tool category or
command classifier rule. Neither proves the work succeeded. After ten minutes
without activity, active agents show Unknown. Finished becomes Idle after a minute.
Use **Yard**, **Agent list** and **Settings** to switch views. Settings puts your
display-name edit first, with storage/privacy and font-license details below.

Review, Fix and Waiting are unsupported: frozen wire v1 cannot label their
provenance honestly. Codex model-request failures are not relabelled as verified
StopFailure; failed requests may be retried. No prompts, paths, commands, transcripts
or tool output are sent to the app by the hook.

## Pair a teammate

1. As workspace owner, open **Peers** → **Create private invite** → **Show private
   link and QR**. The single-use invitation expires after 15 minutes and includes
   your direct IP addresses. Share it privately; never paste it into public logs.
2. On a fresh profile, choose **Join existing workspace**, enter a display name
   and paste the link. Both sides compare all six words through a trusted call or
   in person, then press **Words match**. Reject any mismatch and request a new link.
3. Each side registers and connects its own agents. Signed activity appears in
   both yards. **Synced activity (last known)** is a persisted observation; consult
   **Peers** for the current direct connection. Each device sends only its own log,
   so offline members’ activity is unavailable until they reconnect.
4. **Stop sharing** closes the current endpoint. **Connect paired devices** retries
   saved direct contacts. A confirmed join awaiting the owner’s signed membership
   is saved with the same identity and resumes after reopening; no profile deletion
   is needed. A reservation can resume for five minutes with the same identity/nonce.

Strict P2P has no relay or public address discovery. Firewalls, NAT and changed IP
addresses can prevent contact; there is no fallback. Cross-network and two-Mac
results are not validated. Invitations are pasted into the join form; OS deep-link
launch handling and native QR scanning are not implemented/validated respectively.

## Disconnect and uninstall

Open **Connect agents**, choose the kind, then **Review hook removal**. Review the
diff and press **Remove hook**. Repeat for the other kind before removing Meshyard.
Backups remain beside each settings file (`*.meshyard-backup-*`); do not blindly
restore one over newer unrelated settings. Removing hooks preserves agent history.

## Troubleshooting

- **Waiting for first event:** check that Meshyard is open, the correct agent is
  selected and the CLI has restarted. In Codex inspect `/hooks` and trust the exact
  definitions; project-local hooks also require project trust. Hooks may be disabled
  by CLI or administrator policy. [Current official Codex reference](https://learn.chatgpt.com/docs/hooks).
- **Move to Applications:** hook installation is blocked from the DMG, translocated
  app or temporary bundle. Move the app, reopen it, and review a fresh plan.
- **Settings changed:** review a new plan. Invalid JSON and unsafe config paths are
  left untouched; fix the config in your CLI before trying again.
- **Keychain locked/unavailable:** unlock your login Keychain and restart. Keep the
  original identity; do not delete it or select another backend for an existing workspace.
- **Data directory busy:** quit the other Meshyard instance using the same profile.
- **Graphics unavailable:** the agent list exposes the same real states and provenance.

Build/test instructions and isolated profiles: [Development](docs/DEVELOPMENT.md).
Validation evidence and open native gates: [Rework report](docs/reports/REWORK.md).

The yard uses original pixel art and locally bundled OFL-licensed Pixelify Sans;
body text stays in a readable system font. Dion’s original island map has eleven fixed station buildings. Review, Fix and Waiting
are reserved, not reported hook activity. Three avatars fit each entrance; station counts
and **Agent list** include everyone. The map scales in integer steps (minimum 2x);
focus it and use arrow keys or pan with touch/scrollbars on small windows. Reduce Motion in macOS disables walking and poses.
The tested unique-service Keychain roundtrip does not certify the first default
Keychain launch of the packaged app; that remains the documented owner checkpoint.
