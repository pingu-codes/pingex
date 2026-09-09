# Pingex

A desktop frontend for agent CLIs, Codex first and Claude Code next. This
glossary covers the terms Pingex uses for the harnesses it drives and for its
relationship with the Codex versions it runs against.

## Language

**Harness**:
An agent CLI Pingex drives. Codex over its app-server protocol, Claude Code
over its stdio stream-json protocol, or any agent through the Agent Client
Protocol. The app never branches on which one; it asks the Driver.
_Avoid_: backend, provider, agent (the thing inside the harness), engine

**Home**:
One harness's config directory: `CODEX_HOME` for Codex, `CLAUDE_CONFIG_DIR`
or `~/.claude` for Claude Code, on one Host. A conversation keeps its Home;
changing the default affects new conversations.
_Avoid_: account, workspace (already the multi-project hub), config dir (fine in code, not in prose)

**Profile**:
The projects, Homes and sidebar organization a Pingex window shares. A Profile
can contain Windows and WSL projects together while keeping separate accounts.
_Avoid_: home (a Profile contains Homes), instance, session

**Host**:
Where a project's files and harness processes run, either this computer or
one WSL distribution. Each project has a fixed Host, and every member of a
multi-project workspace must share that Host.
_Avoid_: machine, remote, target, platform (the OS Pingex is built for), environment

**Driver**:
The Rust implementation of the harness interface (`Driver` trait) for one
harness: `CodexDriver`, `ClaudeDriver`, later `AcpDriver`. Owns the child
processes and translates the wire into `HarnessEvent`s.
_Avoid_: adapter (that is what the ACP projects call theirs), session, client

**Capability**:
Something a Driver can do beyond the required core (chat, streaming, tool
items, approvals, interrupt, resume and list, model and effort). Either
declared by the driver at construction or probed once and its refusal
remembered. The UI hides what is absent. Feature is the probed half, kept
under that name inside the Codex driver.
_Avoid_: flag, feature flag, version gate, support level

**Goal**:
An objective a thread keeps working towards across turns (`/goal`). Codex
owns it — the app sets, pauses, caps and clears it over `thread/goal/*` and
shows what Codex reports back. Codex-only today.
_Avoid_: task, mission, loop

**Token budget**:
The cap on a Goal's spend, in tokens. Codex counts every turn it drives
against it and parks the goal as `budgetLimited` once reached; raising or
lifting the cap lets it carry on. Set from the goal banner or
`/goal budget <tokens>`.
_Avoid_: quota, limit (that is the account's rate limit), cost cap

**Usage row**:
What one turn cost, as stored: the wire token figures, the harness's cost
where it gives one, and the Attribution of those tokens. One per turn per
thread, rewritten as the turn's reports arrive; summed for a project or a
Home.
_Avoid_: usage record, token log, billing entry

**Attribution**:
A turn's tokens split across the usage categories — system prompt, skills,
the user's messages, tool use, replies, reasoning. Input tokens are shared
out by the Context composition, so the prompt-side categories are estimates
(marked ≈); replies and reasoning are exact.
_Avoid_: breakdown (the view, not the split), allocation, apportionment

**Context composition**:
What a thread's context window holds right now, by category. Estimated from
the journaled items and the measured context size, or reported by the
harness where it can say (Claude Code's `get_context_usage`).
_Avoid_: context usage (the meter's single figure), context map

**Opening row**:
The one Usage row a thread gets for usage that happened before the ledger
could see it: what a replayed running total exceeds the stored rows by,
booked unattributed under `turn_id = "_opening"`.
_Avoid_: backfill, catch-up row, legacy usage

**Version**:
One of the texts a user message has had. Editing a message never rewrites
it: the edit becomes a new Version, shown under the message as `‹ 2 / 3 ›`,
and every Version keeps the replies that followed it.
_Avoid_: revision, edit (the act, not the result), history

**Branch**:
The thread a Version lives in. Editing forks the thread strictly before the
edited message's turn and sends the new text on the fork; Pingex records the
fork in `thread_branches` and keeps it out of the sidebar. Turn ids survive a
fork, so the original turn's id names the whole group of Versions.
_Avoid_: fork (the operation that makes one), child thread, side question (a different hidden thread)

**Root thread**:
The thread at the top of a family of Branches — the one the sidebar lists and
highlights whichever Branch is on show. Opening it lands on the most recently
active Branch.
_Avoid_: parent (a Branch's parent may itself be a Branch), main thread

**Codex**:
The upstream CLI (`codex`) whose `app-server` subcommand Pingex spawns and
talks JSON-RPC to. One of the harnesses.
_Avoid_: backend, server (ambiguous with the app-server child and the MCP servers Codex hosts)

**Claude Code**:
The `claude` CLI, driven with `-p --input-format stream-json --output-format
stream-json --permission-prompt-tool stdio`. One of the harnesses. One
process is one session.
_Avoid_: Claude (the model), the SDK (Pingex does not use the Node SDK)

**Tier**:
One of the three Codex versions Pingex aims to support at a time: Unstable,
Stable and Last stable. Recorded in `docs/SUPPORTED_VERSIONS.md`.

**Unstable**:
Upstream Codex `main`, as mirrored in `../codex-mirror`. A source build of it
reports version `0.0.0`.
_Avoid_: nightly, HEAD, mirror (the mirror is where it lives, not what it is)

**Stable**:
The latest tagged Codex release; the version Pingex is written and tested against.
_Avoid_: current, latest

**Last stable**:
The tagged release before Stable. Supported so a user one release behind is
not broken.
_Avoid_: previous, legacy, N-1

**Feature**:
An app-server API that some supported tiers lack, tried once per Codex child
and remembered as present or refused. The probed half of a Capability,
specific to the Codex driver.
_Avoid_: flag, capability check, version gate, feature flag
