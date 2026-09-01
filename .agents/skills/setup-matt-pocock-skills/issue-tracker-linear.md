# Issue tracker: Linear

Issues and specs for this repo live in Linear, team **Pingex**. Use the `linear-server` MCP tools for all operations (`mcp__linear-server__*`).

## Conventions

- **Create an issue**: `save_issue` with `team: "Pingex"`, `title`, `description` (Markdown, literal newlines).
- **Read an issue**: `get_issue` (by id or identifier, e.g. `PIN-12`) plus `list_comments`.
- **List issues**: `list_issues` with `team`, `label`, `state`, `parentId`, `assignee` filters.
- **Comment on an issue**: `save_comment`.
- **Apply / remove labels**: `save_issue` with `labels` (replaces the full set — re-send the ones to keep).
- **Close**: `save_issue` with `state: "Done"` (or `"Canceled"` for out-of-scope).

Refer to issues by **title** in prose; the identifier rides inside the link.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## When a skill says "publish to the issue tracker"

Create a Linear issue in team Pingex.

## When a skill says "fetch the relevant ticket"

`get_issue` + `list_comments`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **sub-issues** as tickets.

- **Map**: one issue labelled `wayfinder:map`, holding the Destination / Notes / Decisions-so-far / Not yet specified / Out of scope body.
- **Child ticket**: a sub-issue of the map (`save_issue` with `parentId: <map>`), labelled `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Create tickets in map order so Linear's sub-issue order matches.
- **Blocking**: Linear's **native** relation — `save_issue` with `blockedBy: [<ids>]` (append-only; `removeBlockedBy` to drop). Visible in Linear's UI.
- **Frontier query**: `list_issues` with `parentId: <map>`, `state: "Todo"`/`"Backlog"` (open), drop any with an assignee or with an open issue in its blocked-by relations; first in sub-issue order wins.
- **Claim**: `save_issue` with `assignee: "me"`, the session's first write.
- **Resolve**: `save_comment` with the answer, `save_issue` with `state: "Done"`, then `save_issue` on the map with a `patch` appending `- [<title>](<url>): <gist>` to **Decisions so far**.
