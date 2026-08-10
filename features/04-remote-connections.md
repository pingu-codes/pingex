# Remote connections

Priority: P1

## What it should do

Manage all paired clients and remote hosts, not only create one QR pairing. Show connection health, last seen time, permissions, and safe disconnect/revoke actions.

## How

Keep the existing QR flow as enrollment. Add Rust commands for listing, refreshing, renaming, disconnecting, and revoking connections through the remote-control service. Treat pairing tokens as native-only data and make state changes idempotent.

## What it should look like

Move pairing into a Connections page. Show a prominent `Connect device` action, then device cards with platform, name, online state, last seen, and permission scope. Put destructive revoke actions behind confirmation and show a persistent connection indicator in the app shell.
