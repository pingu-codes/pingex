# Browser and computer use

Priority: P1

## What it should do

Allow Codex to operate approved browser tabs or computer actions while making every navigation, click, upload, and permission boundary visible to the user.

## How

Add a native browser-control bridge with isolated sessions, explicit host/permission grants, streamed action events, screenshots, and approval requests. Represent browser work as typed thread items so it survives resume and can be replayed safely. Fail closed when a target tab or permission is unavailable.

## What it should look like

Add a Browser tab to the right-side panel with a live preview, URL/title bar, action timeline, and stop button. Show approval cards inline for sensitive actions. Settings should expose browser/computer permissions separately from shell permissions.
